"""Core types for Home Assistant, backed by Rust.

This module provides the core types integrations expect:
- HomeAssistant: Main coordinator object
- callback: Decorator marking functions as safe to call from event loop
- Event: Event data container (Rust-backed)
- State: Entity state container (Rust-backed)
- Context: Request context for tracking causality (Rust-backed)

We also re-export native HA symbols that internal modules need.
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

# Try to import Rust implementations
try:
    from ha_core_rs import (
        Context as RustContext,
        Event as RustEvent,
        State as RustState,
        callback as rust_callback,
        split_entity_id,
        valid_entity_id,
    )
    _RUST_AVAILABLE = True
except ImportError:
    _RUST_AVAILABLE = False
    RustContext = None
    RustEvent = None
    RustState = None
    rust_callback = None
    split_entity_id = None
    valid_entity_id = None

# Re-export all type aliases and internal symbols that native modules expect
CALLBACK_TYPE = _native.CALLBACK_TYPE
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


# Use Rust Context if available, otherwise fall back to native
if _RUST_AVAILABLE and RustContext is not None:
    Context = RustContext
else:
    Context = _native.Context


# Use Rust State if available, otherwise fall back to native
if _RUST_AVAILABLE and RustState is not None:
    State = RustState
else:
    State = _native.State


# Use Rust Event if available, otherwise fall back to native
if _RUST_AVAILABLE and RustEvent is not None:
    Event = RustEvent
else:
    Event = _native.Event


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
    "Context",
    # Re-exported from native for internal modules
    "CALLBACK_TYPE",
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


# For debugging: indicate which implementation is being used
def _is_rust_backed() -> bool:
    """Return True if using Rust implementations."""
    return _RUST_AVAILABLE
