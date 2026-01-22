"""Merge native Home Assistant label_registry with Rust implementations.

This shim:
1. Loads the full native label_registry (constants, functions, classes)
2. Imports Rust classes from ha_core_rs.label_registry
3. Re-exports both, with Rust classes taking precedence
"""

from homeassistant._native_loader import load_native_module

# Load native label_registry module
_native = load_native_module("homeassistant.helpers.label_registry")

# Re-export everything from native
_public_names = []
for _name in dir(_native):
    if _name.startswith("__") and _name.endswith("__"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

# Import Rust classes from ha_core_rs (they take precedence)
# Rust LabelRegistry now accepts hass like native HA does
try:
    from ha_core_rs import LabelRegistry, LabelEntry

    globals()["LabelRegistry"] = LabelRegistry
    globals()["LabelEntry"] = LabelEntry
    if "LabelRegistry" not in _public_names:
        _public_names.append("LabelRegistry")
    if "LabelEntry" not in _public_names:
        _public_names.append("LabelEntry")

    # Also patch the native module so async_get uses Rust
    _native.LabelRegistry = LabelRegistry
except ImportError:
    # ha_core_rs not available (e.g., in pure Python mode)
    pass

__all__ = _public_names
