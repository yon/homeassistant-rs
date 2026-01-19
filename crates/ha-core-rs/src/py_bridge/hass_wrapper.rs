//! Python HomeAssistant wrapper
//!
//! Creates a Python-compatible HomeAssistant object that wraps our Rust core
//! for passing to Python integrations.

use ha_event_bus::EventBus;
use ha_registries::Registries;
use ha_service_registry::ServiceRegistry;
use ha_state_machine::StateMachine;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::{Arc, OnceLock};

use super::errors::PyBridgeResult;
use super::pyclass_wrappers::{
    BusWrapper, ConfigWrapper, RegistriesWrapper, ServicesWrapper, StatesWrapper,
};

/// Persistent Python globals for config_entries module
/// This ensures entity/device registries survive across multiple hass wrapper creations
static CONFIG_ENTRIES_GLOBALS: OnceLock<Py<PyDict>> = OnceLock::new();

/// Call a service on a Python entity
///
/// This dispatches to the Python entity's async method (e.g., async_turn_on).
pub fn call_python_entity_service(
    entity_id: &str,
    service: &str,
    service_data: serde_json::Value,
) -> Result<bool, pyo3::PyErr> {
    Python::with_gil(|py| {
        let globals = match CONFIG_ENTRIES_GLOBALS.get() {
            Some(g) => g.bind(py),
            None => return Ok(false), // Not initialized yet
        };

        // Convert service_data to Python dict
        let kwargs = PyDict::new_bound(py);
        if let serde_json::Value::Object(map) = service_data {
            for (k, v) in map {
                let py_val = json_to_pyobject(py, &v)?;
                kwargs.set_item(k, py_val)?;
            }
        }

        // Use a synchronous wrapper that handles entity service calls
        // We directly modify entity attributes and update state, bypassing HA's async_write_ha_state
        let wrapper_code = r#"
def _call_entity_service_sync(entity_id, service, kwargs):
    """Synchronous wrapper for calling entity services.

    Instead of calling the entity's async methods (which require full HA infrastructure),
    we directly modify the entity attributes based on the service and update state.
    """
    entity = _entity_registry.get(entity_id)
    if entity is None:
        _LOGGER.warning(f"Entity not found: {entity_id}")
        return False

    domain = entity_id.split('.')[0]

    try:
        # Handle common services by directly modifying entity attributes
        if service in ('turn_on', 'turn_off', 'toggle'):
            if hasattr(entity, '_attr_is_on'):
                if service == 'turn_on':
                    entity._attr_is_on = True
                elif service == 'turn_off':
                    entity._attr_is_on = False
                elif service == 'toggle':
                    entity._attr_is_on = not entity._attr_is_on
                _LOGGER.debug(f"Set {entity_id}._attr_is_on = {entity._attr_is_on}")
            elif hasattr(entity, '_is_on'):
                if service == 'turn_on':
                    entity._is_on = True
                elif service == 'turn_off':
                    entity._is_on = False
                elif service == 'toggle':
                    entity._is_on = not entity._is_on
                _LOGGER.debug(f"Set {entity_id}._is_on = {entity._is_on}")
            else:
                _LOGGER.warning(f"Entity {entity_id} has no _attr_is_on or _is_on attribute")
                return False

            # Handle brightness for turn_on
            if service == 'turn_on' and 'brightness' in kwargs:
                if hasattr(entity, '_attr_brightness'):
                    entity._attr_brightness = kwargs['brightness']

        elif service == 'lock':
            if hasattr(entity, '_attr_is_locked'):
                entity._attr_is_locked = True
        elif service == 'unlock':
            if hasattr(entity, '_attr_is_locked'):
                entity._attr_is_locked = False
        elif service == 'set_value' and domain == 'number':
            if hasattr(entity, '_attr_native_value'):
                entity._attr_native_value = kwargs.get('value')
        elif service == 'select_option' and domain == 'select':
            if hasattr(entity, '_attr_current_option'):
                entity._attr_current_option = kwargs.get('option')
        elif service == 'press' and domain == 'button':
            # Button press doesn't change state, just acknowledge
            pass
        else:
            _LOGGER.warning(f"Service {service} not implemented for direct attribute modification")
            return False

        # Update state in Rust state machine
        _update_entity_state_sync(entity)
        return True
    except Exception as e:
        _LOGGER.error(f"Error calling {service} on {entity_id}: {e}")
        import traceback
        traceback.print_exc()
        return False

def _update_entity_state_sync(entity):
    """Synchronously update the state of an entity in Rust state machine."""
    if _hass is None or not hasattr(entity, 'entity_id'):
        return

    entity_id = entity.entity_id
    domain = entity_id.split('.')[0]

    # Determine state based on domain and entity attributes
    state = None
    if domain in ('light', 'switch', 'fan', 'siren', 'humidifier'):
        if hasattr(entity, '_attr_is_on'):
            state = 'on' if entity._attr_is_on else 'off'
        elif hasattr(entity, '_is_on'):
            state = 'on' if entity._is_on else 'off'
        else:
            state = 'off'
    elif domain == 'lock':
        if hasattr(entity, '_attr_is_locked'):
            state = 'locked' if entity._attr_is_locked else 'unlocked'
        else:
            state = 'unknown'
    elif domain in ('sensor', 'number'):
        if hasattr(entity, '_attr_native_value'):
            state = str(entity._attr_native_value) if entity._attr_native_value is not None else 'unknown'
        else:
            state = 'unknown'
    elif domain == 'select':
        if hasattr(entity, '_attr_current_option'):
            state = str(entity._attr_current_option) if entity._attr_current_option else 'unknown'
        else:
            state = 'unknown'
    elif domain == 'binary_sensor':
        if hasattr(entity, '_attr_is_on'):
            state = 'on' if entity._attr_is_on else 'off'
        else:
            state = 'off'
    else:
        state = 'unknown'

    # Build attributes dict
    attributes = {}
    if hasattr(entity, '_attr_brightness') and entity._attr_brightness is not None:
        attributes['brightness'] = entity._attr_brightness
    if hasattr(entity, '_attr_color_mode') and entity._attr_color_mode is not None:
        cm = entity._attr_color_mode
        attributes['color_mode'] = cm.value if hasattr(cm, 'value') else str(cm)
    if hasattr(entity, '_attr_hs_color') and entity._attr_hs_color is not None:
        attributes['hs_color'] = list(entity._attr_hs_color)
    if hasattr(entity, '_attr_friendly_name'):
        attributes['friendly_name'] = entity._attr_friendly_name
    elif hasattr(entity, 'name'):
        try:
            attributes['friendly_name'] = entity.name
        except:
            pass

    # Update state in Rust state machine
    if hasattr(_hass, 'states') and hasattr(_hass.states, 'set'):
        _hass.states.set(entity_id, state, attributes)
        _LOGGER.info(f"Updated state: {entity_id} = {state}")
"#;
        // Execute the wrapper code in the globals context so it has access to _entity_registry, _hass, etc.
        py.run_bound(wrapper_code, Some(&globals), None)?;

        let call_fn = globals.get_item("_call_entity_service_sync")?.unwrap();
        let result = call_fn.call1((entity_id, service, &kwargs))?;

        Ok(result.extract::<bool>().unwrap_or(false))
    })
}

/// Get all registered Python devices
pub fn get_python_devices() -> Result<Vec<(String, serde_json::Value)>, pyo3::PyErr> {
    Python::with_gil(|py| {
        let globals = match CONFIG_ENTRIES_GLOBALS.get() {
            Some(g) => g.bind(py),
            None => return Ok(Vec::new()),
        };

        let get_fn = globals.get_item("get_all_devices")?;
        if get_fn.is_none() {
            return Ok(Vec::new());
        }
        let get_fn = get_fn.unwrap();

        let devices = get_fn.call0()?;
        let devices_dict = devices.downcast::<PyDict>()?;

        let mut result = Vec::new();
        for (device_id, device_info) in devices_dict.iter() {
            let device_id: String = device_id.extract()?;
            let device_info = pyobject_to_json(&device_info)?;
            result.push((device_id, device_info));
        }

        Ok(result)
    })
}

/// Get all registered Python entities
pub fn get_python_entities() -> Result<Vec<String>, pyo3::PyErr> {
    Python::with_gil(|py| {
        let globals = match CONFIG_ENTRIES_GLOBALS.get() {
            Some(g) => g.bind(py),
            None => return Ok(Vec::new()),
        };

        let get_fn = globals.get_item("get_all_entities")?;
        if get_fn.is_none() {
            return Ok(Vec::new());
        }
        let get_fn = get_fn.unwrap();

        let entities = get_fn.call0()?;
        let entities_dict = entities.downcast::<PyDict>()?;

        let mut result = Vec::new();
        for (entity_id, _) in entities_dict.iter() {
            let entity_id: String = entity_id.extract()?;
            result.push(entity_id);
        }

        Ok(result)
    })
}

/// Convert JSON value to Python object
fn json_to_pyobject(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok(b.into_py(py)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_py(py))
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_py(py))
            } else {
                Ok(py.None())
            }
        }
        serde_json::Value::String(s) => Ok(s.into_py(py)),
        serde_json::Value::Array(arr) => {
            let list = pyo3::types::PyList::empty_bound(py);
            for item in arr {
                list.append(json_to_pyobject(py, item)?)?;
            }
            Ok(list.into())
        }
        serde_json::Value::Object(obj) => {
            let dict = PyDict::new_bound(py);
            for (k, v) in obj {
                dict.set_item(k, json_to_pyobject(py, v)?)?;
            }
            Ok(dict.into())
        }
    }
}

/// Convert Python object to JSON value
fn pyobject_to_json(obj: &Bound<'_, pyo3::PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        return Ok(serde_json::Value::Null);
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(serde_json::Value::Bool(b));
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(serde_json::Value::Number(i.into()));
    }
    if let Ok(f) = obj.extract::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Ok(serde_json::Value::Number(n));
        }
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(serde_json::Value::String(s));
    }
    if let Ok(list) = obj.downcast::<pyo3::types::PyList>() {
        let arr: Result<Vec<_>, _> = list.iter().map(|item| pyobject_to_json(&item)).collect();
        return Ok(serde_json::Value::Array(arr?));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            if let Ok(key) = k.extract::<String>() {
                map.insert(key, pyobject_to_json(&v)?);
            }
        }
        return Ok(serde_json::Value::Object(map));
    }
    // Default to string representation
    Ok(serde_json::Value::String(obj.to_string()))
}

/// Create a Python HomeAssistant-like object
///
/// This creates a Python object with the core attributes that integrations need:
/// - `bus` - Event bus for firing events
/// - `states` - State machine for entity states
/// - `services` - Service registry for service calls
/// - `config_entries` - Config entries manager with platform setup methods
/// - `data` - Dict for storing integration data
/// - `config` - Configuration with location and components
/// - `loop` - Event loop
/// - `async_create_task` - Task creation method
///
/// Note: This wrapper provides compatibility with common HA integration patterns.
/// Some advanced features may require additional implementation.
pub fn create_hass_wrapper(
    py: Python<'_>,
    bus: Arc<EventBus>,
    states: Arc<StateMachine>,
    services: Arc<ServiceRegistry>,
    registries: Arc<Registries>,
) -> PyBridgeResult<PyObject> {
    // Create a simple namespace object to hold our attributes
    let types = py.import_bound("types")?;
    let simple_namespace = types.getattr("SimpleNamespace")?;

    // Create the hass object
    let hass = simple_namespace.call0()?;

    // Add data dict for integrations to store data
    let data = PyDict::new_bound(py);
    // Add integrations dict that entities expect
    let integrations = PyDict::new_bound(py);
    data.set_item("integrations", &integrations)?;
    hass.setattr("data", data)?;

    // Create #[pyclass] wrapper objects for bus, states, services
    // These call directly into Rust code instead of using Python stubs
    let bus_wrapper = Py::new(py, BusWrapper::new(bus))?;
    hass.setattr("bus", bus_wrapper)?;

    let states_wrapper = Py::new(py, StatesWrapper::new(states))?;
    hass.setattr("states", states_wrapper)?;

    let services_wrapper = Py::new(py, ServicesWrapper::new(services))?;
    hass.setattr("services", services_wrapper)?;

    // Config entries wrapper with platform setup methods
    // Also inject registries wrapper into the Python globals for device/entity registration
    let config_entries_wrapper = create_config_entries_wrapper(py, registries)?;
    hass.setattr("config_entries", config_entries_wrapper)?;

    // Add config attribute with location and components using #[pyclass]
    let config = Py::new(py, ConfigWrapper::new(py)?)?;
    hass.setattr("config", config)?;

    // Add loop attribute (get the running event loop or create one)
    let asyncio = py.import_bound("asyncio")?;
    let threading = py.import_bound("threading")?;
    match asyncio.call_method0("get_running_loop") {
        Ok(loop_) => hass.setattr("loop", loop_)?,
        Err(_) => {
            // No running loop, create one
            let loop_ = asyncio.call_method0("new_event_loop")?;
            hass.setattr("loop", loop_)?;
        }
    }

    // Add loop_thread_id (current thread id, used by entities)
    let current_thread = threading.call_method0("current_thread")?;
    let thread_ident = current_thread.getattr("ident")?;
    hass.setattr("loop_thread_id", thread_ident)?;

    // Add async_create_task method
    let async_create_task = create_async_create_task(py)?;
    hass.setattr("async_create_task", async_create_task)?;

    // Add helpers attribute for helper utilities
    let helpers = simple_namespace.call0()?;
    hass.setattr("helpers", helpers)?;

    Ok(hass.unbind())
}

/// Create a config_entries wrapper with platform setup methods
///
/// Provides:
/// - `async_forward_entry_setups(entry, platforms)` - Forward setup to platforms
/// - `async_unload_platforms(entry, platforms)` - Unload platforms
/// - `flow.async_init(domain, context, data)` - Initialize config flow
fn create_config_entries_wrapper(
    py: Python<'_>,
    registries: Arc<Registries>,
) -> PyResult<PyObject> {
    let types = py.import_bound("types")?;
    let simple_namespace = types.getattr("SimpleNamespace")?;
    let wrapper = simple_namespace.call0()?;

    // Create the config entries methods with actual platform loading
    let code = r#"
import logging
import asyncio
import importlib
from datetime import datetime, timezone

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

    # Get current state
    state = None
    if hasattr(entity, '_attr_is_on'):
        state = 'on' if entity._attr_is_on else 'off'
    elif hasattr(entity, 'is_on'):
        try:
            is_on = entity.is_on
            state = 'on' if is_on else 'off'
        except:
            pass
    elif hasattr(entity, '_attr_native_value'):
        state = str(entity._attr_native_value) if entity._attr_native_value is not None else 'unknown'
    elif hasattr(entity, 'native_value'):
        try:
            val = entity.native_value
            state = str(val) if val is not None else 'unknown'
        except:
            state = 'unknown'
    elif hasattr(entity, '_attr_state'):
        state = str(entity._attr_state)
    elif hasattr(entity, 'state'):
        try:
            state = str(entity.state)
        except:
            pass

    if state is None:
        if domain in ('light', 'switch', 'fan'):
            state = 'off'
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
    base_id = f"{domain}.{suggested_id}" if suggested_id else f"{domain}.{platform}_entity"
    entity_id = base_id
    counter = 1
    while entity_id in existing_ids:
        entity_id = f"{base_id}_{counter}"
        counter += 1
    return entity_id

def _create_add_entities_callback(hass, entry, platform_name):
    """Create the async_add_entities callback for a platform.

    This callback is called by the platform's async_setup_entry to add entities.
    We extract entity state and attributes and set them in the state machine.
    """
    existing_ids = set()

    # Import PlatformData to set on entities before accessing properties
    from homeassistant.helpers.entity_platform import PlatformData

    def add_entities(entities, update_before_add=False, config_subentry_id=None):
        """Add entities to Home Assistant."""
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

                # Set platform_data on entity BEFORE accessing properties
                # This is required for entities with translation keys
                if not hasattr(entity, 'platform_data') or entity.platform_data is None:
                    try:
                        platform_data = PlatformData(hass, domain=domain, platform_name=platform_name)
                        entity.platform_data = platform_data
                    except Exception as e:
                        _LOGGER.debug(f"Could not set platform_data: {e}")

                # Get entity unique_id and generate entity_id
                unique_id = getattr(entity, '_attr_unique_id', None) or getattr(entity, 'unique_id', None)
                suggested_id = unique_id or getattr(entity, '_attr_name', None) or getattr(entity, 'name', 'entity')
                # Clean the suggested_id
                if suggested_id:
                    suggested_id = str(suggested_id).lower().replace(' ', '_').replace('-', '_')

                entity_id = _generate_entity_id(domain, platform_name, suggested_id, existing_ids)
                existing_ids.add(entity_id)

                # Store the entity_id on the entity for future reference
                entity.entity_id = entity_id

                # Set hass reference on entity (required for service calls)
                entity.hass = hass

                # Store entity in registry for service dispatch
                _entity_registry[entity_id] = entity

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

                # Register entity in Rust registry
                if _registries is not None:
                    try:
                        config_entry_id = entry.get("entry_id") if isinstance(entry, dict) else getattr(entry, "entry_id", None)
                        entity_name = getattr(entity, '_attr_name', None)
                        if entity_name is None:
                            try:
                                entity_name = getattr(entity, 'name', None)
                            except:
                                pass
                        _registries.register_entity(
                            platform_name,
                            entity_id,
                            unique_id=unique_id,
                            config_entry_id=config_entry_id,
                            device_id=device_id,
                            name=str(entity_name) if entity_name else None,
                        )
                    except Exception as e:
                        _LOGGER.error(f"Failed to register entity in Rust: {e}")

                # Register domain services if not already done
                _register_domain_services(hass, domain)

                # Get entity state
                state = None
                # Try different state attributes based on entity type
                if hasattr(entity, '_attr_is_on'):
                    state = 'on' if entity._attr_is_on else 'off'
                elif hasattr(entity, 'is_on'):
                    try:
                        is_on = entity.is_on
                        if callable(is_on):
                            is_on = is_on()
                        state = 'on' if is_on else 'off'
                    except:
                        pass
                elif hasattr(entity, '_state'):
                    state = entity._state
                    if isinstance(state, bool):
                        state = 'on' if state else 'off'
                elif hasattr(entity, '_attr_native_value'):
                    state = str(entity._attr_native_value) if entity._attr_native_value is not None else 'unknown'
                elif hasattr(entity, 'native_value'):
                    try:
                        val = entity.native_value
                        if callable(val):
                            val = val()
                        state = str(val) if val is not None else 'unknown'
                    except:
                        state = 'unknown'

                # Default state based on domain
                if state is None:
                    if domain in ('light', 'switch', 'fan'):
                        state = 'off'
                    elif domain == 'binary_sensor':
                        state = 'off'
                    else:
                        state = 'unknown'

                # Convert bool to on/off string
                if isinstance(state, bool):
                    state = 'on' if state else 'off'
                state = str(state)

                # Build attributes dict
                attributes = {}

                # Get friendly name - try _attr_name first, then property
                name = getattr(entity, '_attr_name', None)
                if name is None:
                    try:
                        name = getattr(entity, 'name', None)
                    except (ValueError, AttributeError):
                        pass  # name property might require platform_data
                if name:
                    attributes['friendly_name'] = str(name)
                elif hasattr(entity, '_attr_device_info'):
                    device_info = entity._attr_device_info
                    if device_info and hasattr(device_info, 'get'):
                        attributes['friendly_name'] = device_info.get('name', suggested_id)
                    elif hasattr(device_info, 'name'):
                        attributes['friendly_name'] = device_info.name

                # Get device class - try _attr_ first, then property
                device_class = getattr(entity, '_attr_device_class', None)
                if device_class is None:
                    try:
                        device_class = getattr(entity, 'device_class', None)
                    except (ValueError, AttributeError):
                        pass
                if device_class:
                    # Handle enums
                    if hasattr(device_class, 'value'):
                        device_class = device_class.value
                    attributes['device_class'] = str(device_class)

                # Get unit of measurement - try _attr_ attributes first, then properties
                unit = getattr(entity, '_attr_native_unit_of_measurement', None) or \
                       getattr(entity, '_attr_unit_of_measurement', None)
                if unit is None:
                    # Try properties (might raise ValueError if platform_data not set)
                    try:
                        unit = getattr(entity, 'native_unit_of_measurement', None) or \
                               getattr(entity, 'unit_of_measurement', None)
                    except (ValueError, AttributeError):
                        pass  # Properties require platform_data, skip if not available
                if unit:
                    attributes['unit_of_measurement'] = str(unit)

                # Get icon - try _attr_ first, then property
                icon = getattr(entity, '_attr_icon', None)
                if icon is None:
                    try:
                        icon = getattr(entity, 'icon', None)
                    except (ValueError, AttributeError):
                        pass
                if icon:
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
                _LOGGER.info(f"Adding entity: {entity_id} = {state} (attrs: {list(attributes.keys())})")

                if hasattr(hass, 'states') and hasattr(hass.states, 'async_set'):
                    # Use async_set (need to schedule it since we're in sync context)
                    async def _set_state():
                        await hass.states.async_set(entity_id, state, attributes)
                    asyncio.create_task(_set_state())
                elif hasattr(hass, 'states') and hasattr(hass.states, 'set'):
                    # Use sync set
                    hass.states.set(entity_id, state, attributes)
                else:
                    _LOGGER.warning(f"Cannot set state for {entity_id}: hass.states not available")

            except Exception as e:
                _LOGGER.error(f"Error adding entity: {e}", exc_info=True)

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
"#;

    // Use persistent globals so entity/device registries survive across calls
    let globals = CONFIG_ENTRIES_GLOBALS.get_or_init(|| {
        Python::with_gil(|py| {
            let dict = PyDict::new_bound(py);
            py.run_bound(code, Some(&dict), None)
                .expect("Failed to initialize config_entries Python code");
            dict.unbind()
        })
    });

    let globals = globals.bind(py);

    // Inject the registries wrapper into globals so Python code can call it
    let registries_wrapper = Py::new(py, RegistriesWrapper::new(registries))?;
    globals.set_item("_registries", registries_wrapper)?;

    let async_forward_entry_setups = globals.get_item("async_forward_entry_setups")?.unwrap();
    wrapper.setattr("async_forward_entry_setups", async_forward_entry_setups)?;

    let async_unload_platforms = globals.get_item("async_unload_platforms")?.unwrap();
    wrapper.setattr("async_unload_platforms", async_unload_platforms)?;

    let async_forward_entry_setup = globals.get_item("async_forward_entry_setup")?.unwrap();
    wrapper.setattr("async_forward_entry_setup", async_forward_entry_setup)?;

    let async_forward_entry_unload = globals.get_item("async_forward_entry_unload")?.unwrap();
    wrapper.setattr("async_forward_entry_unload", async_forward_entry_unload)?;

    // Store the set_hass function so integration.py can call it
    let set_hass = globals.get_item("set_hass")?.unwrap();
    wrapper.setattr("set_hass", set_hass)?;

    // Export entity/device registry functions for Rust to call
    let get_entity = globals.get_item("get_entity")?.unwrap();
    wrapper.setattr("get_entity", get_entity)?;

    let get_all_entities = globals.get_item("get_all_entities")?.unwrap();
    wrapper.setattr("get_all_entities", get_all_entities)?;

    let get_all_devices = globals.get_item("get_all_devices")?.unwrap();
    wrapper.setattr("get_all_devices", get_all_devices)?;

    let call_entity_service = globals.get_item("_call_entity_service")?.unwrap();
    wrapper.setattr("call_entity_service", call_entity_service)?;

    // Create the flow sub-object
    let flow = create_config_flow_wrapper(py)?;
    wrapper.setattr("flow", flow)?;

    Ok(wrapper.unbind())
}

/// Create a config flow wrapper
fn create_config_flow_wrapper(py: Python<'_>) -> PyResult<PyObject> {
    let types = py.import_bound("types")?;
    let simple_namespace = types.getattr("SimpleNamespace")?;
    let flow = simple_namespace.call0()?;

    let code = r#"
import logging
import asyncio

_LOGGER = logging.getLogger(__name__)

async def async_init(domain, *, context=None, data=None):
    """Initialize a config flow.

    This is called to start a configuration flow for an integration.
    For now, we log and return a mock flow ID.
    """
    _LOGGER.info(f"Config flow init for {domain}, context={context}")
    await asyncio.sleep(0)
    return {"flow_id": f"{domain}_flow_1", "type": "form"}
"#;

    let globals = PyDict::new_bound(py);
    py.run_bound(code, Some(&globals), None)?;

    let async_init = globals.get_item("async_init")?.unwrap();
    flow.setattr("async_init", async_init)?;

    Ok(flow.unbind())
}

/// Create an async_create_task function
fn create_async_create_task(py: Python<'_>) -> PyResult<PyObject> {
    let code = r#"
import asyncio
import logging

_LOGGER = logging.getLogger(__name__)

def async_create_task(coro, name=None, eager_start=False):
    """Create an async task.

    This wraps asyncio.create_task to match HA's API.
    """
    try:
        loop = asyncio.get_running_loop()
        task = loop.create_task(coro, name=name)
        _LOGGER.debug(f"Created task: {name or 'unnamed'}")
        return task
    except RuntimeError:
        # No running loop - schedule it for later
        _LOGGER.warning(f"No running loop for task: {name or 'unnamed'}")
        return asyncio.ensure_future(coro)
"#;

    let globals = PyDict::new_bound(py);
    py.run_bound(code, Some(&globals), None)?;

    let func = globals.get_item("async_create_task")?.unwrap();
    Ok(func.unbind())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_hass_wrapper() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let temp_dir = TempDir::new().unwrap();
            let bus = Arc::new(EventBus::new());
            let states = Arc::new(StateMachine::new(bus.clone()));
            let services = Arc::new(ServiceRegistry::new());
            let registries = Arc::new(Registries::new(temp_dir.path()));

            let result = create_hass_wrapper(py, bus, states, services, registries);
            assert!(result.is_ok());

            let hass = result.unwrap();
            let hass = hass.bind(py);

            // Verify core attributes exist
            assert!(hass.hasattr("bus").unwrap());
            assert!(hass.hasattr("states").unwrap());
            assert!(hass.hasattr("services").unwrap());
            assert!(hass.hasattr("data").unwrap());
            assert!(hass.hasattr("config").unwrap());
            assert!(hass.hasattr("loop").unwrap());

            // Verify new attributes for demo integration support
            assert!(hass.hasattr("config_entries").unwrap());
            assert!(hass.hasattr("async_create_task").unwrap());
            assert!(hass.hasattr("helpers").unwrap());

            // Verify config_entries has the required methods
            let config_entries = hass.getattr("config_entries").unwrap();
            assert!(config_entries
                .hasattr("async_forward_entry_setups")
                .unwrap());
            assert!(config_entries.hasattr("async_unload_platforms").unwrap());
            assert!(config_entries.hasattr("flow").unwrap());

            // Verify flow has async_init
            let flow = config_entries.getattr("flow").unwrap();
            assert!(flow.hasattr("async_init").unwrap());

            // Verify config has location attributes
            let config = hass.getattr("config").unwrap();
            assert!(config.hasattr("latitude").unwrap());
            assert!(config.hasattr("longitude").unwrap());
            assert!(config.hasattr("components").unwrap());
        });
    }
}
