"""Config entries wrapper with platform setup methods.

Called from Rust via py.run_bound() with CONFIG_ENTRIES_GLOBALS persistent dict.
Provides: async_forward_entry_setups, async_unload_platforms, set_hass,
          get_entity, get_all_entities, get_all_devices, _call_entity_service,
          async_entries, async_entry_for_domain_unique_id, and entity lifecycle.

Rust injects _registries (RegistriesWrapper pyclass) into globals after init.
"""
import logging
import asyncio
import importlib
from datetime import datetime, timezone

# Import UNDEFINED sentinel to filter out undefined values
try:
    from homeassistant.const import UNDEFINED
    _UNDEFINED = UNDEFINED
except ImportError:
    # Fallback for older HA versions
    try:
        from homeassistant.helpers.typing import UNDEFINED
        _UNDEFINED = UNDEFINED
    except ImportError:
        _UNDEFINED = None

def _is_undefined(value):
    """Check if a value is the UNDEFINED sentinel."""
    if _UNDEFINED is None:
        # Check by string representation as fallback
        return 'UndefinedType' in str(type(value)) or str(value) == 'UndefinedType._singleton'
    return value is _UNDEFINED

def _get_value_or_none(value):
    """Return the value if it's not UNDEFINED, otherwise return None."""
    if _is_undefined(value):
        return None
    return value

_LOGGER = logging.getLogger(__name__)

# Store for loaded platforms per entry
_loaded_platforms = {}

# Store reference to hass (set by integration.py when calling setup)
_hass = None

# Global entity registry: entity_id -> entity instance
_entity_registry = {}

# Global device registry: device_id -> device_info dict
_device_registry = {}

# Track which domains have registered services
_registered_service_domains = set()

# Global config entry registry: entry_id -> config entry object
_config_entries = {}

def set_hass(hass):
    """Store the hass reference for platform setup."""
    global _hass
    _hass = hass

def get_entity(entity_id):
    """Get an entity instance by entity_id."""
    return _entity_registry.get(entity_id)

def get_all_entities():
    """Get all registered entities."""
    return dict(_entity_registry)

def get_all_devices():
    """Get all registered devices."""
    return dict(_device_registry)

async def _call_entity_service(entity_id, service, **kwargs):
    """Call a service method on an entity."""
    entity = _entity_registry.get(entity_id)
    if entity is None:
        _LOGGER.warning(f"Entity not found: {entity_id}")
        return False

    # Map service names to method names
    method_name = f'async_{service}'
    if not hasattr(entity, method_name):
        # Try without async_ prefix
        method_name = service
        if not hasattr(entity, method_name):
            _LOGGER.warning(f"Entity {entity_id} has no method {service}")
            return False

    try:
        method = getattr(entity, method_name)
        if asyncio.iscoroutinefunction(method):
            await method(**kwargs)
        else:
            method(**kwargs)

        # Update state after service call
        await _update_entity_state(entity)
        return True
    except Exception as e:
        _LOGGER.error(f"Error calling {service} on {entity_id}: {e}")
        return False

async def _update_entity_state(entity):
    """Update the state of an entity in the state machine."""
    global _hass
    if _hass is None or not hasattr(entity, 'entity_id'):
        return

    entity_id = entity.entity_id
    domain = entity_id.split('.')[0]

    # Get current state - check domain first to use correct attribute
    state = None

    # Sensor/number entities use native_value
    if domain in ('sensor', 'number'):
        if hasattr(entity, '_attr_native_value'):
            val = entity._attr_native_value
            state = str(val) if val is not None else 'unknown'
        elif hasattr(entity, 'native_value'):
            try:
                val = entity.native_value
                state = str(val) if val is not None else 'unknown'
            except:
                state = 'unknown'
        else:
            state = 'unknown'
    # Toggle entities (light, switch, fan, etc.) use is_on
    elif domain in ('light', 'switch', 'fan', 'siren', 'humidifier', 'binary_sensor'):
        if hasattr(entity, '_attr_is_on'):
            state = 'on' if entity._attr_is_on else 'off'
        elif hasattr(entity, 'is_on'):
            try:
                is_on = entity.is_on
                state = 'on' if is_on else 'off'
            except:
                state = 'off'
        else:
            state = 'off'
    # Select entities use current_option
    elif domain == 'select':
        if hasattr(entity, '_attr_current_option'):
            state = str(entity._attr_current_option) if entity._attr_current_option else 'unknown'
        elif hasattr(entity, 'current_option'):
            try:
                val = entity.current_option
                state = str(val) if val is not None else 'unknown'
            except:
                state = 'unknown'
        else:
            state = 'unknown'
    # Lock entities use is_locked
    elif domain == 'lock':
        if hasattr(entity, '_attr_is_locked'):
            state = 'locked' if entity._attr_is_locked else 'unlocked'
        elif hasattr(entity, 'is_locked'):
            try:
                is_locked = entity.is_locked
                state = 'locked' if is_locked else 'unlocked'
            except:
                state = 'unknown'
        else:
            state = 'unknown'
    # Cover entities use is_closed
    elif domain == 'cover':
        if hasattr(entity, '_attr_is_closed'):
            is_closed = entity._attr_is_closed
            if is_closed is None:
                state = 'unknown'
            else:
                state = 'closed' if is_closed else 'open'
        elif hasattr(entity, 'is_closed'):
            try:
                is_closed = entity.is_closed
                if is_closed is None:
                    state = 'unknown'
                else:
                    state = 'closed' if is_closed else 'open'
            except:
                state = 'unknown'
        else:
            state = 'unknown'
    # Other domains - try state property
    else:
        if hasattr(entity, '_attr_state'):
            state = str(entity._attr_state) if entity._attr_state is not None else 'unknown'
        elif hasattr(entity, 'state'):
            try:
                val = entity.state
                state = str(val) if val is not None else 'unknown'
            except:
                state = 'unknown'
        else:
            state = 'unknown'

    # Get attributes
    attributes = {}
    if hasattr(entity, '_attr_brightness') and entity._attr_brightness is not None:
        attributes['brightness'] = entity._attr_brightness
    if hasattr(entity, '_attr_color_mode') and entity._attr_color_mode is not None:
        cm = entity._attr_color_mode
        attributes['color_mode'] = cm.value if hasattr(cm, 'value') else str(cm)

    # Update state
    if hasattr(_hass, 'states') and hasattr(_hass.states, 'set'):
        _hass.states.set(entity_id, state, attributes)
        _LOGGER.debug(f"Updated state: {entity_id} = {state}")

def _register_domain_services(hass, domain):
    """Register standard services for an entity domain."""
    global _registered_service_domains

    if domain in _registered_service_domains:
        return
    _registered_service_domains.add(domain)

    # Define services per domain
    domain_services = {
        'light': ['turn_on', 'turn_off', 'toggle'],
        'switch': ['turn_on', 'turn_off', 'toggle'],
        'fan': ['turn_on', 'turn_off', 'toggle', 'set_percentage', 'set_preset_mode'],
        'cover': ['open_cover', 'close_cover', 'stop_cover', 'set_cover_position'],
        'lock': ['lock', 'unlock', 'open'],
        'climate': ['set_temperature', 'set_hvac_mode', 'set_preset_mode'],
        'media_player': ['turn_on', 'turn_off', 'play_media', 'media_play', 'media_pause', 'media_stop'],
        'vacuum': ['start', 'stop', 'pause', 'return_to_base'],
        'button': ['press'],
        'number': ['set_value'],
        'select': ['select_option'],
        'humidifier': ['turn_on', 'turn_off', 'set_humidity', 'set_mode'],
        'siren': ['turn_on', 'turn_off'],
        'valve': ['open_valve', 'close_valve'],
        'water_heater': ['set_temperature', 'set_operation_mode'],
        'alarm_control_panel': ['alarm_arm_home', 'alarm_arm_away', 'alarm_disarm', 'alarm_trigger'],
    }

    services = domain_services.get(domain, [])

    for service in services:
        _LOGGER.info(f"Registering service: {domain}.{service}")
        # Store service info for Rust to query
        # The actual dispatch happens via _call_entity_service

def _generate_entity_id(domain, platform, suggested_id, existing_ids):
    """Generate a unique entity ID."""
    import re

    if suggested_id:
        # Clean up the suggested_id - strip config entry ID prefix if present
        # Config entry IDs look like: 64da23b80e7c7deaf579d5b3f5e9e201
        # Pattern: hex string (32 chars) followed by underscore
        clean_id = re.sub(r'^[a-f0-9]{32}_', '', suggested_id)

        # If we stripped something, use the cleaner version
        # Otherwise use the original
        final_id = clean_id if clean_id != suggested_id else suggested_id

        # Replace device-specific prefixes that are too long
        # e.g., "my_integration_device_name_temperature" -> "temperature"
        # Keep it reasonable for sun which has "solar_rising", "next_dawn", etc.
        base_id = f"{domain}.{final_id}"
    else:
        base_id = f"{domain}.{platform}_entity"

    entity_id = base_id
    counter = 1
    while entity_id in existing_ids:
        entity_id = f"{base_id}_{counter}"
        counter += 1
    return entity_id

async def _call_entity_lifecycle(hass, entity, entity_id):
    """Call async_added_to_hass and update state after it completes."""
    try:
        if hasattr(entity, 'async_added_to_hass'):
            await entity.async_added_to_hass()
            _LOGGER.debug(f" async_added_to_hass completed for {entity_id}")

            # After lifecycle method completes, re-read and update state
            _update_entity_state_after_lifecycle(hass, entity, entity_id)
    except Exception as e:
        _LOGGER.debug(f" Error in entity lifecycle for {entity_id}: {e}")
        import traceback
        traceback.print_exc()

def _update_entity_state_after_lifecycle(hass, entity, entity_id):
    """Update entity state in state machine after lifecycle methods complete."""
    domain = entity_id.split('.')[0]

    # Get state - entities may have computed values now
    state = None
    if hasattr(entity, 'state'):
        try:
            state = entity.state
            if state is not None:
                state = str(state)
        except Exception:
            pass

    if state is None:
        if hasattr(entity, '_attr_native_value'):
            val = entity._attr_native_value
            state = str(val) if val is not None else 'unknown'
        elif hasattr(entity, 'native_value'):
            try:
                val = entity.native_value
                state = str(val) if val is not None else 'unknown'
            except Exception:
                state = 'unknown'
        elif hasattr(entity, '_attr_is_on'):
            state = 'on' if entity._attr_is_on else 'off'
        elif hasattr(entity, 'is_on'):
            try:
                state = 'on' if entity.is_on else 'off'
            except Exception:
                state = 'unknown'
        else:
            state = 'unknown'

    # Get attributes
    attributes = {}

    # Get extra_state_attributes if available (e.g., Sun entity has sunrise/sunset times)
    if hasattr(entity, 'extra_state_attributes'):
        try:
            extra = entity.extra_state_attributes
            if extra:
                for k, v in extra.items():
                    if v is not None and not _is_undefined(v):
                        # Convert datetime to ISO string
                        if hasattr(v, 'isoformat'):
                            attributes[k] = v.isoformat()
                        else:
                            attributes[k] = v
        except Exception as e:
            _LOGGER.debug(f"Error getting extra_state_attributes: {e}")

    # Get friendly name - follows HA's _friendly_name_internal() logic
    # For has_entity_name=True: friendly_name = "{device_name} {entity_name}"
    friendly_name = None

    # Try to get original_name from entity registry
    if hass and hasattr(hass, 'data'):
        try:
            from homeassistant.helpers import entity_registry as er
            if er.DATA_REGISTRY in hass.data:
                entity_reg = hass.data[er.DATA_REGISTRY]
                if entity_id in entity_reg.entities:
                    reg_entry = entity_reg.entities[entity_id]
                    entity_name = reg_entry.original_name
                    if entity_name:
                        # For has_entity_name=True, combine device name + entity name
                        if reg_entry.has_entity_name:
                            device_info = getattr(entity, '_attr_device_info', None)
                            if device_info is None:
                                try:
                                    device_info = getattr(entity, 'device_info', None)
                                except:
                                    pass
                            if device_info:
                                dev_name = None
                                if hasattr(device_info, 'get'):
                                    dev_name = _get_value_or_none(device_info.get('name'))
                                elif hasattr(device_info, 'name'):
                                    dev_name = _get_value_or_none(device_info.name)
                                if dev_name:
                                    friendly_name = f"{dev_name} {entity_name}"
                                else:
                                    friendly_name = entity_name
                        else:
                            friendly_name = entity_name
        except Exception as e:
            pass  # Fall back to entity attribute

    # Fall back to entity's name attribute
    if not friendly_name:
        name = _get_value_or_none(getattr(entity, '_attr_name', None))
        if name is None:
            try:
                name = _get_value_or_none(getattr(entity, 'name', None))
            except Exception:
                pass
        if name and not _is_undefined(name):
            friendly_name = str(name)

    if friendly_name:
        attributes['friendly_name'] = friendly_name

    # Get device class - try _attr_ first, then property, then entity_description
    device_class = _get_value_or_none(getattr(entity, '_attr_device_class', None))
    if device_class is None:
        try:
            device_class = _get_value_or_none(getattr(entity, 'device_class', None))
        except Exception:
            pass
        # Fall back to entity_description.device_class if property returned None
        if device_class is None and hasattr(entity, 'entity_description'):
            ed = getattr(entity, 'entity_description', None)
            if ed is not None:
                ed_dc = getattr(ed, 'device_class', None)
                if ed_dc is not None and not _is_undefined(ed_dc):
                    device_class = ed_dc
    if device_class and not _is_undefined(device_class):
        if hasattr(device_class, 'value'):
            device_class = device_class.value
        attributes['device_class'] = str(device_class)

    # Get unit of measurement
    unit = _get_value_or_none(getattr(entity, '_attr_native_unit_of_measurement', None))
    if unit is None:
        try:
            unit = _get_value_or_none(getattr(entity, 'native_unit_of_measurement', None))
        except Exception:
            pass
    if unit and not _is_undefined(unit):
        attributes['unit_of_measurement'] = str(unit)

    # Update state in state machine
    _LOGGER.debug(f" Updating state after lifecycle: {entity_id} = {state}")
    if hasattr(hass, 'states') and hasattr(hass.states, 'set'):
        try:
            hass.states.set(entity_id, state, attributes)
        except Exception as e:
            _LOGGER.debug(f" Error setting state: {e}")

def _create_add_entities_callback(hass, entry, platform_name):
    """Create the async_add_entities callback for a platform.

    This callback is called by the platform's async_setup_entry to add entities.
    We extract entity state and attributes and set them in the state machine.
    """
    existing_ids = set()

    # Import PlatformData to set on entities before accessing properties
    from homeassistant.helpers.entity_platform import PlatformData

    # List to track entities that need lifecycle calls
    _pending_lifecycle = []

    def add_entities(entities, update_before_add=False, config_subentry_id=None):
        """Add entities to Home Assistant."""
        entities = list(entities)  # Convert to list so we can iterate multiple times
        for entity in entities:
            try:
                # Get domain from the entity class or default to platform
                domain = getattr(entity, 'platform', None)
                if domain is None:
                    # Try to infer from class name (e.g., LightEntity -> light)
                    class_name = entity.__class__.__name__
                    if 'Light' in class_name:
                        domain = 'light'
                    elif 'Sensor' in class_name:
                        domain = 'sensor'
                    elif 'Switch' in class_name:
                        domain = 'switch'
                    elif 'Binary' in class_name:
                        domain = 'binary_sensor'
                    elif 'Climate' in class_name:
                        domain = 'climate'
                    elif 'Cover' in class_name:
                        domain = 'cover'
                    elif 'Fan' in class_name:
                        domain = 'fan'
                    elif 'Lock' in class_name:
                        domain = 'lock'
                    elif 'Media' in class_name:
                        domain = 'media_player'
                    elif 'Vacuum' in class_name:
                        domain = 'vacuum'
                    elif 'Camera' in class_name:
                        domain = 'camera'
                    elif 'Alarm' in class_name:
                        domain = 'alarm_control_panel'
                    elif 'Weather' in class_name:
                        domain = 'weather'
                    elif 'Number' in class_name:
                        domain = 'number'
                    elif 'Select' in class_name:
                        domain = 'select'
                    elif 'Button' in class_name:
                        domain = 'button'
                    else:
                        domain = platform_name

                # Get the integration domain from the config entry (e.g., "airthings", "unifi")
                # This is different from entity domain (e.g., "sensor", "light")
                integration_domain = entry.get("domain") if isinstance(entry, dict) else getattr(entry, "domain", None)

                # Set platform_data on entity BEFORE accessing properties
                # This is required for entities with translation keys
                if not hasattr(entity, 'platform_data') or entity.platform_data is None:
                    try:
                        # platform_name should be the integration domain, not entity domain
                        platform_data = PlatformData(hass, domain=domain, platform_name=integration_domain or platform_name)
                        entity.platform_data = platform_data
                    except Exception as e:
                        _LOGGER.debug(f"Could not set platform_data: {e}")

                # Get entity unique_id - required for registry lookup
                unique_id = getattr(entity, '_attr_unique_id', None) or getattr(entity, 'unique_id', None)
                if unique_id is None:
                    # Generate a fallback unique_id
                    unique_id = f"{integration_domain}_{len(existing_ids)}"
                unique_id = str(unique_id)

                # Set hass reference on entity (required for service calls)
                entity.hass = hass

                # Extract device_info and register device in Rust registry
                device_id = None
                device_info = getattr(entity, '_attr_device_info', None)
                if device_info is None:
                    try:
                        device_info = getattr(entity, 'device_info', None)
                    except:
                        pass
                if device_info:
                    # Extract device identifiers
                    identifiers = []
                    raw_identifiers = None
                    if hasattr(device_info, 'identifiers'):
                        raw_identifiers = device_info.identifiers
                    elif isinstance(device_info, dict):
                        raw_identifiers = device_info.get('identifiers')

                    if raw_identifiers:
                        for id_tuple in raw_identifiers:
                            if isinstance(id_tuple, (tuple, list)) and len(id_tuple) >= 2:
                                identifiers.append((str(id_tuple[0]), str(id_tuple[1])))

                    # Extract connections (e.g., MAC addresses)
                    connections = []
                    raw_connections = None
                    if hasattr(device_info, 'connections'):
                        raw_connections = device_info.connections
                    elif isinstance(device_info, dict):
                        raw_connections = device_info.get('connections')

                    if raw_connections:
                        for conn in raw_connections:
                            if isinstance(conn, (tuple, list)) and len(conn) >= 2:
                                connections.append((str(conn[0]), str(conn[1])))

                    # Extract device info fields
                    def get_field(obj, field):
                        if hasattr(obj, field):
                            return getattr(obj, field)
                        elif isinstance(obj, dict):
                            return obj.get(field)
                        return None

                    dev_name = get_field(device_info, 'name') or 'Unknown Device'
                    dev_manufacturer = get_field(device_info, 'manufacturer')
                    dev_model = get_field(device_info, 'model')
                    dev_sw_version = get_field(device_info, 'sw_version')
                    dev_hw_version = get_field(device_info, 'hw_version')

                    # Convert to strings if not None
                    dev_name = str(dev_name) if dev_name else 'Unknown Device'
                    dev_manufacturer = str(dev_manufacturer) if dev_manufacturer else None
                    dev_model = str(dev_model) if dev_model else None
                    dev_sw_version = str(dev_sw_version) if dev_sw_version else None
                    dev_hw_version = str(dev_hw_version) if dev_hw_version else None

                    # Register device in Rust registry if we have identifiers
                    if identifiers and _registries is not None:
                        try:
                            config_entry_id = entry.get("entry_id") if isinstance(entry, dict) else getattr(entry, "entry_id", "unknown")
                            device_id = _registries.register_device(
                                config_entry_id,
                                identifiers,
                                connections,
                                dev_name,
                                manufacturer=dev_manufacturer,
                                model=dev_model,
                                sw_version=dev_sw_version,
                                hw_version=dev_hw_version,
                            )
                            _LOGGER.debug(f"Registered device in Rust registry: {device_id} = {dev_name}")
                        except Exception as e:
                            _LOGGER.error(f"Failed to register device in Rust: {e}")
                            # Fall back to Python-only storage
                            device_id = f"{identifiers[0][0]}_{identifiers[0][1]}" if identifiers else None

                    # Also store in Python registry for backward compatibility
                    if identifiers:
                        py_device_id = f"{identifiers[0][0]}_{identifiers[0][1]}"
                        if py_device_id not in _device_registry:
                            _device_registry[py_device_id] = {
                                'name': dev_name,
                                'manufacturer': dev_manufacturer,
                                'model': dev_model,
                                'identifiers': identifiers,
                            }

                # Register entity in Rust registry - this looks up existing entity_id or generates new one
                entity_id = None
                if _registries is not None:
                    try:
                        config_entry_id = entry.get("entry_id") if isinstance(entry, dict) else getattr(entry, "entry_id", None)
                        entity_name = _get_value_or_none(getattr(entity, '_attr_name', None))
                        if entity_name is None:
                            try:
                                entity_name = _get_value_or_none(getattr(entity, 'name', None))
                            except:
                                pass
                        # Get device_class for proper frontend icons
                        # Try _attr_device_class first (direct attribute)
                        device_class = _get_value_or_none(getattr(entity, '_attr_device_class', None))
                        if device_class is None:
                            # Try the device_class property (which may read from entity_description)
                            try:
                                device_class = _get_value_or_none(getattr(entity, 'device_class', None))
                            except Exception:
                                pass
                            # If property returned None but entity_description has device_class, use that
                            if device_class is None and hasattr(entity, 'entity_description'):
                                ed = getattr(entity, 'entity_description', None)
                                if ed is not None:
                                    ed_dc = getattr(ed, 'device_class', None)
                                    if ed_dc is not None and not _is_undefined(ed_dc):
                                        device_class = ed_dc
                        # Get suggested object_id from entity name
                        suggested_object_id = None
                        if entity_name and not _is_undefined(entity_name):
                            suggested_object_id = str(entity_name).lower().replace(' ', '_').replace('-', '_')
                        elif unique_id:
                            suggested_object_id = str(unique_id).lower().replace(' ', '_').replace('-', '_')

                        # Call Rust registry - it looks up existing entity_id or generates new one
                        result = _registries.register_entity(
                            domain,  # Entity domain (e.g., "sensor", "light")
                            integration_domain or platform_name,  # Platform (e.g., "airthings")
                            unique_id,  # Unique ID for lookup
                            suggested_object_id=suggested_object_id,
                            config_entry_id=config_entry_id,
                            device_id=device_id,
                            name=str(entity_name) if entity_name and not _is_undefined(entity_name) else None,
                            original_device_class=str(device_class) if device_class and not _is_undefined(device_class) else None,
                        )
                        # Get the resolved entity_id from Rust
                        entity_id = result.get('entity_id')
                        _LOGGER.debug(f"Registered entity: {entity_id} (unique_id={unique_id})")
                    except Exception as e:
                        _LOGGER.error(f"Failed to register entity in Rust: {e}")

                # Fall back to generated entity_id if Rust registration failed
                if entity_id is None:
                    suggested_id = unique_id or 'entity'
                    suggested_id = str(suggested_id).lower().replace(' ', '_').replace('-', '_')
                    entity_id = _generate_entity_id(domain, platform_name, suggested_id, existing_ids)

                existing_ids.add(entity_id)

                # Store the entity_id on the entity
                entity.entity_id = entity_id

                # Store entity in registry for service dispatch
                _entity_registry[entity_id] = entity

                # Register domain services if not already done
                _register_domain_services(hass, domain)

                # Get entity state - check domain first to use correct attribute
                state = None

                # Sensor/number entities use native_value
                if domain in ('sensor', 'number'):
                    if hasattr(entity, '_attr_native_value'):
                        val = entity._attr_native_value
                        state = str(val) if val is not None else 'unknown'
                    elif hasattr(entity, 'native_value'):
                        try:
                            val = entity.native_value
                            if callable(val):
                                val = val()
                            state = str(val) if val is not None else 'unknown'
                        except:
                            state = 'unknown'
                    else:
                        state = 'unknown'
                # Toggle entities (light, switch, fan, etc.) use is_on
                elif domain in ('light', 'switch', 'fan', 'siren', 'humidifier', 'binary_sensor'):
                    if hasattr(entity, '_attr_is_on'):
                        state = 'on' if entity._attr_is_on else 'off'
                    elif hasattr(entity, 'is_on'):
                        try:
                            is_on = entity.is_on
                            if callable(is_on):
                                is_on = is_on()
                            state = 'on' if is_on else 'off'
                        except:
                            state = 'off'
                    else:
                        state = 'off'
                # Select entities use current_option
                elif domain == 'select':
                    if hasattr(entity, '_attr_current_option'):
                        state = str(entity._attr_current_option) if entity._attr_current_option else 'unknown'
                    elif hasattr(entity, 'current_option'):
                        try:
                            val = entity.current_option
                            state = str(val) if val is not None else 'unknown'
                        except:
                            state = 'unknown'
                    else:
                        state = 'unknown'
                # Lock entities use is_locked
                elif domain == 'lock':
                    if hasattr(entity, '_attr_is_locked'):
                        state = 'locked' if entity._attr_is_locked else 'unlocked'
                    elif hasattr(entity, 'is_locked'):
                        try:
                            is_locked = entity.is_locked
                            state = 'locked' if is_locked else 'unlocked'
                        except:
                            state = 'unknown'
                    else:
                        state = 'unknown'
                # Cover entities use is_closed
                elif domain == 'cover':
                    if hasattr(entity, '_attr_is_closed'):
                        is_closed = entity._attr_is_closed
                        if is_closed is None:
                            state = 'unknown'
                        else:
                            state = 'closed' if is_closed else 'open'
                    elif hasattr(entity, 'is_closed'):
                        try:
                            is_closed = entity.is_closed
                            if is_closed is None:
                                state = 'unknown'
                            else:
                                state = 'closed' if is_closed else 'open'
                        except:
                            state = 'unknown'
                    else:
                        state = 'unknown'
                # Other domains - try state property or _attr_state
                else:
                    if hasattr(entity, '_attr_state'):
                        state = str(entity._attr_state) if entity._attr_state is not None else 'unknown'
                    elif hasattr(entity, 'state'):
                        try:
                            val = entity.state
                            state = str(val) if val is not None else 'unknown'
                        except:
                            state = 'unknown'
                    elif hasattr(entity, '_state'):
                        state = entity._state
                        if isinstance(state, bool):
                            state = 'on' if state else 'off'
                        elif state is not None:
                            state = str(state)
                        else:
                            state = 'unknown'
                    else:
                        state = 'unknown'

                # Convert bool to on/off string
                if isinstance(state, bool):
                    state = 'on' if state else 'off'
                state = str(state)

                # Build attributes dict
                attributes = {}

                # Get friendly name - follows HA's _friendly_name_internal() logic
                # For has_entity_name=True: friendly_name = "{device_name} {entity_name}"
                # For has_entity_name=False: friendly_name = entity.name
                friendly_name = None

                # Try to get original_name from the entity registry
                if hass and hasattr(hass, 'data'):
                    try:
                        from homeassistant.helpers import entity_registry as er
                        if er.DATA_REGISTRY in hass.data:
                            entity_reg = hass.data[er.DATA_REGISTRY]
                            if entity_id in entity_reg.entities:
                                reg_entry = entity_reg.entities[entity_id]
                                entity_name = reg_entry.original_name
                                if entity_name:
                                    # For has_entity_name=True, combine device name + entity name
                                    # This matches HA's _friendly_name_internal() behavior
                                    if reg_entry.has_entity_name and device_info:
                                        dev_name = None
                                        if hasattr(device_info, 'get'):
                                            dev_name = _get_value_or_none(device_info.get('name'))
                                        elif hasattr(device_info, 'name'):
                                            dev_name = _get_value_or_none(device_info.name)
                                        if dev_name:
                                            friendly_name = f"{dev_name} {entity_name}"
                                        else:
                                            friendly_name = entity_name
                                    else:
                                        friendly_name = entity_name
                    except Exception as e:
                        _LOGGER.debug(f"Could not get friendly_name from registry: {e}")

                # Fall back to entity's name attribute
                if not friendly_name:
                    name = _get_value_or_none(getattr(entity, '_attr_name', None))
                    if name is None:
                        try:
                            name = _get_value_or_none(getattr(entity, 'name', None))
                        except (ValueError, AttributeError):
                            pass  # name property might require platform_data
                    if name and not _is_undefined(name):
                        friendly_name = str(name)
                    elif hasattr(entity, '_attr_device_info'):
                        device_info_attr = entity._attr_device_info
                        if device_info_attr and hasattr(device_info_attr, 'get'):
                            dev_name = _get_value_or_none(device_info_attr.get('name'))
                            if dev_name:
                                friendly_name = str(dev_name)
                        elif hasattr(device_info_attr, 'name'):
                            dev_name = _get_value_or_none(device_info_attr.name)
                            if dev_name:
                                friendly_name = str(dev_name)

                if friendly_name:
                    attributes['friendly_name'] = friendly_name

                # Get device class - try _attr_ first, then property, then entity_description
                device_class = _get_value_or_none(getattr(entity, '_attr_device_class', None))
                if device_class is None:
                    try:
                        device_class = _get_value_or_none(getattr(entity, 'device_class', None))
                    except (ValueError, AttributeError):
                        pass
                    # Fall back to entity_description.device_class if property returned None
                    if device_class is None and hasattr(entity, 'entity_description'):
                        ed = getattr(entity, 'entity_description', None)
                        if ed is not None:
                            ed_dc = getattr(ed, 'device_class', None)
                            if ed_dc is not None and not _is_undefined(ed_dc):
                                device_class = ed_dc
                if device_class and not _is_undefined(device_class):
                    # Handle enums
                    if hasattr(device_class, 'value'):
                        device_class = device_class.value
                    attributes['device_class'] = str(device_class)

                # Get unit of measurement - try _attr_ attributes first, then properties
                unit = _get_value_or_none(getattr(entity, '_attr_native_unit_of_measurement', None)) or \
                       _get_value_or_none(getattr(entity, '_attr_unit_of_measurement', None))
                if unit is None:
                    # Try properties (might raise ValueError if platform_data not set)
                    try:
                        unit = _get_value_or_none(getattr(entity, 'native_unit_of_measurement', None)) or \
                               _get_value_or_none(getattr(entity, 'unit_of_measurement', None))
                    except (ValueError, AttributeError):
                        pass  # Properties require platform_data, skip if not available
                if unit and not _is_undefined(unit):
                    attributes['unit_of_measurement'] = str(unit)

                # Get icon - try _attr_ first, then property
                icon = _get_value_or_none(getattr(entity, '_attr_icon', None))
                if icon is None:
                    try:
                        icon = _get_value_or_none(getattr(entity, 'icon', None))
                    except (ValueError, AttributeError):
                        pass
                if icon and not _is_undefined(icon):
                    attributes['icon'] = str(icon)

                # Light-specific attributes
                if domain == 'light':
                    brightness = getattr(entity, '_brightness', None) or getattr(entity, '_attr_brightness', None)
                    if brightness is not None:
                        attributes['brightness'] = brightness

                    color_mode = getattr(entity, '_color_mode', None) or getattr(entity, '_attr_color_mode', None)
                    if color_mode:
                        if hasattr(color_mode, 'value'):
                            color_mode = color_mode.value
                        attributes['color_mode'] = str(color_mode)

                    color_modes = getattr(entity, '_color_modes', None) or getattr(entity, '_attr_supported_color_modes', None)
                    if color_modes:
                        attributes['supported_color_modes'] = [str(m.value) if hasattr(m, 'value') else str(m) for m in color_modes]

                    hs_color = getattr(entity, '_hs_color', None) or getattr(entity, '_attr_hs_color', None)
                    if hs_color:
                        attributes['hs_color'] = list(hs_color)

                    ct = getattr(entity, '_ct', None) or getattr(entity, '_attr_color_temp_kelvin', None)
                    if ct:
                        attributes['color_temp_kelvin'] = ct

                    effect = getattr(entity, '_effect', None) or getattr(entity, '_attr_effect', None)
                    if effect:
                        attributes['effect'] = str(effect)

                    effect_list = getattr(entity, '_effect_list', None) or getattr(entity, '_attr_effect_list', None)
                    if effect_list:
                        attributes['effect_list'] = list(effect_list)

                # Get supported features - try _attr_ first, then property
                features = getattr(entity, '_attr_supported_features', None)
                if features is None:
                    try:
                        features = getattr(entity, 'supported_features', None)
                    except (ValueError, AttributeError):
                        pass
                if features:
                    if hasattr(features, 'value'):
                        features = features.value
                    attributes['supported_features'] = int(features)

                # Set the state in hass.states
                # Use print for debugging since Python logging might not be configured
                _LOGGER.debug(f" Adding entity: {entity_id} = {state} (attrs: {list(attributes.keys())})")

                # Use synchronous set method - our StatesWrapper supports this
                if hasattr(hass, 'states') and hasattr(hass.states, 'set'):
                    try:
                        hass.states.set(entity_id, state, attributes)
                        _LOGGER.debug(f" Successfully set state for {entity_id}")
                    except Exception as e:
                        _LOGGER.debug(f" Error setting state for {entity_id}: {e}")
                else:
                    _LOGGER.debug(f" Cannot set state for {entity_id}: hass.states.set not available")

                # Track entity for lifecycle call
                _pending_lifecycle.append((entity, entity_id))

            except Exception as e:
                _LOGGER.error(f"Error adding entity: {e}", exc_info=True)

        # Schedule lifecycle calls for all entities
        # We do this after all entities are added to ensure they can find each other if needed
        for entity, entity_id in _pending_lifecycle:
            asyncio.create_task(_call_entity_lifecycle(hass, entity, entity_id))

        _pending_lifecycle.clear()

    return add_entities

async def async_forward_entry_setups(entry, platforms):
    """Forward the setup of an entry to platforms.

    This loads the platform modules and calls their async_setup_entry functions.
    """
    global _hass

    entry_id = entry.get("entry_id") if isinstance(entry, dict) else getattr(entry, "entry_id", "unknown")
    domain = entry.get("domain") if isinstance(entry, dict) else getattr(entry, "domain", "unknown")

    _LOGGER.info(f"Forward entry setup for {domain} ({entry_id}): {list(platforms)}")

    # Track which platforms are loaded for this entry
    if entry_id not in _loaded_platforms:
        _loaded_platforms[entry_id] = set()

    for platform in platforms:
        # Normalize platform name (might be Platform enum or string)
        platform_name = str(platform).split(".")[-1] if "." in str(platform) else str(platform)
        platform_name = platform_name.lower()

        try:
            # Import the platform module
            module_path = f"homeassistant.components.{domain}.{platform_name}"
            _LOGGER.debug(f"Importing platform module: {module_path}")

            platform_module = importlib.import_module(module_path)

            # Check if it has async_setup_entry
            if hasattr(platform_module, 'async_setup_entry'):
                _LOGGER.debug(f"Calling async_setup_entry for {domain}.{platform_name}")

                # Create the add_entities callback
                if _hass is not None:
                    add_entities = _create_add_entities_callback(_hass, entry, platform_name)

                    # Call the platform's async_setup_entry
                    await platform_module.async_setup_entry(_hass, entry, add_entities)
                    _LOGGER.info(f"Platform {platform_name} setup complete for {domain}")
                else:
                    _LOGGER.warning(f"Cannot set up platform {platform_name}: hass not available")
            else:
                _LOGGER.debug(f"Platform {module_path} has no async_setup_entry")

            _loaded_platforms[entry_id].add(platform_name)

        except ImportError as e:
            _LOGGER.warning(f"Could not import platform {domain}.{platform_name}: {e}")
        except Exception as e:
            _LOGGER.error(f"Error setting up platform {domain}.{platform_name}: {e}", exc_info=True)

async def async_unload_platforms(entry, platforms):
    """Forward the unloading of an entry to platforms."""
    entry_id = entry.get("entry_id") if isinstance(entry, dict) else getattr(entry, "entry_id", "unknown")
    domain = entry.get("domain") if isinstance(entry, dict) else getattr(entry, "domain", "unknown")

    _LOGGER.info(f"Unload platforms for {domain} ({entry_id}): {list(platforms)}")

    # Remove platforms from tracking
    if entry_id in _loaded_platforms:
        for platform in platforms:
            platform_name = str(platform).split(".")[-1] if "." in str(platform) else str(platform)
            _loaded_platforms[entry_id].discard(platform_name)

    await asyncio.sleep(0)
    return True

async def async_forward_entry_setup(entry, platform):
    """Forward setup of a single platform (legacy method)."""
    await async_forward_entry_setups(entry, [platform])

async def async_forward_entry_unload(entry, platform):
    """Forward unload of a single platform (legacy method)."""
    return await async_unload_platforms(entry, [platform])

def async_entries(domain=None, include_ignore=True, include_disabled=True):
    """Return config entries, optionally filtered by domain.

    Args:
        domain: Optional domain to filter by.
        include_ignore: Include ignored entries (default True).
        include_disabled: Include disabled entries (default True).

    Returns:
        List of stored config entries matching the filter.
    """
    entries = list(_config_entries.values())
    if domain is not None:
        entries = [e for e in entries if getattr(e, 'domain', None) == domain]
    return entries

def async_update_entry(entry, *, data=None, options=None, title=None,
                        unique_id=None, minor_version=None, version=None,
                        pref_disable_new_entities=None, pref_disable_polling=None,
                        discovery_keys=None):
    """Update a config entry.

    Stub implementation that accepts common parameters integrations pass
    during setup. Returns True to indicate the entry was 'updated'.
    """
    _LOGGER.debug(f"async_update_entry called for {getattr(entry, 'domain', '?')}")
    # For ConfigEntryWrapper objects, update mutable fields if possible
    if data is not None and hasattr(entry, '_data'):
        try:
            entry._data = data
        except (AttributeError, TypeError):
            pass
    if title is not None and hasattr(entry, '_title'):
        try:
            entry._title = title
        except (AttributeError, TypeError):
            pass
    return True

def store_config_entry(entry):
    """Store a config entry for later lookup by async_get_entry.

    Called from Rust before async_setup_entry so that device_registry
    and other HA helpers can find the entry by ID.
    """
    entry_id = getattr(entry, 'entry_id', None)
    if entry_id:
        _config_entries[entry_id] = entry
        _LOGGER.debug(f"Stored config entry {entry_id} (domain={getattr(entry, 'domain', '?')})")

def async_get_entry(entry_id):
    """Get a config entry by ID.

    Returns the stored config entry or None if not found.
    """
    entry = _config_entries.get(entry_id)
    if entry is None:
        _LOGGER.debug(f"async_get_entry({entry_id}) -> None (not found)")
    return entry

def async_entry_for_domain_unique_id(domain, unique_id):
    """Get entry by domain and unique_id.

    Used by config flows to check if an entry already exists.

    Args:
        domain: The integration domain.
        unique_id: The unique ID to look up.

    Returns:
        The matching config entry or None if not found.
    """
    for entry in _config_entries.values():
        if (getattr(entry, 'domain', None) == domain
                and getattr(entry, 'unique_id', None) == unique_id):
            return entry
    return None
