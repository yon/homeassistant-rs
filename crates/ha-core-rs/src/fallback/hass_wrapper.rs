//! Python HomeAssistant wrapper
//!
//! Creates a Python-compatible HomeAssistant object that wraps our Rust core
//! for passing to Python integrations.

use ha_event_bus::EventBus;
use ha_service_registry::ServiceRegistry;
use ha_state_machine::StateMachine;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PySet};
use std::sync::Arc;

use super::errors::FallbackResult;

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
) -> FallbackResult<PyObject> {
    // Create a simple namespace object to hold our attributes
    let types = py.import_bound("types")?;
    let simple_namespace = types.getattr("SimpleNamespace")?;

    // Create the hass object
    let hass = simple_namespace.call0()?;

    // Add data dict for integrations to store data
    let data = PyDict::new_bound(py);
    hass.setattr("data", data)?;

    // Create wrapper objects for bus, states, services
    // Bus wrapper with async_fire method
    let bus_wrapper = create_bus_wrapper(py, bus)?;
    hass.setattr("bus", bus_wrapper)?;

    // States wrapper with get/set methods
    let states_wrapper = create_states_wrapper(py, states)?;
    hass.setattr("states", states_wrapper)?;

    // Services wrapper with async_call method
    let services_wrapper = create_services_wrapper(py, services)?;
    hass.setattr("services", services_wrapper)?;

    // Config entries wrapper with platform setup methods
    let config_entries_wrapper = create_config_entries_wrapper(py)?;
    hass.setattr("config_entries", config_entries_wrapper)?;

    // Add config attribute with location and components
    let config = create_config_wrapper(py)?;
    hass.setattr("config", config)?;

    // Add loop attribute (get the running event loop or create one)
    let asyncio = py.import_bound("asyncio")?;
    match asyncio.call_method0("get_running_loop") {
        Ok(loop_) => hass.setattr("loop", loop_)?,
        Err(_) => {
            // No running loop, create one
            let loop_ = asyncio.call_method0("new_event_loop")?;
            hass.setattr("loop", loop_)?;
        }
    }

    // Add async_create_task method
    let async_create_task = create_async_create_task(py)?;
    hass.setattr("async_create_task", async_create_task)?;

    // Add helpers attribute for helper utilities
    let helpers = simple_namespace.call0()?;
    hass.setattr("helpers", helpers)?;

    Ok(hass.unbind())
}

/// Create a bus wrapper with async_fire method
fn create_bus_wrapper(py: Python<'_>, _bus: Arc<EventBus>) -> PyResult<PyObject> {
    let types = py.import_bound("types")?;
    let simple_namespace = types.getattr("SimpleNamespace")?;
    let bus = simple_namespace.call0()?;

    // For now, create a simple fire function that logs
    // In the future, this should actually fire events via our Rust EventBus
    let code = r#"
async def async_fire(event_type, event_data=None, origin=None, context=None):
    """Fire an event."""
    import logging
    logging.getLogger(__name__).debug(f"Event fired: {event_type}")
"#;

    let globals = PyDict::new_bound(py);
    py.run_bound(code, Some(&globals), None)?;
    let async_fire = globals.get_item("async_fire")?.unwrap();
    bus.setattr("async_fire", async_fire)?;

    Ok(bus.unbind())
}

/// Create a states wrapper with get method
fn create_states_wrapper(py: Python<'_>, states: Arc<StateMachine>) -> PyResult<PyObject> {
    let types = py.import_bound("types")?;
    let simple_namespace = types.getattr("SimpleNamespace")?;
    let wrapper = simple_namespace.call0()?;

    // Create a get function that retrieves state from our StateMachine
    // We need to capture the states Arc, but PyO3 closures are tricky
    // For now, create a simple wrapper that returns None
    let code = r#"
def get(entity_id):
    """Get state of an entity."""
    return None

async def async_set(entity_id, new_state, attributes=None, force_update=False, context=None):
    """Set state of an entity."""
    import logging
    logging.getLogger(__name__).debug(f"State set: {entity_id} = {new_state}")
"#;

    let globals = PyDict::new_bound(py);
    py.run_bound(code, Some(&globals), None)?;

    let get_fn = globals.get_item("get")?.unwrap();
    wrapper.setattr("get", get_fn)?;

    let async_set = globals.get_item("async_set")?.unwrap();
    wrapper.setattr("async_set", async_set)?;

    // Store the actual states for later use if needed
    let _ = states; // Currently unused but will be needed for real implementation

    Ok(wrapper.unbind())
}

/// Create a services wrapper with async_call method
fn create_services_wrapper(py: Python<'_>, _services: Arc<ServiceRegistry>) -> PyResult<PyObject> {
    let types = py.import_bound("types")?;
    let simple_namespace = types.getattr("SimpleNamespace")?;
    let wrapper = simple_namespace.call0()?;

    // Create async_call that logs for now
    let code = r#"
async def async_call(domain, service, service_data=None, blocking=False, context=None, target=None):
    """Call a service."""
    import logging
    logging.getLogger(__name__).debug(f"Service called: {domain}.{service}")

async def async_register(domain, service, service_func, schema=None):
    """Register a service."""
    import logging
    logging.getLogger(__name__).debug(f"Service registered: {domain}.{service}")
"#;

    let globals = PyDict::new_bound(py);
    py.run_bound(code, Some(&globals), None)?;

    let async_call = globals.get_item("async_call")?.unwrap();
    wrapper.setattr("async_call", async_call)?;

    let async_register = globals.get_item("async_register")?.unwrap();
    wrapper.setattr("async_register", async_register)?;

    Ok(wrapper.unbind())
}

/// Create a config_entries wrapper with platform setup methods
///
/// Provides:
/// - `async_forward_entry_setups(entry, platforms)` - Forward setup to platforms
/// - `async_unload_platforms(entry, platforms)` - Unload platforms
/// - `flow.async_init(domain, context, data)` - Initialize config flow
fn create_config_entries_wrapper(py: Python<'_>) -> PyResult<PyObject> {
    let types = py.import_bound("types")?;
    let simple_namespace = types.getattr("SimpleNamespace")?;
    let wrapper = simple_namespace.call0()?;

    // Create the config entries methods
    let code = r#"
import logging
import asyncio

_LOGGER = logging.getLogger(__name__)

# Store for loaded platforms per entry
_loaded_platforms = {}

async def async_forward_entry_setups(entry, platforms):
    """Forward the setup of an entry to platforms.

    This is called by integrations to set up their platforms.
    For now, we log the platforms and simulate successful setup.
    """
    entry_id = entry.get("entry_id") if isinstance(entry, dict) else getattr(entry, "entry_id", "unknown")
    domain = entry.get("domain") if isinstance(entry, dict) else getattr(entry, "domain", "unknown")

    _LOGGER.info(f"Forward entry setup for {domain} ({entry_id}): {list(platforms)}")

    # Track which platforms are loaded for this entry
    if entry_id not in _loaded_platforms:
        _loaded_platforms[entry_id] = set()

    for platform in platforms:
        platform_name = str(platform).split(".")[-1] if "." in str(platform) else str(platform)
        _loaded_platforms[entry_id].add(platform_name)
        _LOGGER.debug(f"  Platform {platform_name} setup complete")

    # Simulate async work
    await asyncio.sleep(0)

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

    let globals = PyDict::new_bound(py);
    py.run_bound(code, Some(&globals), None)?;

    let async_forward_entry_setups = globals.get_item("async_forward_entry_setups")?.unwrap();
    wrapper.setattr("async_forward_entry_setups", async_forward_entry_setups)?;

    let async_unload_platforms = globals.get_item("async_unload_platforms")?.unwrap();
    wrapper.setattr("async_unload_platforms", async_unload_platforms)?;

    let async_forward_entry_setup = globals.get_item("async_forward_entry_setup")?.unwrap();
    wrapper.setattr("async_forward_entry_setup", async_forward_entry_setup)?;

    let async_forward_entry_unload = globals.get_item("async_forward_entry_unload")?.unwrap();
    wrapper.setattr("async_forward_entry_unload", async_forward_entry_unload)?;

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

/// Create a config wrapper with location and components
fn create_config_wrapper(py: Python<'_>) -> PyResult<PyObject> {
    let types = py.import_bound("types")?;
    let simple_namespace = types.getattr("SimpleNamespace")?;
    let config = simple_namespace.call0()?;

    // Basic config attributes
    config.setattr("config_dir", "/config")?;
    config.setattr("latitude", 32.87336)?; // Default: San Diego
    config.setattr("longitude", -117.22743)?;
    config.setattr("elevation", 0)?;
    config.setattr("time_zone", "UTC")?;
    config.setattr("units", "metric")?;
    config.setattr("location_name", "Home")?;

    // Components set - tracks loaded components
    let components = PySet::empty_bound(py)?;
    config.setattr("components", components)?;

    // Internal URL (for some integrations)
    config.setattr("internal_url", py.None())?;
    config.setattr("external_url", py.None())?;

    Ok(config.unbind())
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

    #[test]
    fn test_create_hass_wrapper() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let bus = Arc::new(EventBus::new());
            let states = Arc::new(StateMachine::new(bus.clone()));
            let services = Arc::new(ServiceRegistry::new());

            let result = create_hass_wrapper(py, bus, states, services);
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
