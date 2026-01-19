"""Core types for Home Assistant, backed by Rust.

This module provides the core types integrations expect:
- HomeAssistant: Main coordinator object (Rust-backed proxy)
- callback: Decorator marking functions as safe to call from event loop
- Event: Event data container
- State: Entity state container

We also re-export native HA symbols that internal modules need. This ensures
that when native modules do `from homeassistant.core import CALLBACK_TYPE`,
they find the symbol in our shim.
"""

from __future__ import annotations

import asyncio
import logging
from collections.abc import Callable
from typing import Any, TypeVar

from homeassistant._native_loader import load_native_module

_LOGGER = logging.getLogger(__name__)

# Load native core module to re-export symbols that native modules need
_native = load_native_module("homeassistant.core")

# Re-export all type aliases and internal symbols that native modules expect
CALLBACK_TYPE = _native.CALLBACK_TYPE
Context = _native.Context
HassJob = _native.HassJob
HassJobType = _native.HassJobType
ReleaseChannel = _native.ReleaseChannel
get_hassjob_callable_job_type = _native.get_hassjob_callable_job_type
get_release_channel = _native.get_release_channel
CoreState = _native.CoreState
EventOrigin = _native.EventOrigin

# Type variable for callback decorator
_CallableT = TypeVar("_CallableT", bound=Callable[..., Any])


def callback(func: _CallableT) -> _CallableT:
    """Mark function as safe to call from within the event loop.

    This is a decorator that marks a function as safe to call synchronously
    from within the event loop. Functions marked with this decorator should
    complete quickly and not perform blocking I/O.
    """
    func._hass_callback = True  # type: ignore[attr-defined]
    return func


class Event:
    """Representation of an event within Home Assistant."""

    __slots__ = ("event_type", "data", "origin", "time_fired", "context")

    def __init__(
        self,
        event_type: str,
        data: dict[str, Any] | None = None,
        origin: str = "LOCAL",
        time_fired: Any = None,
        context: Any = None,
    ) -> None:
        """Initialize an event."""
        self.event_type = event_type
        self.data = data or {}
        self.origin = origin
        self.time_fired = time_fired
        self.context = context

    def as_dict(self) -> dict[str, Any]:
        """Return dictionary representation of event."""
        return {
            "event_type": self.event_type,
            "data": self.data,
            "origin": self.origin,
            "time_fired": self.time_fired,
            "context": self.context,
        }


class State:
    """Representation of an entity state."""

    __slots__ = (
        "entity_id",
        "state",
        "attributes",
        "last_changed",
        "last_reported",
        "last_updated",
        "context",
    )

    def __init__(
        self,
        entity_id: str,
        state: str,
        attributes: dict[str, Any] | None = None,
        last_changed: Any = None,
        last_reported: Any = None,
        last_updated: Any = None,
        context: Any = None,
    ) -> None:
        """Initialize a state."""
        self.entity_id = entity_id
        self.state = state
        self.attributes = attributes or {}
        self.last_changed = last_changed
        self.last_reported = last_reported
        self.last_updated = last_updated
        self.context = context

    @property
    def domain(self) -> str:
        """Return domain of entity."""
        return self.entity_id.split(".", 1)[0]

    @property
    def name(self) -> str | None:
        """Return friendly name of entity."""
        return self.attributes.get("friendly_name")

    @property
    def object_id(self) -> str:
        """Return object ID of entity."""
        return self.entity_id.split(".", 1)[1]

    def as_dict(self) -> dict[str, Any]:
        """Return dictionary representation of state."""
        return {
            "entity_id": self.entity_id,
            "state": self.state,
            "attributes": self.attributes,
            "last_changed": self.last_changed,
            "last_reported": self.last_reported,
            "last_updated": self.last_updated,
            "context": self.context,
        }


class HomeAssistant:
    """Root object of the Home Assistant system.

    This is a Rust-backed proxy. The actual state management is done in Rust,
    and this class provides the Python interface integrations expect.
    """

    def __init__(self, rust_hass: Any = None) -> None:
        """Initialize Home Assistant."""
        self._rust = rust_hass
        self._loop: asyncio.AbstractEventLoop | None = None

        # These will be set by the Rust bridge
        self.states: Any = None
        self.bus: Any = None
        self.services: Any = None
        self.config: Any = None
        self.config_entries: Any = None
        self.data: dict[str, Any] = {}

        if rust_hass is not None:
            # Wire up Rust-backed components
            self.states = getattr(rust_hass, "states", None)
            self.bus = getattr(rust_hass, "bus", None)
            self.services = getattr(rust_hass, "services", None)
            self.config = getattr(rust_hass, "config", None)
            self.config_entries = getattr(rust_hass, "config_entries", None)

    @property
    def loop(self) -> asyncio.AbstractEventLoop:
        """Return the event loop."""
        if self._loop is None:
            self._loop = asyncio.get_running_loop()
        return self._loop

    def __getattr__(self, name: str) -> Any:
        """Log and raise for unimplemented attributes.

        This enables discovery of what integrations need.
        """
        _LOGGER.warning(
            "UNIMPLEMENTED: HomeAssistant.%s accessed - needs Rust port",
            name,
        )
        raise NotImplementedError(
            f"HomeAssistant.{name} not yet ported to Rust"
        )


# Re-export common symbols that integrations might import from core
__all__ = [
    "HomeAssistant",
    "callback",
    "Event",
    "State",
    # Re-exported from native for internal modules
    "CALLBACK_TYPE",
    "Context",
    "CoreState",
    "EventOrigin",
    "HassJob",
    "HassJobType",
    "ReleaseChannel",
    "get_hassjob_callable_job_type",
    "get_release_channel",
]

# Re-export any other public symbols from native core that we haven't defined
# This catches symbols that native modules might need
for _name in dir(_native):
    if not _name.startswith("_") and _name not in globals():
        globals()[_name] = getattr(_native, _name)
        __all__.append(_name)
