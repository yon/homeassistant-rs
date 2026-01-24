"""Pytest configuration for Home Assistant compatibility tests.

This conftest patches HA's core types with our Rust implementations to verify
API compatibility. The wrappers delegate to Rust for core operations while
providing HA-compatible API surface.

Usage:
    # Run with Rust components
    pytest ../core/tests/test_core.py -k "test_state" -p tests.ha_compat.conftest

    # Run without Rust (baseline)
    USE_RUST_COMPONENTS=0 pytest ../core/tests/test_core.py -k "test_state"
"""

import asyncio
import os
import time
from collections import defaultdict
from collections.abc import Callable, Iterable
from datetime import datetime, timezone
from typing import Any
from unittest.mock import patch

# Import UNDEFINED sentinel for distinguishing "not passed" from "None"
try:
    from homeassistant.helpers.typing import UNDEFINED, UndefinedType
except ImportError:
    # Fallback sentinel if HA not available
    class UndefinedType:
        _singleton = None
        def __repr__(self):
            return "UNDEFINED"
    UndefinedType._singleton = UndefinedType()
    UNDEFINED = UndefinedType._singleton

import pytest

# Import HA exceptions and types for API compatibility
try:
    from homeassistant.exceptions import InvalidEntityFormatError
    from homeassistant.util.read_only_dict import ReadOnlyDict
    from homeassistant.core import EventOrigin, State as NativeState
    from homeassistant.util import dt as dt_util
    from homeassistant.util.ulid import ulid_at_time
except ImportError:
    class InvalidEntityFormatError(Exception):
        pass

    class ReadOnlyDict(dict):
        pass

    import enum
    class EventOrigin(enum.Enum):
        local = "LOCAL"
        remote = "REMOTE"

    NativeState = object
    dt_util = None
    ulid_at_time = None

# Check environment variable to enable/disable Rust patching
USE_RUST_COMPONENTS = os.environ.get("USE_RUST_COMPONENTS", "1") != "0"

# Import Rust extension if available
_rust_available = False
_rust_hass = None  # Shared Rust HomeAssistant instance
_UNDEFINED = object()  # Sentinel to distinguish "not provided" from "set to None"

if USE_RUST_COMPONENTS:
    try:
        import ha_core_rs
        _rust_available = True
    except ImportError:
        print("Warning: ha_core_rs not found, running pure Python tests")
        USE_RUST_COMPONENTS = False


def _get_rust_hass():
    """Get or create the shared Rust HomeAssistant instance."""
    global _rust_hass
    if _rust_hass is None and _rust_available:
        _rust_hass = ha_core_rs.HomeAssistant()
    return _rust_hass


def _parse_iso_datetime(iso_str: str) -> datetime:
    """Parse ISO format datetime string to datetime object."""
    if iso_str.endswith('Z'):
        iso_str = iso_str[:-1] + '+00:00'
    return datetime.fromisoformat(iso_str)


# =============================================================================
# Rust-backed State wrapper
# =============================================================================

class RustState(NativeState):
    """Wrapper that provides HA-compatible State API backed by Rust storage."""

    __slots__ = ()  # All slots defined in parent NativeState

    def __init__(
        self,
        entity_id: str,
        state: str,
        attributes: dict[str, Any] | None = None,
        last_changed: datetime | None = None,
        last_reported: datetime | None = None,
        last_updated: datetime | None = None,
        context: "RustContext | None" = None,
        validate_entity_id: bool | None = True,
        state_info: Any = None,
        last_updated_timestamp: float | None = None,
    ) -> None:
        """Initialize a new state."""
        self._cache: dict[str, Any] = {}

        if validate_entity_id and not ha_core_rs.valid_entity_id(entity_id):
            raise InvalidEntityFormatError(
                f"Invalid entity id encountered: {entity_id}. "
                "Format should be <domain>.<object_id>"
            )

        self.entity_id = entity_id
        self.state = state
        if attributes is None:
            self.attributes = ReadOnlyDict({})
        elif isinstance(attributes, ReadOnlyDict):
            self.attributes = attributes
        else:
            self.attributes = ReadOnlyDict(attributes)

        now = datetime.now(timezone.utc)
        self.last_reported = last_reported or now
        self.last_updated = last_updated or self.last_reported
        self.last_changed = last_changed or self.last_updated

        if last_updated_timestamp is None:
            last_updated_timestamp = self.last_updated.timestamp()
        self.last_updated_timestamp = last_updated_timestamp

        if self.last_changed == self.last_updated:
            self._cache["last_changed_timestamp"] = last_updated_timestamp
        if self.last_reported is self.last_updated:
            self._cache["last_reported_timestamp"] = last_updated_timestamp

        self.context = context or RustContext()
        self.state_info = state_info
        self.domain, self.object_id = entity_id.split('.', 1)

    @classmethod
    def from_rust(cls, rust_state, context: "RustContext | None" = None) -> "RustState":
        """Create RustState from ha_core_rs PyState."""
        state = cls.__new__(cls)
        state._cache = {}
        state.entity_id = str(rust_state.entity_id)
        state.state = rust_state.state
        # Convert Rust attributes dict to ReadOnlyDict
        state.attributes = ReadOnlyDict(dict(rust_state.attributes))
        state.last_changed = _parse_iso_datetime(rust_state.last_changed)
        state.last_updated = _parse_iso_datetime(rust_state.last_updated)
        state.last_updated_timestamp = state.last_updated.timestamp()
        state.last_reported = state.last_updated
        state.context = context or RustContext()
        state.state_info = None
        state.domain, state.object_id = state.entity_id.split('.', 1)
        return state

    @classmethod
    def from_dict(cls, json_dict: dict[str, Any] | None) -> "RustState | None":
        """Create State from a dict (e.g., from JSON)."""
        if json_dict is None:
            return None

        if "entity_id" not in json_dict or "state" not in json_dict:
            return None

        last_changed = json_dict.get("last_changed")
        if isinstance(last_changed, str):
            last_changed = _parse_iso_datetime(last_changed)

        last_updated = json_dict.get("last_updated")
        if isinstance(last_updated, str):
            last_updated = _parse_iso_datetime(last_updated)

        last_reported = json_dict.get("last_reported")
        if isinstance(last_reported, str):
            last_reported = _parse_iso_datetime(last_reported)

        context_dict = json_dict.get("context")
        context = None
        if context_dict:
            context = RustContext(
                id=context_dict.get("id"),
                user_id=context_dict.get("user_id"),
                parent_id=context_dict.get("parent_id"),
            )

        return cls(
            entity_id=json_dict["entity_id"],
            state=json_dict["state"],
            attributes=json_dict.get("attributes", {}),
            last_changed=last_changed,
            last_updated=last_updated,
            context=context,
            validate_entity_id=False,
            last_reported=last_reported,
        )

    @property
    def as_compressed_state(self) -> dict[str, Any]:
        result = {
            "s": self.state,
            "a": self.attributes,
            "c": self.context.id,
            "lc": self.last_changed_timestamp,
        }
        if self.last_updated_timestamp != self.last_changed_timestamp:
            result["lu"] = self.last_updated_timestamp
        return result

    @property
    def as_compressed_state_json(self) -> bytes:
        import orjson
        if "as_compressed_state_json" not in self._cache:
            compressed = self.as_compressed_state
            self._cache["as_compressed_state_json"] = (
                b'"' + self.entity_id.encode() + b'":' + orjson.dumps(compressed)
            )
        return self._cache["as_compressed_state_json"]

    @property
    def as_dict_json(self) -> bytes:
        import orjson
        if "as_dict_json" not in self._cache:
            d = {
                "entity_id": self.entity_id,
                "state": self.state,
                "attributes": self.attributes,
                "last_changed": self.last_changed.isoformat(),
                "last_reported": self.last_reported.isoformat(),
                "last_updated": self.last_updated.isoformat(),
                "context": self.context.as_dict(),
            }
            self._cache["as_dict_json"] = orjson.dumps(d)
        return self._cache["as_dict_json"]

    @property
    def json_fragment(self) -> Any:
        import orjson
        if "json_fragment" not in self._cache:
            self._cache["json_fragment"] = orjson.Fragment(orjson.dumps(self.as_dict()))
        return self._cache["json_fragment"]

    @property
    def last_changed_timestamp(self) -> float:
        if "last_changed_timestamp" not in self._cache:
            if self.last_changed == self.last_updated:
                self._cache["last_changed_timestamp"] = self.last_updated_timestamp
            else:
                self._cache["last_changed_timestamp"] = self.last_changed.timestamp()
        return self._cache["last_changed_timestamp"]

    @property
    def last_reported_timestamp(self) -> float:
        if "last_reported_timestamp" not in self._cache:
            self._cache["last_reported_timestamp"] = self.last_reported.timestamp()
        return self._cache["last_reported_timestamp"]

    @property
    def name(self) -> str:
        return self.attributes.get('friendly_name') or self.object_id.replace('_', ' ')

    def as_dict(self) -> ReadOnlyDict:
        if "as_dict" not in self._cache:
            self._cache["as_dict"] = ReadOnlyDict({
                "entity_id": self.entity_id,
                "state": self.state,
                "attributes": self.attributes,
                "last_changed": self.last_changed.isoformat(),
                "last_reported": self.last_reported.isoformat(),
                "last_updated": self.last_updated.isoformat(),
                "context": self.context.as_dict(),
            })
        return self._cache["as_dict"]

    def expire(self) -> None:
        pass

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, RustState):
            return NotImplemented
        return (
            self.entity_id == other.entity_id
            and self.state == other.state
            and self.attributes == other.attributes
        )

    def __hash__(self) -> int:
        return hash((self.entity_id, self.state))

    def __repr__(self) -> str:
        last_changed_str = self.last_changed.isoformat()
        if self.last_changed.tzinfo is None:
            last_changed_str += "+00:00"
        attrs_str = ""
        if self.attributes:
            attrs_str = "; " + ", ".join(f"{k}={v}" for k, v in self.attributes.items())
        return f"<state {self.entity_id}={self.state}{attrs_str} @ {last_changed_str}>"


# =============================================================================
# Rust-backed Context wrapper
# =============================================================================

class RustContext:
    """Wrapper that provides HA-compatible Context API backed by Rust."""

    __slots__ = ("_id", "_user_id", "_parent_id", "_origin_event", "_cache")

    def __init__(
        self,
        user_id=None,
        parent_id=None,
        id: str | None = None,
    ) -> None:
        if _rust_available and id is None:
            # Use Rust to generate ULID
            rust_ctx = ha_core_rs.Context()
            self._id = rust_ctx.id
        else:
            if id is None:
                import ulid
                self._id = str(ulid.new())
            else:
                self._id = id
        self._user_id = user_id
        self._parent_id = parent_id
        self._origin_event = None
        self._cache: dict[str, Any] = {}

    @property
    def id(self) -> str:
        return self._id

    @property
    def json_fragment(self) -> Any:
        import orjson
        if "json_fragment" not in self._cache:
            self._cache["json_fragment"] = orjson.Fragment(orjson.dumps(self.as_dict()))
        return self._cache["json_fragment"]

    @property
    def origin_event(self) -> Any:
        return self._origin_event

    @origin_event.setter
    def origin_event(self, value: Any) -> None:
        self._origin_event = value

    @property
    def parent_id(self) -> str | None:
        return self._parent_id

    @property
    def user_id(self) -> str | None:
        return self._user_id

    def as_dict(self) -> ReadOnlyDict:
        return ReadOnlyDict({
            "id": self._id,
            "parent_id": self._parent_id,
            "user_id": self._user_id,
        })

    def __eq__(self, other: object) -> bool:
        # Duck-type: compare with any Context-like object (native HA or RustContext)
        try:
            return self._id == other.id
        except AttributeError:
            return NotImplemented

    def __hash__(self) -> int:
        return hash(self._id)

    def __repr__(self) -> str:
        return f"<Context id={self._id}, user_id={self._user_id}>"


# =============================================================================
# Rust-backed StateMachine wrapper
# =============================================================================

class RustStateMachine:
    """Wrapper that provides HA-compatible StateMachine API backed by Rust storage."""

    __slots__ = ("_rust_states", "_bus", "_loop", "_contexts", "_reservations")

    def __init__(self, bus: "RustEventBus", loop: asyncio.AbstractEventLoop) -> None:
        rust_hass = _get_rust_hass()
        self._rust_states = rust_hass.states  # PyStateMachine from Rust
        self._bus = bus
        self._loop = loop
        # Track contexts separately since Rust State doesn't store full context
        self._contexts: dict[str, RustContext] = {}
        self._reservations: set[str] = set()

    @property
    def _states_data(self) -> dict[str, RustState]:
        """Return states as dict for compatibility."""
        result = {}
        for rust_state in self._rust_states.all():
            entity_id = str(rust_state.entity_id)
            ctx = self._contexts.get(entity_id)
            result[entity_id] = RustState.from_rust(rust_state, ctx)
        return result

    def all(self, domain_filter: str | Iterable[str] | None = None) -> list[RustState]:
        if domain_filter is None:
            rust_states = self._rust_states.all()
        elif isinstance(domain_filter, str):
            rust_states = self._rust_states.domain_states(domain_filter)
        else:
            rust_states = []
            for domain in domain_filter:
                rust_states.extend(self._rust_states.domain_states(domain))

        return [
            RustState.from_rust(rs, self._contexts.get(str(rs.entity_id)))
            for rs in rust_states
        ]

    def async_all(
        self, domain_filter: str | Iterable[str] | None = None
    ) -> list[RustState]:
        return self.all(domain_filter)

    def async_available(self, entity_id: str) -> bool:
        entity_id = entity_id.lower()
        return (
            self._rust_states.get(entity_id) is None
            and entity_id not in self._reservations
        )

    def async_entity_ids(
        self, domain_filter: str | Iterable[str] | None = None
    ) -> list[str]:
        if domain_filter is None:
            return self._rust_states.all_entity_ids()
        if isinstance(domain_filter, str):
            return self._rust_states.entity_ids(domain_filter)
        # Multiple domains
        result = []
        for domain in domain_filter:
            result.extend(self._rust_states.entity_ids(domain))
        return result

    def async_entity_ids_count(
        self, domain_filter: str | Iterable[str] | None = None
    ) -> int:
        return len(self.async_entity_ids(domain_filter))

    def async_remove(self, entity_id: str, context: RustContext | None = None) -> bool:
        entity_id = entity_id.lower()
        if self._rust_states.get(entity_id) is None:
            return False

        rust_ctx = None
        if context is not None:
            rust_ctx = ha_core_rs.Context(
                user_id=context.user_id, parent_id=context.parent_id,
            )
        # Rust StateStore.remove() fires STATE_CHANGED event internally
        self._rust_states.remove(entity_id, rust_ctx)
        self._contexts.pop(entity_id, None)
        return True

    def async_reserve(self, entity_id: str) -> None:
        self._reservations.add(entity_id.lower())

    def async_set(
        self,
        entity_id: str,
        new_state: str,
        attributes: dict[str, Any] | None = None,
        force_update: bool = False,
        context: RustContext | None = None,
    ) -> None:
        self.async_set_internal(
            entity_id, new_state, attributes, force_update, context,
            state_info=None, timestamp=None
        )

    def async_set_internal(
        self,
        entity_id: str,
        new_state: str,
        attributes: dict[str, Any] | None = None,
        force_update: bool = False,
        context: RustContext | None = None,
        *,
        state_info: Any = None,
        timestamp: datetime | None = None,
    ) -> None:
        entity_id = entity_id.lower()
        context = context or RustContext()

        rust_ctx = ha_core_rs.Context(
            user_id=context.user_id,
            parent_id=context.parent_id,
        )
        try:
            # Rust handles: validation, change detection, storage, and event firing
            self._rust_states.async_set(
                entity_id, new_state, attributes or {}, rust_ctx, force_update
            )
        except ValueError as e:
            msg = str(e)
            if "State max length" in msg:
                from homeassistant.exceptions import InvalidStateError
                raise InvalidStateError(msg)
            raise InvalidEntityFormatError(
                f"Invalid entity id encountered: {entity_id}. "
                "Format should be <domain>.<object_id>"
            )

        self._contexts[entity_id] = context
        self._reservations.discard(entity_id)

    def entity_ids(self, domain_filter: str | None = None) -> list[str]:
        if domain_filter:
            return self._rust_states.entity_ids(domain_filter)
        return self._rust_states.all_entity_ids()

    def get(self, entity_id: str) -> RustState | None:
        entity_id = entity_id.lower()
        rust_state = self._rust_states.get(entity_id)
        if rust_state is None:
            return None
        return RustState.from_rust(rust_state, self._contexts.get(entity_id))

    def is_state(self, entity_id: str, state: str) -> bool:
        return self._rust_states.is_state(entity_id.lower(), state)

    def remove(self, entity_id: str) -> bool:
        entity_id = entity_id.lower()
        result = self._rust_states.remove(entity_id)
        if result is not None:
            self._contexts.pop(entity_id, None)
            return True
        return False

    def set(
        self,
        entity_id: str,
        new_state: str,
        attributes: dict[str, Any] | None = None,
        force_update: bool = False,
        context: RustContext | None = None,
    ) -> None:
        self.async_set(entity_id, new_state, attributes, force_update, context)


# =============================================================================
# Rust-backed EventBus wrapper
# =============================================================================

class RustEventBus:
    """Thin wrapper around Rust PyEventBus providing HA-compatible API.

    All event dispatch happens in Rust. Python listeners are registered as
    sync callbacks on the Rust EventBus and fire inline during fire().
    """

    __slots__ = ("_hass", "_rust_bus")

    def __init__(self, hass: Any) -> None:
        self._hass = hass
        self._rust_bus = _get_rust_hass().bus  # PyEventBus from Rust

    @property
    def _debug(self) -> bool:
        return False

    def _to_rust_context(self, context: RustContext | None) -> Any:
        if context is None:
            return None
        return ha_core_rs.Context(
            user_id=context.user_id,
            parent_id=context.parent_id,
        )

    def async_fire(
        self,
        event_type: str,
        event_data: dict[str, Any] | None = None,
        origin: EventOrigin = EventOrigin.local,
        context: RustContext | None = None,
        time_fired: float | None = None,
    ) -> None:
        self._rust_bus.async_fire(
            event_type, event_data, context=self._to_rust_context(context),
        )

    def async_fire_internal(
        self,
        event_type: str,
        event_data: dict[str, Any] | None = None,
        origin: Any = None,
        context: RustContext | None = None,
        time_fired: float | None = None,
    ) -> None:
        self.async_fire(event_type, event_data, origin, context, time_fired)

    def async_listen(
        self,
        event_type: str,
        listener: Callable,
        run_immediately: bool = False,
        event_filter: Callable | None = None,
    ) -> Callable[[], None]:
        return self._rust_bus.async_listen(event_type, listener, run_immediately, event_filter)

    def async_listen_once(
        self,
        event_type: str,
        listener: Callable,
        run_immediately: bool = False,
    ) -> Callable[[], None]:
        return self._rust_bus.async_listen_once(event_type, listener, run_immediately)

    def async_listeners(self) -> dict[str, int]:
        return self._rust_bus.async_listeners()

    def fire(
        self,
        event_type: str,
        event_data: dict[str, Any] | None = None,
        origin: EventOrigin = EventOrigin.local,
        context: RustContext | None = None,
        time_fired: float | None = None,
    ) -> None:
        self.async_fire(event_type, event_data, origin, context, time_fired)

    def listen(
        self,
        event_type: str,
        listener: Callable,
    ) -> Callable[[], None]:
        return self.async_listen(event_type, listener)

    def listen_once(
        self,
        event_type: str,
        listener: Callable,
    ) -> Callable[[], None]:
        return self.async_listen_once(event_type, listener)

    def listeners(self) -> dict[str, int]:
        return self.async_listeners()


# =============================================================================
# Rust-backed Event wrapper
# =============================================================================

class RustEvent:
    """Wrapper for Event compatible with homeassistant.core.Event."""

    __slots__ = ("event_type", "data", "origin", "time_fired_timestamp", "context", "_cache")

    def __class_getitem__(cls, item):
        """Support generic subscript syntax (e.g., Event[EventStateChangedData])."""
        return cls

    def __init__(
        self,
        event_type: str,
        data: dict[str, Any] | None = None,
        origin: EventOrigin = EventOrigin.local,
        time_fired_timestamp: float | None = None,
        context: RustContext | None = None,
    ) -> None:
        self._cache: dict[str, Any] = {}
        self.event_type = event_type
        self.data = data or {}
        self.origin = origin
        self.time_fired_timestamp = time_fired_timestamp or time.time()

        if not context:
            import homeassistant.core as ha_core
            if ulid_at_time is not None:
                context = ha_core.Context(id=ulid_at_time(self.time_fired_timestamp))
            else:
                context = ha_core.Context()
        self.context = context

        if hasattr(context, 'origin_event') and not context.origin_event:
            context.origin_event = self

    @property
    def _as_dict(self) -> dict[str, Any]:
        if "_as_dict" not in self._cache:
            self._cache["_as_dict"] = {
                "event_type": self.event_type,
                "data": self.data,
                "origin": self.origin.value,
                "time_fired": self.time_fired.isoformat(),
                "context": self.context.as_dict(),
            }
        return self._cache["_as_dict"]

    @property
    def json_fragment(self) -> Any:
        import orjson
        from homeassistant.helpers.json import json_encoder_default
        if "json_fragment" not in self._cache:
            self._cache["json_fragment"] = orjson.Fragment(
                orjson.dumps(self._as_dict, default=json_encoder_default)
            )
        return self._cache["json_fragment"]

    @property
    def time_fired(self) -> datetime:
        if "time_fired" not in self._cache:
            if dt_util is not None:
                self._cache["time_fired"] = dt_util.utc_from_timestamp(self.time_fired_timestamp)
            else:
                self._cache["time_fired"] = datetime.fromtimestamp(
                    self.time_fired_timestamp, tz=timezone.utc
                )
        return self._cache["time_fired"]

    def as_dict(self) -> ReadOnlyDict:
        if "_as_read_only_dict" not in self._cache:
            as_dict = self._as_dict
            if not isinstance(as_dict["data"], ReadOnlyDict):
                as_dict["data"] = ReadOnlyDict(as_dict["data"])
            if not isinstance(as_dict["context"], ReadOnlyDict):
                as_dict["context"] = ReadOnlyDict(as_dict["context"])
            self._cache["_as_read_only_dict"] = ReadOnlyDict(as_dict)
        return self._cache["_as_read_only_dict"]

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, RustEvent):
            return NotImplemented
        return (
            self.event_type == other.event_type
            and self.data == other.data
            and self.origin == other.origin
            and self.time_fired_timestamp == other.time_fired_timestamp
            and self.context == other.context
        )

    def __hash__(self) -> int:
        ctx_id = self.context.id if hasattr(self.context, 'id') else id(self.context)
        return hash((self.event_type, self.time_fired_timestamp, ctx_id))

    def __repr__(self) -> str:
        origin_char = "L" if self.origin == EventOrigin.local else "R"
        if self.data:
            data_str = ", ".join(f"{k}={v}" for k, v in self.data.items())
            return f"<Event {self.event_type}[{origin_char}]: {data_str}>"
        return f"<Event {self.event_type}[{origin_char}]>"


# =============================================================================
# Rust-backed ServiceCall wrapper
# =============================================================================

class RustServiceCall:
    """Wrapper for ServiceCall compatible with homeassistant.core.ServiceCall."""

    __slots__ = ("hass", "domain", "service", "data", "context", "return_response")

    def __init__(
        self,
        hass: Any,
        domain: str,
        service: str,
        data: dict[str, Any] | None = None,
        context: RustContext | None = None,
        return_response: bool = False,
    ) -> None:
        self.hass = hass
        self.domain = domain
        self.service = service
        self.data = ReadOnlyDict(data or {})
        self.context = context or RustContext()
        self.return_response = return_response

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, RustServiceCall):
            return False
        return (
            self.domain == other.domain
            and self.service == other.service
            and self.data == other.data
        )

    def __repr__(self) -> str:
        if self.data:
            data_str = ", ".join(f"{k}={v}" for k, v in self.data.items())
            return f"<ServiceCall {self.domain}.{self.service} (c:{self.context.id}): {data_str}>"
        return f"<ServiceCall {self.domain}.{self.service} (c:{self.context.id})>"


# =============================================================================
# Rust-backed Storage wrapper
# =============================================================================

class RustStorage:
    """Wrapper that provides HA-compatible Storage API backed by Rust."""

    def __init__(self, config_dir: str):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_storage = ha_core_rs.Storage(config_dir)
        self._config_dir = config_dir

    @property
    def config_dir(self) -> str:
        return self._config_dir

    async def async_delete(self, key: str) -> None:
        self._rust_storage.async_delete(key)

    async def async_exists(self, key: str) -> bool:
        return self._rust_storage.async_exists(key)

    async def async_list_keys(self) -> list[str]:
        return self._rust_storage.async_list_keys()

    async def async_load(self, key: str) -> dict | None:
        return self._rust_storage.async_load(key)

    async def async_save(
        self,
        key: str,
        data: dict,
        version: int = 1,
        minor_version: int = 1,
    ) -> None:
        self._rust_storage.async_save(key, data, version, minor_version)


# =============================================================================
# Rust-backed EntityRegistry wrappers
# =============================================================================

def _convert_categories(categories):
    """Convert categories from Rust JSON format to HA format.

    Categories can be either:
    - A dict (scope -> category_id mapping) - stays as dict
    - A set of strings (from Python) - becomes list in JSON, convert back to set
    - None/empty - returns empty dict
    """
    if not categories:
        return {}
    # If it's already a dict, return as-is
    if isinstance(categories, dict):
        return categories
    # If it's a list (from JSON array, originally a set), convert to set
    if isinstance(categories, list):
        return set(categories)
    return {}


def _rust_entry_to_registry_entry(rust_entry):
    """Convert a Rust EntityEntry to HA's RegistryEntry.

    This ensures tests comparing entries work correctly since HA's RegistryEntry
    is an attrs frozen class with proper equality comparison.
    """
    from homeassistant.helpers import entity_registry as er
    from homeassistant.helpers.entity import EntityCategory

    # Convert string enums to HA enum types
    disabled_by = None
    if rust_entry.disabled_by:
        disabled_by = er.RegistryEntryDisabler(rust_entry.disabled_by)

    hidden_by = None
    if rust_entry.hidden_by:
        hidden_by = er.RegistryEntryHider(rust_entry.hidden_by)

    entity_category = None
    if rust_entry.entity_category:
        entity_category = EntityCategory(rust_entry.entity_category)

    # Parse timestamps
    created_at = _parse_iso_datetime(rust_entry.created_at)
    modified_at = _parse_iso_datetime(rust_entry.modified_at)

    # Convert capabilities and options
    capabilities = rust_entry.capabilities
    options = rust_entry.options

    return er.RegistryEntry(
        entity_id=rust_entry.entity_id,
        unique_id=rust_entry.unique_id or "",
        platform=rust_entry.platform,
        previous_unique_id=rust_entry.previous_unique_id,
        aliases=set(rust_entry.aliases),
        area_id=rust_entry.area_id,
        categories=_convert_categories(rust_entry.categories),
        capabilities=capabilities,
        config_entry_id=rust_entry.config_entry_id,
        config_subentry_id=rust_entry.config_subentry_id,
        created_at=created_at,
        device_class=rust_entry.device_class,
        device_id=rust_entry.device_id,
        disabled_by=disabled_by,
        entity_category=entity_category,
        has_entity_name=rust_entry.has_entity_name,
        hidden_by=hidden_by,
        icon=rust_entry.icon,
        id=rust_entry.id,
        labels=set(rust_entry.labels),
        modified_at=modified_at,
        name=rust_entry.name,
        options=options,
        original_device_class=rust_entry.original_device_class,
        original_icon=rust_entry.original_icon,
        original_name=rust_entry.original_name,
        suggested_object_id=rust_entry.suggested_object_id,
        supported_features=rust_entry.supported_features,
        translation_key=rust_entry.translation_key,
        unit_of_measurement=rust_entry.unit_of_measurement,
    )


def _rust_entry_to_deleted_registry_entry(rust_entry):
    """Convert a Rust EntityEntry to HA's DeletedRegistryEntry.

    Used for entries in deleted_entities.
    """
    from homeassistant.helpers import entity_registry as er
    from homeassistant.helpers.entity import EntityCategory

    # Convert string enums to HA enum types
    disabled_by = None
    if rust_entry.disabled_by:
        disabled_by = er.RegistryEntryDisabler(rust_entry.disabled_by)

    hidden_by = None
    if rust_entry.hidden_by:
        hidden_by = er.RegistryEntryHider(rust_entry.hidden_by)

    # Parse timestamps
    created_at = _parse_iso_datetime(rust_entry.created_at)
    modified_at = _parse_iso_datetime(rust_entry.modified_at)

    # Get orphaned_timestamp from Rust entry
    orphaned_timestamp = rust_entry.orphaned_timestamp

    return er.DeletedRegistryEntry(
        entity_id=rust_entry.entity_id,
        unique_id=rust_entry.unique_id or "",
        platform=rust_entry.platform,
        aliases=set(rust_entry.aliases),
        area_id=rust_entry.area_id,
        categories=_convert_categories(rust_entry.categories),
        config_entry_id=rust_entry.config_entry_id,
        config_subentry_id=rust_entry.config_subentry_id,
        created_at=created_at,
        device_class=rust_entry.device_class,
        disabled_by=disabled_by,
        hidden_by=hidden_by,
        icon=rust_entry.icon,
        id=rust_entry.id,
        labels=set(rust_entry.labels),
        modified_at=modified_at,
        name=rust_entry.name,
        options=rust_entry.options,
        orphaned_timestamp=orphaned_timestamp,
    )


class _MockStore:
    """Mock store for flush_store compatibility.

    Bridges HA's flush_store helper to actually save the Rust registry.
    """
    def __init__(self, registry):
        self._registry = registry
        self._data = True  # Non-None so flush_store proceeds

    def _async_cleanup_final_write_listener(self):
        pass

    def _async_cleanup_delay_listener(self):
        pass

    async def _async_handle_write_data(self):
        # Actually save the registry to storage
        self._registry._rust_registry.async_save()


class RustEntityRegistryItems(dict):
    """Dict subclass that provides extra lookup methods like HA's EntityRegistryItems.

    Filter methods delegate to Rust indices for O(1) lookups instead of O(n) iteration.
    """

    def __init__(self, data, rust_registry=None, wrapper_fn=None):
        super().__init__(data)
        self._rust_registry = rust_registry
        self._wrapper_fn = wrapper_fn

    def get_device_ids(self):
        """Return device ids."""
        return {entry.device_id for entry in self.values() if entry.device_id is not None}

    def get_entity_id(self, key: tuple[str, str, str]) -> str | None:
        """Get entity_id from (domain, platform, unique_id)."""
        domain, platform, unique_id = key
        entity_id = self._rust_registry.async_get_entity_id(domain, platform, unique_id)
        return entity_id

    def get_entries_for_area_id(self, area_id: str) -> list:
        """Get entries for area (Rust index for matching IDs, dict order preserved)."""
        matching = {e.entity_id for e in self._rust_registry.async_entries_for_area(area_id)}
        return [entry for eid, entry in self.items() if eid in matching]

    def get_entries_for_config_entry_id(self, config_entry_id: str) -> list:
        """Get entries for config entry (Rust index for matching IDs, dict order preserved)."""
        matching = {e.entity_id for e in self._rust_registry.async_entries_for_config_entry(config_entry_id)}
        return [entry for eid, entry in self.items() if eid in matching]

    def get_entries_for_device_id(self, device_id: str, include_disabled_entities: bool = False) -> list:
        """Get entries for device (Rust index for matching IDs, dict order preserved)."""
        matching = {e.entity_id for e in self._rust_registry.async_entries_for_device(device_id)}
        return [
            entry for eid, entry in self.items()
            if eid in matching
            and (include_disabled_entities or not entry.disabled_by)
        ]

    def get_entries_for_label(self, label_id: str) -> list:
        """Get entries for label (Rust index for matching IDs, dict order preserved)."""
        matching = {e.entity_id for e in self._rust_registry.async_entries_for_label(label_id)}
        return [entry for eid, entry in self.items() if eid in matching]

    def get_entry(self, entity_id_or_uuid: str) -> object | None:
        """Get entry by entity_id or UUID."""
        # Try direct entity_id lookup first
        if entity_id_or_uuid in self:
            return self[entity_id_or_uuid]
        # Try UUID lookup via Rust
        for entry in self.values():
            if entry.id == entity_id_or_uuid:
                return entry
        return None


class RustEntityRegistry:
    """Wrapper that provides HA-compatible EntityRegistry API backed by Rust."""

    def __init__(self, hass):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.EntityRegistry(hass)
        self._hass = hass
        # Cache wrapper objects to maintain identity (for `is` checks in tests)
        self._entry_cache: dict[str, RustEntityEntry] = {}
        # Allow mock_registry to override entities with a native EntityRegistryItems
        self._entities_override = None
        # Mock store for flush_store compatibility - references this registry
        self._store = _MockStore(self)

    def _fire_event(self, action: str, entity_id: str, changes: dict | None = None, old_entity_id: str | None = None) -> None:
        """Fire entity registry updated event."""
        if self._hass is None:
            return
        from homeassistant.helpers import entity_registry as er
        data: dict = {"action": action, "entity_id": entity_id}
        if changes:
            data["changes"] = changes
        if old_entity_id:
            data["old_entity_id"] = old_entity_id
        self._hass.bus.async_fire(er.EVENT_ENTITY_REGISTRY_UPDATED, data)

    async def async_load(self) -> None:
        """Load entities from storage."""
        self._rust_registry.async_load()

    async def async_save(self) -> None:
        """Save entities to storage."""
        self._rust_registry.async_save()

    def _get_or_create_wrapper(self, rust_entry, force_new: bool = False):
        """Get cached wrapper or create and cache a new one.

        Returns actual HA RegistryEntry objects to ensure equality checks work correctly.
        The force_new parameter is used when data has been updated and we need a new instance.
        """
        entity_id = rust_entry.entity_id
        # Return cached entry for identity checks (is)
        if not force_new and entity_id in self._entry_cache:
            return self._entry_cache[entity_id]
        # Create new RegistryEntry from current Rust state
        entry = _rust_entry_to_registry_entry(rust_entry)
        self._entry_cache[entity_id] = entry
        return entry

    def async_clear_area_id(self, area_id: str) -> None:
        """Clear area id from registry entries."""
        # Update active entities to remove area_id
        # Use empty string as "clear" marker for Rust
        for entry in self._rust_registry.async_entries_for_area(area_id):
            self.async_update_entity(entry.entity_id, area_id="")
        # Clear from deleted entities
        self._rust_registry.clear_deleted_area_id(area_id)

    def async_clear_category_id(self, scope: str, category_id: str) -> None:
        """Clear a category from registry entries matching scope and category_id."""
        for entry in list(self.entities.values()):
            if entry.categories.get(scope) == category_id:
                new_categories = dict(entry.categories)
                del new_categories[scope]
                self.async_update_entity(entry.entity_id, categories=new_categories)
        # Clear from deleted entities
        self._rust_registry.clear_deleted_category_id(scope, category_id)

    def async_clear_config_entry(self, config_entry_id: str) -> None:
        """Clear config entry from registry entries."""
        import time

        # Get all entity IDs for this config entry via Rust index
        entity_ids = [
            entry.entity_id
            for entry in self._rust_registry.async_entries_for_config_entry(config_entry_id)
        ]
        # Bulk remove all entities at once in Rust, then fire events
        removed = self._rust_registry.async_bulk_remove(entity_ids)
        for entity_id in removed:
            self._entry_cache.pop(entity_id, None)
            self._fire_event("remove", entity_id)
        # Also clear config_entry_id from deleted entities and mark orphaned
        now_time = time.time()
        self._rust_registry.clear_deleted_config_entry(config_entry_id, now_time)

    def async_clear_config_subentry(
        self, config_entry_id: str, config_subentry_id: str
    ) -> None:
        """Clear config subentry from registry entries."""
        import time

        # Get entities for config entry and filter by subentry
        entity_ids = [
            entry.entity_id
            for entry in self._rust_registry.async_entries_for_config_entry(config_entry_id)
            if entry.config_subentry_id == config_subentry_id
        ]
        # Remove each matching entity
        for entity_id in entity_ids:
            self.async_remove(entity_id)
        # Update deleted entities matching this subentry
        now_time = time.time()
        self._rust_registry.clear_deleted_config_subentry(
            config_entry_id, config_subentry_id, now_time
        )

    def async_clear_label_id(self, label_id: str) -> None:
        """Clear label from registry entries."""
        # Use Rust label index for O(1) lookup instead of iterating all entities
        for entry in self._rust_registry.async_entries_for_label(label_id):
            new_labels = set(entry.labels) - {label_id}
            self.async_update_entity(entry.entity_id, labels=new_labels)
        # Clear from deleted entities
        self._rust_registry.clear_deleted_label_id(label_id)

    def async_device_ids(self) -> set[str]:
        """Return set of device IDs that have registered entities."""
        device_ids = set()
        for entity_id, entry in self._rust_registry.entities.items():
            if entry.device_id:
                device_ids.add(entry.device_id)
        return device_ids

    def async_generate_entity_id(
        self,
        domain: str,
        suggested_object_id: str,
        *,
        current_entity_id: str | None = None,
    ) -> str:
        """Generate an entity ID that does not conflict with registered entities."""
        from homeassistant.const import MAX_LENGTH_STATE_DOMAIN
        from homeassistant.exceptions import MaxLengthExceeded

        if len(domain) > MAX_LENGTH_STATE_DOMAIN:
            raise MaxLengthExceeded(domain, "domain", MAX_LENGTH_STATE_DOMAIN)

        # Get entity IDs from state machine to pass as reserved IDs
        reserved_ids = self._get_state_machine_entity_ids()
        return self._rust_registry.async_generate_entity_id(
            domain, suggested_object_id,
            current_entity_id=current_entity_id,
            reserved_ids=reserved_ids
        )

    def async_get(self, entity_id_or_id: str):
        """Get entity by entity_id or internal ID."""
        # Try by entity_id first
        entry = self._rust_registry.async_get(entity_id_or_id)
        if entry is not None:
            return self._get_or_create_wrapper(entry)
        # Try by internal ID (UUID) - search Rust registry
        for rust_entry in self._rust_registry.entities.values():
            if rust_entry.id == entity_id_or_id:
                return self._get_or_create_wrapper(rust_entry)
        return None

    def async_get_entity_id(
        self, domain: str, platform: str, unique_id: str
    ) -> str | None:
        return self._rust_registry.async_get_entity_id(domain, platform, unique_id)

    def _get_state_machine_entity_ids(self) -> list[str]:
        """Get entity IDs from state machine (including reservations) to use as reserved IDs."""
        if self._hass is not None and hasattr(self._hass, 'states'):
            ids = self._hass.states.async_entity_ids()
            # Also include reserved entity IDs
            if hasattr(self._hass.states, '_reservations'):
                ids = list(set(ids) | self._hass.states._reservations)
            return ids
        return []

    def async_get_or_create(
        self,
        domain: str,
        platform: str,
        unique_id: str,
        *,
        config_entry=UNDEFINED,
        config_entry_id=UNDEFINED,
        config_subentry_id=UNDEFINED,
        device_id=UNDEFINED,
        known_object_ids: list[str] | None = None,
        suggested_object_id: str | None = None,
        calculated_object_id: str | None = None,
        disabled_by: str | None = None,
        hidden_by: str | None = None,
        has_entity_name=UNDEFINED,
        capabilities=UNDEFINED,
        supported_features=UNDEFINED,
        device_class=UNDEFINED,
        unit_of_measurement=UNDEFINED,
        original_name=UNDEFINED,
        original_icon=UNDEFINED,
        original_device_class=UNDEFINED,
        entity_category=UNDEFINED,
        translation_key=UNDEFINED,
        get_initial_options: Callable | None = None,
    ):
        # Extract config_entry_id from config_entry object if provided
        if config_entry is not UNDEFINED:
            if config_entry is None:
                config_entry_id = None
            elif config_entry_id is UNDEFINED:
                config_entry_id = config_entry.entry_id

        # Validate config entry exists in hass
        if config_entry_id is not UNDEFINED and config_entry_id is not None and self._hass is not None:
            if self._hass.config_entries.async_get_entry(config_entry_id) is None:
                raise ValueError(
                    f"Config entry {config_entry_id} does not exist"
                )

        # Validate device_id exists in device registry
        if device_id is not UNDEFINED and device_id is not None and self._hass is not None:
            from homeassistant.helpers import device_registry as dr
            if dr.DATA_REGISTRY in self._hass.data:
                dev_reg = self._hass.data[dr.DATA_REGISTRY]
                if dev_reg.async_get(device_id) is None:
                    raise ValueError(
                        f"Device {device_id} does not exist"
                    )

        # Validate disabled_by is not a raw string (must be enum)
        if disabled_by is not None and isinstance(disabled_by, str) and not hasattr(disabled_by, 'name'):
            raise ValueError(
                f"disabled_by must be a RegistryEntryDisabler instance, got {disabled_by!r}"
            )

        # Validate entity_category is not a raw string (must be enum)
        if entity_category is not UNDEFINED and entity_category is not None and isinstance(entity_category, str) and not hasattr(entity_category, 'name'):
            raise ValueError(
                f"entity_category must be an EntityCategory instance, got {entity_category!r}"
            )

        # Validate hidden_by is not a raw string (must be enum)
        if hidden_by is not None and isinstance(hidden_by, str) and not hasattr(hidden_by, 'name'):
            raise ValueError(
                f"hidden_by must be a RegistryEntryHider instance, got {hidden_by!r}"
            )

        # Validate unique_id is hashable (lists, dicts etc. are not)
        try:
            hash(unique_id)
        except TypeError as err:
            raise TypeError(
                f"unique_id must be hashable, got {type(unique_id).__name__}"
            ) from err

        # Convert unique_id to string if not already (native HA does this with a warning)
        if not isinstance(unique_id, str):
            import logging
            _LOGGER = logging.getLogger("homeassistant.helpers.entity_registry")
            _LOGGER.error(
                "'%s' from integration %s has a non string unique_id '%s', "
                "please create a bug report",
                domain,
                platform,
                unique_id,
            )
            unique_id = str(unique_id)

        # Check if entity already exists (for event firing)
        existing_entity_id = self._rust_registry.async_get_entity_id(domain, platform, unique_id)
        is_restoring_deleted = self._rust_registry.is_deleted(domain, platform, unique_id)
        is_new = existing_entity_id is None and not is_restoring_deleted

        # Apply config entry preference for disabling new entities (only for new registrations)
        if (
            existing_entity_id is None
            and disabled_by is None
            and config_entry is not UNDEFINED
            and config_entry is not None
            and getattr(config_entry, 'pref_disable_new_entities', False)
        ):
            disabled_by = "integration"

        # Track old values for update event
        old_config_entry_id = None
        if existing_entity_id is not None:
            existing_entry = self._rust_registry.async_get(existing_entity_id)
            if existing_entry is not None:
                old_config_entry_id = existing_entry.config_entry_id

        # Pass current Python time as timestamp (respects freezer in tests)
        timestamp_iso = datetime.now(timezone.utc).isoformat()

        # Helper: convert UNDEFINED to None for Rust
        def to_rust(value):
            return None if value is UNDEFINED else value

        # Helper: convert UNDEFINED to None, Python None to "" (clear marker)
        def to_rust_string(value):
            if value is UNDEFINED:
                return None
            if value is None:
                return ""  # Empty string = clear in Rust
            return value

        # Call Rust - all business logic is handled there
        # Pass state machine entity IDs as reserved IDs for conflict resolution
        reserved_ids = self._get_state_machine_entity_ids()
        entry = self._rust_registry.async_get_or_create(
            domain=domain,
            platform=platform,
            unique_id=unique_id,
            config_entry_id=to_rust_string(config_entry_id),
            config_subentry_id=to_rust_string(config_subentry_id),
            device_id=to_rust_string(device_id),
            suggested_object_id=calculated_object_id or suggested_object_id,
            disabled_by=disabled_by,
            hidden_by=hidden_by,
            has_entity_name=to_rust(has_entity_name),
            capabilities=to_rust(capabilities),
            supported_features=to_rust(supported_features),
            device_class=to_rust_string(device_class),
            unit_of_measurement=to_rust_string(unit_of_measurement),
            original_name=to_rust_string(original_name),
            original_icon=to_rust_string(original_icon),
            original_device_class=to_rust_string(original_device_class),
            entity_category=to_rust(entity_category),
            translation_key=to_rust_string(translation_key),
            reserved_ids=reserved_ids,
            created_at=timestamp_iso if is_new else None,
            modified_at=timestamp_iso,
        )

        # Apply initial options for new entities
        if is_new and get_initial_options is not None:
            initial_options = get_initial_options()
            if initial_options:
                for domain_key, domain_opts in initial_options.items():
                    entry = self._rust_registry.async_update_entity_options(
                        entry.entity_id, domain_key, domain_opts
                    )

        # Check if we need to force a new RegistryEntry wrapper
        has_update_params = any(v is not UNDEFINED for v in [
            config_entry_id, config_subentry_id, device_id, has_entity_name,
            capabilities, supported_features, device_class, unit_of_measurement,
            original_name, original_icon, original_device_class, entity_category,
            translation_key
        ])
        force_new = is_new or has_update_params
        wrapped = self._get_or_create_wrapper(entry, force_new=force_new)

        # Fire create event if this was a new entity
        if is_new:
            self._fire_event("create", wrapped.entity_id)
        else:
            # Check if config_entry_id changed and fire update event
            if config_entry_id is not UNDEFINED:
                new_config_entry_id = wrapped.config_entry_id
                if old_config_entry_id != new_config_entry_id:
                    self._fire_event(
                        "update",
                        wrapped.entity_id,
                        changes={"config_entry_id": old_config_entry_id},
                    )

        return wrapped

    def async_is_registered(self, entity_id: str) -> bool:
        return self._rust_registry.async_get(entity_id) is not None

    def async_remove(self, entity_id: str) -> None:
        self._rust_registry.async_remove(entity_id)
        # Remove from cache
        self._entry_cache.pop(entity_id, None)
        # Remove from override if set (mock_registry scenario)
        if self._entities_override is not None and entity_id in self._entities_override:
            del self._entities_override[entity_id]
        # Fire remove event
        self._fire_event("remove", entity_id)

    def async_schedule_save(self) -> None:
        """Schedule saving to storage (stub for HA API compatibility)."""
        pass

    def async_update_entity(
        self,
        entity_id: str,
        **kwargs,
    ):
        # Validate config_entry_id exists if being updated
        if 'config_entry_id' in kwargs and kwargs['config_entry_id'] is not None and self._hass is not None:
            if self._hass.config_entries.async_get_entry(kwargs['config_entry_id']) is None:
                raise ValueError(
                    f"Config entry {kwargs['config_entry_id']} does not exist"
                )

        # Validate device_id exists if being updated
        if 'device_id' in kwargs and kwargs['device_id'] is not None and self._hass is not None:
            from homeassistant.helpers import device_registry as dr
            if dr.DATA_REGISTRY in self._hass.data:
                dev_reg = self._hass.data[dr.DATA_REGISTRY]
                if dev_reg.async_get(kwargs['device_id']) is None:
                    raise ValueError(
                        f"Device {kwargs['device_id']} does not exist"
                    )

        # Validate disabled_by is not a raw string (must be enum)
        if 'disabled_by' in kwargs and kwargs['disabled_by'] is not None and isinstance(kwargs['disabled_by'], str) and not hasattr(kwargs['disabled_by'], 'name'):
            raise ValueError(
                f"disabled_by must be a RegistryEntryDisabler instance, got {kwargs['disabled_by']!r}"
            )

        # Validate entity_category is not a raw string (must be enum)
        if 'entity_category' in kwargs and kwargs['entity_category'] is not None and isinstance(kwargs['entity_category'], str) and not hasattr(kwargs['entity_category'], 'name'):
            raise ValueError(
                f"entity_category must be an EntityCategory instance, got {kwargs['entity_category']!r}"
            )

        # Validate hidden_by is not a raw string (must be enum)
        if 'hidden_by' in kwargs and kwargs['hidden_by'] is not None and isinstance(kwargs['hidden_by'], str) and not hasattr(kwargs['hidden_by'], 'name'):
            raise ValueError(
                f"hidden_by must be a RegistryEntryHider instance, got {kwargs['hidden_by']!r}"
            )

        # Validate new_unique_id is hashable
        if 'new_unique_id' in kwargs and kwargs['new_unique_id'] is not None:
            try:
                hash(kwargs['new_unique_id'])
            except TypeError as err:
                raise TypeError(
                    f"unique_id must be hashable, got {type(kwargs['new_unique_id']).__name__}"
                ) from err
            # Convert non-string unique_id with warning
            if not isinstance(kwargs['new_unique_id'], str):
                import logging
                _LOGGER = logging.getLogger("homeassistant.helpers.entity_registry")
                old_entry_for_log = self._rust_registry.async_get(entity_id)
                domain = old_entry_for_log.domain if old_entry_for_log else "unknown"
                platform = old_entry_for_log.platform if old_entry_for_log else "unknown"
                _LOGGER.error(
                    "'%s' from integration %s has a non string unique_id '%s', "
                    "please create a bug report",
                    domain,
                    platform,
                    kwargs['new_unique_id'],
                )
                kwargs['new_unique_id'] = str(kwargs['new_unique_id'])

        # Get old entry to track changes
        old_entry = self._rust_registry.async_get(entity_id)
        old_entity_id = old_entry.entity_id if old_entry else None

        # Compute config_entry_is_disabled for Rust disabled_by propagation
        config_entry_is_disabled = None
        if 'config_entry_id' in kwargs and 'disabled_by' not in kwargs:
            new_ce_id = kwargs['config_entry_id']
            if new_ce_id is not None and self._hass is not None:
                new_ce = self._hass.config_entries.async_get_entry(new_ce_id)
                if new_ce is not None:
                    config_entry_is_disabled = bool(new_ce.disabled_by)

        # Transform kwargs for Rust: None means "clear" for optional string fields
        rust_kwargs = {}
        clear_string_fields = {
            'disabled_by', 'hidden_by', 'area_id', 'device_class',
            'unit_of_measurement', 'config_entry_id', 'config_subentry_id',
            'device_id', 'entity_category', 'original_device_class',
            'original_icon', 'original_name', 'translation_key', 'name', 'icon',
        }
        enum_string_fields = {'entity_category', 'disabled_by', 'hidden_by'}
        for key, value in kwargs.items():
            if key in clear_string_fields and value is None:
                rust_kwargs[key] = ""  # Empty string = clear in Rust
            elif key in enum_string_fields and value is not None:
                rust_kwargs[key] = str(value.value) if hasattr(value, 'value') else str(value)
            else:
                rust_kwargs[key] = value

        if config_entry_is_disabled is not None:
            rust_kwargs['config_entry_is_disabled'] = config_entry_is_disabled
        entry = self._rust_registry.async_update_entity(entity_id, **rust_kwargs)
        # Force new RegistryEntry since data was updated (RegistryEntry is frozen)
        wrapped = self._get_or_create_wrapper(entry, force_new=True)

        # Update cache if entity_id changed
        if old_entity_id and old_entity_id != wrapped.entity_id:
            self._entry_cache.pop(old_entity_id, None)

        # Fire update event with changes
        changes = {k: v for k, v in kwargs.items() if v is not None}
        if changes:
            self._fire_event(
                "update",
                wrapped.entity_id,
                changes=changes,
                old_entity_id=old_entity_id if old_entity_id != wrapped.entity_id else None,
            )

        self.async_schedule_save()
        return wrapped

    def async_update_entity_options(
        self,
        entity_id: str,
        domain: str,
        options: dict | None,
    ):
        """Update entity options for a specific domain."""
        entry = self._rust_registry.async_update_entity_options(entity_id, domain, options)
        # Force new RegistryEntry since data was updated
        return self._get_or_create_wrapper(entry, force_new=True)

    @property
    def deleted_entities(self):
        """Return dict of (domain, platform, unique_id) to deleted DeletedRegistryEntry."""
        return {
            key: _rust_entry_to_deleted_registry_entry(entry)
            for key, entry in self._rust_registry.deleted_entities.items()
        }

    @deleted_entities.setter
    def deleted_entities(self, value):
        """Allow setting deleted_entities (used in test setup)."""
        # No-op for Rust backend - tests that set this to {} are clearing it
        pass

    @property
    def entities(self):
        """Return dict of entity_id to RegistryEntry."""
        if self._entities_override is not None:
            return self._entities_override
        data = {
            entity_id: self._get_or_create_wrapper(entry)
            for entity_id, entry in self._rust_registry.entities.items()
        }
        return RustEntityRegistryItems(
            data,
            rust_registry=self._rust_registry,
            wrapper_fn=self._get_or_create_wrapper,
        )

    @entities.setter
    def entities(self, value):
        """Allow mock_registry to replace entities with a native EntityRegistryItems."""
        self._entities_override = value

    def __iter__(self):
        return iter(self.entities.values())

    def __len__(self) -> int:
        return len(self._rust_registry)


# =============================================================================
# Rust-backed DeviceRegistry wrappers
# =============================================================================

# Sentinel to distinguish "not passed" from "passed as None"
_GOC_UNSET = object()

class RustDeviceEntry:
    """Wrapper for DeviceEntry compatible with homeassistant.helpers.device_registry."""

    __slots__ = ("_rust_entry", "_field_overrides")

    _DISABLED_BY_MAP = None  # Lazy-loaded

    @classmethod
    def _get_disabled_by_map(cls):
        if cls._DISABLED_BY_MAP is None:
            from homeassistant.helpers.device_registry import DeviceEntryDisabler
            cls._DISABLED_BY_MAP = {e.value: e for e in DeviceEntryDisabler}
        return cls._DISABLED_BY_MAP

    def __init__(self, rust_entry, field_overrides=None):
        self._rust_entry = rust_entry
        self._field_overrides = field_overrides or {}

    @property
    def area_id(self) -> str | None:
        return self._rust_entry.area_id

    @property
    def config_entries(self) -> set[str]:
        return set(self._rust_entry.config_entries)

    @property
    def config_entries_subentries(self) -> dict[str, set[str | None]]:
        raw = self._rust_entry.config_entries_subentries
        return {k: set(v) for k, v in raw.items()}

    @property
    def configuration_url(self) -> str | None:
        return self._rust_entry.configuration_url

    @property
    def connections(self) -> set[tuple[str, str]]:
        return set(self._rust_entry.connections)

    @property
    def created_at(self) -> datetime:
        from datetime import timezone
        return datetime.fromtimestamp(self._rust_entry.created_at_timestamp, tz=timezone.utc)

    @property
    def dict_repr(self) -> dict:
        """Return a dict representation of the entry."""
        return {
            "area_id": self.area_id,
            "configuration_url": self.configuration_url,
            "config_entries": list(self.config_entries),
            "config_entries_subentries": {
                config_entry_id: list(subentries)
                for config_entry_id, subentries in self.config_entries_subentries.items()
            },
            "connections": [list(c) for c in self.connections],
            "created_at": self._rust_entry.created_at_timestamp,
            "disabled_by": self.disabled_by,
            "entry_type": self.entry_type,
            "hw_version": self.hw_version,
            "id": self.id,
            "identifiers": [list(i) for i in self.identifiers],
            "labels": list(self.labels),
            "manufacturer": self.manufacturer,
            "model": self.model,
            "model_id": self.model_id,
            "modified_at": self._rust_entry.modified_at_timestamp,
            "name_by_user": self.name_by_user,
            "name": self._field_overrides.get('name', self.name),
            "primary_config_entry": self.primary_config_entry,
            "serial_number": self.serial_number,
            "sw_version": self.sw_version,
            "via_device_id": self.via_device_id,
        }

    @property
    def disabled(self) -> bool:
        return self.disabled_by is not None

    @property
    def disabled_by(self):
        val = self._rust_entry.disabled_by
        if val:
            return self._get_disabled_by_map().get(val, val)
        return None

    @property
    def entry_type(self):
        val = self._rust_entry.entry_type
        if val:
            from homeassistant.helpers.device_registry import DeviceEntryType
            return DeviceEntryType(val)
        return None

    @property
    def hw_version(self) -> str | None:
        return self._rust_entry.hw_version

    @property
    def id(self) -> str:
        return self._rust_entry.id

    @property
    def identifiers(self) -> set[tuple[str, str]]:
        return set(self._rust_entry.identifiers)

    @property
    def json_repr(self) -> bytes | None:
        """Return a cached JSON representation of the entry."""
        import orjson
        try:
            return orjson.dumps(self.dict_repr)
        except (ValueError, TypeError):
            return None

    @property
    def labels(self) -> set[str]:
        return set(self._rust_entry.labels)

    @property
    def manufacturer(self) -> str | None:
        return self._rust_entry.manufacturer

    @property
    def model(self) -> str | None:
        return self._rust_entry.model

    @property
    def model_id(self) -> str | None:
        return self._rust_entry.model_id

    @property
    def modified_at(self) -> datetime:
        from datetime import timezone
        return datetime.fromtimestamp(self._rust_entry.modified_at_timestamp, tz=timezone.utc)

    @property
    def name(self) -> str | None:
        return self._rust_entry.name

    @property
    def name_by_user(self) -> str | None:
        return self._rust_entry.name_by_user

    @property
    def orphaned_timestamp(self) -> float | None:
        return self._rust_entry.orphaned_timestamp

    @property
    def primary_config_entry(self) -> str | None:
        return self._rust_entry.primary_config_entry

    @property
    def serial_number(self) -> str | None:
        return self._rust_entry.serial_number

    @property
    def suggested_area(self) -> str | None:
        return self._rust_entry.suggested_area

    @property
    def sw_version(self) -> str | None:
        return self._rust_entry.sw_version

    @property
    def via_device_id(self) -> str | None:
        return self._rust_entry.via_device_id

    def __eq__(self, other: object) -> bool:
        if isinstance(other, RustDeviceEntry):
            return (
                self.dict_repr == other.dict_repr
                and self.suggested_area == other.suggested_area
            )
        from homeassistant.helpers import device_registry as dr
        if isinstance(other, dr.DeviceEntry):
            # Compare timestamps with second precision to avoid float issues
            created_match = (
                abs(self.created_at.timestamp() - other.created_at.timestamp()) < 1.0
                if self.created_at and other.created_at else self.created_at == other.created_at
            )
            modified_match = (
                abs(self.modified_at.timestamp() - other.modified_at.timestamp()) < 1.0
                if self.modified_at and other.modified_at else self.modified_at == other.modified_at
            )
            return (
                self.id == other.id
                and self.area_id == other.area_id
                and self.config_entries == other.config_entries
                and self.config_entries_subentries == other.config_entries_subentries
                and self.configuration_url == other.configuration_url
                and self.connections == other.connections
                and created_match
                and self.disabled_by == other.disabled_by
                and self.entry_type == other.entry_type
                and self.hw_version == other.hw_version
                and self.identifiers == other.identifiers
                and self.labels == other.labels
                and self.manufacturer == other.manufacturer
                and self.model == other.model
                and self.model_id == other.model_id
                and modified_match
                and self.name == other.name
                and self.name_by_user == other.name_by_user
                and self.primary_config_entry == other.primary_config_entry
                and self.serial_number == other.serial_number
                and self.suggested_area == other.suggested_area
                and self.sw_version == other.sw_version
                and self.via_device_id == other.via_device_id
            )
        return False

    def __hash__(self) -> int:
        return hash(self.id)

    def __repr__(self) -> str:
        return repr(self._rust_entry)


class RustDeviceRegistryItems(dict):
    """Dict subclass that provides extra lookup methods like HA's DeviceRegistryItems."""

    def __init__(self, registry, data):
        super().__init__(data)
        self._registry = registry

    def get_devices_for_area_id(self, area_id: str) -> list:
        """Get devices for area."""
        return self._registry.async_entries_for_area(area_id)

    def get_devices_for_config_entry_id(self, config_entry_id: str) -> list:
        """Get devices for config entry."""
        return self._registry.async_entries_for_config_entry(config_entry_id)

    def get_devices_for_label(self, label: str) -> list:
        """Get devices that have the specified label."""
        result = [
            entry for entry in self.values()
            if label in entry.labels
        ]
        result.sort(key=lambda e: e.created_at)
        return result


class DeletedDeviceRegistryItems(dict):
    """Dict subclass for deleted devices with get_entry() lookup."""

    def __init__(self, rust_registry, entry_cache):
        self._rust_registry = rust_registry
        # Populate dict from Rust deleted devices - reuse cached wrappers when data unchanged
        raw = rust_registry.deleted_devices
        entries = {}
        for k, v in raw.items():
            # Build a fingerprint from the mutable fields to detect changes
            fingerprint = (
                tuple(sorted(v.config_entries)),
                tuple(sorted(
                    (ce_id, tuple(sorted(str(s) for s in subs)))
                    for ce_id, subs in v.config_entries_subentries.items()
                )),
                v.orphaned_timestamp,
            )
            cached = entry_cache.get(k)
            if cached is not None and cached[0] == fingerprint:
                entries[k] = cached[1]
            else:
                wrapper = RustDeviceEntry(v)
                entry_cache[k] = (fingerprint, wrapper)
                entries[k] = wrapper
        super().__init__(entries)

    def get_entry(self, identifiers=None, connections=None):
        """Find a deleted device by identifiers or connections."""
        ident_list = [tuple(i) for i in identifiers] if identifiers else []
        conn_list = [tuple(c) for c in connections] if connections else []
        result = self._rust_registry.async_get_deleted_by_identifiers_or_connections(
            ident_list, conn_list
        )
        if result is None:
            return None
        # Return the cached wrapper from the dict if available (for identity stability)
        device_id = result.id
        if device_id in self:
            return self[device_id]
        return RustDeviceEntry(result)


class RustDeviceRegistry:
    """Wrapper that provides HA-compatible DeviceRegistry API backed by Rust."""

    def __init__(self, hass):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.DeviceRegistry(hass)
        self.hass = hass
        # Store non-serializable field overrides (e.g., Unserializable name objects)
        self._field_overrides: dict[str, dict[str, object]] = {}
        # Cache for returning same entry object on no-op updates
        self._device_entries: dict[str, "RustDeviceEntry"] = {}
        # Cache for deleted device entries (identity stability for `is` checks)
        self._deleted_entry_cache: dict[str, tuple] = {}

    async def async_load(self) -> None:
        # No-op for testing - Rust registries start empty in test context
        pass

    async def async_save(self) -> None:
        # No-op for testing - persistence not needed for unit tests
        pass

    def async_clear_area_id(self, area_id: str) -> None:
        """Clear an area id from all devices (active and deleted)."""
        modified_ids = self._rust_registry.async_clear_area_id(area_id)
        for dev_id in modified_ids:
            self._fire_event("update", dev_id, changes={"area_id": area_id})
        # Also clear from deleted devices
        self._rust_registry.async_clear_area_id_from_deleted(area_id)

    def async_clear_config_entry(self, config_entry_id: str) -> None:
        """Clear a config entry from all devices (active and deleted)."""
        # Capture old snapshots and dict_reprs before clearing (needed for events)
        affected = {}
        for dev in self._rust_registry.async_entries_for_config_entry(config_entry_id):
            affected[dev.id] = (
                self._get_device_snapshot(dev),
                RustDeviceEntry(dev, self._field_overrides.get(dev.id, {})).dict_repr,
            )

        # Clear in Rust - returns (removed_ids, [(dev_id, changed_fields)])
        removed_ids, updated = self._rust_registry.async_clear_config_entry_with_changes(
            config_entry_id
        )

        # Fire update events first (with old values from snapshots)
        for dev_id, changed_fields in updated:
            old_snapshot = affected.get(dev_id, ({},))[0]
            changes = {
                field: old_snapshot.get(field)
                for field in changed_fields
                if field not in self.RUNTIME_ONLY_ATTRS
            }
            if changes:
                self._fire_event("update", dev_id, changes=changes)

        # Fire remove events
        for dev_id in removed_ids:
            old_dict_repr = affected.get(dev_id, (None, None))[1]
            self._fire_event("remove", dev_id, device=old_dict_repr)

        # Also clear from deleted devices (sets orphaned_timestamp when empty)
        import time
        self._rust_registry.async_clear_config_entry_from_deleted(
            config_entry_id, time.time()
        )

    def async_clear_config_subentry(
        self, config_entry_id: str, config_subentry_id: str | None
    ) -> None:
        """Clear config subentry from device registry entries."""
        # For active devices with this config entry, remove the subentry
        for entry in self._rust_registry.async_entries_for_config_entry(config_entry_id):
            self.async_update_device(
                entry.id,
                remove_config_entry_id=config_entry_id,
                remove_config_subentry_id=config_subentry_id,
            )
        # For deleted devices, clear the subentry directly
        import time
        self._rust_registry.async_clear_config_subentry_from_deleted(
            config_entry_id, config_subentry_id, time.time()
        )

    def async_clear_label_id(self, label_id: str) -> None:
        """Clear a label from all devices (active and deleted)."""
        modified_ids = self._rust_registry.async_clear_label_id(label_id)
        for dev_id in modified_ids:
            self._fire_event("update", dev_id, changes={"labels": label_id})
        # Also clear from deleted devices
        self._rust_registry.async_clear_label_id_from_deleted(label_id)

    def async_entries_for_area(self, area_id: str) -> list[RustDeviceEntry]:
        return [
            RustDeviceEntry(entry)
            for entry in self._rust_registry.async_entries_for_area(area_id)
        ]

    def async_entries_for_config_entry(
        self, config_entry_id: str
    ) -> list[RustDeviceEntry]:
        return [
            RustDeviceEntry(entry)
            for entry in self._rust_registry.async_entries_for_config_entry(
                config_entry_id
            )
        ]

    def _fire_event(self, action: str, device_id: str, changes: dict | None = None, device: dict | None = None) -> None:
        """Fire device registry updated event."""
        from homeassistant.helpers import device_registry as dr
        data: dict = {"action": action, "device_id": device_id}
        if changes is not None:
            data["changes"] = changes
        if device is not None:
            data["device"] = device
        self.hass.bus.async_fire(dr.EVENT_DEVICE_REGISTRY_UPDATED, data)

    # Runtime-only attributes - don't trigger events/persistence when changed alone
    RUNTIME_ONLY_ATTRS = {"suggested_area"}

    def _get_device_snapshot(self, raw_entry) -> dict:
        """Get a snapshot of device entry fields for change detection."""
        from homeassistant.helpers import device_registry as dr
        disabled_by = None
        if raw_entry.disabled_by:
            disabled_by = dr.DeviceEntryDisabler(raw_entry.disabled_by)
        entry_type = None
        if raw_entry.entry_type:
            entry_type = dr.DeviceEntryType(raw_entry.entry_type)
        # Get config_entries_subentries as dict of sets
        config_entries_subentries = {}
        try:
            raw_subentries = raw_entry.config_entries_subentries
            if isinstance(raw_subentries, dict):
                config_entries_subentries = {
                    k: set(v) for k, v in raw_subentries.items()
                }
        except Exception:
            pass
        return {
            "area_id": raw_entry.area_id,
            "config_entries": set(raw_entry.config_entries) if raw_entry.config_entries else set(),
            "config_entries_subentries": config_entries_subentries,
            "configuration_url": raw_entry.configuration_url,
            "connections": set((c[0], c[1]) for c in raw_entry.connections) if raw_entry.connections else set(),
            "disabled_by": disabled_by,
            "entry_type": entry_type,
            "hw_version": raw_entry.hw_version,
            "identifiers": set((i[0], i[1]) for i in raw_entry.identifiers) if raw_entry.identifiers else set(),
            "labels": set(raw_entry.labels) if raw_entry.labels else set(),
            "manufacturer": raw_entry.manufacturer,
            "model": raw_entry.model,
            "model_id": raw_entry.model_id,
            "name": raw_entry.name,
            "name_by_user": raw_entry.name_by_user,
            "primary_config_entry": raw_entry.primary_config_entry,
            "serial_number": raw_entry.serial_number,
            "suggested_area": raw_entry.suggested_area,
            "sw_version": raw_entry.sw_version,
            "via_device_id": raw_entry.via_device_id,
        }

    def _compute_changes(self, old_snapshot: dict, new_snapshot: dict) -> dict:
        """Compute changes between old and new device snapshots."""
        changes = {}
        for key, old_val in old_snapshot.items():
            new_val = new_snapshot.get(key)
            if old_val != new_val:
                changes[key] = old_val
        return changes

    def async_get(self, device_id: str) -> RustDeviceEntry | None:
        entry = self._rust_registry.async_get(device_id)
        return RustDeviceEntry(entry) if entry else None

    def async_get_device(
        self,
        identifiers: set[tuple[str, str]] | None = None,
        connections: set[tuple[str, str]] | None = None,
    ) -> RustDeviceEntry | None:
        entry = self._rust_registry.async_get_device(
            identifiers=identifiers if identifiers else None,
            connections=connections if connections else None,
        )
        return RustDeviceEntry(entry) if entry else None

    def async_get_or_create(
        self,
        *,
        config_entry_id: str,
        config_subentry_id: str | None = None,
        identifiers: set[tuple[str, str]] | None = None,
        connections: set[tuple[str, str]] | None = None,
        manufacturer: str | None = None,
        model: str | None = None,
        model_id: str | None = None,
        name: str | None = None,
        serial_number: str | None = None,
        suggested_area: str | None = None,
        sw_version: str | None = None,
        hw_version: str | None = None,
        via_device: tuple[str, str] | None = None,
        configuration_url=_GOC_UNSET,
        entry_type=_GOC_UNSET,
        disabled_by=None,
        default_manufacturer: str | None = None,
        default_model: str | None = None,
        default_name: str | None = None,
    ) -> RustDeviceEntry:
        # Require at least one identifier or connection
        if not identifiers and not connections:
            from homeassistant.exceptions import HomeAssistantError
            raise HomeAssistantError(
                "A device must have at least one identifier or connection"
            )

        # Look up existing device before get_or_create to detect create vs update
        existing = self._rust_registry.async_get_device(
            identifiers=identifiers if identifiers else None,
            connections=connections if connections else None,
        )

        # If no existing device, check if there's a matching deleted device to restore
        was_restored = False
        if existing is None:
            ident_list = [tuple(i) for i in identifiers] if identifiers else []
            conn_list = [tuple(c) for c in connections] if connections else []
            deleted = self._rust_registry.async_get_deleted_by_identifiers_or_connections(
                ident_list, conn_list
            )
            if deleted is not None:
                # Restore as a fresh entry preserving only user customizations
                now_ts = datetime.now(timezone.utc).timestamp()
                self._rust_registry.async_restore_deleted_fresh(
                    deleted.id,
                    ident_list,
                    conn_list,
                    config_entry_id,
                    config_subentry_id,
                    now_ts,
                )
                was_restored = True
                # Keep existing=None so we fire "create" event below

        old_snapshot = self._get_device_snapshot(existing) if existing else None

        # Look up the domain of the current primary config entry (for Rust promotion decision)
        current_primary_domain = None
        if existing and existing.primary_config_entry:
            primary_entry = self.hass.config_entries.async_get_entry(existing.primary_config_entry)
            if primary_entry is not None:
                current_primary_domain = primary_entry.domain

        # Compute initial_disabled_by (only for new devices)
        initial_disabled_by = None
        if disabled_by is not None and existing is None:
            initial_disabled_by = str(disabled_by.value) if hasattr(disabled_by, 'value') else str(disabled_by)

        # Convert entry_type: _GOC_UNSET=don't set, None=clear, enum=set
        rust_entry_type = None
        if entry_type is not _GOC_UNSET:
            if entry_type is None:
                rust_entry_type = ""  # Empty string = clear
            elif hasattr(entry_type, 'value'):
                rust_entry_type = str(entry_type.value)
            else:
                rust_entry_type = str(entry_type)

        # Convert configuration_url: _GOC_UNSET=don't set, None=clear, str=set
        rust_config_url = None
        if configuration_url is not _GOC_UNSET:
            if configuration_url is None:
                rust_config_url = ""  # Empty string = clear
            else:
                rust_config_url = str(configuration_url)

        now_ts = datetime.now(timezone.utc).timestamp()
        entry, changed_fields = self._rust_registry.async_get_or_create(
            config_entry_id=config_entry_id,
            config_subentry_id=config_subentry_id,
            identifiers=list(identifiers) if identifiers else None,
            connections=list(connections) if connections else None,
            manufacturer=manufacturer,
            model=model,
            model_id=model_id,
            name=name,
            serial_number=serial_number,
            suggested_area=suggested_area,
            sw_version=sw_version,
            hw_version=hw_version,
            via_device=via_device,
            configuration_url=rust_config_url,
            entry_type=rust_entry_type,
            default_manufacturer=default_manufacturer,
            default_model=default_model,
            default_name=default_name,
            created_at=now_ts,
            current_primary_domain=current_primary_domain,
            initial_disabled_by=initial_disabled_by,
        )

        # Log warning if via_device references a non-existing device
        if via_device and not entry.via_device_id:
            import logging
            _LOGGER = logging.getLogger("homeassistant.helpers.device_registry")
            _LOGGER.error(
                'calls `device_registry.async_get_or_create` '
                'referencing a non existing `via_device` '
                '("%s","%s")',
                via_device[0],
                via_device[1],
            )

        # Handle suggested_area: create area and set area_id on device
        # (cross-registry coordination stays in Python)
        # Skip for restored devices - they preserve their existing area_id
        if suggested_area and not entry.area_id and not was_restored:
            from homeassistant.helpers import area_registry as ar
            if ar.DATA_REGISTRY in self.hass.data:
                area_reg = self.hass.data[ar.DATA_REGISTRY]
                area = area_reg.async_get_area_by_name(suggested_area)
                if area is None:
                    area = area_reg.async_create(suggested_area)
                entry, area_changes = self._rust_registry.async_update_device(
                    entry.id, area_id=area.id, modified_at=now_ts
                )
                changed_fields = list(set(changed_fields) | set(area_changes))

        # Fire device registry events
        if existing is None:
            self._fire_event("create", entry.id)
        elif changed_fields:
            changes = {field: old_snapshot[field] for field in changed_fields if field in old_snapshot}
            if changes:
                self._fire_event("update", entry.id, changes=changes)

        return RustDeviceEntry(entry)

    def async_purge_expired_orphaned_devices(self) -> None:
        """Purge expired orphaned deleted devices."""
        import time
        from homeassistant.helpers.device_registry import ORPHANED_DEVICE_KEEP_SECONDS
        self._rust_registry.async_purge_expired_orphaned_devices(
            time.time(), float(ORPHANED_DEVICE_KEEP_SECONDS)
        )

    def async_remove_device(self, device_id: str) -> None:
        from homeassistant.helpers import entity_registry as er
        # Clear cache
        self._device_entries.pop(device_id, None)
        # Get device info before removal (for entity cleanup and event)
        device_entry = self._rust_registry.async_get(device_id)
        device_config_entries = set(device_entry.config_entries) if device_entry else set()

        # Build dict_repr before removal for the event
        device_dict_repr = None
        if device_entry:
            device_dict_repr = RustDeviceEntry(device_entry, self._field_overrides.get(device_id, {})).dict_repr

        # Rust handles removal AND via_device_id cleanup on other devices
        self._rust_registry.async_remove_device(device_id)

        # Fire remove event
        if device_dict_repr is not None:
            self._fire_event("remove", device_id, device=device_dict_repr)

        # Clean up entities associated with this device (cross-registry coordination)
        if er.DATA_REGISTRY in self.hass.data:
            entity_reg = self.hass.data[er.DATA_REGISTRY]
            entities = entity_reg.entities.get_entries_for_device_id(
                device_id, include_disabled_entities=True
            )
            for entity in entities:
                if entity.config_entry_id in device_config_entries:
                    entity_reg.async_remove(entity.entity_id)
                else:
                    entity_reg.async_update_entity(entity.entity_id, device_id=None)

    def async_schedule_save(self) -> None:
        """Schedule a save - no-op for testing."""
        pass

    def async_update_device(
        self,
        device_id: str,
        **kwargs,
    ) -> RustDeviceEntry:
        from homeassistant.helpers import entity_registry as er
        from homeassistant.exceptions import HomeAssistantError

        # Extract config entry add/remove params
        # Use _UNSET sentinel to distinguish "not passed" from "passed as None"
        _UNSET = object()
        add_config_entry_id = kwargs.pop('add_config_entry_id', None)
        add_config_subentry_id = kwargs.pop('add_config_subentry_id', _UNSET)
        remove_config_entry_id = kwargs.pop('remove_config_entry_id', None)
        remove_config_subentry_id = kwargs.pop('remove_config_subentry_id', _UNSET)

        # Validate: can't add/remove subentry without specifying config entry
        if add_config_subentry_id is not _UNSET and add_config_entry_id is None:
            raise HomeAssistantError(
                "Can't add config subentry without specifying config entry"
            )
        if remove_config_subentry_id is not _UNSET and remove_config_entry_id is None:
            raise HomeAssistantError(
                "Can't remove config subentry without specifying config entry"
            )

        # Validate add_config_entry_id
        if add_config_entry_id is not None:
            if self.hass.config_entries.async_get_entry(add_config_entry_id) is None:
                raise HomeAssistantError(
                    f"Can't link device to unknown config entry {add_config_entry_id}"
                )

        # Validate add_config_subentry_id references a real subentry
        if add_config_subentry_id is not _UNSET and add_config_subentry_id is not None:
            config_entry = self.hass.config_entries.async_get_entry(add_config_entry_id)
            if config_entry is not None:
                valid_subentry_ids = set()
                if hasattr(config_entry, 'subentries') and config_entry.subentries:
                    for se_id in config_entry.subentries:
                        valid_subentry_ids.add(se_id)
                if add_config_subentry_id not in valid_subentry_ids:
                    raise HomeAssistantError(
                        f"Config entry {add_config_entry_id} has no subentry "
                        f"{add_config_subentry_id}"
                    )

        # Extract merge/new connections/identifiers (passed directly to Rust)
        new_connections = kwargs.pop('new_connections', None)
        merge_connections = kwargs.pop('merge_connections', None)
        new_identifiers = kwargs.pop('new_identifiers', None)
        merge_identifiers = kwargs.pop('merge_identifiers', None)

        # Get old device state for change detection
        old_device = self._rust_registry.async_get(device_id)
        old_snapshot = self._get_device_snapshot(old_device) if old_device else None
        old_disabled_by = old_device.disabled_by if old_device else None

        # Build Rust params for config entry add/remove
        rust_add_ce_id = None
        rust_add_sub_id = None  # None means "add the None subentry"
        rust_add_ce_disabled = None
        rust_remove_ce_id = None
        rust_remove_sub_only = None
        rust_remove_sub_id = None
        rust_ce_disabled_map = None

        if add_config_entry_id is not None:
            rust_add_ce_id = add_config_entry_id
            rust_add_sub_id = add_config_subentry_id if add_config_subentry_id is not _UNSET else None
            add_ce = self.hass.config_entries.async_get_entry(add_config_entry_id)
            rust_add_ce_disabled = bool(add_ce.disabled_by) if add_ce else False

        if remove_config_entry_id is not None:
            rust_remove_ce_id = remove_config_entry_id
            if remove_config_subentry_id is not _UNSET:
                rust_remove_sub_only = True
                rust_remove_sub_id = remove_config_subentry_id
            else:
                rust_remove_sub_only = False
            if old_device:
                rust_ce_disabled_map = {}
                for ce_id in old_device.config_entries:
                    ce = self.hass.config_entries.async_get_entry(ce_id)
                    rust_ce_disabled_map[ce_id] = bool(ce.disabled_by) if ce else True

        # Enum conversions for Rust
        if 'disabled_by' in kwargs:
            db = kwargs['disabled_by']
            if db is not None and hasattr(db, 'value'):
                kwargs['disabled_by'] = str(db.value)
            elif db is None:
                kwargs['disabled_by'] = ""  # Empty string = clear

        if 'entry_type' in kwargs:
            et = kwargs['entry_type']
            if et is not None and hasattr(et, 'value'):
                kwargs['entry_type'] = str(et.value)
            elif et is None:
                kwargs['entry_type'] = ""  # Empty string = clear

        # Clearable fields: None means "clear", convert to "" for Rust
        for field in ('area_id', 'via_device_id', 'name_by_user',
                      'configuration_url', 'suggested_area'):
            if field in kwargs and kwargs[field] is None:
                kwargs[field] = ""

        # Convert labels set to list for Rust
        if 'labels' in kwargs and isinstance(kwargs['labels'], set):
            kwargs['labels'] = list(kwargs['labels'])

        # Handle non-string field values that Rust can't accept
        if 'name' in kwargs and kwargs['name'] is not None and not isinstance(kwargs['name'], str):
            if device_id not in self._field_overrides:
                self._field_overrides[device_id] = {}
            self._field_overrides[device_id]['name'] = kwargs.pop('name')

        # Build the Rust call kwargs
        rust_kwargs = dict(kwargs)
        rust_kwargs['modified_at'] = datetime.now(timezone.utc).timestamp()

        # Pass merge/new connections/identifiers directly to Rust
        if merge_connections is not None:
            rust_kwargs['merge_connections'] = merge_connections
        if new_connections is not None:
            rust_kwargs['new_connections'] = new_connections
        if merge_identifiers is not None:
            rust_kwargs['merge_identifiers'] = merge_identifiers
        if new_identifiers is not None:
            rust_kwargs['new_identifiers'] = new_identifiers

        if rust_add_ce_id is not None:
            rust_kwargs['add_config_entry_id'] = rust_add_ce_id
            if rust_add_sub_id is not None:
                rust_kwargs['add_config_subentry_id'] = rust_add_sub_id
            rust_kwargs['add_config_entry_disabled'] = rust_add_ce_disabled

        if rust_remove_ce_id is not None:
            rust_kwargs['remove_config_entry_id'] = rust_remove_ce_id
            rust_kwargs['remove_config_subentry_only'] = rust_remove_sub_only
            if rust_remove_sub_id is not None:
                rust_kwargs['remove_config_subentry_id'] = rust_remove_sub_id
            if rust_ce_disabled_map is not None:
                rust_kwargs['config_entry_disabled_map'] = rust_ce_disabled_map

        if rust_kwargs:
            try:
                entry, changed_fields = self._rust_registry.async_update_device(device_id, **rust_kwargs)
            except ValueError as e:
                # Rust raises ValueError for validation/collision errors;
                # convert to HomeAssistantError for HA test compatibility
                raise HomeAssistantError(str(e)) from e
        else:
            entry = self._rust_registry.async_get(device_id)
            changed_fields = []

        # Handle device removal (Rust returns None when last config entry removed)
        # Note: Rust already handles via_device_id cleanup on other devices
        if entry is None:
            self._device_entries.pop(device_id, None)
            device_config_entries = set(old_device.config_entries) if old_device else set()

            # Fire remove event
            if old_device:
                device_dict_repr = RustDeviceEntry(old_device, self._field_overrides.get(device_id, {})).dict_repr
                self._fire_event("remove", device_id, device=device_dict_repr)
            else:
                self._fire_event("remove", device_id)

            # Clean up entities associated with this device (cross-registry)
            if er.DATA_REGISTRY in self.hass.data:
                entity_reg = self.hass.data[er.DATA_REGISTRY]
                entities = entity_reg.entities.get_entries_for_device_id(
                    device_id, include_disabled_entities=True
                )
                for entity in entities:
                    if entity.config_entry_id in device_config_entries:
                        entity_reg.async_remove(entity.entity_id)
                    else:
                        entity_reg.async_update_entity(entity.entity_id, device_id=None)
            return None

        # Clean up entities when a config entry is removed from the device (device still exists)
        if remove_config_entry_id and er.DATA_REGISTRY in self.hass.data:
            # Check if the config entry was actually removed from the device
            if remove_config_entry_id not in entry.config_entries:
                entity_reg = self.hass.data[er.DATA_REGISTRY]
                entities = entity_reg.entities.get_entries_for_device_id(
                    device_id, include_disabled_entities=True
                )
                for entity in entities:
                    if entity.config_entry_id == remove_config_entry_id:
                        entity_reg.async_remove(entity.entity_id)

        # Fire event if there are changes
        new_device = RustDeviceEntry(entry, self._field_overrides.get(device_id, {}))
        if not changed_fields:
            # No-op: return cached entry for identity semantics
            if device_id in self._device_entries:
                return self._device_entries[device_id]
            self._device_entries[device_id] = new_device
            return new_device
        # Build changes dict from old_snapshot and changed field names
        changes = {field: old_snapshot[field] for field in changed_fields if field in old_snapshot} if old_snapshot else {}
        if changes:
            # Only fire event and save if there are non-runtime-only changes
            if changes.keys() - self.RUNTIME_ONLY_ATTRS:
                self._fire_event("update", device_id, changes=changes)
                self.async_schedule_save()

        # Handle device disabled_by changes → update entities
        if er.DATA_REGISTRY in self.hass.data and old_disabled_by != new_device.disabled_by:
            entity_reg = self.hass.data[er.DATA_REGISTRY]
            if not new_device.disabled_by:
                # Device re-enabled - re-enable entities that were disabled by DEVICE
                entities = entity_reg.entities.get_entries_for_device_id(
                    device_id, include_disabled_entities=True
                )
                for entity in entities:
                    if entity.disabled_by and entity.disabled_by.value == "device":
                        entity_reg.async_update_entity(entity.entity_id, disabled_by=None)
            elif str(new_device.disabled_by) != "config_entry":
                # Device disabled (not by config entry) - disable entities
                entities = entity_reg.entities.get_entries_for_device_id(device_id)
                for entity in entities:
                    entity_reg.async_update_entity(
                        entity.entity_id,
                        disabled_by=er.RegistryEntryDisabler.DEVICE,
                    )

        self._device_entries[device_id] = new_device
        return new_device

    def _async_update_device(self, device_id: str, **kwargs) -> RustDeviceEntry | None:
        """Private update method called by native HA helpers (e.g., async_config_entry_disabled_by_changed)."""
        return self.async_update_device(device_id, **kwargs)

    @property
    def deleted_devices(self) -> DeletedDeviceRegistryItems:
        return DeletedDeviceRegistryItems(self._rust_registry, self._deleted_entry_cache)

    @property
    def devices(self) -> RustDeviceRegistryItems:
        entries = [
            RustDeviceEntry(entry, self._field_overrides.get(entry.id, {}))
            for entry in self._rust_registry.devices.values()
        ]
        # Sort by insertion_order to match native HA's insertion-order dict behavior
        entries.sort(key=lambda e: e._rust_entry.insertion_order)
        data = {entry.id: entry for entry in entries}
        return RustDeviceRegistryItems(self, data)

    def __iter__(self):
        return iter(self.devices.values())

    def __len__(self) -> int:
        return len(self._rust_registry)


# =============================================================================
# Rust-backed AreaRegistry wrappers
# =============================================================================

class RustAreaEntry:
    """Wrapper for AreaEntry compatible with homeassistant.helpers.area_registry."""

    __slots__ = ("_rust_entry",)

    def __init__(self, rust_entry):
        self._rust_entry = rust_entry

    @property
    def aliases(self) -> set[str]:
        return set(self._rust_entry.aliases)

    @property
    def created_at(self) -> datetime:
        return _parse_iso_datetime(self._rust_entry.created_at)

    @property
    def floor_id(self) -> str | None:
        return self._rust_entry.floor_id

    @property
    def humidity_entity_id(self) -> str | None:
        return self._rust_entry.humidity_entity_id

    @property
    def icon(self) -> str | None:
        return self._rust_entry.icon

    @property
    def id(self) -> str:
        return self._rust_entry.id

    @property
    def json_fragment(self):
        """Return a pre-serialized JSON fragment for this area entry."""
        import orjson
        from homeassistant.helpers.json import json_fragment
        return json_fragment(
            orjson.dumps({
                "aliases": list(self.aliases),
                "area_id": self.id,
                "floor_id": self.floor_id,
                "humidity_entity_id": self.humidity_entity_id,
                "icon": self.icon,
                "labels": list(self.labels),
                "name": self.name,
                "picture": self.picture,
                "temperature_entity_id": self.temperature_entity_id,
                "created_at": self.created_at.timestamp(),
                "modified_at": self.modified_at.timestamp(),
            })
        )

    @property
    def labels(self) -> set[str]:
        return set(self._rust_entry.labels)

    @property
    def modified_at(self) -> datetime:
        return _parse_iso_datetime(self._rust_entry.modified_at)

    @property
    def name(self) -> str:
        return self._rust_entry.name

    @property
    def normalized_name(self) -> str:
        return self._rust_entry.normalized_name

    @property
    def picture(self) -> str | None:
        return self._rust_entry.picture

    @property
    def temperature_entity_id(self) -> str | None:
        return self._rust_entry.temperature_entity_id

    def __eq__(self, other: object) -> bool:
        if isinstance(other, RustAreaEntry):
            # Delegate to underlying Rust __eq__ which compares all fields
            return self._rust_entry == other._rust_entry
        # Support comparison with HA's AreaEntry dataclass
        if hasattr(other, 'id') and hasattr(other, 'name'):
            other_id = other.id
            if hasattr(other_id, 'match'):
                # ANY sentinel - just check other fields
                pass
            elif self.id != other_id:
                return False
            return (
                self.name == other.name
                and self.aliases == getattr(other, 'aliases', set())
                and self.floor_id == getattr(other, 'floor_id', None)
                and self.humidity_entity_id == getattr(other, 'humidity_entity_id', None)
                and self.icon == getattr(other, 'icon', None)
                and self.labels == getattr(other, 'labels', set())
                and self.picture == getattr(other, 'picture', None)
                and self.temperature_entity_id == getattr(other, 'temperature_entity_id', None)
                and self.created_at == getattr(other, 'created_at', None)
                and self.modified_at == getattr(other, 'modified_at', None)
            )
        return NotImplemented

    def __hash__(self) -> int:
        return hash(self.id)

    def __repr__(self) -> str:
        return repr(self._rust_entry)


class RustAreaRegistryItems(dict):
    """Dict subclass that provides HA-compatible AreaRegistryItems methods."""

    def __init__(self, data: dict, rust_registry):
        super().__init__(data)
        self._rust_registry = rust_registry

    def get_areas_for_floor(self, floor_id: str) -> list:
        """Get areas for a given floor, sorted by creation time."""
        entries = [
            RustAreaEntry(entry)
            for entry in self._rust_registry.async_get_areas_for_floor(floor_id)
        ]
        entries.sort(key=lambda e: e.created_at)
        return entries

    def get_areas_for_label(self, label_id: str) -> list:
        """Get areas for a given label, sorted by creation time."""
        entries = [
            RustAreaEntry(entry)
            for entry in self._rust_registry.async_get_areas_for_label(label_id)
        ]
        entries.sort(key=lambda e: e.created_at)
        return entries


class RustAreaRegistry:
    """Wrapper that provides HA-compatible AreaRegistry API backed by Rust."""

    def __init__(self, hass):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.AreaRegistry(hass)
        self._hass = hass
        self._ordered_ids = None

    async def async_load(self) -> None:
        # No-op for testing - Rust registries start empty in test context
        pass

    async def async_save(self) -> None:
        # No-op for testing - persistence not needed for unit tests
        pass

    def _fire_event(self, action: str, area_id: str) -> None:
        self._hass.bus.async_fire(
            "area_registry_updated",
            {"action": action, "area_id": area_id},
        )

    def async_create(
        self,
        name: str,
        *,
        aliases: set[str] | None = None,
        floor_id: str | None = None,
        humidity_entity_id: str | None = None,
        icon: str | None = None,
        labels: set[str] | None = None,
        picture: str | None = None,
        temperature_entity_id: str | None = None,
    ) -> RustAreaEntry:
        entry = self._rust_registry.async_create(
            name=name,
            aliases=list(aliases) if aliases else None,
            floor_id=floor_id,
            humidity_entity_id=humidity_entity_id,
            icon=icon,
            labels=list(labels) if labels else None,
            picture=picture,
            temperature_entity_id=temperature_entity_id,
        )
        area = RustAreaEntry(entry)
        self._fire_event("create", area.id)
        return area

    def async_delete(self, area_id: str) -> None:
        self._rust_registry.async_delete(area_id)
        self._fire_event("remove", area_id)

    def async_get(self, area_id: str) -> RustAreaEntry | None:
        entry = self._rust_registry.async_get_area(area_id)
        return RustAreaEntry(entry) if entry else None

    def async_get_area(self, area_id: str) -> RustAreaEntry | None:
        return self.async_get(area_id)

    def async_get_area_by_name(self, name: str) -> RustAreaEntry | None:
        entry = self._rust_registry.async_get_area_by_name(name)
        return RustAreaEntry(entry) if entry else None

    def async_get_or_create(self, name: str) -> RustAreaEntry:
        """Get an area by name or create it if it doesn't exist."""
        existing = self._rust_registry.async_get_area_by_name(name)
        if existing:
            return RustAreaEntry(existing)
        return self.async_create(name)

    def async_list_areas(self):
        """Get all areas."""
        return self.areas.values()

    def async_reorder(self, area_ids: list[str]) -> None:
        """Reorder areas."""
        current_ids = set(self._rust_registry.areas.keys())
        if set(area_ids) != current_ids:
            raise ValueError(
                "The area_ids list must contain all existing area IDs exactly once"
            )
        self._ordered_ids = list(area_ids)
        self._fire_event("reorder", "")

    def async_update(
        self,
        area_id: str,
        **kwargs,
    ) -> RustAreaEntry:
        # Convert set types to list for Rust
        if 'aliases' in kwargs and isinstance(kwargs['aliases'], set):
            kwargs['aliases'] = list(kwargs['aliases'])
        if 'labels' in kwargs and isinstance(kwargs['labels'], set):
            kwargs['labels'] = list(kwargs['labels'])
        # Convert None to empty string for clearable fields (Rust uses "" as sentinel)
        for field in ('floor_id', 'humidity_entity_id', 'icon', 'picture', 'temperature_entity_id'):
            if field in kwargs and kwargs[field] is None:
                kwargs[field] = ""
        entry = self._rust_registry.async_update(area_id, **kwargs)
        area = RustAreaEntry(entry)
        self._fire_event("update", area_id)
        return area

    @property
    def areas(self):
        # Return a dict subclass that provides get_areas_for_floor/get_areas_for_label
        all_areas = {entry.id: entry for entry in self._rust_registry.areas.values()}
        if self._ordered_ids is not None:
            # Use explicit ordering from async_reorder
            entries = [all_areas[aid] for aid in self._ordered_ids if aid in all_areas]
        else:
            # Sort by created_at for deterministic order (DashMap doesn't preserve insertion order)
            entries = sorted(all_areas.values(), key=lambda e: e.created_at)
        data = {
            entry.id: RustAreaEntry(entry)
            for entry in entries
        }
        return RustAreaRegistryItems(data, self._rust_registry)

    def __iter__(self):
        return iter(self.areas.values())

    def __len__(self) -> int:
        return len(self._rust_registry)


# =============================================================================
# Rust-backed FloorRegistry wrappers
# =============================================================================

class RustFloorEntry:
    """Wrapper for FloorEntry compatible with homeassistant.helpers.floor_registry."""

    __slots__ = ("_rust_entry",)

    def __init__(self, rust_entry):
        self._rust_entry = rust_entry

    @property
    def aliases(self) -> set[str]:
        return set(self._rust_entry.aliases)

    @property
    def created_at(self) -> datetime:
        return _parse_iso_datetime(self._rust_entry.created_at)

    @property
    def floor_id(self) -> str:
        return self._rust_entry.floor_id

    @property
    def icon(self) -> str | None:
        return self._rust_entry.icon

    @property
    def level(self) -> int | None:
        return self._rust_entry.level

    @property
    def modified_at(self) -> datetime:
        return _parse_iso_datetime(self._rust_entry.modified_at)

    @property
    def name(self) -> str:
        return self._rust_entry.name

    @property
    def normalized_name(self) -> str:
        return self._rust_entry.normalized_name

    def __eq__(self, other: object) -> bool:
        if isinstance(other, RustFloorEntry):
            return self.floor_id == other.floor_id and self.name == other.name
        # Cross-type comparison with HA's FloorEntry dataclass
        if hasattr(other, 'floor_id') and hasattr(other, 'name'):
            return (
                self.floor_id == getattr(other, 'floor_id', None)
                and self.name == other.name
                and self.icon == getattr(other, 'icon', None)
                and self.aliases == getattr(other, 'aliases', set())
                and self.level == getattr(other, 'level', None)
                and self.created_at == getattr(other, 'created_at', None)
                and self.modified_at == getattr(other, 'modified_at', None)
            )
        return NotImplemented

    def __hash__(self) -> int:
        return hash(self.floor_id)

    def __repr__(self) -> str:
        return repr(self._rust_entry)


class RustFloorRegistry:
    """Wrapper that provides HA-compatible FloorRegistry API backed by Rust."""

    def __init__(self, hass):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.FloorRegistry(hass)
        self._hass = hass
        self._ordered_ids = []  # Track insertion/reorder order

    async def async_load(self) -> None:
        # No-op for testing - Rust registries start empty in test context
        pass

    async def async_save(self) -> None:
        # No-op for testing - persistence not needed for unit tests
        pass

    def _fire_event(self, action: str, floor_id: str) -> None:
        self._hass.bus.async_fire(
            "floor_registry_updated",
            {"action": action, "floor_id": floor_id},
        )

    def async_create(
        self,
        name: str,
        *,
        aliases: set[str] | None = None,
        level: int | None = None,
        icon: str | None = None,
    ) -> RustFloorEntry:
        entry = self._rust_registry.async_create(
            name=name,
            aliases=list(aliases) if aliases else None,
            level=level,
            icon=icon,
        )
        floor = RustFloorEntry(entry)
        self._ordered_ids.append(floor.floor_id)
        self._fire_event("create", floor.floor_id)
        return floor

    def async_delete(self, floor_id: str) -> None:
        self._rust_registry.async_delete(floor_id)
        if floor_id in self._ordered_ids:
            self._ordered_ids.remove(floor_id)
        self._fire_event("remove", floor_id)
        # Cross-registry cleanup: clear floor_id from areas that reference this floor
        area_reg = self._hass.data.get("area_registry")
        if area_reg is not None:
            area_reg._rust_registry.async_clear_floor_id(floor_id)

    def async_get(self, floor_id: str) -> RustFloorEntry | None:
        entry = self._rust_registry.async_get_floor(floor_id)
        return RustFloorEntry(entry) if entry else None

    def async_get_floor(self, floor_id: str) -> RustFloorEntry | None:
        return self.async_get(floor_id)

    def async_get_floor_by_name(self, name: str) -> RustFloorEntry | None:
        entry = self._rust_registry.async_get_floor_by_name(name)
        return RustFloorEntry(entry) if entry else None

    def async_list_floors(self):
        """Get all floors in maintained order."""
        floors_dict = self.floors
        result = []
        for fid in self._ordered_ids:
            if fid in floors_dict:
                result.append(floors_dict[fid])
        return result

    def async_reorder(self, floor_ids: list[str]) -> None:
        """Reorder floors."""
        current_ids = set(self.floors.keys())
        if set(floor_ids) != current_ids:
            raise ValueError(
                "The floor_ids list must contain all existing floor IDs exactly once"
            )
        self._ordered_ids = list(floor_ids)
        self._fire_event("reorder", "")

    def async_update(
        self,
        floor_id: str,
        *,
        name=_UNDEFINED,
        aliases=_UNDEFINED,
        icon=_UNDEFINED,
        level=_UNDEFINED,
    ) -> RustFloorEntry:
        old = self._rust_registry.async_get_floor(floor_id)
        if old is None:
            raise ValueError(f"Floor not found: {floor_id}")
        # Resolve UNDEFINED: keep old value; None/value: set explicitly
        new_name = old.name if name is _UNDEFINED else name
        new_aliases = list(old.aliases) if aliases is _UNDEFINED else (
            list(aliases) if isinstance(aliases, (set, list)) else []
        )
        new_icon = old.icon if icon is _UNDEFINED else icon
        new_level = old.level if level is _UNDEFINED else level
        # Use async_set_fields which always sets all fields
        entry = self._rust_registry.async_set_fields(
            floor_id, new_name,
            level=new_level, aliases=new_aliases, icon=new_icon,
        )
        floor = RustFloorEntry(entry)
        # Only fire event if data actually changed (Rust updates modified_at only on real changes)
        if entry.modified_at != old.modified_at:
            self._fire_event("update", floor_id)
        return floor

    def sorted_by_level(self) -> list[RustFloorEntry]:
        return [
            RustFloorEntry(entry)
            for entry in self._rust_registry.sorted_by_level()
        ]

    @property
    def floors(self) -> dict[str, RustFloorEntry]:
        # Sort by created_at for deterministic order (DashMap doesn't preserve insertion order)
        entries = sorted(self._rust_registry.floors.values(), key=lambda e: e.created_at)
        return {
            entry.floor_id: RustFloorEntry(entry)
            for entry in entries
        }

    def __iter__(self):
        return iter(self.floors.values())

    def __len__(self) -> int:
        return len(self._rust_registry)


# =============================================================================
# Rust-backed LabelRegistry wrappers
# =============================================================================

class RustLabelEntry:
    """Wrapper for LabelEntry compatible with homeassistant.helpers.label_registry."""

    __slots__ = ("_rust_entry",)

    def __init__(self, rust_entry):
        self._rust_entry = rust_entry

    @property
    def color(self) -> str | None:
        return self._rust_entry.color

    @property
    def created_at(self) -> datetime:
        return _parse_iso_datetime(self._rust_entry.created_at)

    @property
    def description(self) -> str | None:
        return self._rust_entry.description

    @property
    def icon(self) -> str | None:
        return self._rust_entry.icon

    @property
    def label_id(self) -> str:
        return self._rust_entry.label_id

    @property
    def modified_at(self) -> datetime:
        return _parse_iso_datetime(self._rust_entry.modified_at)

    @property
    def name(self) -> str:
        return self._rust_entry.name

    @property
    def normalized_name(self) -> str:
        return self._rust_entry.normalized_name

    def __eq__(self, other: object) -> bool:
        if isinstance(other, RustLabelEntry):
            return self.label_id == other.label_id and self.name == other.name
        if hasattr(other, 'label_id') and hasattr(other, 'name'):
            return (
                self.label_id == getattr(other, 'label_id', None)
                and self.name == other.name
                and self.icon == getattr(other, 'icon', None)
                and self.color == getattr(other, 'color', None)
                and self.description == getattr(other, 'description', None)
                and self.created_at == getattr(other, 'created_at', None)
                and self.modified_at == getattr(other, 'modified_at', None)
            )
        return NotImplemented

    def __hash__(self) -> int:
        return hash(self.label_id)

    def __repr__(self) -> str:
        return repr(self._rust_entry)


class RustLabelRegistry:
    """Wrapper that provides HA-compatible LabelRegistry API backed by Rust."""

    def __init__(self, hass):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.LabelRegistry(hass)
        self._hass = hass

    async def async_load(self) -> None:
        # No-op for testing - Rust registries start empty in test context
        pass

    async def async_save(self) -> None:
        # No-op for testing - persistence not needed for unit tests
        pass

    def _fire_event(self, action: str, label_id: str) -> None:
        self._hass.bus.async_fire(
            "label_registry_updated",
            {"action": action, "label_id": label_id},
        )

    def async_create(
        self,
        name: str,
        *,
        color: str | None = None,
        icon: str | None = None,
        description: str | None = None,
    ) -> RustLabelEntry:
        entry = self._rust_registry.async_create(
            name=name,
            color=color,
            icon=icon,
            description=description,
        )
        label = RustLabelEntry(entry)
        self._fire_event("create", label.label_id)
        return label

    def async_delete(self, label_id: str) -> None:
        self._rust_registry.async_delete(label_id)
        self._fire_event("remove", label_id)
        # Cross-registry cleanup: clear label from areas that reference it
        area_reg = self._hass.data.get("area_registry")
        if area_reg is not None:
            area_reg._rust_registry.async_clear_label_id(label_id)

    def async_get(self, label_id: str) -> RustLabelEntry | None:
        entry = self._rust_registry.async_get_label(label_id)
        return RustLabelEntry(entry) if entry else None

    def async_get_label(self, label_id: str) -> RustLabelEntry | None:
        return self.async_get(label_id)

    def async_get_label_by_name(self, name: str) -> RustLabelEntry | None:
        entry = self._rust_registry.async_get_label_by_name(name)
        return RustLabelEntry(entry) if entry else None

    def async_list_labels(self):
        """Get all labels sorted by creation order."""
        return sorted(self.labels.values(), key=lambda l: l.created_at)

    def async_update(
        self,
        label_id: str,
        *,
        name=_UNDEFINED,
        icon=_UNDEFINED,
        color=_UNDEFINED,
        description=_UNDEFINED,
    ) -> RustLabelEntry:
        old = self._rust_registry.async_get_label(label_id)
        if old is None:
            raise ValueError(f"Label not found: {label_id}")
        # Resolve UNDEFINED: keep old value; None: clear field; str: set value
        new_name = old.name if name is _UNDEFINED else name
        new_icon = old.icon if icon is _UNDEFINED else icon
        new_color = old.color if color is _UNDEFINED else color
        new_description = old.description if description is _UNDEFINED else description
        # Use async_set_fields which always sets all fields (None = clear)
        entry = self._rust_registry.async_set_fields(
            label_id, new_name,
            icon=new_icon, color=new_color, description=new_description,
        )
        label = RustLabelEntry(entry)
        # Only fire event if data actually changed (Rust updates modified_at only on real changes)
        if entry.modified_at != old.modified_at:
            self._fire_event("update", label_id)
        return label

    @property
    def labels(self) -> dict[str, RustLabelEntry]:
        # Sort by created_at for deterministic order (DashMap doesn't preserve insertion order)
        entries = sorted(self._rust_registry.labels.values(), key=lambda e: e.created_at)
        return {
            entry.label_id: RustLabelEntry(entry)
            for entry in entries
        }

    def __iter__(self):
        return iter(self.labels.values())

    def __len__(self) -> int:
        return len(self._rust_registry)


# =============================================================================
# Rust-backed Template wrappers
# =============================================================================

class RustTemplate:
    """Wrapper that provides HA-compatible Template API backed by Rust."""

    def __init__(self, template: str, hass: Any = None):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        rust_hass = _get_rust_hass() if hass is None else hass
        # Use the state machine from the hass instance
        if hasattr(rust_hass, '_rust_states'):
            self._rust_template = ha_core_rs.Template(template, rust_hass._rust_states)
        elif hasattr(rust_hass, 'states'):
            self._rust_template = ha_core_rs.Template(template, rust_hass.states)
        else:
            self._rust_template = ha_core_rs.Template(template, rust_hass.states)
        self._template_str = template
        self._hass = hass

    @property
    def template(self) -> str:
        return self._template_str

    async def async_render(
        self,
        variables: dict | None = None,
        parse_result: bool = True,
    ) -> Any:
        if variables:
            return self._rust_template.async_render_with_variables(variables)
        return self._rust_template.async_render()

    def is_static(self) -> bool:
        return self._rust_template.is_static()

    def __repr__(self) -> str:
        return repr(self._rust_template)


class RustTemplateEngine:
    """Wrapper for TemplateEngine for advanced usage."""

    def __init__(self, state_machine):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        if hasattr(state_machine, '_rust_states'):
            self._rust_engine = ha_core_rs.TemplateEngine(state_machine._rust_states)
        else:
            self._rust_engine = ha_core_rs.TemplateEngine(state_machine)

    def evaluate(self, template: str) -> Any:
        return self._rust_engine.evaluate(template)

    def evaluate_with_context(self, template: str, context: dict) -> Any:
        return self._rust_engine.evaluate_with_context(template, context)

    @staticmethod
    def is_template(template: str) -> bool:
        return ha_core_rs.TemplateEngine.is_template(template)

    def render(self, template: str) -> str:
        return self._rust_engine.render(template)

    def render_with_context(self, template: str, context: dict) -> str:
        return self._rust_engine.render_with_context(template, context)


# =============================================================================
# Rust-backed ConfigEntries wrappers
# =============================================================================

class RustConfigEntry:
    """Wrapper for ConfigEntry compatible with homeassistant.config_entries."""

    __slots__ = ("_rust_entry",)

    def __init__(self, rust_entry):
        self._rust_entry = rust_entry

    @property
    def created_at(self) -> datetime:
        return _parse_iso_datetime(self._rust_entry.created_at)

    @property
    def data(self) -> dict:
        return self._rust_entry.data

    @property
    def disabled_by(self) -> str | None:
        return self._rust_entry.disabled_by

    @property
    def domain(self) -> str:
        return self._rust_entry.domain

    @property
    def entry_id(self) -> str:
        return self._rust_entry.entry_id

    @property
    def minor_version(self) -> int:
        return self._rust_entry.minor_version

    @property
    def modified_at(self) -> datetime:
        return _parse_iso_datetime(self._rust_entry.modified_at)

    @property
    def options(self) -> dict:
        return self._rust_entry.options

    @property
    def pref_disable_new_entities(self) -> bool:
        return self._rust_entry.pref_disable_new_entities

    @property
    def pref_disable_polling(self) -> bool:
        return self._rust_entry.pref_disable_polling

    @property
    def reason(self) -> str | None:
        return self._rust_entry.reason

    @property
    def source(self) -> str:
        return self._rust_entry.source

    @property
    def state(self) -> str:
        return self._rust_entry.state

    @property
    def title(self) -> str:
        return self._rust_entry.title

    @property
    def unique_id(self) -> str | None:
        return self._rust_entry.unique_id

    @property
    def version(self) -> int:
        return self._rust_entry.version

    def is_disabled(self) -> bool:
        return self._rust_entry.is_disabled()

    def is_loaded(self) -> bool:
        return self._rust_entry.is_loaded()

    def supports_unload(self) -> bool:
        return self._rust_entry.supports_unload()

    def __eq__(self, other: object) -> bool:
        if isinstance(other, RustConfigEntry):
            return self.entry_id == other.entry_id
        return False

    def __hash__(self) -> int:
        return hash(self.entry_id)

    def __repr__(self) -> str:
        return repr(self._rust_entry)


class RustConfigEntries:
    """Wrapper that provides HA-compatible ConfigEntries API backed by Rust."""

    def __init__(self, storage: RustStorage):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_entries = ha_core_rs.ConfigEntries(storage._rust_storage)
        self._storage = storage

    async def async_load(self) -> None:
        self._rust_entries.async_load()

    async def async_save(self) -> None:
        self._rust_entries.async_save()

    async def async_add(
        self,
        domain: str,
        title: str,
        *,
        data: dict | None = None,
        options: dict | None = None,
        unique_id: str | None = None,
        source: str | None = None,
        version: int | None = None,
        minor_version: int | None = None,
    ) -> RustConfigEntry:
        entry = self._rust_entries.async_add(
            domain=domain,
            title=title,
            data=data,
            options=options,
            unique_id=unique_id,
            source=source,
            version=version,
            minor_version=minor_version,
        )
        return RustConfigEntry(entry)

    def async_entries(self, domain: str | None = None) -> list[RustConfigEntry]:
        return [
            RustConfigEntry(entry)
            for entry in self._rust_entries.async_entries(domain)
        ]

    def async_get_entry(self, entry_id: str) -> RustConfigEntry | None:
        entry = self._rust_entries.async_get_entry(entry_id)
        return RustConfigEntry(entry) if entry else None

    def async_get_entry_by_unique_id(
        self, domain: str, unique_id: str
    ) -> RustConfigEntry | None:
        entry = self._rust_entries.async_get_entry_by_unique_id(domain, unique_id)
        return RustConfigEntry(entry) if entry else None

    def async_loaded_entries(self, domain: str) -> list[RustConfigEntry]:
        return [
            RustConfigEntry(entry)
            for entry in self._rust_entries.async_loaded_entries(domain)
        ]

    async def async_reload(self, entry_id: str) -> None:
        self._rust_entries.async_reload(entry_id)

    async def async_remove(self, entry_id: str) -> RustConfigEntry:
        entry = self._rust_entries.async_remove(entry_id)
        return RustConfigEntry(entry)

    async def async_setup(self, entry_id: str) -> None:
        self._rust_entries.async_setup(entry_id)

    async def async_unload(self, entry_id: str) -> None:
        self._rust_entries.async_unload(entry_id)

    async def async_update_entry(
        self,
        entry_id: str,
        **kwargs,
    ) -> RustConfigEntry:
        entry = self._rust_entries.async_update_entry(entry_id, **kwargs)
        return RustConfigEntry(entry)

    def domains(self) -> list[str]:
        return self._rust_entries.domains()

    def entry_ids(self) -> list[str]:
        return self._rust_entries.entry_ids()

    def __len__(self) -> int:
        return len(self._rust_entries)


# =============================================================================
# Rust-backed Automation wrappers
# =============================================================================

class RustAutomation:
    """Wrapper for Automation compatible with homeassistant.components.automation."""

    __slots__ = ("_rust_automation",)

    def __init__(self, rust_automation):
        self._rust_automation = rust_automation

    @property
    def actions(self) -> list:
        return self._rust_automation.actions

    @property
    def alias(self) -> str | None:
        return self._rust_automation.alias

    @property
    def conditions(self) -> list:
        return self._rust_automation.conditions

    @property
    def current_runs(self) -> int:
        return self._rust_automation.current_runs

    @property
    def description(self) -> str | None:
        return self._rust_automation.description

    @property
    def enabled(self) -> bool:
        return self._rust_automation.enabled

    @property
    def id(self) -> str:
        return self._rust_automation.id

    @property
    def last_triggered(self) -> datetime | None:
        ts = self._rust_automation.last_triggered
        return _parse_iso_datetime(ts) if ts else None

    @property
    def mode(self) -> str:
        return self._rust_automation.mode

    @property
    def triggers(self) -> list:
        return self._rust_automation.triggers

    @property
    def variables(self) -> dict:
        return self._rust_automation.variables

    def can_run(self) -> bool:
        return self._rust_automation.can_run()

    def __eq__(self, other: object) -> bool:
        if isinstance(other, RustAutomation):
            return self.id == other.id
        return False

    def __hash__(self) -> int:
        return hash(self.id)

    def __repr__(self) -> str:
        return repr(self._rust_automation)


class RustAutomationManager:
    """Wrapper that provides HA-compatible AutomationManager API backed by Rust."""

    def __init__(self):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_manager = ha_core_rs.AutomationManager()

    def async_all(self) -> list[RustAutomation]:
        return [
            RustAutomation(automation)
            for automation in self._rust_manager.async_all()
        ]

    async def async_disable(self, automation_id: str) -> None:
        self._rust_manager.async_disable(automation_id)

    async def async_enable(self, automation_id: str) -> None:
        self._rust_manager.async_enable(automation_id)

    def async_get(self, automation_id: str) -> RustAutomation | None:
        automation = self._rust_manager.async_get(automation_id)
        return RustAutomation(automation) if automation else None

    async def async_load(self, configs: list) -> None:
        self._rust_manager.async_load(configs)

    async def async_reload(self, configs: list) -> None:
        self._rust_manager.async_reload(configs)

    async def async_remove(self, automation_id: str) -> RustAutomation:
        automation = self._rust_manager.async_remove(automation_id)
        return RustAutomation(automation)

    async def async_toggle(self, automation_id: str) -> bool:
        return self._rust_manager.async_toggle(automation_id)

    def decrement_runs(self, automation_id: str) -> None:
        self._rust_manager.decrement_runs(automation_id)

    def increment_runs(self, automation_id: str) -> None:
        self._rust_manager.increment_runs(automation_id)

    def mark_triggered(self, automation_id: str) -> None:
        self._rust_manager.mark_triggered(automation_id)

    def __len__(self) -> int:
        return len(self._rust_manager)


# =============================================================================
# Pytest hooks for patching
# =============================================================================

# Tests that require the disable_translations_once fixture because they
# depend on translations NOT being cached at test start
TESTS_NEEDING_FRESH_TRANSLATIONS = {
    "test_call_service_not_found",
    "test_eventbus_max_length_exceeded",
    "test_parallel_error",
    "test_serviceregistry_service_that_not_exists",
}


def pytest_configure(config):
    """Configure pytest with Rust patches."""
    if not USE_RUST_COMPONENTS or not _rust_available:
        return

    print("\n" + "=" * 60)
    print("  Running with RUST components patched in")
    print("  StateMachine uses Rust storage via ha_core_rs")
    print("=" * 60 + "\n")


def pytest_collection_modifyitems(session, config, items):
    """Apply disable_translations_once fixture to tests that need fresh translations.

    Some tests depend on translations not being cached when they start. The
    session-scoped translations_once fixture caches translations, which can cause
    these tests to fail if other tests ran first and populated the cache.
    """
    for item in items:
        if item.name in TESTS_NEEDING_FRESH_TRANSLATIONS:
            # Add the disable_translations_once fixture to these tests
            item.fixturenames.append("disable_translations_once")


@pytest.fixture(autouse=True)
def patch_ha_core():
    """Automatically patch HA core with Rust implementations."""
    global _rust_hass

    if not USE_RUST_COMPONENTS or not _rust_available:
        yield
        return

    # Reset the shared Rust instance for each test
    _rust_hass = ha_core_rs.HomeAssistant()

    import homeassistant.core as ha_core

    with patch.object(ha_core, 'Event', RustEvent), \
         patch.object(ha_core, 'ServiceCall', RustServiceCall), \
         patch.object(ha_core, 'State', RustState):
        yield

    # Clean up
    _rust_hass = None


# NOTE: Condition/trigger patching is NOT enabled by default because:
# 1. HA's condition tests check specific tracing, error messages, and validation
# 2. Our Rust evaluator implements the core logic but not all HA-specific behaviors
# 3. Core type patching (State, Context, etc.) already exercises Rust code paths
#
# To enable Rust condition evaluation patching for specific tests, use:
#     USE_RUST_CONDITIONS=1 .venv/bin/python tests/ha_compat/run_tests.py ...
USE_RUST_CONDITIONS = os.environ.get("USE_RUST_CONDITIONS", "0") == "1"


@pytest.fixture(autouse=True)
def patch_condition_helper():
    """Optionally patch HA condition helper with Rust implementation.

    Enable by setting USE_RUST_CONDITIONS=1 environment variable.
    This is separate from core patching because condition tests may depend
    on specific HA behaviors (tracing, error messages, etc.).
    """
    if not USE_RUST_CONDITIONS or not USE_RUST_COMPONENTS or not _rust_available:
        yield
        return

    try:
        from homeassistant.helpers import condition
        with patch.object(condition, 'async_from_config', rust_async_from_config):
            yield
    except ImportError:
        yield


# =============================================================================
# Fixtures
# =============================================================================

@pytest.fixture
def rust_area_registry(rust_storage):
    """Provide a Rust-backed AreaRegistry for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")
    return RustAreaRegistry(rust_storage)


@pytest.fixture
def rust_automation_manager():
    """Provide a Rust-backed AutomationManager for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")
    return RustAutomationManager()


@pytest.fixture
def rust_config_entries(rust_storage):
    """Provide a Rust-backed ConfigEntries for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")
    return RustConfigEntries(rust_storage)


@pytest.fixture
def rust_device_registry(rust_storage):
    """Provide a Rust-backed DeviceRegistry for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")
    return RustDeviceRegistry(rust_storage)


@pytest.fixture
def rust_entity_registry(rust_storage):
    """Provide a Rust-backed EntityRegistry for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")
    return RustEntityRegistry(rust_storage)


@pytest.fixture
def rust_floor_registry(rust_storage):
    """Provide a Rust-backed FloorRegistry for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")
    return RustFloorRegistry(rust_storage)


@pytest.fixture
def rust_hass():
    """Provide a pure Rust HomeAssistant instance for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")
    return ha_core_rs.HomeAssistant()


@pytest.fixture
def rust_label_registry(rust_storage):
    """Provide a Rust-backed LabelRegistry for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")
    return RustLabelRegistry(rust_storage)


@pytest.fixture
def rust_state_machine():
    """Provide a Rust-backed StateMachine for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")

    class MockBus:
        def async_fire(self, *args, **kwargs):
            pass

    return RustStateMachine(MockBus(), asyncio.get_event_loop())


@pytest.fixture
def rust_storage(tmp_path):
    """Provide a Rust-backed Storage for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")
    return RustStorage(str(tmp_path))


@pytest.fixture
def rust_template_engine(rust_state_machine):
    """Provide a Rust-backed TemplateEngine for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")
    return RustTemplateEngine(rust_state_machine)


# =============================================================================
# Rust-backed Condition Evaluation wrappers
# =============================================================================

class RustConditionChecker:
    """A callable condition checker backed by Rust ConditionEvaluator.

    This syncs states from HA's state machine to Rust before evaluation,
    allowing the Rust evaluator to see current entity states.
    """

    def __init__(self, rust_hass, condition_config: dict):
        self._rust_hass = rust_hass
        self._config = condition_config

    def _sync_states_from_ha(self, hass: Any) -> None:
        """Sync states from HA's state machine to Rust."""
        rust_states = self._rust_hass.states

        # Get all states from HA and set them in Rust
        for state in hass.states.async_all():
            attrs = dict(state.attributes) if state.attributes else {}
            rust_states.set(state.entity_id, state.state, attrs)

    def __call__(self, hass: Any, variables: dict | None = None) -> bool:
        """Evaluate the condition.

        Syncs states from HA to Rust, then evaluates using the Rust evaluator.
        """
        # Sync states from HA to Rust state machine
        self._sync_states_from_ha(hass)

        # Evaluate using Rust
        return self._rust_hass.condition_evaluator.evaluate(self._config, variables or {})


async def rust_async_from_config(hass: Any, config: dict) -> RustConditionChecker:
    """Create a condition checker from config using Rust evaluator.

    This matches the signature of homeassistant.helpers.condition.async_from_config.
    """
    if not _rust_available:
        raise RuntimeError("ha_core_rs not available")

    rust_hass = _get_rust_hass()
    return RustConditionChecker(rust_hass, config)


@pytest.fixture
def rust_condition_evaluator():
    """Provide a Rust-backed ConditionEvaluator for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")

    rust_hass = _get_rust_hass()
    return rust_hass.condition_evaluator


@pytest.fixture
def rust_trigger_evaluator():
    """Provide a Rust-backed TriggerEvaluator for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")

    rust_hass = _get_rust_hass()
    return rust_hass.trigger_evaluator


# =============================================================================
# Rust Server Fixtures for WebSocket API Testing
# =============================================================================

import signal
import subprocess
from pathlib import Path
from typing import AsyncGenerator

import aiohttp
import pytest_asyncio


# Configuration for Rust server
RUST_SERVER_HOST = "127.0.0.1"
RUST_SERVER_PORT = 18123  # Use different port to avoid conflicts
RUST_SERVER_URL = f"http://{RUST_SERVER_HOST}:{RUST_SERVER_PORT}"
RUST_WS_URL = f"ws://{RUST_SERVER_HOST}:{RUST_SERVER_PORT}/api/websocket"


def _get_repo_root() -> Path:
    """Get the repository root directory."""
    return Path(__file__).parent.parent.parent


class RustServerProcess:
    """Manages the Rust HA server process for testing."""

    def __init__(self, config_dir: Path | None = None):
        self.process: subprocess.Popen | None = None
        self.config_dir = config_dir
        self._started = False

    def start(self, timeout: float = 30.0) -> None:
        """Start the Rust server and wait for it to be ready."""
        if self._started:
            return

        repo_root = _get_repo_root()
        # The binary is named "homeassistant" per Cargo.toml [[bin]] config
        server_bin = repo_root / "target" / "debug" / "homeassistant"

        if not server_bin.exists():
            # Try release build
            server_bin = repo_root / "target" / "release" / "homeassistant"

        if not server_bin.exists():
            raise RuntimeError(
                f"Rust server binary not found. Run 'cargo build -p ha-server' first.\n"
                f"Looked for: {server_bin}"
            )

        env = os.environ.copy()
        env["HA_PORT"] = str(RUST_SERVER_PORT)
        env["HA_HOST"] = RUST_SERVER_HOST
        env["RUST_LOG"] = "warn"  # Reduce log noise during tests

        if self.config_dir:
            env["HA_CONFIG_DIR"] = str(self.config_dir)

        self.process = subprocess.Popen(
            [str(server_bin)],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        # Wait for server to be ready
        start_time = time.time()
        while time.time() - start_time < timeout:
            try:
                import urllib.request
                urllib.request.urlopen(f"{RUST_SERVER_URL}/api/", timeout=1)
                self._started = True
                return
            except Exception:
                if self.process.poll() is not None:
                    # Process died
                    stdout, stderr = self.process.communicate()
                    raise RuntimeError(
                        f"Rust server process died.\n"
                        f"stdout: {stdout.decode()}\n"
                        f"stderr: {stderr.decode()}"
                    )
                time.sleep(0.1)

        self.stop()
        raise RuntimeError(f"Rust server did not start within {timeout}s")

    def stop(self) -> None:
        """Stop the Rust server."""
        if self.process:
            self.process.send_signal(signal.SIGTERM)
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
            self.process = None
            self._started = False


class RustWebSocketClient:
    """WebSocket client for testing against our Rust server."""

    def __init__(self, session: aiohttp.ClientSession):
        self.session = session
        self.ws: aiohttp.ClientWebSocketResponse | None = None
        self._msg_id = 0

    async def connect(self) -> None:
        """Connect to the Rust server WebSocket."""
        self.ws = await self.session.ws_connect(RUST_WS_URL)

        # Wait for auth_required
        msg = await self.ws.receive_json()
        assert msg["type"] == "auth_required", f"Expected auth_required, got {msg}"

        # Send auth (our test server accepts any token)
        await self.ws.send_json({"type": "auth", "access_token": "test_token"})

        # Wait for auth_ok
        msg = await self.ws.receive_json()
        assert msg["type"] == "auth_ok", f"Expected auth_ok, got {msg}"

    async def close(self) -> None:
        """Close the WebSocket connection."""
        if self.ws:
            await self.ws.close()
            self.ws = None

    async def send_json(self, data: dict) -> None:
        """Send JSON data to the server."""
        if not self.ws:
            raise RuntimeError("Not connected")
        await self.ws.send_json(data)

    async def send_json_auto_id(self, data: dict) -> None:
        """Send JSON with auto-incremented ID."""
        self._msg_id += 1
        data["id"] = self._msg_id
        await self.send_json(data)

    async def receive_json(self, timeout: float = 10.0) -> dict:
        """Receive JSON from the server with timeout."""
        if not self.ws:
            raise RuntimeError("Not connected")
        try:
            return await asyncio.wait_for(self.ws.receive_json(), timeout=timeout)
        except asyncio.TimeoutError:
            raise TimeoutError(f"No response from server within {timeout}s")

    async def call(self, msg_type: str, **kwargs) -> dict:
        """Send a command and wait for the response."""
        self._msg_id += 1
        msg = {"type": msg_type, "id": self._msg_id, **kwargs}
        await self.send_json(msg)
        return await self.receive_json()


class RustClientAdapter:
    """Adapter that wraps RustWebSocketClient to match MockHAClientWebSocket interface.

    This adapter provides API compatibility with HA's test client interface,
    allowing native HA tests to run against our Rust WebSocket server.

    The adapter proxies method calls to the underlying RustWebSocketClient while
    providing the same interface that HA tests expect from MockHAClientWebSocket.

    Note: Some native HA tests depend on Python-side fixtures (mock_registry, etc.)
    that populate test data. Those tests may need additional setup to work with
    the Rust server, which has its own separate data store.
    """

    def __init__(self, ws_client: RustWebSocketClient):
        self._client = ws_client
        self._msg_id = 0

    async def send_json(self, data: dict) -> None:
        """Send JSON data to the server."""
        await self._client.send_json(data)

    async def send_json_auto_id(self, data: dict) -> None:
        """Send JSON with auto-incremented message ID.

        This matches the MockHAClientWebSocket.send_json_auto_id interface that
        native HA tests use.
        """
        self._msg_id += 1
        data = {**data, "id": self._msg_id}
        await self._client.send_json(data)

    async def receive_json(self, timeout: float = 10.0) -> dict:
        """Receive JSON from the server."""
        return await self._client.receive_json(timeout=timeout)

    async def close(self) -> None:
        """Close the WebSocket connection."""
        await self._client.close()

    async def remove_device(self, device_id: str, config_entry_id: str) -> dict:
        """Remove a device from the registry.

        This matches the MockHAClientWebSocket.remove_device interface.
        """
        return await self._client.call(
            "config/device_registry/remove_config_entry",
            device_id=device_id,
            config_entry_id=config_entry_id,
        )


@pytest.fixture(scope="session")
def rust_server(tmp_path_factory) -> RustServerProcess:
    """Start the Rust server for the test session.

    This fixture starts a single Rust server instance for all tests in the session,
    avoiding the overhead of starting/stopping the server for each test.

    The server runs on port 18123 to avoid conflicts with the default 8123 port.
    """
    config_dir = tmp_path_factory.mktemp("config")
    server = RustServerProcess(config_dir)
    server.start()
    yield server
    server.stop()


@pytest_asyncio.fixture
async def rust_ws_client(rust_server) -> AsyncGenerator[RustWebSocketClient, None]:
    """Provide a connected WebSocket client to the Rust server.

    This fixture creates a new WebSocket connection for each test, handling
    authentication automatically. The client can be used to send commands
    to the Rust server and receive responses.

    Example:
        async def test_example(rust_ws_client):
            response = await rust_ws_client.call("get_states")
            assert response["success"] is True
    """
    async with aiohttp.ClientSession() as session:
        client = RustWebSocketClient(session)
        await client.connect()
        yield client
        await client.close()


@pytest_asyncio.fixture
async def rust_http_client(rust_server) -> AsyncGenerator[aiohttp.ClientSession, None]:
    """Provide an HTTP client session for REST API tests.

    This fixture provides an aiohttp ClientSession pre-configured with the
    Rust server's base URL, allowing direct HTTP requests to REST endpoints.

    Example:
        async def test_example(rust_http_client):
            async with rust_http_client.get("/api/") as response:
                assert response.status == 200
    """
    async with aiohttp.ClientSession(base_url=RUST_SERVER_URL) as session:
        yield session


@pytest_asyncio.fixture
async def client(rust_ws_client) -> AsyncGenerator[RustClientAdapter, None]:
    """Provide a client adapter compatible with HA's MockHAClientWebSocket.

    This fixture provides a drop-in replacement for the 'client' fixture used
    in native HA tests (e.g., tests/components/config/test_entity_registry.py).
    It wraps the rust_ws_client with an adapter that provides the same interface.

    Usage:
        # In test files, use the 'client' fixture as you would in native HA tests
        async def test_list_entities(client):
            await client.send_json_auto_id({"type": "config/entity_registry/list"})
            msg = await client.receive_json()
            assert msg["success"] is True

    Note: Tests that depend on Python-side test fixtures (mock_registry, etc.)
    may need additional setup since the Rust server has its own data store.
    """
    yield RustClientAdapter(rust_ws_client)


# =============================================================================
# Registry Patching for Rust Implementations
# Patches HA's registry async_get functions to return Rust-backed registries
# =============================================================================

# Storage for Rust registries during tests
_test_rust_registries: dict = {}


@pytest.fixture(autouse=True)
def patch_registry_lookups(tmp_path):
    """Patch HA's registry lookup functions to return Rust-backed implementations.

    This autouse fixture patches the async_get() functions in HA's registry modules
    to return our Rust-backed registries instead of Python ones. This ensures that
    all tests that use registries (via fixtures or direct lookup) use Rust.
    """
    if not _rust_available:
        yield
        return

    # Create a mock hass object with config.path() for the registries
    class MockConfig:
        def __init__(self, base_path):
            self._base_path = base_path

        def path(self, *args):
            import os
            return os.path.join(self._base_path, *args)

    class MockHass:
        def __init__(self, storage_path):
            self.config = MockConfig(storage_path)
            self.data = {}

    mock_hass = MockHass(str(tmp_path))

    # Create Rust-backed registries using hass (new API)
    rust_entity_reg = RustEntityRegistry(mock_hass)
    rust_device_reg = RustDeviceRegistry(mock_hass)
    rust_area_reg = RustAreaRegistry(mock_hass)
    rust_floor_reg = RustFloorRegistry(mock_hass)
    rust_label_reg = RustLabelRegistry(mock_hass)

    # Store references
    global _test_rust_registries
    _test_rust_registries = {
        'entity': rust_entity_reg,
        'device': rust_device_reg,
        'area': rust_area_reg,
        'floor': rust_floor_reg,
        'label': rust_label_reg,
    }

    # Import registry modules
    from homeassistant.helpers import entity_registry as er
    from homeassistant.helpers import device_registry as dr
    from homeassistant.helpers import area_registry as ar
    from homeassistant.helpers import floor_registry as fr
    from homeassistant.helpers import label_registry as lr

    # Save original functions and classes
    orig_er_get = er.async_get
    orig_dr_get = dr.async_get
    orig_ar_get = ar.async_get
    orig_fr_get = fr.async_get
    orig_lr_get = lr.async_get
    orig_er_class = er.EntityRegistry
    orig_ar_entries_for_floor = getattr(ar, 'async_entries_for_floor', None)
    orig_ar_entries_for_label = getattr(ar, 'async_entries_for_label', None)

    # Create patched versions that return Rust registries
    # Add cache_clear as no-op for compatibility with tests that call it
    def patched_er_get(hass):
        # Store hass reference for event firing
        rust_entity_reg._hass = hass
        # Also store in hass.data for code that accesses it directly
        if er.DATA_REGISTRY not in hass.data:
            hass.data[er.DATA_REGISTRY] = rust_entity_reg
        return hass.data[er.DATA_REGISTRY]
    patched_er_get.cache_clear = lambda: None

    def patched_dr_get(hass):
        rust_device_reg.hass = hass
        if dr.DATA_REGISTRY not in hass.data:
            hass.data[dr.DATA_REGISTRY] = rust_device_reg
        return rust_device_reg
    patched_dr_get.cache_clear = lambda: None

    def patched_ar_get(hass):
        rust_area_reg._hass = hass
        if ar.DATA_REGISTRY not in hass.data:
            hass.data[ar.DATA_REGISTRY] = rust_area_reg
        return rust_area_reg
    patched_ar_get.cache_clear = lambda: None

    def patched_fr_get(hass):
        rust_floor_reg._hass = hass
        if fr.DATA_REGISTRY not in hass.data:
            hass.data[fr.DATA_REGISTRY] = rust_floor_reg
        return rust_floor_reg
    patched_fr_get.cache_clear = lambda: None

    def patched_lr_get(hass):
        rust_label_reg._hass = hass
        if lr.DATA_REGISTRY not in hass.data:
            hass.data[lr.DATA_REGISTRY] = rust_label_reg
        return rust_label_reg
    patched_lr_get.cache_clear = lambda: None

    # Create a factory class that returns new RustEntityRegistry instances
    # This is needed for tests that directly instantiate er.EntityRegistry(hass)
    # IMPORTANT: Use the same storage path (mock_hass) for consistency
    class PatchedEntityRegistry(RustEntityRegistry):
        """Patched EntityRegistry that uses Rust backend with consistent storage."""
        def __init__(self, hass):
            # Use mock_hass for storage path consistency, but store the real hass
            # for event firing and data access
            super().__init__(mock_hass)
            self._hass = hass

    # Patched async_entries_for_floor / async_entries_for_label
    def patched_ar_entries_for_floor(registry, floor_id):
        return registry.areas.get_areas_for_floor(floor_id)

    def patched_ar_entries_for_label(registry, label_id):
        return registry.areas.get_areas_for_label(label_id)

    # Apply patches
    er.async_get = patched_er_get
    dr.async_get = patched_dr_get
    ar.async_get = patched_ar_get
    fr.async_get = patched_fr_get
    lr.async_get = patched_lr_get
    er.EntityRegistry = PatchedEntityRegistry  # Patch class for direct instantiation
    ar.async_entries_for_floor = patched_ar_entries_for_floor
    ar.async_entries_for_label = patched_ar_entries_for_label

    try:
        yield
    finally:
        # Restore original functions and classes
        er.async_get = orig_er_get
        dr.async_get = orig_dr_get
        ar.async_get = orig_ar_get
        fr.async_get = orig_fr_get
        lr.async_get = orig_lr_get
        er.EntityRegistry = orig_er_class
        if orig_ar_entries_for_floor is not None:
            ar.async_entries_for_floor = orig_ar_entries_for_floor
        if orig_ar_entries_for_label is not None:
            ar.async_entries_for_label = orig_ar_entries_for_label
        _test_rust_registries = {}
