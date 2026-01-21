"""Merge native Home Assistant floor_registry with Rust implementations.

This shim:
1. Loads the full native floor_registry (constants, functions, classes)
2. Imports Rust classes from ha_core_rs.floor_registry
3. Re-exports both, with Rust classes taking precedence
"""

from homeassistant._native_loader import load_native_module

# Load native floor_registry module
_native = load_native_module("homeassistant.helpers.floor_registry")

# Re-export everything from native
_public_names = []
for _name in dir(_native):
    if _name.startswith("_"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

# Import Rust classes from ha_core_rs (they take precedence)
try:
    from ha_core_rs.floor_registry import FloorRegistry, FloorEntry

    globals()["FloorRegistry"] = FloorRegistry
    globals()["FloorEntry"] = FloorEntry
    if "FloorRegistry" not in _public_names:
        _public_names.append("FloorRegistry")
    if "FloorEntry" not in _public_names:
        _public_names.append("FloorEntry")
except ImportError:
    # ha_core_rs not available (e.g., in pure Python mode)
    pass

__all__ = _public_names
