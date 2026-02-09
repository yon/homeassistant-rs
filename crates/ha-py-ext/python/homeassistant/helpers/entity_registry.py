"""Merge native Home Assistant entity_registry with Rust implementations.

This shim:
1. Loads the full native entity_registry (constants, functions, classes)
2. Imports Rust classes from ha_core_rs.entity_registry
3. Re-exports both, with Rust classes taking precedence
4. Patches entities property to return dict subclass with helper methods
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

    # Dict subclass that mimics EntityRegistryItems with helper methods.
    # Python HA integrations call registry.entities.get_entries_for_config_entry_id()
    # which requires the entities property to return more than a plain dict.
    class _EntityRegistryItems(dict):
        """Dict subclass with HA helper methods for entity lookup."""

        def __init__(self, data, registry):
            super().__init__(data)
            self._registry = registry

        def get_entries_for_config_entry_id(self, config_entry_id):
            """Get entities for a config entry."""
            return self._registry.async_entries_for_config_entry(config_entry_id)

        def get_entries_for_device_id(self, device_id, include_disabled_entities=False):
            """Get entities for a device."""
            entries = self._registry.async_entries_for_device(device_id)
            if not include_disabled_entities:
                entries = [e for e in entries if not getattr(e, 'disabled_by', None)]
            return entries

        def get_entry(self, key):
            """Get entry by entity_id."""
            return self.get(key)

    # Patch the entities property to return _EntityRegistryItems
    try:
        _rust_entities_getter = EntityRegistry.entities.__get__
        def _entities_property(self):
            raw = _rust_entities_getter(self)
            return _EntityRegistryItems(raw, self)
        EntityRegistry.entities = property(_entities_property)
    except AttributeError:
        pass

    # Also patch the native module so async_get uses Rust
    _native.EntityRegistry = EntityRegistry
except ImportError:
    # ha_core_rs not available (e.g., in pure Python mode)
    pass

__all__ = _public_names
