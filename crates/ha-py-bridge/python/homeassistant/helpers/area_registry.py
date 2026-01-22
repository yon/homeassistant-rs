"""Merge native Home Assistant area_registry with Rust implementations.

This shim:
1. Loads the full native area_registry (constants, functions, classes)
2. Imports Rust classes from ha_core_rs.area_registry
3. Re-exports both, with Rust classes taking precedence
"""

from homeassistant._native_loader import load_native_module

# Load native area_registry module
_native = load_native_module("homeassistant.helpers.area_registry")

# Re-export everything from native
_public_names = []
for _name in dir(_native):
    if _name.startswith("_"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

# Import Rust classes from ha_core_rs (they take precedence)
# Rust AreaRegistry now accepts hass like native HA does
try:
    from ha_core_rs import AreaRegistry, AreaEntry

    globals()["AreaRegistry"] = AreaRegistry
    globals()["AreaEntry"] = AreaEntry
    if "AreaRegistry" not in _public_names:
        _public_names.append("AreaRegistry")
    if "AreaEntry" not in _public_names:
        _public_names.append("AreaEntry")

    # Also patch the native module so async_get uses Rust
    _native.AreaRegistry = AreaRegistry
except ImportError:
    # ha_core_rs not available (e.g., in pure Python mode)
    pass

__all__ = _public_names
