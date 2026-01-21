"""Merge native Home Assistant device_registry with Rust implementations.

This shim:
1. Loads the full native device_registry (constants, functions, classes)
2. Imports Rust classes from ha_core_rs.device_registry
3. Re-exports both, with Rust classes taking precedence
"""

from homeassistant._native_loader import load_native_module

# Load native device_registry module (has EVENT_DEVICE_REGISTRY_UPDATED, etc.)
_native = load_native_module("homeassistant.helpers.device_registry")

# Re-export everything from native
_public_names = []
for _name in dir(_native):
    if _name.startswith("_"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

# Import Rust classes from ha_core_rs (they take precedence)
try:
    from ha_core_rs.device_registry import DeviceRegistry, DeviceEntry

    globals()["DeviceRegistry"] = DeviceRegistry
    globals()["DeviceEntry"] = DeviceEntry
    if "DeviceRegistry" not in _public_names:
        _public_names.append("DeviceRegistry")
    if "DeviceEntry" not in _public_names:
        _public_names.append("DeviceEntry")
except ImportError:
    # ha_core_rs not available (e.g., in pure Python mode)
    pass

__all__ = _public_names
