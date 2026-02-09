"""Merge native Home Assistant device_registry with Rust implementations.

This shim:
1. Loads the full native device_registry (constants, functions, classes)
2. Imports Rust classes from ha_core_rs (which accept hass like native HA)
3. Re-exports both, with Rust classes taking precedence
4. Patches devices property to return dict subclass with helper methods
"""

from homeassistant._native_loader import load_native_module

# Load native device_registry module (has EVENT_DEVICE_REGISTRY_UPDATED, etc.)
_native = load_native_module("homeassistant.helpers.device_registry")

# Re-export everything from native first
_public_names = []
for _name in dir(_native):
    if _name.startswith("__") and _name.endswith("__"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

# Import Rust classes from ha_core_rs (they take precedence)
# Rust DeviceRegistry now accepts hass like native HA does
try:
    from ha_core_rs import DeviceRegistry, DeviceEntry

    globals()["DeviceRegistry"] = DeviceRegistry
    globals()["DeviceEntry"] = DeviceEntry
    if "DeviceRegistry" not in _public_names:
        _public_names.append("DeviceRegistry")
    if "DeviceEntry" not in _public_names:
        _public_names.append("DeviceEntry")

    # Wrap async_update_device to return just DeviceEntry (not tuple)
    # The Rust method _async_update_device_with_changes returns (entry, changed_fields)
    # but Python HA expects just DeviceEntry | None
    _original_aud = DeviceRegistry._async_update_device_with_changes

    def _async_update_device_wrapper(self, device_id, **kwargs):
        result = _original_aud(self, device_id, **kwargs)
        if isinstance(result, tuple):
            return result[0]
        return result

    DeviceRegistry.async_update_device = _async_update_device_wrapper

    # Dict subclass that mimics ActiveDeviceRegistryItems with helper methods.
    # Python HA integrations call registry.devices.get_devices_for_config_entry_id()
    # which requires the devices property to return more than a plain dict.
    class _DeviceRegistryItems(dict):
        """Dict subclass with HA helper methods for device lookup."""

        def __init__(self, data, registry):
            super().__init__(data)
            self._registry = registry

        def get_devices_for_config_entry_id(self, config_entry_id):
            """Get devices for a config entry."""
            return self._registry.async_entries_for_config_entry(config_entry_id)

        def get_devices_for_area_id(self, area_id):
            """Get devices for an area."""
            return self._registry.async_entries_for_area(area_id)

    # Patch the devices property to return _DeviceRegistryItems
    _original_devices = DeviceRegistry.devices.fget if hasattr(DeviceRegistry.devices, 'fget') else None

    def _devices_wrapper(self):
        # Get the raw dict from the Rust getter
        raw = type(self).devices.fget(self) if _original_devices else {}
        return _DeviceRegistryItems(raw, self)

    # PyO3 exposes getters as descriptors; override with a Python property
    try:
        _rust_devices_getter = DeviceRegistry.devices.__get__
        def _devices_property(self):
            raw = _rust_devices_getter(self)
            return _DeviceRegistryItems(raw, self)
        DeviceRegistry.devices = property(_devices_property)
    except AttributeError:
        pass

    # Also patch the native module so async_get uses Rust
    _native.DeviceRegistry = DeviceRegistry
except ImportError:
    # ha_core_rs not available (e.g., in pure Python mode)
    pass

__all__ = _public_names
