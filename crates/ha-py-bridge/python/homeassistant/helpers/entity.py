"""Entity base class - inherits from Python HA, overrides Rust-routed methods.

This module provides the Entity base class that all platform entities inherit from.
We inherit from native HA's Entity to get:
- All 25+ _attr_* cached properties with metaclass magic
- Property getters/setters that work automatically
- Future Python HA Entity changes inherited automatically

We only override methods that must route to Rust (async_write_ha_state).
"""

from __future__ import annotations

import logging
from typing import Any

from homeassistant._native_loader import load_native_module

_LOGGER = logging.getLogger(__name__)

# Load native HA entity module
_native = load_native_module("homeassistant.helpers.entity")


class RustStateMixin:
    """Mixin that routes state writes to Rust.

    This is a plain mixin with no base class, so it avoids metaclass conflicts
    when used with native HA entity classes that use CachedProperties metaclass.
    """

    hass: Any  # Type hint to satisfy mypy
    entity_id: str
    state: Any
    extra_state_attributes: dict[str, Any] | None
    name: str | None

    def async_write_ha_state(self) -> None:
        """Write state to Rust state machine instead of Python HA.

        This is the key method that routes state updates to our Rust core.
        """
        if self.hass is None:
            return

        # Get state and attributes
        try:
            state = self.state
        except Exception:  # noqa: BLE001
            _LOGGER.exception("Error getting state for %s", self.entity_id)
            return

        attributes = {}
        try:
            if self.extra_state_attributes:
                attributes.update(self.extra_state_attributes)
            # Add capability attributes
            if hasattr(self, "capability_attributes") and self.capability_attributes:
                attributes.update(self.capability_attributes)
            # Add state attributes from _attr_ properties
            if hasattr(self, "state_attributes") and self.state_attributes:
                attributes.update(self.state_attributes)
        except Exception:  # noqa: BLE001
            _LOGGER.exception("Error getting attributes for %s", self.entity_id)

        # Add common attributes
        if self.name:
            attributes["friendly_name"] = self.name

        # Route to Rust via PyO3 wrapper
        if hasattr(self.hass, "states") and self.hass.states is not None:
            try:
                self.hass.states.async_set(
                    self.entity_id,
                    state,
                    attributes,
                )
            except Exception:  # noqa: BLE001
                _LOGGER.exception("Error writing state for %s", self.entity_id)

    def _async_write_ha_state(self) -> None:
        """Internal state write - also routes to Rust."""
        self.async_write_ha_state()


class Entity(RustStateMixin, _native.Entity):
    """Entity that routes state writes to Rust.

    Inherits all _attr_* properties and metaclass magic from Python HA.
    The RustStateMixin provides the async_write_ha_state override.
    """

    pass


# Re-export other symbols from native module that integrations might need
DeviceInfo = _native.DeviceInfo
EntityCategory = _native.EntityCategory
EntityDescription = _native.EntityDescription

# Re-export any other public symbols
_public_names = [
    "Entity",
    "RustStateMixin",
    "DeviceInfo",
    "EntityCategory",
    "EntityDescription",
]

# Add any other public names from native that we haven't explicitly defined
for _name in dir(_native):
    if not _name.startswith("_") and _name not in globals():
        globals()[_name] = getattr(_native, _name)
        _public_names.append(_name)

__all__ = _public_names
