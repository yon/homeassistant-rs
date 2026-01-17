"""Pytest configuration for Home Assistant compatibility tests.

This conftest patches HA's core types with our Rust implementations to verify
API compatibility. When USE_RUST_COMPONENTS is True, HA's StateMachine, EventBus,
and ServiceRegistry are replaced with Rust-backed versions.

Usage:
    # Run with Rust components
    pytest ../core/tests/test_core.py -k "test_state" -p tests.ha_compat.conftest

    # Run without Rust (baseline)
    USE_RUST_COMPONENTS=0 pytest ../core/tests/test_core.py -k "test_state"
"""

import asyncio
import os
import sys
import time
from collections.abc import Callable, Coroutine, Iterable
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
    # Fallback if HA not installed
    class InvalidEntityFormatError(Exception):
        """Raised when an invalid entity format is encountered."""
        pass

    class ReadOnlyDict(dict):
        """Fallback read-only dict."""
        pass

    import enum
    class EventOrigin(enum.Enum):
        """Fallback EventOrigin enum."""
        local = "LOCAL"
        remote = "REMOTE"

    dt_util = None
    ulid_at_time = None

# Check environment variable to enable/disable Rust patching
USE_RUST_COMPONENTS = os.environ.get("USE_RUST_COMPONENTS", "1") != "0"

# Import Rust extension if available
_rust_available = False
if USE_RUST_COMPONENTS:
    try:
        import ha_core_rs
        _rust_available = True
    except ImportError:
        print("Warning: ha_core_rs not found, running pure Python tests")
        USE_RUST_COMPONENTS = False


def _parse_iso_datetime(iso_str: str) -> datetime:
    """Parse ISO format datetime string to datetime object."""
    # Handle both 'Z' and '+00:00' timezone formats
    if iso_str.endswith('Z'):
        iso_str = iso_str[:-1] + '+00:00'
    return datetime.fromisoformat(iso_str)


# =============================================================================
# Rust-backed State wrapper
# =============================================================================

class RustState:
    """Wrapper that makes ha_core_rs.State compatible with homeassistant.core.State.

    This provides the same API as HA's State class but backed by Rust storage.
    """

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
        # Initialize cache first (used for timestamps)
        self._cache: dict[str, Any] = {}

        if validate_entity_id and '.' not in entity_id:
            raise InvalidEntityFormatError(f"Invalid entity id: {entity_id}")

        self.entity_id = entity_id
        self.state = state
        # HA uses ReadOnlyDict for attributes - preserve if already ReadOnlyDict
        if attributes is None:
            self.attributes = ReadOnlyDict({})
        elif isinstance(attributes, ReadOnlyDict):
            self.attributes = attributes  # Preserve identity
        else:
            self.attributes = ReadOnlyDict(attributes)

        # Chain defaults like HA does
        now = datetime.now(timezone.utc)
        self.last_reported = last_reported or now
        self.last_updated = last_updated or self.last_reported
        self.last_changed = last_changed or self.last_updated

        # Use provided timestamp or calculate from datetime
        if last_updated_timestamp is None:
            last_updated_timestamp = self.last_updated.timestamp()
        self.last_updated_timestamp = last_updated_timestamp

        # Cache timestamps like HA does - if times are equal, use the same timestamp
        if self.last_changed == self.last_updated:
            self._cache["last_changed_timestamp"] = last_updated_timestamp
        # Use identity check for last_reported like HA does
        if self.last_reported is self.last_updated:
            self._cache["last_reported_timestamp"] = last_updated_timestamp
        self.context = context or RustContext()
        self.state_info = state_info

        # Parse domain and object_id
        self.domain, self.object_id = entity_id.split('.', 1)

    @classmethod
    def from_rust(cls, rust_state) -> "RustState":
        """Create RustState from ha_core_rs.State."""
        state = cls.__new__(cls)
        state.entity_id = str(rust_state.entity_id)
        state.state = rust_state.state
        state.attributes = rust_state.attributes
        state.last_changed = _parse_iso_datetime(rust_state.last_changed)
        state.last_updated = _parse_iso_datetime(rust_state.last_updated)
        state.last_updated_timestamp = state.last_updated.timestamp()
        state.last_reported = state.last_updated  # Rust doesn't track this separately yet
        state.context = RustContext()  # TODO: Get from Rust state
        state.state_info = None
        state._cache = {}
        state.domain, state.object_id = state.entity_id.split('.', 1)
        return state

    @classmethod
    def from_dict(cls, json_dict: dict[str, Any] | None) -> "RustState | None":
        """Create State from a dict (e.g., from JSON)."""
        if json_dict is None:
            return None

        # Validate required fields
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
        """Return compressed state dict."""
        result = {
            "s": self.state,
            "a": self.attributes,
            "c": self.context.id,
            "lc": self.last_changed_timestamp,
        }
        # Only include 'lu' if last_updated differs from last_changed
        if self.last_updated_timestamp != self.last_changed_timestamp:
            result["lu"] = self.last_updated_timestamp
        return result

    @property
    def as_compressed_state_json(self) -> bytes:
        """Return compressed state as JSON bytes."""
        import orjson
        if "as_compressed_state_json" not in self._cache:
            compressed = self.as_compressed_state
            self._cache["as_compressed_state_json"] = (
                b'"' + self.entity_id.encode() + b'":' +
                orjson.dumps(compressed)
            )
        return self._cache["as_compressed_state_json"]

    @property
    def as_dict_json(self) -> bytes:
        """Return state as JSON bytes."""
        import orjson
        if "as_dict_json" not in self._cache:
            # Need to use dict with correct key order for JSON
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
        """Return JSON fragment for serialization."""
        import orjson
        if "json_fragment" not in self._cache:
            self._cache["json_fragment"] = orjson.Fragment(
                orjson.dumps(self.as_dict())
            )
        return self._cache["json_fragment"]

    @property
    def last_changed_timestamp(self) -> float:
        """Return last changed as timestamp."""
        if "last_changed_timestamp" not in self._cache:
            # If last_changed equals last_updated, use the same timestamp
            # to avoid floating point precision differences
            if self.last_changed == self.last_updated:
                self._cache["last_changed_timestamp"] = self.last_updated_timestamp
            else:
                self._cache["last_changed_timestamp"] = self.last_changed.timestamp()
        return self._cache["last_changed_timestamp"]

    @property
    def last_reported_timestamp(self) -> float:
        """Return last reported as timestamp."""
        if "last_reported_timestamp" not in self._cache:
            self._cache["last_reported_timestamp"] = self.last_reported.timestamp()
        return self._cache["last_reported_timestamp"]

    @property
    def name(self) -> str:
        """Return friendly name or object_id."""
        return self.attributes.get('friendly_name') or self.object_id.replace('_', ' ')

    def as_dict(self) -> ReadOnlyDict:
        """Return state as a dict for JSON serialization."""
        if "as_dict" not in self._cache:
            # HA returns ReadOnlyDict with specific key order
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

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, RustState):
            return False
        return (
            self.entity_id == other.entity_id
            and self.state == other.state
            and self.attributes == other.attributes
        )

    def __repr__(self) -> str:
        # HA always shows +00:00 in repr, even if datetime has no timezone
        last_changed_str = self.last_changed.isoformat()
        if self.last_changed.tzinfo is None:
            last_changed_str += "+00:00"

        # Include brief attribute summary if attributes exist
        attrs_str = ""
        if self.attributes:
            attrs_str = "; " + ", ".join(f"{k}={v}" for k, v in self.attributes.items())

        return f"<state {self.entity_id}={self.state}{attrs_str} @ {last_changed_str}>"


# =============================================================================
# Rust-backed Context wrapper
# =============================================================================

class RustContext:
    """Wrapper that makes ha_core_rs.Context compatible with homeassistant.core.Context."""

    __slots__ = ("_id", "_user_id", "_parent_id", "_origin_event", "_cache")

    def __init__(
        self,
        id: str | None = None,  # noqa: A002 - match HA's API
        user_id: str | None = None,
        parent_id: str | None = None,
    ) -> None:
        """Initialize context."""
        if _rust_available and id is None:
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
        """Return JSON fragment for serialization."""
        import orjson
        if "json_fragment" not in self._cache:
            self._cache["json_fragment"] = orjson.Fragment(
                orjson.dumps(self.as_dict())
            )
        return self._cache["json_fragment"]

    @property
    def origin_event(self) -> Any:
        """Return origin event (always None for now)."""
        return self._origin_event

    @origin_event.setter
    def origin_event(self, value: Any) -> None:
        """Set origin event."""
        self._origin_event = value

    @property
    def parent_id(self) -> str | None:
        return self._parent_id

    @property
    def user_id(self) -> str | None:
        return self._user_id

    def as_dict(self) -> ReadOnlyDict:
        # Key order must match HA: id, parent_id, user_id
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
    """Wrapper that makes ha_core_rs.StateMachine compatible with homeassistant.core.StateMachine."""

    __slots__ = ("_rust_hass", "_bus", "_loop", "_states", "_reservations")

    def __init__(self, bus: "RustEventBus", loop: asyncio.AbstractEventLoop) -> None:
        """Initialize state machine."""
        self._rust_hass = ha_core_rs.HomeAssistant()
        self._bus = bus
        self._loop = loop
        self._states: dict[str, RustState] = {}
        self._reservations: set[str] = set()

    @property
    def _states_data(self) -> dict[str, RustState]:
        """Internal state storage."""
        return self._states

    def all(self, domain_filter: str | Iterable[str] | None = None) -> list[RustState]:
        """Return all states matching the filter."""
        entity_ids = self.async_entity_ids(domain_filter)
        return [self._states[eid] for eid in entity_ids if eid in self._states]

    def async_all(
        self, domain_filter: str | Iterable[str] | None = None
    ) -> list[RustState]:
        """Return all states matching the filter (async version)."""
        return self.all(domain_filter)

    def async_available(self, entity_id: str) -> bool:
        """Check if entity id is available."""
        entity_id = entity_id.lower()
        return entity_id not in self._states and entity_id not in self._reservations

    def async_entity_ids(
        self, domain_filter: str | Iterable[str] | None = None
    ) -> list[str]:
        """Return list of entity ids (async version)."""
        if domain_filter is None:
            return list(self._states.keys())
        if isinstance(domain_filter, str):
            return [eid for eid in self._states if eid.startswith(f"{domain_filter}.")]
        # Multiple domains
        domains = set(domain_filter)
        return [eid for eid in self._states if eid.split('.', 1)[0] in domains]

    def async_entity_ids_count(
        self, domain_filter: str | Iterable[str] | None = None
    ) -> int:
        """Return count of entity ids."""
        return len(self.async_entity_ids(domain_filter))

    def async_remove(self, entity_id: str, context: RustContext | None = None) -> bool:
        """Remove an entity (async version)."""
        entity_id = entity_id.lower()
        old_state = self._states.pop(entity_id, None)
        if old_state is not None:
            self._bus.async_fire(
                "state_changed",
                {"entity_id": entity_id, "old_state": old_state, "new_state": None},
                context=context,
            )
            return True
        return False

    def async_reserve(self, entity_id: str) -> None:
        """Reserve an entity id."""
        self._reservations.add(entity_id.lower())

    def async_set(
        self,
        entity_id: str,
        new_state: str,
        attributes: dict[str, Any] | None = None,
        force_update: bool = False,
        context: RustContext | None = None,
    ) -> None:
        """Set state of an entity (async version)."""
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
        """Set state of an entity (internal async version)."""
        entity_id = entity_id.lower()

        # Validate state length
        if len(new_state) > 255:
            raise ValueError(f"State max length exceeded: {len(new_state)} > 255")

        # Validate entity_id format
        if '.' not in entity_id:
            raise InvalidEntityFormatError(f"Invalid entity id: {entity_id}")

        old_state = self._states.get(entity_id)
        now = timestamp or datetime.now(timezone.utc)
        context = context or RustContext()

        # Determine if state actually changed
        same_attrs = False
        if old_state is not None:
            same_state = old_state.state == new_state
            same_attrs = old_state.attributes == (attributes or {})

            if same_state and same_attrs and not force_update:
                # Only update last_reported
                return

            if same_state and not force_update:
                # State same, only attributes changed - keep last_changed
                last_changed = old_state.last_changed
            else:
                last_changed = now
        else:
            last_changed = now

        # Reuse old attributes dict if unchanged (optimization for identity check)
        if same_attrs and old_state is not None:
            final_attrs = old_state.attributes
        else:
            final_attrs = attributes or {}

        # Create new state
        state = RustState(
            entity_id=entity_id,
            state=new_state,
            attributes=final_attrs,
            last_changed=last_changed,
            last_updated=now,
            last_reported=now,
            context=context,
        )

        self._states[entity_id] = state
        self._reservations.discard(entity_id)

        # Fire state_changed event
        self._bus.async_fire(
            "state_changed",
            {"entity_id": entity_id, "old_state": old_state, "new_state": state},
            context=context,
        )

    def entity_ids(self, domain_filter: str | None = None) -> list[str]:
        """Return list of entity ids."""
        if domain_filter:
            return [eid for eid in self._states if eid.startswith(f"{domain_filter}.")]
        return list(self._states.keys())

    def get(self, entity_id: str) -> RustState | None:
        """Get state for an entity."""
        return self._states.get(entity_id.lower())

    def is_state(self, entity_id: str, state: str) -> bool:
        """Check if entity is in a specific state."""
        current = self.get(entity_id)
        return current is not None and current.state == state

    def remove(self, entity_id: str) -> bool:
        """Remove an entity from the state machine."""
        entity_id = entity_id.lower()
        if entity_id in self._states:
            del self._states[entity_id]
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
        """Set state of an entity."""
        self.async_set(entity_id, new_state, attributes, force_update, context)


# =============================================================================
# Rust-backed EventBus wrapper
# =============================================================================

class RustEventBus:
    """Wrapper that makes ha_core_rs.EventBus compatible with homeassistant.core.EventBus."""

    __slots__ = ("_hass", "_listeners", "_loop", "_filters")

    def __init__(self, hass: Any) -> None:
        """Initialize event bus."""
        self._hass = hass
        self._listeners: dict[str, list[tuple[Callable, Callable | None]]] = {}
        self._filters: dict[str, list[Callable]] = {}  # event_filter functions
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
        """Fire an event (async version)."""
        self.async_fire_internal(event_type, event_data, origin, context, time_fired)

    def async_fire_internal(
        self,
        event_type: str,
        event_data: dict[str, Any] | None = None,
        origin: Any = None,
        context: RustContext | None = None,
        time_fired: float | None = None,
    ) -> None:
        """Fire an event (internal).

        Implements lazy object creation - Event/Context objects are only created
        if there's a listener that will actually receive the event.
        """
        import homeassistant.core as ha_core

        # Gather listeners for this event type and MATCH_ALL
        listeners_with_filters = list(self._listeners.get(event_type, []))
        listeners_with_filters.extend(self._listeners.get("*", []))  # MATCH_ALL

        if not listeners_with_filters:
            # No listeners at all - don't create any objects
            return

        # Check filters BEFORE creating Event object (lazy creation optimization)
        # First pass: check if ANY listener will receive this event
        event_data_dict = event_data or {}
        listeners_to_call = []

        for callback, event_filter in listeners_with_filters:
            if event_filter is not None:
                # Check filter with just the data (before creating Event)
                try:
                    if not event_filter(event_data_dict):
                        continue  # Filtered out
                except Exception:
                    continue
            listeners_to_call.append(callback)

        if not listeners_to_call:
            # All listeners filtered out - don't create Event object
            return

        # Now we know at least one listener will receive the event - create objects
        # Use timestamp like HA does
        time_fired_timestamp = time_fired if time_fired is not None else time.time()

        # Use the Event/Context classes from ha_core (so tests can mock them)
        event = ha_core.Event(
            event_type,
            event_data_dict,
            origin or EventOrigin.local,
            time_fired_timestamp,
            context,
        )

        # Call listeners that passed the filter
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
        """Listen for events (async version)."""
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
        """Listen for an event once (async version)."""
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
        """Return dict of event types and listener counts."""
        return {event_type: len(listeners) for event_type, listeners in self._listeners.items()}

    def fire(
        self,
        event_type: str,
        event_data: dict[str, Any] | None = None,
        origin: EventOrigin = EventOrigin.local,
        context: RustContext | None = None,
        time_fired: float | None = None,
    ) -> None:
        """Fire an event."""
        self.async_fire(event_type, event_data, origin, context, time_fired)

    def listen(
        self,
        event_type: str,
        listener: Callable,
    ) -> Callable[[], None]:
        """Listen for events."""
        return self.async_listen(event_type, listener)

    def listen_once(
        self,
        event_type: str,
        listener: Callable,
    ) -> Callable[[], None]:
        """Listen for an event once."""
        return self.async_listen_once(event_type, listener)

    def listeners(self) -> dict[str, int]:
        """Return dict of event types and listener counts."""
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
        """Initialize a new event."""
        self._cache: dict[str, Any] = {}
        self.event_type = event_type
        self.data = data or {}
        self.origin = origin
        self.time_fired_timestamp = time_fired_timestamp or time.time()

        if not context:
            # Use ha_core.Context dynamically so tests can mock it
            import homeassistant.core as ha_core
            if ulid_at_time is not None:
                context = ha_core.Context(id=ulid_at_time(self.time_fired_timestamp))
            else:
                context = ha_core.Context()
        self.context = context

        # Set origin_event on context if not already set
        if hasattr(context, 'origin_event') and not context.origin_event:
            context.origin_event = self

    @property
    def _as_dict(self) -> dict[str, Any]:
        """Create a dict representation (internal, cached)."""
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
        """Return JSON fragment for serialization."""
        import orjson
        if "json_fragment" not in self._cache:
            self._cache["json_fragment"] = orjson.Fragment(
                orjson.dumps(self._as_dict)
            )
        return self._cache["json_fragment"]

    @property
    def time_fired(self) -> datetime:
        """Return time fired as datetime."""
        if "time_fired" not in self._cache:
            if dt_util is not None:
                self._cache["time_fired"] = dt_util.utc_from_timestamp(self.time_fired_timestamp)
            else:
                self._cache["time_fired"] = datetime.fromtimestamp(
                    self.time_fired_timestamp, tz=timezone.utc
                )
        return self._cache["time_fired"]

    def as_dict(self) -> ReadOnlyDict:
        """Return a ReadOnlyDict representation of this Event."""
        if "_as_read_only_dict" not in self._cache:
            as_dict = self._as_dict
            # Wrap data and context in ReadOnlyDict if not already
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
        # Format: <Event event_type[origin_char]: key=value, key2=value2>
        origin_char = "L" if self.origin == EventOrigin.local else "R"
        if self.data:
            # Format data as key=value pairs like HA does
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
        """Initialize a service call."""
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
        """Return the representation of the service."""
        if self.data:
            # Format data as key=value pairs like HA's repr_helper
            data_str = ", ".join(f"{k}={v}" for k, v in self.data.items())
            return f"<ServiceCall {self.domain}.{self.service} (c:{self.context.id}): {data_str}>"
        return f"<ServiceCall {self.domain}.{self.service} (c:{self.context.id})>"


# =============================================================================
# Pytest hooks for patching
# =============================================================================

def pytest_configure(config):
    """Configure pytest with Rust patches."""
    if not USE_RUST_COMPONENTS or not _rust_available:
        return

    print("\n" + "=" * 60)
    print("  Running with RUST components patched in")
    print("=" * 60 + "\n")


@pytest.fixture(autouse=True)
def patch_ha_core():
    """Automatically patch HA core with Rust implementations."""
    if not USE_RUST_COMPONENTS or not _rust_available:
        yield
        return

    # Patch the core module
    import homeassistant.core as ha_core

    with patch.object(ha_core, 'State', RustState), \
         patch.object(ha_core, 'Context', RustContext), \
         patch.object(ha_core, 'Event', RustEvent), \
         patch.object(ha_core, 'ServiceCall', RustServiceCall):
        yield

    # Restore originals (handled by context manager)


# =============================================================================
# Fixtures
# =============================================================================

@pytest.fixture
def rust_hass():
    """Provide a pure Rust HomeAssistant instance for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")
    return ha_core_rs.HomeAssistant()


@pytest.fixture
def rust_state_machine():
    """Provide a Rust-backed StateMachine for testing."""
    if not _rust_available:
        pytest.skip("ha_core_rs not available")

    class MockBus:
        def async_fire(self, *args, **kwargs):
            pass

    return RustStateMachine(MockBus(), asyncio.get_event_loop())
