"""HA Python registry initialization.

Called from Rust via py.run_bound() with empty globals.
Defines _init_registries(hass, config_dir) which is extracted and called by Rust.
"""
import logging
import json
import os

_LOGGER = logging.getLogger(__name__)

def _init_registries(hass, config_dir):
    """Initialize HA Python registries, loading from disk if available.

    This sets up the entity_registry and device_registry so that
    EntityComponent and other HA code can use them. If config_dir is
    provided, loads saved registry data so entity_ids are preserved.
    """
    # First, set up the loader's data structures (required for async_get_integration to work)
    try:
        from homeassistant import loader
        if loader.DATA_COMPONENTS not in hass.data:
            hass.data[loader.DATA_COMPONENTS] = {}
        if loader.DATA_INTEGRATIONS not in hass.data:
            hass.data[loader.DATA_INTEGRATIONS] = {}
        if loader.DATA_MISSING_PLATFORMS not in hass.data:
            hass.data[loader.DATA_MISSING_PLATFORMS] = {}
        if loader.DATA_PRELOAD_PLATFORMS not in hass.data:
            hass.data[loader.DATA_PRELOAD_PLATFORMS] = loader.BASE_PRELOAD_PLATFORMS.copy()
        _LOGGER.debug("Set up loader data structures in hass.data")
    except Exception as e:
        _LOGGER.warning(f"Could not set up loader data structures: {e}")

    try:
        from homeassistant.helpers import entity_registry as er
        from homeassistant.helpers import device_registry as dr

        # Get or create entity registry
        entity_reg = er.EntityRegistry(hass)

        # Initialize the entities container
        entity_reg.entities = er.EntityRegistryItems()
        entity_reg.deleted_entities = {}

        # Try to load entity registry from disk
        if config_dir:
            entity_registry_path = os.path.join(config_dir, '.storage', 'core.entity_registry')
            if os.path.exists(entity_registry_path):
                try:
                    with open(entity_registry_path, 'r') as f:
                        data = json.load(f)

                    entities_data = data.get('data', {}).get('entities', [])
                    _LOGGER.info(f"Loading {len(entities_data)} entities from Python registry file")

                    for entry_data in entities_data:
                        try:
                            # Create RegistryEntry from saved data
                            entry = er.RegistryEntry(
                                entity_id=entry_data.get('entity_id'),
                                unique_id=entry_data.get('unique_id'),
                                platform=entry_data.get('platform'),
                                config_entry_id=entry_data.get('config_entry_id'),
                                config_subentry_id=entry_data.get('config_subentry_id'),
                                device_id=entry_data.get('device_id'),
                                area_id=entry_data.get('area_id'),
                                disabled_by=er.RegistryEntryDisabler(entry_data['disabled_by']) if entry_data.get('disabled_by') else None,
                                hidden_by=er.RegistryEntryHider(entry_data['hidden_by']) if entry_data.get('hidden_by') else None,
                                entity_category=entry_data.get('entity_category'),
                                capabilities=entry_data.get('capabilities'),
                                original_device_class=entry_data.get('original_device_class'),
                                original_icon=entry_data.get('original_icon'),
                                original_name=entry_data.get('original_name'),
                                name=entry_data.get('name'),
                                icon=entry_data.get('icon'),
                                aliases=set(entry_data.get('aliases', [])),
                                id=entry_data.get('id'),
                                has_entity_name=entry_data.get('has_entity_name', False),
                                options=entry_data.get('options'),
                                translation_key=entry_data.get('translation_key'),
                                categories=entry_data.get('categories', {}),
                                labels=set(entry_data.get('labels', [])),
                                created_at=entry_data.get('created_at', 0),
                                modified_at=entry_data.get('modified_at', 0),
                                suggested_object_id=entry_data.get('suggested_object_id'),
                                supported_features=entry_data.get('supported_features', 0),
                                unit_of_measurement=entry_data.get('unit_of_measurement'),
                            )
                            entity_reg.entities[entry.entity_id] = entry
                        except Exception as e:
                            _LOGGER.debug(f"Could not load entity entry: {e}")

                    _LOGGER.info(f"Loaded {len(entity_reg.entities)} entities into Python registry")
                except Exception as e:
                    _LOGGER.warning(f"Could not load entity registry from disk: {e}")

        entity_reg._entities_data = entity_reg.entities.data

        # Store in hass.data with the expected key
        hass.data[er.DATA_REGISTRY] = entity_reg
        _LOGGER.debug("Initialized entity registry in hass.data")

        # Get or create device registry
        device_reg = dr.DeviceRegistry(hass)

        # Initialize the devices container
        device_reg.devices = dr.ActiveDeviceRegistryItems()
        device_reg.deleted_devices = {}

        # Try to load device registry from disk
        if config_dir:
            device_registry_path = os.path.join(config_dir, '.storage', 'core.device_registry')
            if os.path.exists(device_registry_path):
                try:
                    with open(device_registry_path, 'r') as f:
                        data = json.load(f)

                    devices_data = data.get('data', {}).get('devices', [])
                    _LOGGER.info(f"Loading {len(devices_data)} devices from registry")

                    for dev_data in devices_data:
                        try:
                            # Parse identifiers and connections
                            identifiers = set()
                            for id_tuple in dev_data.get('identifiers', []):
                                if isinstance(id_tuple, (list, tuple)) and len(id_tuple) >= 2:
                                    identifiers.add((str(id_tuple[0]), str(id_tuple[1])))

                            connections = set()
                            for conn in dev_data.get('connections', []):
                                if isinstance(conn, (list, tuple)) and len(conn) >= 2:
                                    connections.add((str(conn[0]), str(conn[1])))

                            entry = dr.DeviceEntry(
                                area_id=dev_data.get('area_id'),
                                config_entries=set(dev_data.get('config_entries', [])),
                                connections=connections,
                                disabled_by=dr.DeviceEntryDisabler(dev_data['disabled_by']) if dev_data.get('disabled_by') else None,
                                hw_version=dev_data.get('hw_version'),
                                id=dev_data.get('id'),
                                identifiers=identifiers,
                                labels=set(dev_data.get('labels', [])),
                                manufacturer=dev_data.get('manufacturer'),
                                model=dev_data.get('model'),
                                model_id=dev_data.get('model_id'),
                                name=dev_data.get('name'),
                                name_by_user=dev_data.get('name_by_user'),
                                serial_number=dev_data.get('serial_number'),
                                sw_version=dev_data.get('sw_version'),
                                via_device_id=dev_data.get('via_device_id'),
                            )
                            device_reg.devices[entry.id] = entry
                        except Exception as e:
                            _LOGGER.debug(f"Could not load device entry: {e}")

                    _LOGGER.info(f"Loaded {len(device_reg.devices)} devices from disk")
                except Exception as e:
                    _LOGGER.warning(f"Could not load device registry from disk: {e}")

        device_reg._device_data = device_reg.devices.data

        # Store in hass.data with the expected key
        hass.data[dr.DATA_REGISTRY] = device_reg
        _LOGGER.debug("Initialized device registry in hass.data")

        return True
    except Exception as e:
        _LOGGER.warning(f"Could not initialize HA registries: {e}")
        import traceback
        traceback.print_exc()
        return False
