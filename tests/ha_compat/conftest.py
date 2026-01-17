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
    def name(self) -> str:
        return self.attributes.get('friendly_name') or self.object_id.replace('_', ' ')

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
    def json_fragment(self) -> Any:
        import orjson
        if "json_fragment" not in self._cache:
            self._cache["json_fragment"] = orjson.Fragment(orjson.dumps(self.as_dict()))
        return self._cache["json_fragment"]

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

    def expire(self) -> None:
        pass

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

    def __repr__(self) -> str:
        last_changed_str = self.last_changed.isoformat()
        if self.last_changed.tzinfo is None:
            last_changed_str += "+00:00"
        attrs_str = ""
        if self.attributes:
            attrs_str = "; " + ", ".join(f"{k}={v}" for k, v in self.attributes.items())
        return f"<state {self.entity_id}={self.state}{attrs_str} @ {last_changed_str}>"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, RustState):
            return False
        return (
            self.entity_id == other.entity_id
            and self.state == other.state
            and self.attributes == other.attributes
        )


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
    def user_id(self) -> str | None:
        return self._user_id

    @property
    def parent_id(self) -> str | None:
        return self._parent_id

    @property
    def origin_event(self) -> Any:
        return self._origin_event

    @origin_event.setter
    def origin_event(self, value: Any) -> None:
        self._origin_event = value

    @property
    def json_fragment(self) -> Any:
        import orjson
        if "json_fragment" not in self._cache:
            self._cache["json_fragment"] = orjson.Fragment(orjson.dumps(self.as_dict()))
        return self._cache["json_fragment"]

    def as_dict(self) -> ReadOnlyDict:
        return ReadOnlyDict({
            "id": self._id,
            "parent_id": self._parent_id,
            "user_id": self._user_id,
        })

    def __repr__(self) -> str:
        return f"<Context id={self._id}, user_id={self._user_id}>"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, RustContext):
            return False
        return self._id == other._id


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

    def entity_ids(self, domain_filter: str | None = None) -> list[str]:
        if domain_filter:
            return self._rust_states.entity_ids(domain_filter)
        return self._rust_states.all_entity_ids()

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

    def set(
        self,
        entity_id: str,
        new_state: str,
        attributes: dict[str, Any] | None = None,
        force_update: bool = False,
        context: RustContext | None = None,
    ) -> None:
        self.async_set(entity_id, new_state, attributes, force_update, context)

    def async_reserve(self, entity_id: str) -> None:
        self._reservations.add(entity_id.lower())

    def async_available(self, entity_id: str) -> bool:
        entity_id = entity_id.lower()
        return (
            self._rust_states.get(entity_id) is None
            and entity_id not in self._reservations
        )

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

    def async_listeners(self) -> dict[str, int]:
        return {event_type: len(listeners) for event_type, listeners in self._listeners.items()}

    def listeners(self) -> dict[str, int]:
        return self.async_listeners()

    @property
    def _debug(self) -> bool:
        return False

    def fire(
        self,
        event_type: str,
        event_data: dict[str, Any] | None = None,
        origin: EventOrigin = EventOrigin.local,
        context: RustContext | None = None,
        time_fired: float | None = None,
    ) -> None:
        self.async_fire(event_type, event_data, origin, context, time_fired)

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

    def listen(
        self,
        event_type: str,
        listener: Callable,
    ) -> Callable[[], None]:
        return self.async_listen(event_type, listener)

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

    def listen_once(
        self,
        event_type: str,
        listener: Callable,
    ) -> Callable[[], None]:
        return self.async_listen_once(event_type, listener)

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
    def time_fired(self) -> datetime:
        if "time_fired" not in self._cache:
            if dt_util is not None:
                self._cache["time_fired"] = dt_util.utc_from_timestamp(self.time_fired_timestamp)
            else:
                self._cache["time_fired"] = datetime.fromtimestamp(
                    self.time_fired_timestamp, tz=timezone.utc
                )
        return self._cache["time_fired"]

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

    def as_dict(self) -> ReadOnlyDict:
        if "_as_read_only_dict" not in self._cache:
            as_dict = self._as_dict
            if not isinstance(as_dict["data"], ReadOnlyDict):
                as_dict["data"] = ReadOnlyDict(as_dict["data"])
            if not isinstance(as_dict["context"], ReadOnlyDict):
                as_dict["context"] = ReadOnlyDict(as_dict["context"])
            self._cache["_as_read_only_dict"] = ReadOnlyDict(as_dict)
        return self._cache["_as_read_only_dict"]

    @property
    def json_fragment(self) -> Any:
        import orjson
        if "json_fragment" not in self._cache:
            self._cache["json_fragment"] = orjson.Fragment(orjson.dumps(self._as_dict))
        return self._cache["json_fragment"]

    def __repr__(self) -> str:
        origin_char = "L" if self.origin == EventOrigin.local else "R"
        if self.data:
            data_str = ", ".join(f"{k}={v}" for k, v in self.data.items())
            return f"<Event {self.event_type}[{origin_char}]: {data_str}>"
        return f"<Event {self.event_type}[{origin_char}]>"

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

    def __repr__(self) -> str:
        if self.data:
            data_str = ", ".join(f"{k}={v}" for k, v in self.data.items())
            return f"<ServiceCall {self.domain}.{self.service} (c:{self.context.id}): {data_str}>"
        return f"<ServiceCall {self.domain}.{self.service} (c:{self.context.id})>"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, RustServiceCall):
            return False
        return (
            self.domain == other.domain
            and self.service == other.service
            and self.data == other.data
        )


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

class RustEntityEntry:
    """Wrapper for EntityEntry compatible with homeassistant.helpers.entity_registry."""

    __slots__ = ("_rust_entry",)

    def __init__(self, rust_entry):
        self._rust_entry = rust_entry

    @property
    def aliases(self) -> set[str]:
        return set(self._rust_entry.aliases)

    @property
    def area_id(self) -> str | None:
        return self._rust_entry.area_id

    @property
    def capabilities(self) -> dict | None:
        return self._rust_entry.capabilities

    @property
    def config_entry_id(self) -> str | None:
        return self._rust_entry.config_entry_id

    @property
    def created_at(self) -> datetime:
        return _parse_iso_datetime(self._rust_entry.created_at)

    @property
    def device_class(self) -> str | None:
        return self._rust_entry.device_class

    @property
    def device_id(self) -> str | None:
        return self._rust_entry.device_id

    @property
    def disabled_by(self) -> str | None:
        return self._rust_entry.disabled_by

    @property
    def domain(self) -> str:
        return self._rust_entry.domain

    @property
    def entity_category(self) -> str | None:
        return self._rust_entry.entity_category

    @property
    def entity_id(self) -> str:
        return self._rust_entry.entity_id

    @property
    def hidden_by(self) -> str | None:
        return self._rust_entry.hidden_by

    @property
    def icon(self) -> str | None:
        return self._rust_entry.icon

    @property
    def labels(self) -> set[str]:
        return set(self._rust_entry.labels)

    @property
    def modified_at(self) -> datetime:
        return _parse_iso_datetime(self._rust_entry.modified_at)

    @property
    def name(self) -> str | None:
        return self._rust_entry.name

    @property
    def options(self) -> dict:
        return self._rust_entry.options

    @property
    def original_device_class(self) -> str | None:
        return self._rust_entry.original_device_class

    @property
    def original_icon(self) -> str | None:
        return self._rust_entry.original_icon

    @property
    def original_name(self) -> str | None:
        return self._rust_entry.original_name

    @property
    def platform(self) -> str:
        return self._rust_entry.platform

    @property
    def supported_features(self) -> int:
        return self._rust_entry.supported_features

    @property
    def translation_key(self) -> str | None:
        return self._rust_entry.translation_key

    @property
    def unique_id(self) -> str:
        return self._rust_entry.unique_id

    @property
    def unit_of_measurement(self) -> str | None:
        return self._rust_entry.unit_of_measurement

    def __eq__(self, other: object) -> bool:
        if isinstance(other, RustEntityEntry):
            return self.entity_id == other.entity_id
        return False

    def __repr__(self) -> str:
        return repr(self._rust_entry)


class RustEntityRegistry:
    """Wrapper that provides HA-compatible EntityRegistry API backed by Rust."""

    def __init__(self, storage: RustStorage):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.EntityRegistry(storage._rust_storage)
        self._storage = storage

    async def async_load(self) -> None:
        self._rust_registry.async_load()

    async def async_save(self) -> None:
        self._rust_registry.async_save()

    def async_get(self, entity_id: str) -> RustEntityEntry | None:
        entry = self._rust_registry.async_get(entity_id)
        return RustEntityEntry(entry) if entry else None

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
        config_entry_id: str | None = None,
        device_id: str | None = None,
        known_object_ids: list[str] | None = None,
        suggested_object_id: str | None = None,
        disabled_by: str | None = None,
        hidden_by: str | None = None,
        has_entity_name: bool = False,
        capabilities: dict | None = None,
        supported_features: int | None = None,
        device_class: str | None = None,
        unit_of_measurement: str | None = None,
        original_name: str | None = None,
        original_icon: str | None = None,
        original_device_class: str | None = None,
        entity_category: str | None = None,
        translation_key: str | None = None,
    ) -> RustEntityEntry:
        entry = self._rust_registry.async_get_or_create(
            domain=domain,
            platform=platform,
            unique_id=unique_id,
            config_entry_id=config_entry_id,
            device_id=device_id,
            known_object_ids=known_object_ids,
            suggested_object_id=suggested_object_id,
            disabled_by=disabled_by,
            hidden_by=hidden_by,
            has_entity_name=has_entity_name,
            capabilities=capabilities,
            supported_features=supported_features,
            device_class=device_class,
            unit_of_measurement=unit_of_measurement,
            original_name=original_name,
            original_icon=original_icon,
            original_device_class=original_device_class,
            entity_category=entity_category,
            translation_key=translation_key,
        )
        return RustEntityEntry(entry)

    def async_update_entity(
        self,
        entity_id: str,
        **kwargs,
    ) -> RustEntityEntry:
        entry = self._rust_registry.async_update_entity(entity_id, **kwargs)
        return RustEntityEntry(entry)

    def async_remove(self, entity_id: str) -> None:
        self._rust_registry.async_remove(entity_id)

    @property
    def entities(self) -> dict[str, RustEntityEntry]:
        return {
            entry.entity_id: RustEntityEntry(entry)
            for entry in self._rust_registry.entities
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

    def __init__(self, storage: RustStorage):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.DeviceRegistry(storage._rust_storage)
        self._storage = storage

    async def async_load(self) -> None:
        self._rust_registry.async_load()

    async def async_save(self) -> None:
        self._rust_registry.async_save()

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

    def async_update_device(
        self,
        device_id: str,
        **kwargs,
    ) -> RustDeviceEntry:
        entry = self._rust_registry.async_update_device(device_id, **kwargs)
        return RustDeviceEntry(entry)

    def async_remove_device(self, device_id: str) -> None:
        self._rust_registry.async_remove_device(device_id)

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

    def __init__(self, storage: RustStorage):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.AreaRegistry(storage._rust_storage)
        self._storage = storage

    async def async_load(self) -> None:
        self._rust_registry.async_load()

    async def async_save(self) -> None:
        self._rust_registry.async_save()

    def async_get(self, area_id: str) -> RustAreaEntry | None:
        entry = self._rust_registry.async_get(area_id)
        return RustAreaEntry(entry) if entry else None

    def async_get_area_by_name(self, name: str) -> RustAreaEntry | None:
        entry = self._rust_registry.async_get_area_by_name(name)
        return RustAreaEntry(entry) if entry else None

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

    def async_update(
        self,
        area_id: str,
        **kwargs,
    ) -> RustAreaEntry:
        entry = self._rust_registry.async_update(area_id, **kwargs)
        return RustAreaEntry(entry)

    def async_delete(self, area_id: str) -> None:
        self._rust_registry.async_delete(area_id)

    @property
    def areas(self) -> dict[str, RustAreaEntry]:
        return {
            entry.id: RustAreaEntry(entry)
            for entry in self._rust_registry.areas
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

    def __init__(self, storage: RustStorage):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.FloorRegistry(storage._rust_storage)
        self._storage = storage

    async def async_load(self) -> None:
        self._rust_registry.async_load()

    async def async_save(self) -> None:
        self._rust_registry.async_save()

    def async_get(self, floor_id: str) -> RustFloorEntry | None:
        entry = self._rust_registry.async_get(floor_id)
        return RustFloorEntry(entry) if entry else None

    def async_get_floor_by_name(self, name: str) -> RustFloorEntry | None:
        entry = self._rust_registry.async_get_floor_by_name(name)
        return RustFloorEntry(entry) if entry else None

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

    def async_update(
        self,
        floor_id: str,
        **kwargs,
    ) -> RustFloorEntry:
        entry = self._rust_registry.async_update(floor_id, **kwargs)
        return RustFloorEntry(entry)

    def async_delete(self, floor_id: str) -> None:
        self._rust_registry.async_delete(floor_id)

    def sorted_by_level(self) -> list[RustFloorEntry]:
        return [
            RustFloorEntry(entry)
            for entry in self._rust_registry.sorted_by_level()
        ]

    @property
    def floors(self) -> dict[str, RustFloorEntry]:
        return {
            entry.floor_id: RustFloorEntry(entry)
            for entry in self._rust_registry.floors
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

    def __init__(self, storage: RustStorage):
        if not _rust_available:
            raise RuntimeError("ha_core_rs not available")
        self._rust_registry = ha_core_rs.LabelRegistry(storage._rust_storage)
        self._storage = storage

    async def async_load(self) -> None:
        self._rust_registry.async_load()

    async def async_save(self) -> None:
        self._rust_registry.async_save()

    def async_get(self, label_id: str) -> RustLabelEntry | None:
        entry = self._rust_registry.async_get(label_id)
        return RustLabelEntry(entry) if entry else None

    def async_get_label_by_name(self, name: str) -> RustLabelEntry | None:
        entry = self._rust_registry.async_get_label_by_name(name)
        return RustLabelEntry(entry) if entry else None

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

    def async_update(
        self,
        label_id: str,
        **kwargs,
    ) -> RustLabelEntry:
        entry = self._rust_registry.async_update(label_id, **kwargs)
        return RustLabelEntry(entry)

    def async_delete(self, label_id: str) -> None:
        self._rust_registry.async_delete(label_id)

    @property
    def labels(self) -> dict[str, RustLabelEntry]:
        return {
            entry.label_id: RustLabelEntry(entry)
            for entry in self._rust_registry.labels
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

    def render(self, template: str) -> str:
        return self._rust_engine.render(template)

    def render_with_context(self, template: str, context: dict) -> str:
        return self._rust_engine.render_with_context(template, context)

    def evaluate(self, template: str) -> Any:
        return self._rust_engine.evaluate(template)

    def evaluate_with_context(self, template: str, context: dict) -> Any:
        return self._rust_engine.evaluate_with_context(template, context)

    @staticmethod
    def is_template(template: str) -> bool:
        return ha_core_rs.TemplateEngine.is_template(template)


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

    def async_get_entry(self, entry_id: str) -> RustConfigEntry | None:
        entry = self._rust_entries.async_get_entry(entry_id)
        return RustConfigEntry(entry) if entry else None

    def async_entries(self, domain: str | None = None) -> list[RustConfigEntry]:
        return [
            RustConfigEntry(entry)
            for entry in self._rust_entries.async_entries(domain)
        ]

    def async_loaded_entries(self, domain: str) -> list[RustConfigEntry]:
        return [
            RustConfigEntry(entry)
            for entry in self._rust_entries.async_loaded_entries(domain)
        ]

    def async_get_entry_by_unique_id(
        self, domain: str, unique_id: str
    ) -> RustConfigEntry | None:
        entry = self._rust_entries.async_get_entry_by_unique_id(domain, unique_id)
        return RustConfigEntry(entry) if entry else None

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

    async def async_update_entry(
        self,
        entry_id: str,
        **kwargs,
    ) -> RustConfigEntry:
        entry = self._rust_entries.async_update_entry(entry_id, **kwargs)
        return RustConfigEntry(entry)

    async def async_remove(self, entry_id: str) -> RustConfigEntry:
        entry = self._rust_entries.async_remove(entry_id)
        return RustConfigEntry(entry)

    async def async_setup(self, entry_id: str) -> None:
        self._rust_entries.async_setup(entry_id)

    async def async_unload(self, entry_id: str) -> None:
        self._rust_entries.async_unload(entry_id)

    async def async_reload(self, entry_id: str) -> None:
        self._rust_entries.async_reload(entry_id)

    def entry_ids(self) -> list[str]:
        return self._rust_entries.entry_ids()

    def domains(self) -> list[str]:
        return self._rust_entries.domains()

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

    async def async_load(self, configs: list) -> None:
        self._rust_manager.async_load(configs)

    async def async_reload(self, configs: list) -> None:
        self._rust_manager.async_reload(configs)

    def async_get(self, automation_id: str) -> RustAutomation | None:
        automation = self._rust_manager.async_get(automation_id)
        return RustAutomation(automation) if automation else None

    def async_all(self) -> list[RustAutomation]:
        return [
            RustAutomation(automation)
            for automation in self._rust_manager.async_all()
        ]

    async def async_enable(self, automation_id: str) -> None:
        self._rust_manager.async_enable(automation_id)

    async def async_disable(self, automation_id: str) -> None:
        self._rust_manager.async_disable(automation_id)

    async def async_toggle(self, automation_id: str) -> bool:
        return self._rust_manager.async_toggle(automation_id)

    async def async_remove(self, automation_id: str) -> RustAutomation:
        automation = self._rust_manager.async_remove(automation_id)
        return RustAutomation(automation)

    def mark_triggered(self, automation_id: str) -> None:
        self._rust_manager.mark_triggered(automation_id)

    def increment_runs(self, automation_id: str) -> None:
        self._rust_manager.increment_runs(automation_id)

    def decrement_runs(self, automation_id: str) -> None:
        self._rust_manager.decrement_runs(automation_id)

    def __len__(self) -> int:
        return len(self._rust_manager)


# =============================================================================
# Pytest hooks for patching
# =============================================================================

def pytest_configure(config):
    """Configure pytest with Rust patches."""
    if not USE_RUST_COMPONENTS or not _rust_available:
        return

    print("\n" + "=" * 60)
    print("  Running with RUST components patched in")
    print("  StateMachine uses Rust storage via ha_core_rs")
    print("=" * 60 + "\n")


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
