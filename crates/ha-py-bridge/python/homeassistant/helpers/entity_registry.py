"""Merge native Home Assistant entity_registry with Rust implementations.

This shim:
1. Loads the full native entity_registry (constants, functions, classes)
2. Imports Rust classes from ha_core_rs.entity_registry
3. Re-exports both, with Rust classes taking precedence
"""

from homeassistant._native_loader import load_native_module

# Load native entity_registry module
_native = load_native_module("homeassistant.helpers.entity_registry")

# Re-export everything from native (including private names needed by tests)
_public_names = []
for _name in dir(_native):
    # Skip dunder methods and internal loader attributes
    if _name.startswith("__") and _name.endswith("__"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

# Import Rust classes from ha_core_rs (they take precedence)
# Rust EntityRegistry now accepts hass like native HA does
try:
    from ha_core_rs import EntityRegistry, EntityEntry

    globals()["EntityRegistry"] = EntityRegistry
    globals()["EntityEntry"] = EntityEntry
    if "EntityRegistry" not in _public_names:
        _public_names.append("EntityRegistry")
    if "EntityEntry" not in _public_names:
        _public_names.append("EntityEntry")

    # Also patch the native module so async_get uses Rust
    _native.EntityRegistry = EntityRegistry
except ImportError:
    # ha_core_rs not available (e.g., in pure Python mode)
    pass

__all__ = _public_names
