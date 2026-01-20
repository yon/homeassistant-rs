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
    BusWrapper, ConfigWrapper, HassWrapper, RegistriesWrapper, ServicesWrapper, StatesWrapper,
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
    config_dir: Option<&std::path::Path>,
) -> PyBridgeResult<PyObject> {
    // Create SimpleNamespace for helpers (doesn't need to be hashable)
    let types = py.import_bound("types")?;
    let simple_namespace = types.getattr("SimpleNamespace")?;

    // Create #[pyclass] wrapper objects for bus, states, services
    // These call directly into Rust code instead of using Python stubs
    let bus_wrapper = Py::new(py, BusWrapper::new(bus))?;
    let states_wrapper = Py::new(py, StatesWrapper::new(states))?;
    let services_wrapper = Py::new(py, ServicesWrapper::new(services))?;

    // Config entries wrapper with platform setup methods
    // Also inject registries wrapper into the Python globals for device/entity registration
    let config_entries_wrapper = create_config_entries_wrapper(py, registries)?;

    // Add config attribute with location and components using #[pyclass]
    let config = Py::new(py, ConfigWrapper::new(py)?)?;

    // Add loop attribute (get the running event loop or create one)
    let asyncio = py.import_bound("asyncio")?;
    let threading = py.import_bound("threading")?;
    let loop_ = match asyncio.call_method0("get_running_loop") {
        Ok(loop_) => loop_.unbind(),
        Err(_) => {
            // No running loop, create one
            asyncio.call_method0("new_event_loop")?.unbind()
        }
    };

    // Add loop_thread_id (current thread id, used by entities)
    let current_thread = threading.call_method0("current_thread")?;
    let thread_ident = current_thread.getattr("ident")?.unbind();

    // Add async_create_task method
    let async_create_task = create_async_create_task(py)?;

    // Add helpers attribute for helper utilities (SimpleNamespace is fine here)
    let helpers = simple_namespace.call0()?.unbind();

    // Create timeout factory function
    let timeout = create_timeout_factory(py)?;

    // Create the hashable HassWrapper #[pyclass]
    let hass = Py::new(
        py,
        HassWrapper::new(
            py,
            bus_wrapper,
            states_wrapper,
            services_wrapper,
            config,
            config_entries_wrapper,
            helpers,
            loop_,
            thread_ident,
            async_create_task,
            timeout,
        )?,
    )?;

    // Initialize HA Python registries so EntityComponent can use them
    // This needs to be done AFTER hass is created since registries need hass reference
    // Pass config_dir so it can load entity registry from disk
    initialize_ha_registries(py, &hass, config_dir)?;

    Ok(hass.into_any())
}

/// Initialize HA Python registries so EntityComponent can use them
///
/// This creates the entity_registry and device_registry instances that
/// HA's EntityComponent expects to find. If config_dir is provided,
/// loads the registries from disk so that existing entity_ids are preserved.
fn initialize_ha_registries(
    py: Python<'_>,
    hass: &Py<HassWrapper>,
    config_dir: Option<&std::path::Path>,
) -> PyResult<()> {
    let code = r#"
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
"#;

    let globals = PyDict::new_bound(py);
    py.run_bound(code, Some(&globals), None)?;

    let init_fn = globals.get_item("_init_registries")?.unwrap();
    let config_dir_str = config_dir.map(|p| p.to_string_lossy().to_string());
    let _ = init_fn.call1((hass, config_dir_str))?;

    Ok(())
}

/// Create a TimeoutManager instance
///
/// This provides `hass.timeout` as a `TimeoutManager` instance with
/// `async_timeout(seconds, zone_name, cool_down, cancel_message)` method.
fn create_timeout_factory(py: Python<'_>) -> PyResult<PyObject> {
    let code = r#"
from homeassistant.util.timeout import TimeoutManager

# Create a TimeoutManager instance
# This needs to be created when an event loop is running
def _create_timeout_manager():
    """Create a TimeoutManager instance.

    TimeoutManager needs a running event loop, so we wrap the creation
    to be called lazily when actually used.
    """
    import asyncio
    try:
        asyncio.get_running_loop()
        return TimeoutManager()
    except RuntimeError:
        # No running loop yet - create a dummy that will work later
        # Return a class that delays TimeoutManager creation until first use
        class LazyTimeoutManager:
            _instance = None

            def async_timeout(self, timeout, zone_name="global", cool_down=0, cancel_message=None):
                if self._instance is None:
                    self._instance = TimeoutManager()
                return self._instance.async_timeout(timeout, zone_name, cool_down, cancel_message)

            def async_freeze(self, zone_name="global"):
                if self._instance is None:
                    self._instance = TimeoutManager()
                return self._instance.async_freeze(zone_name)

            def freeze(self, zone_name="global"):
                if self._instance is None:
                    self._instance = TimeoutManager()
                return self._instance.freeze(zone_name)

        return LazyTimeoutManager()

timeout_manager = _create_timeout_manager()
"#;

    let globals = PyDict::new_bound(py);
    py.run_bound(code, Some(&globals), None)?;

    let timeout_manager = globals.get_item("timeout_manager")?.unwrap();
    Ok(timeout_manager.unbind())
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

    # Get device class
    device_class = _get_value_or_none(getattr(entity, '_attr_device_class', None))
    if device_class is None:
        try:
            device_class = _get_value_or_none(getattr(entity, 'device_class', None))
        except Exception:
            pass
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

                # Get entity unique_id and try to look up existing entity_id from registry
                unique_id = getattr(entity, '_attr_unique_id', None) or getattr(entity, 'unique_id', None)

                # Get the integration domain from the config entry
                integration_domain = entry.get("domain") if isinstance(entry, dict) else getattr(entry, "domain", None)

                # First, try to find existing entity_id from loaded entity registry
                entity_id = None
                if unique_id and hass and hasattr(hass, 'data'):
                    try:
                        from homeassistant.helpers import entity_registry as er
                        if er.DATA_REGISTRY in hass.data:
                            entity_reg = hass.data[er.DATA_REGISTRY]
                            # Look up by unique_id and platform (platform in registry is integration domain, not entity type)
                            for reg_entry in entity_reg.entities.values():
                                if reg_entry.unique_id == unique_id and reg_entry.platform == integration_domain:
                                    entity_id = reg_entry.entity_id
                                    _LOGGER.debug(f"Found existing entity_id from registry: {entity_id} (unique_id={unique_id})")
                                    break
                    except Exception as e:
                        _LOGGER.debug(f"Could not look up entity in registry: {e}")

                # If not found in registry, generate a new entity_id
                if entity_id is None:
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
                        entity_name = _get_value_or_none(getattr(entity, '_attr_name', None))
                        if entity_name is None:
                            try:
                                entity_name = _get_value_or_none(getattr(entity, 'name', None))
                            except:
                                pass
                        _registries.register_entity(
                            platform_name,
                            entity_id,
                            unique_id=unique_id,
                            config_entry_id=config_entry_id,
                            device_id=device_id,
                            name=str(entity_name) if entity_name and not _is_undefined(entity_name) else None,
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

                # Get device class - try _attr_ first, then property
                device_class = _get_value_or_none(getattr(entity, '_attr_device_class', None))
                if device_class is None:
                    try:
                        device_class = _get_value_or_none(getattr(entity, 'device_class', None))
                    except (ValueError, AttributeError):
                        pass
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

def async_entries(domain=None):
    """Return config entries for a domain.

    This is a stub that returns an empty list since we don't track config entries here.
    Integrations that check for existing entries will think there are none.
    """
    # For now, return empty list - integrations will proceed as if no entries exist
    return []
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

    // Add async_entries method for checking existing entries
    let async_entries = globals.get_item("async_entries")?.unwrap();
    wrapper.setattr("async_entries", async_entries)?;

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

            let result = create_hass_wrapper(py, bus, states, services, registries, None);
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
