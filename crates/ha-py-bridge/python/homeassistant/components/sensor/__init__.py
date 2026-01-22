"""Sensor component shim - inherits from Python HA, uses RustStateMixin for state writes."""

from __future__ import annotations

from homeassistant._native_loader import load_native_module
from homeassistant.helpers.entity import RustStateMixin

# Load native HA sensor module
_native = load_native_module("homeassistant.components.sensor")

# Get the metaclass from the native class to avoid metaclass conflicts
# when the native module is loaded fresh with a new CachedProperties instance
_SensorEntityMeta = type(_native.SensorEntity)


class SensorEntity(RustStateMixin, _native.SensorEntity, metaclass=_SensorEntityMeta):
    """Sensor entity that routes state writes to Rust."""

    pass


# Re-export key types
SensorDeviceClass = _native.SensorDeviceClass
SensorEntityDescription = _native.SensorEntityDescription
SensorStateClass = _native.SensorStateClass
DOMAIN = _native.DOMAIN

__all__ = [
    "SensorEntity",
    "SensorDeviceClass",
    "SensorEntityDescription",
    "SensorStateClass",
    "DOMAIN",
]

# Re-export any other public names
for _name in dir(_native):
    if not _name.startswith("_") and _name not in globals():
        globals()[_name] = getattr(_native, _name)
        __all__.append(_name)
