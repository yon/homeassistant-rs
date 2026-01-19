"""Switch component shim - inherits from Python HA, uses RustStateMixin for state writes."""

from __future__ import annotations

from homeassistant._native_loader import load_native_module
from homeassistant.helpers.entity import RustStateMixin

# Load native HA switch module
_native = load_native_module("homeassistant.components.switch")


class SwitchEntity(RustStateMixin, _native.SwitchEntity):
    """Switch entity that routes state writes to Rust."""

    pass


# Re-export key types
SwitchDeviceClass = _native.SwitchDeviceClass
SwitchEntityDescription = _native.SwitchEntityDescription
DOMAIN = _native.DOMAIN

__all__ = [
    "SwitchEntity",
    "SwitchDeviceClass",
    "SwitchEntityDescription",
    "DOMAIN",
]

# Re-export any other public names
for _name in dir(_native):
    if not _name.startswith("_") and _name not in globals():
        globals()[_name] = getattr(_native, _name)
        __all__.append(_name)
