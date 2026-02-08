"""Merge native Home Assistant storage with Rust implementations.

This shim:
1. Loads the full native storage (Store, async_migrator, etc.)
2. Imports Rust classes from ha_core_rs.storage
3. Re-exports both, with Rust classes taking precedence
"""

from homeassistant._native_loader import load_native_module

# Load native storage module
_native = load_native_module("homeassistant.helpers.storage")

# Re-export everything from native
_public_names = []
for _name in dir(_native):
    if _name.startswith("__") and _name.endswith("__"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

# Import Rust classes from ha_core_rs (they take precedence)
try:
    from ha_core_rs.storage import Storage

    globals()["Storage"] = Storage
    if "Storage" not in _public_names:
        _public_names.append("Storage")
except ImportError:
    # ha_core_rs not available (e.g., in pure Python mode)
    pass

__all__ = _public_names
