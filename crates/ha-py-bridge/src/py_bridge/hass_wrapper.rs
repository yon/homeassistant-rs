//! Python HomeAssistant wrapper
//!
//! Creates a Python-compatible HomeAssistant object that wraps our Rust core
//! for passing to Python integrations.

use ha_api::ApplicationCredentialsStore;
use ha_event_bus::EventBus;
use ha_registries::Registries;
use ha_service_registry::ServiceRegistry;
use ha_state_store::StateStore;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::{Arc, OnceLock};

use super::errors::PyBridgeResult;
use super::py_utils::{json_to_pyobject, pyobject_to_json};
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
        let wrapper_code = include_str!("../../embedded_python/entity_service.py");
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
///
/// # Arguments
/// * `event_loop` - Optional event loop to use. If not provided, gets/creates one.
///                  This should be the same event loop used by AsyncBridge.
pub fn create_hass_wrapper(
    py: Python<'_>,
    bus: Arc<EventBus>,
    states: Arc<StateStore>,
    services: Arc<ServiceRegistry>,
    registries: Arc<Registries>,
    config_dir: Option<&std::path::Path>,
    event_loop: Option<PyObject>,
) -> PyBridgeResult<PyObject> {
    create_hass_wrapper_internal(
        py, bus, states, services, registries, config_dir, event_loop, true,
        None, // No credentials for regular hass wrapper
    )
}

/// Create a minimal Python HomeAssistant-like object for config flows
///
/// This is a simplified version that skips HA Python registry initialization.
/// Used for config flows which don't need the full HA infrastructure but need
/// a hass object with async_add_executor_job and other basic methods.
pub fn create_hass_wrapper_for_config_flow(
    py: Python<'_>,
    bus: Arc<EventBus>,
    states: Arc<StateStore>,
    services: Arc<ServiceRegistry>,
    registries: Arc<Registries>,
    config_dir: Option<&std::path::Path>,
    application_credentials: ApplicationCredentialsStore,
) -> PyBridgeResult<PyObject> {
    // Skip registry initialization (the `false` parameter) since config flows
    // don't need them and they require an asyncio event loop which isn't available
    // when called from a sync REST handler context.
    create_hass_wrapper_internal(
        py,
        bus,
        states,
        services,
        registries,
        config_dir,
        None,
        false,
        Some(application_credentials),
    )
}

/// Internal implementation of hass wrapper creation
///
/// # Arguments
/// * `init_registries` - If true, initialize Python HA registries. Set to false
///                       for config flows to avoid asyncio event loop requirements.
/// * `application_credentials` - Optional credentials store for OAuth integrations.
///                               If provided, injects into Python's hass.data for OAuth flows.
fn create_hass_wrapper_internal(
    py: Python<'_>,
    bus: Arc<EventBus>,
    states: Arc<StateStore>,
    services: Arc<ServiceRegistry>,
    registries: Arc<Registries>,
    config_dir: Option<&std::path::Path>,
    event_loop: Option<PyObject>,
    init_registries: bool,
    application_credentials: Option<ApplicationCredentialsStore>,
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

    // Add loop attribute - use provided event loop or get/create one
    // IMPORTANT: This must be the same loop that AsyncBridge uses, otherwise
    // Futures created by hass.loop.create_future() will be on a different loop
    let asyncio = py.import_bound("asyncio")?;
    let threading = py.import_bound("threading")?;
    let loop_ = if let Some(loop_obj) = event_loop {
        // Use the provided event loop (from AsyncBridge)
        loop_obj
    } else {
        // Fallback: get running loop or create one
        match asyncio.call_method0("get_running_loop") {
            Ok(loop_) => loop_.unbind(),
            Err(_) => {
                // No running loop, create one
                asyncio.call_method0("new_event_loop")?.unbind()
            }
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
    // NOTE: Skip for config flows since they don't need registries and the initialization
    // requires an asyncio event loop which isn't available in sync REST handler context
    if init_registries {
        initialize_ha_registries(py, &hass, config_dir)?;
    }

    // Inject application credentials into hass.data for OAuth config flows
    if let Some(credentials_store) = application_credentials {
        inject_application_credentials(py, &hass, &credentials_store)?;
    }

    Ok(hass.into_any())
}

/// Inject application credentials into Python's hass.data
///
/// This sets up the `application_credentials` component's storage in hass.data
/// so that Python OAuth config flows can find the credentials stored in Rust.
fn inject_application_credentials(
    py: Python<'_>,
    hass: &Py<HassWrapper>,
    credentials_store: &ApplicationCredentialsStore,
) -> PyResult<()> {
    // Convert credentials to Python-compatible format
    let credentials_list: Vec<_> = credentials_store
        .iter()
        .map(|entry| {
            let cred = entry.value();
            serde_json::json!({
                "id": cred.id,
                "domain": cred.domain,
                "client_id": cred.client_id,
                "client_secret": cred.client_secret,
                "auth_domain": cred.auth_domain,
                "name": cred.name,
            })
        })
        .collect();

    let credentials_json =
        serde_json::to_string(&credentials_list).unwrap_or_else(|_| "[]".to_string());

    // Python code to set up application_credentials component in hass.data
    // This creates a mock storage collection that provides credentials to OAuth flows
    let code = include_str!("../../embedded_python/application_credentials.py");

    let globals = PyDict::new_bound(py);
    globals.set_item("_hass", hass.bind(py))?;
    globals.set_item("_credentials_json", credentials_json)?;

    py.run_bound(code, Some(&globals), None)?;

    Ok(())
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
    let code = include_str!("../../embedded_python/registries_init.py");

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
    let code = include_str!("../../embedded_python/config_entries_wrapper.py");

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

    // Add async_entry_for_domain_unique_id method for checking existing entries by unique_id
    let async_entry_for_domain_unique_id = globals
        .get_item("async_entry_for_domain_unique_id")?
        .unwrap();
    wrapper.setattr(
        "async_entry_for_domain_unique_id",
        async_entry_for_domain_unique_id,
    )?;

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

def async_progress_by_handler(handler, match_context=None, include_uninitialized=False):
    """Return the flows in progress by handler.

    Returns list of flow progress dicts for the given handler (domain).
    For now, returns empty list - no flows in progress tracked.
    """
    _LOGGER.debug(f"async_progress_by_handler({handler}, match_context={match_context})")
    return []

def async_progress(include_uninitialized=False):
    """Return all flows in progress.

    For now, returns empty list - no flows in progress tracked.
    """
    return []
"#;

    let globals = PyDict::new_bound(py);
    py.run_bound(code, Some(&globals), None)?;

    let async_init = globals.get_item("async_init")?.unwrap();
    flow.setattr("async_init", async_init)?;

    let async_progress_by_handler = globals.get_item("async_progress_by_handler")?.unwrap();
    flow.setattr("async_progress_by_handler", async_progress_by_handler)?;

    let async_progress = globals.get_item("async_progress")?.unwrap();
    flow.setattr("async_progress", async_progress)?;

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
            let states = Arc::new(StateStore::new(bus.clone()));
            let services = Arc::new(ServiceRegistry::new());
            let registries = Arc::new(Registries::new(temp_dir.path()));

            let result = create_hass_wrapper(py, bus, states, services, registries, None, None);
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
