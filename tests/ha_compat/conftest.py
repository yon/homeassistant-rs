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
    from homeassistant.core import EventOrigin
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

    dt_util = None
    ulid_at_time = None

# Check environment variable to enable/disable Rust patching
USE_RUST_COMPONENTS = os.environ.get("USE_RUST_COMPONENTS", "1") != "0"

# Import Rust extension if available
_rust_available = False
_rust_hass = None  # Shared Rust HomeAssistant instance

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

class RustState:
    """Wrapper that provides HA-compatible State API backed by Rust storage."""

    __slots__ = ("entity_id", "domain", "object_id", "state", "attributes",
                 "last_changed", "last_updated", "last_updated_timestamp",
                 "last_reported", "context", "state_info", "_cache")

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

        if validate_entity_id and '.' not in entity_id:
            raise InvalidEntityFormatError(f"Invalid entity id: {entity_id}")

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
            return False
        return (
            self.entity_id == other.entity_id
            and self.state == other.state
            and self.attributes == other.attributes
        )

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
        id: str | None = None,
        user_id: str | None = None,
        parent_id: str | None = None,
    ) -> None:
        if _rust_available and id is None:
            # Use Rust to generate ULID
            rust_ctx = ha_core_rs.Context(user_id=user_id, parent_id=parent_id)
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
        if not isinstance(other, RustContext):
            return False
        return self._id == other._id

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
        old_rust_state = self._rust_states.get(entity_id)
        if old_rust_state is None:
            return False

        old_state = RustState.from_rust(old_rust_state, self._contexts.get(entity_id))
        self._rust_states.remove(entity_id)
        self._contexts.pop(entity_id, None)

        self._bus.async_fire(
            "state_changed",
            {"entity_id": entity_id, "old_state": old_state, "new_state": None},
            context=context,
        )
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

        if len(new_state) > 255:
            raise ValueError(f"State max length exceeded: {len(new_state)} > 255")

        if '.' not in entity_id:
            raise InvalidEntityFormatError(f"Invalid entity id: {entity_id}")

        context = context or RustContext()
        now = timestamp or datetime.now(timezone.utc)

        # Get old state from Rust
        old_rust_state = self._rust_states.get(entity_id)
        old_state = None
        if old_rust_state is not None:
            old_state = RustState.from_rust(old_rust_state, self._contexts.get(entity_id))

        # Determine if state actually changed
        same_attrs = False
        if old_state is not None:
            same_state = old_state.state == new_state
            same_attrs = old_state.attributes == (attributes or {})

            if same_state and same_attrs and not force_update:
                return

            if same_state and not force_update:
                last_changed = old_state.last_changed
            else:
                last_changed = now
        else:
            last_changed = now

        # Reuse old attributes dict if unchanged
        if same_attrs and old_state is not None:
            final_attrs = dict(old_state.attributes)
        else:
            final_attrs = attributes or {}

        # Store in Rust - this is the actual state storage
        rust_ctx = ha_core_rs.Context(
            user_id=context.user_id,
            parent_id=context.parent_id
        )
        self._rust_states.set(entity_id, new_state, final_attrs, rust_ctx)

        # Track context separately for full HA compatibility
        self._contexts[entity_id] = context
        self._reservations.discard(entity_id)

        # Create new state wrapper for the event
        new_rust_state = self._rust_states.get(entity_id)
        new_state_obj = RustState.from_rust(new_rust_state, context)
        # Override timestamps with our calculated values for HA compatibility
        new_state_obj.last_changed = last_changed
        new_state_obj.last_updated = now
        new_state_obj.last_reported = now
        new_state_obj.last_updated_timestamp = now.timestamp()
        if last_changed == now:
            new_state_obj._cache["last_changed_timestamp"] = now.timestamp()

        # Fire state_changed event
        self._bus.async_fire(
            "state_changed",
            {"entity_id": entity_id, "old_state": old_state, "new_state": new_state_obj},
            context=context,
        )

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
    """Wrapper that provides HA-compatible EventBus API backed by Rust."""

    __slots__ = ("_hass", "_rust_bus", "_listeners", "_loop", "_filters")

    def __init__(self, hass: Any) -> None:
        self._hass = hass
        rust_hass = _get_rust_hass()
        self._rust_bus = rust_hass.bus  # PyEventBus from Rust
        # Keep Python listeners for HA compatibility (filters, run_immediately, etc.)
        self._listeners: dict[str, list[tuple[Callable, Callable | None]]] = {}
        self._loop = asyncio.get_event_loop()

    @property
    def _debug(self) -> bool:
        return False

    def async_fire(
        self,
        event_type: str,
        event_data: dict[str, Any] | None = None,
        origin: EventOrigin = EventOrigin.local,
        context: RustContext | None = None,
        time_fired: float | None = None,
    ) -> None:
        self.async_fire_internal(event_type, event_data, origin, context, time_fired)

    def async_fire_internal(
        self,
        event_type: str,
        event_data: dict[str, Any] | None = None,
        origin: Any = None,
        context: RustContext | None = None,
        time_fired: float | None = None,
    ) -> None:
        import homeassistant.core as ha_core

        # Gather listeners
        listeners_with_filters = list(self._listeners.get(event_type, []))
        listeners_with_filters.extend(self._listeners.get("*", []))

        if not listeners_with_filters:
            return

        event_data_dict = event_data or {}
        listeners_to_call = []

        for callback, event_filter in listeners_with_filters:
            if event_filter is not None:
                try:
                    if not event_filter(event_data_dict):
                        continue
                except Exception:
                    continue
            listeners_to_call.append(callback)

        if not listeners_to_call:
            return

        time_fired_timestamp = time_fired if time_fired is not None else time.time()

        event = ha_core.Event(
            event_type,
            event_data_dict,
            origin or EventOrigin.local,
            time_fired_timestamp,
            context,
        )

        for callback in listeners_to_call:
            try:
                if asyncio.iscoroutinefunction(callback):
                    asyncio.create_task(callback(event))
                else:
                    callback(event)
            except Exception as e:
                print(f"Error in event listener: {e}")

    def async_listen(
        self,
        event_type: str,
        listener: Callable,
        run_immediately: bool = False,
        event_filter: Callable | None = None,
    ) -> Callable[[], None]:
        if event_type not in self._listeners:
            self._listeners[event_type] = []

        entry = (listener, event_filter)
        self._listeners[event_type].append(entry)

        def remove_listener() -> None:
            try:
                self._listeners[event_type].remove(entry)
            except ValueError:
                pass

        return remove_listener

    def async_listen_once(
        self,
        event_type: str,
        listener: Callable,
        run_immediately: bool = False,
    ) -> Callable[[], None]:
        remove_listener: Callable[[], None] | None = None

        def one_time_listener(event: "RustEvent") -> None:
            nonlocal remove_listener
            if remove_listener:
                remove_listener()
            if asyncio.iscoroutinefunction(listener):
                asyncio.create_task(listener(event))
            else:
                listener(event)

        remove_listener = self.async_listen(event_type, one_time_listener, run_immediately)
        return remove_listener

    def async_listeners(self) -> dict[str, int]:
        return {event_type: len(listeners) for event_type, listeners in self._listeners.items()}

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
        if "json_fragment" not in self._cache:
            self._cache["json_fragment"] = orjson.Fragment(orjson.dumps(self._as_dict))
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
            return False
        return (
            self.event_type == other.event_type
            and self.data == other.data
            and self.origin == other.origin
            and self.time_fired_timestamp == other.time_fired_timestamp
            and self.context == other.context
        )

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
        categories=dict(rust_entry.categories) if rust_entry.categories else {},
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


class RustEntityRegistry:
    """Wrapper that provides HA-compatible EntityRegistry API backed by Rust."""

    def __init__(self, hass):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.EntityRegistry(hass)
        self._hass = hass
        # Cache wrapper objects to maintain identity (for `is` checks in tests)
        self._entry_cache: dict[str, RustEntityEntry] = {}

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
        # No-op for testing - Rust registries start empty in test context
        # (no Tokio runtime available in Python's asyncio)
        pass

    async def async_save(self) -> None:
        # No-op for testing - persistence not needed for unit tests
        pass

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

    def async_device_ids(self) -> set[str]:
        """Return set of device IDs that have registered entities."""
        device_ids = set()
        for entity_id, entry in self._rust_registry.entities.items():
            if entry.device_id:
                device_ids.add(entry.device_id)
        return device_ids

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

        # Check if entity already exists (to determine if we need to fire create event)
        existing_entity_id = self._rust_registry.async_get_entity_id(domain, platform, unique_id)
        is_new = existing_entity_id is None

        # Pass current Python time as timestamp (respects freezer in tests)
        timestamp_iso = datetime.now(timezone.utc).isoformat()
        # created_at only for new entities, modified_at for all updates
        created_at_iso = timestamp_iso if is_new else None
        modified_at_iso = timestamp_iso

        # Helper: convert UNDEFINED to None (don't change), None to "" (clear), value to value
        def to_rust_optional(value, default_for_new=None):
            if value is UNDEFINED:
                # Not provided - return default for new entities, None for existing
                return default_for_new if is_new else None
            if value is None:
                return ""  # Empty string = clear in Rust
            return value

        # disabled_by and hidden_by only affect newly created entities,
        # not existing ones (see native HA entity_registry.py comments)
        entry = self._rust_registry.async_get_or_create(
            domain=domain,
            platform=platform,
            unique_id=unique_id,
            config_entry_id=to_rust_optional(config_entry_id),
            config_subentry_id=to_rust_optional(config_subentry_id),
            device_id=to_rust_optional(device_id),
            suggested_object_id=suggested_object_id,
            disabled_by=disabled_by if is_new else None,
            hidden_by=hidden_by if is_new else None,
            has_entity_name=None if has_entity_name is UNDEFINED else has_entity_name,
            capabilities=None if capabilities is UNDEFINED else capabilities,
            supported_features=None if supported_features is UNDEFINED else supported_features,
            device_class=to_rust_optional(device_class),
            unit_of_measurement=to_rust_optional(unit_of_measurement),
            original_name=to_rust_optional(original_name),
            original_icon=to_rust_optional(original_icon),
            original_device_class=to_rust_optional(original_device_class),
            entity_category=None if entity_category is UNDEFINED else entity_category,
            translation_key=to_rust_optional(translation_key),
            created_at=created_at_iso,
            modified_at=modified_at_iso,
        )
        # Check if we need to force a new RegistryEntry
        # For new entities: always create new (timestamps just set)
        # For existing: only force new if update parameters were provided
        # Note: disabled_by and hidden_by are NOT update params (only affect creation)
        has_update_params = (
            config_entry_id is not UNDEFINED
            or config_subentry_id is not UNDEFINED
            or device_id is not UNDEFINED
            or has_entity_name is not UNDEFINED
            or capabilities is not UNDEFINED
            or supported_features is not UNDEFINED
            or device_class is not UNDEFINED
            or unit_of_measurement is not UNDEFINED
            or original_name is not UNDEFINED
            or original_icon is not UNDEFINED
            or original_device_class is not UNDEFINED
            or entity_category is not UNDEFINED
            or translation_key is not UNDEFINED
        )
        force_new = is_new or (not is_new and has_update_params)
        wrapped = self._get_or_create_wrapper(entry, force_new=force_new)

        # Fire create event if this was a new entity
        if is_new:
            self._fire_event("create", wrapped.entity_id)

        return wrapped

    def async_is_registered(self, entity_id: str) -> bool:
        return self._rust_registry.async_get(entity_id) is not None

    def async_remove(self, entity_id: str) -> None:
        self._rust_registry.async_remove(entity_id)
        # Remove from cache
        self._entry_cache.pop(entity_id, None)
        # Fire remove event
        self._fire_event("remove", entity_id)

    def async_update_entity(
        self,
        entity_id: str,
        **kwargs,
    ):
        # Get old entry to track changes
        old_entry = self._rust_registry.async_get(entity_id)
        old_entity_id = old_entry.entity_id if old_entry else None

        entry = self._rust_registry.async_update_entity(entity_id, **kwargs)
        # Force new RegistryEntry since data was updated (RegistryEntry is frozen)
        wrapped = self._get_or_create_wrapper(entry, force_new=True)

        # Fire update event with changes
        changes = {k: v for k, v in kwargs.items() if v is not None}
        if changes:
            self._fire_event(
                "update",
                wrapped.entity_id,
                changes=changes,
                old_entity_id=old_entity_id if old_entity_id != wrapped.entity_id else None,
            )

        return wrapped

    @property
    def entities(self):
        """Return dict of entity_id to RegistryEntry."""
        return {
            entity_id: self._get_or_create_wrapper(entry)
            for entity_id, entry in self._rust_registry.entities.items()
        }

    def __iter__(self):
        return iter(self.entities.values())

    def __len__(self) -> int:
        return len(self._rust_registry)


# =============================================================================
# Rust-backed DeviceRegistry wrappers
# =============================================================================

class RustDeviceEntry:
    """Wrapper for DeviceEntry compatible with homeassistant.helpers.device_registry."""

    __slots__ = ("_rust_entry",)

    def __init__(self, rust_entry):
        self._rust_entry = rust_entry

    @property
    def area_id(self) -> str | None:
        return self._rust_entry.area_id

    @property
    def config_entries(self) -> set[str]:
        return set(self._rust_entry.config_entries)

    @property
    def configuration_url(self) -> str | None:
        return self._rust_entry.configuration_url

    @property
    def connections(self) -> set[tuple[str, str]]:
        return set(self._rust_entry.connections)

    @property
    def created_at(self) -> datetime:
        return _parse_iso_datetime(self._rust_entry.created_at)

    @property
    def disabled_by(self) -> str | None:
        return self._rust_entry.disabled_by

    @property
    def entry_type(self) -> str | None:
        return self._rust_entry.entry_type

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
        return _parse_iso_datetime(self._rust_entry.modified_at)

    @property
    def name(self) -> str | None:
        return self._rust_entry.name

    @property
    def name_by_user(self) -> str | None:
        return self._rust_entry.name_by_user

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
            return self.id == other.id
        return False

    def __repr__(self) -> str:
        return repr(self._rust_entry)


class RustDeviceRegistry:
    """Wrapper that provides HA-compatible DeviceRegistry API backed by Rust."""

    def __init__(self, hass):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.DeviceRegistry(hass)
        self._hass = hass

    async def async_load(self) -> None:
        # No-op for testing - Rust registries start empty in test context
        pass

    async def async_save(self) -> None:
        # No-op for testing - persistence not needed for unit tests
        pass

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

    def async_get(self, device_id: str) -> RustDeviceEntry | None:
        entry = self._rust_registry.async_get(device_id)
        return RustDeviceEntry(entry) if entry else None

    def async_get_device(
        self,
        identifiers: set[tuple[str, str]] | None = None,
        connections: set[tuple[str, str]] | None = None,
    ) -> RustDeviceEntry | None:
        entry = self._rust_registry.async_get_device(
            identifiers=list(identifiers) if identifiers else None,
            connections=list(connections) if connections else None,
        )
        return RustDeviceEntry(entry) if entry else None

    def async_get_or_create(
        self,
        *,
        config_entry_id: str,
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
        configuration_url: str | None = None,
        entry_type: str | None = None,
    ) -> RustDeviceEntry:
        entry = self._rust_registry.async_get_or_create(
            config_entry_id=config_entry_id,
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
            configuration_url=configuration_url,
            entry_type=entry_type,
        )
        return RustDeviceEntry(entry)

    def async_remove_device(self, device_id: str) -> None:
        self._rust_registry.async_remove_device(device_id)

    def async_update_device(
        self,
        device_id: str,
        **kwargs,
    ) -> RustDeviceEntry:
        entry = self._rust_registry.async_update_device(device_id, **kwargs)
        return RustDeviceEntry(entry)

    @property
    def devices(self) -> dict[str, RustDeviceEntry]:
        return {
            entry.id: RustDeviceEntry(entry)
            for entry in self._rust_registry.devices
        }

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
    def icon(self) -> str | None:
        return self._rust_entry.icon

    @property
    def id(self) -> str:
        return self._rust_entry.id

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

    def __eq__(self, other: object) -> bool:
        if isinstance(other, RustAreaEntry):
            return self.id == other.id
        return False

    def __repr__(self) -> str:
        return repr(self._rust_entry)


class RustAreaRegistry:
    """Wrapper that provides HA-compatible AreaRegistry API backed by Rust."""

    def __init__(self, hass):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.AreaRegistry(hass)
        self._hass = hass

    async def async_load(self) -> None:
        # No-op for testing - Rust registries start empty in test context
        pass

    async def async_save(self) -> None:
        # No-op for testing - persistence not needed for unit tests
        pass

    def async_create(
        self,
        name: str,
        *,
        aliases: set[str] | None = None,
        floor_id: str | None = None,
        icon: str | None = None,
        picture: str | None = None,
        labels: set[str] | None = None,
    ) -> RustAreaEntry:
        entry = self._rust_registry.async_create(
            name=name,
            aliases=list(aliases) if aliases else None,
            floor_id=floor_id,
            icon=icon,
            picture=picture,
            labels=list(labels) if labels else None,
        )
        return RustAreaEntry(entry)

    def async_delete(self, area_id: str) -> None:
        self._rust_registry.async_delete(area_id)

    def async_get(self, area_id: str) -> RustAreaEntry | None:
        entry = self._rust_registry.async_get(area_id)
        return RustAreaEntry(entry) if entry else None

    def async_get_area_by_name(self, name: str) -> RustAreaEntry | None:
        entry = self._rust_registry.async_get_area_by_name(name)
        return RustAreaEntry(entry) if entry else None

    def async_list_areas(self):
        """Get all areas."""
        return self.areas.values()

    def async_update(
        self,
        area_id: str,
        **kwargs,
    ) -> RustAreaEntry:
        entry = self._rust_registry.async_update(area_id, **kwargs)
        return RustAreaEntry(entry)

    @property
    def areas(self) -> dict[str, RustAreaEntry]:
        # _rust_registry.areas returns a dict, iterate over values
        return {
            entry.id: RustAreaEntry(entry)
            for entry in self._rust_registry.areas.values()
        }

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
            return self.floor_id == other.floor_id
        return False

    def __repr__(self) -> str:
        return repr(self._rust_entry)


class RustFloorRegistry:
    """Wrapper that provides HA-compatible FloorRegistry API backed by Rust."""

    def __init__(self, hass):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.FloorRegistry(hass)
        self._hass = hass

    async def async_load(self) -> None:
        # No-op for testing - Rust registries start empty in test context
        pass

    async def async_save(self) -> None:
        # No-op for testing - persistence not needed for unit tests
        pass

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
        return RustFloorEntry(entry)

    def async_delete(self, floor_id: str) -> None:
        self._rust_registry.async_delete(floor_id)

    def async_get(self, floor_id: str) -> RustFloorEntry | None:
        entry = self._rust_registry.async_get(floor_id)
        return RustFloorEntry(entry) if entry else None

    def async_get_floor_by_name(self, name: str) -> RustFloorEntry | None:
        entry = self._rust_registry.async_get_floor_by_name(name)
        return RustFloorEntry(entry) if entry else None

    def async_list_floors(self):
        """Get all floors."""
        return self.floors.values()

    def async_update(
        self,
        floor_id: str,
        **kwargs,
    ) -> RustFloorEntry:
        entry = self._rust_registry.async_update(floor_id, **kwargs)
        return RustFloorEntry(entry)

    def sorted_by_level(self) -> list[RustFloorEntry]:
        return [
            RustFloorEntry(entry)
            for entry in self._rust_registry.sorted_by_level()
        ]

    @property
    def floors(self) -> dict[str, RustFloorEntry]:
        # _rust_registry.floors returns a dict, iterate over values
        return {
            entry.floor_id: RustFloorEntry(entry)
            for entry in self._rust_registry.floors.values()
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
            return self.label_id == other.label_id
        return False

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
        return RustLabelEntry(entry)

    def async_delete(self, label_id: str) -> None:
        self._rust_registry.async_delete(label_id)

    def async_get(self, label_id: str) -> RustLabelEntry | None:
        entry = self._rust_registry.async_get(label_id)
        return RustLabelEntry(entry) if entry else None

    def async_get_label_by_name(self, name: str) -> RustLabelEntry | None:
        entry = self._rust_registry.async_get_label_by_name(name)
        return RustLabelEntry(entry) if entry else None

    def async_list_labels(self):
        """Get all labels."""
        return self.labels.values()

    def async_update(
        self,
        label_id: str,
        **kwargs,
    ) -> RustLabelEntry:
        entry = self._rust_registry.async_update(label_id, **kwargs)
        return RustLabelEntry(entry)

    @property
    def labels(self) -> dict[str, RustLabelEntry]:
        # _rust_registry.labels returns a dict, iterate over values
        return {
            entry.label_id: RustLabelEntry(entry)
            for entry in self._rust_registry.labels.values()
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

    with patch.object(ha_core, 'Context', RustContext), \
         patch.object(ha_core, 'Event', RustEvent), \
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

    # Save original functions
    orig_er_get = er.async_get
    orig_dr_get = dr.async_get
    orig_ar_get = ar.async_get
    orig_fr_get = fr.async_get
    orig_lr_get = lr.async_get

    # Create patched versions that return Rust registries
    # Add cache_clear as no-op for compatibility with tests that call it
    def patched_er_get(hass):
        # Store hass reference for event firing
        rust_entity_reg._hass = hass
        # Also store in hass.data for code that accesses it directly
        if er.DATA_REGISTRY not in hass.data:
            hass.data[er.DATA_REGISTRY] = rust_entity_reg
        return rust_entity_reg
    patched_er_get.cache_clear = lambda: None

    def patched_dr_get(hass):
        if dr.DATA_REGISTRY not in hass.data:
            hass.data[dr.DATA_REGISTRY] = rust_device_reg
        return rust_device_reg
    patched_dr_get.cache_clear = lambda: None

    def patched_ar_get(hass):
        if ar.DATA_REGISTRY not in hass.data:
            hass.data[ar.DATA_REGISTRY] = rust_area_reg
        return rust_area_reg
    patched_ar_get.cache_clear = lambda: None

    def patched_fr_get(hass):
        if fr.DATA_REGISTRY not in hass.data:
            hass.data[fr.DATA_REGISTRY] = rust_floor_reg
        return rust_floor_reg
    patched_fr_get.cache_clear = lambda: None

    def patched_lr_get(hass):
        if lr.DATA_REGISTRY not in hass.data:
            hass.data[lr.DATA_REGISTRY] = rust_label_reg
        return rust_label_reg
    patched_lr_get.cache_clear = lambda: None

    # Apply patches
    er.async_get = patched_er_get
    dr.async_get = patched_dr_get
    ar.async_get = patched_ar_get
    fr.async_get = patched_fr_get
    lr.async_get = patched_lr_get

    try:
        yield
    finally:
        # Restore original functions
        er.async_get = orig_er_get
        dr.async_get = orig_dr_get
        ar.async_get = orig_ar_get
        fr.async_get = orig_fr_get
        lr.async_get = orig_lr_get
        _test_rust_registries = {}
