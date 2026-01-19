//! PyO3 class wrappers for Home Assistant components
//!
//! These `#[pyclass]` structs replace Python SimpleNamespace wrappers,
//! allowing Python integrations to call directly into Rust code.

use ha_core::{Context, EntityId, Event};
use ha_event_bus::EventBus;
use ha_registries::{DeviceConnection, DeviceIdentifier, Registries};
use ha_service_registry::ServiceRegistry;
use ha_state_machine::StateMachine;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyTuple};
use std::collections::HashMap;
use std::sync::Arc;

/// Convert a Python value to serde_json::Value
fn py_to_json(value: &Bound<'_, PyAny>) -> serde_json::Value {
    if value.is_none() {
        return serde_json::Value::Null;
    }
    if let Ok(b) = value.extract::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Ok(i) = value.extract::<i64>() {
        return serde_json::Value::Number(i.into());
    }
    if let Ok(f) = value.extract::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }
    if let Ok(s) = value.extract::<String>() {
        return serde_json::Value::String(s);
    }
    if let Ok(list) = value.downcast::<PyList>() {
        let arr: Vec<serde_json::Value> = list.iter().map(|item| py_to_json(&item)).collect();
        return serde_json::Value::Array(arr);
    }
    if let Ok(dict) = value.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            if let Ok(key) = k.extract::<String>() {
                map.insert(key, py_to_json(&v));
            }
        }
        return serde_json::Value::Object(map);
    }
    // Default to string representation
    serde_json::Value::String(value.to_string())
}

// ============================================================================
// StatesWrapper - wraps Rust StateMachine
// ============================================================================

/// Python wrapper for the Rust StateMachine
///
/// Provides direct access to state storage without Python intermediaries.
#[pyclass(name = "StatesWrapper")]
pub struct StatesWrapper {
    states: Arc<StateMachine>,
}

impl StatesWrapper {
    pub fn new(states: Arc<StateMachine>) -> Self {
        Self { states }
    }
}

#[pymethods]
impl StatesWrapper {
    /// Get the state of an entity
    ///
    /// Returns a dict with 'state' and 'attributes', or None if not found.
    fn get(&self, py: Python<'_>, entity_id: &str) -> PyResult<PyObject> {
        match self.states.get(entity_id) {
            Some(state) => {
                let dict = PyDict::new_bound(py);
                dict.set_item("state", &state.state)?;
                dict.set_item("entity_id", state.entity_id.to_string())?;

                // Convert attributes to Python dict
                let attrs = PyDict::new_bound(py);
                for (key, value) in &state.attributes {
                    let py_value = json_to_py(py, value)?;
                    attrs.set_item(key, py_value)?;
                }
                dict.set_item("attributes", attrs)?;

                Ok(dict.into())
            }
            None => Ok(py.None()),
        }
    }

    /// Set the state of an entity (sync version)
    #[pyo3(signature = (entity_id, new_state, attributes=None, _force_update=None, _context=None))]
    fn set(
        &self,
        entity_id: &str,
        new_state: &str,
        attributes: Option<&Bound<'_, PyDict>>,
        _force_update: Option<bool>,
        _context: Option<PyObject>,
    ) -> PyResult<()> {
        let entity_id: EntityId = entity_id
            .parse()
            .map_err(|e| PyValueError::new_err(format!("Invalid entity_id: {}", e)))?;

        let attrs = match attributes {
            Some(dict) => {
                let mut map = HashMap::new();
                for (k, v) in dict.iter() {
                    if let Ok(key) = k.extract::<String>() {
                        map.insert(key, py_to_json(&v));
                    }
                }
                map
            }
            None => HashMap::new(),
        };

        let context = Context::new();
        self.states.set(entity_id, new_state, attrs, context);
        Ok(())
    }

    /// Set the state of an entity (async version)
    ///
    /// Note: This is actually sync since Rust StateMachine operations are fast.
    /// The async interface is for compatibility with HA's API.
    #[pyo3(signature = (entity_id, new_state, attributes=None, force_update=None, context=None))]
    fn async_set<'py>(
        &self,
        py: Python<'py>,
        entity_id: &str,
        new_state: &str,
        attributes: Option<&Bound<'py, PyDict>>,
        force_update: Option<bool>,
        context: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Perform the sync operation
        self.set(entity_id, new_state, attributes, force_update, context)?;

        // Return a completed coroutine
        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (py.None(),))?;
        Ok(future)
    }

    /// Get all entity IDs
    #[pyo3(signature = (domain_filter=None))]
    fn async_entity_ids<'py>(
        &self,
        py: Python<'py>,
        domain_filter: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let entity_ids: Vec<String> = if let Some(domain) = domain_filter {
            self.states
                .all()
                .iter()
                .filter(|s| s.entity_id.domain() == domain)
                .map(|s| s.entity_id.to_string())
                .collect()
        } else {
            self.states
                .all()
                .iter()
                .map(|s| s.entity_id.to_string())
                .collect()
        };

        // Return as completed future
        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        let list = PyList::new_bound(py, entity_ids);
        future.call_method1("set_result", (list,))?;
        Ok(future)
    }
}

// ============================================================================
// BusWrapper - wraps Rust EventBus
// ============================================================================

/// Python wrapper for the Rust EventBus
#[pyclass(name = "BusWrapper")]
pub struct BusWrapper {
    bus: Arc<EventBus>,
}

impl BusWrapper {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self { bus }
    }
}

#[pymethods]
impl BusWrapper {
    /// Fire an event
    #[pyo3(signature = (event_type, event_data=None, _origin=None, _context=None))]
    fn async_fire<'py>(
        &self,
        py: Python<'py>,
        event_type: &str,
        event_data: Option<&Bound<'py, PyDict>>,
        _origin: Option<&str>,
        _context: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Convert event data to JSON
        let data: serde_json::Value = match event_data {
            Some(dict) => py_to_json(dict.as_any()),
            None => serde_json::Value::Object(serde_json::Map::new()),
        };

        // Fire the event via Rust EventBus
        let context = Context::new();
        let event = Event::new(event_type, data, context);
        self.bus.fire(event);

        tracing::debug!(event_type = %event_type, "Fired event via Rust EventBus");

        // Return completed future
        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (py.None(),))?;
        Ok(future)
    }

    /// Listen for events (placeholder - returns a dummy unsub function)
    fn async_listen<'py>(
        &self,
        py: Python<'py>,
        event_type: &str,
        _listener: PyObject,
    ) -> PyResult<Bound<'py, PyAny>> {
        tracing::debug!(event_type = %event_type, "Event listener registered (stub)");

        // Return a dummy unsubscribe function
        let code = "lambda: None";
        let unsub = py.eval_bound(code, None, None)?;

        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (unsub,))?;
        Ok(future)
    }
}

// ============================================================================
// ServicesWrapper - wraps Rust ServiceRegistry
// ============================================================================

/// Python wrapper for the Rust ServiceRegistry
#[pyclass(name = "ServicesWrapper")]
pub struct ServicesWrapper {
    services: Arc<ServiceRegistry>,
}

impl ServicesWrapper {
    pub fn new(services: Arc<ServiceRegistry>) -> Self {
        Self { services }
    }
}

#[pymethods]
impl ServicesWrapper {
    /// Call a service
    #[pyo3(signature = (domain, service, service_data=None, _blocking=None, _context=None, _target=None))]
    fn async_call<'py>(
        &self,
        py: Python<'py>,
        domain: &str,
        service: &str,
        service_data: Option<&Bound<'py, PyDict>>,
        _blocking: Option<bool>,
        _context: Option<PyObject>,
        _target: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _data: serde_json::Value = match service_data {
            Some(dict) => py_to_json(dict.as_any()),
            None => serde_json::Value::Object(serde_json::Map::new()),
        };

        tracing::debug!(domain = %domain, service = %service, "Service call via Rust");

        // Note: ServiceRegistry::call is async, so we just log for now
        // TODO: Bridge to Tokio runtime for actual service calls
        let _ = self.services.has_service(domain, service);

        // Return completed future
        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (py.None(),))?;
        Ok(future)
    }

    /// Register a service
    #[pyo3(signature = (domain, service, _service_func, _schema=None))]
    fn async_register<'py>(
        &self,
        py: Python<'py>,
        domain: &str,
        service: &str,
        _service_func: PyObject,
        _schema: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        tracing::debug!(domain = %domain, service = %service, "Service registration (stub)");

        // TODO: Actually register the Python service function
        // For now, just log it

        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (py.None(),))?;
        Ok(future)
    }

    /// Check if a service exists
    fn has_service(&self, domain: &str, service: &str) -> bool {
        self.services.has_service(domain, service)
    }
}

// ============================================================================
// ConfigWrapper - configuration data
// ============================================================================

/// Python wrapper for Home Assistant configuration
#[pyclass(name = "ConfigWrapper")]
pub struct ConfigWrapper {
    #[pyo3(get)]
    config_dir: String,
    #[pyo3(get)]
    latitude: f64,
    #[pyo3(get)]
    longitude: f64,
    #[pyo3(get)]
    elevation: i32,
    #[pyo3(get)]
    time_zone: String,
    #[pyo3(get)]
    units: String,
    #[pyo3(get)]
    location_name: String,
    #[pyo3(get)]
    internal_url: Option<String>,
    #[pyo3(get)]
    external_url: Option<String>,
    components: Py<PySet>,
}

impl ConfigWrapper {
    pub fn new(py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            config_dir: "/config".to_string(),
            latitude: 32.87336,
            longitude: -117.22743,
            elevation: 0,
            time_zone: "UTC".to_string(),
            units: "metric".to_string(),
            location_name: "Home".to_string(),
            internal_url: None,
            external_url: None,
            components: PySet::empty_bound(py)?.unbind(),
        })
    }
}

#[pymethods]
impl ConfigWrapper {
    #[getter]
    fn components(&self, py: Python<'_>) -> PyResult<Py<PySet>> {
        Ok(self.components.clone_ref(py))
    }
}

// ============================================================================
// RegistriesWrapper - wraps Rust Registries for device/entity registration
// ============================================================================

/// Python wrapper for the Rust Registries
///
/// Provides methods to register devices and entities from Python integrations.
#[pyclass(name = "RegistriesWrapper")]
pub struct RegistriesWrapper {
    registries: Arc<Registries>,
}

impl RegistriesWrapper {
    pub fn new(registries: Arc<Registries>) -> Self {
        Self { registries }
    }
}

#[pymethods]
impl RegistriesWrapper {
    /// Register a device and return its device_id
    ///
    /// # Arguments
    /// * `config_entry_id` - The config entry that owns this device
    /// * `identifiers` - List of (domain, id) tuples to identify the device
    /// * `connections` - List of (connection_type, id) tuples (e.g., MAC addresses)
    /// * `name` - Device name
    /// * `manufacturer` - Optional manufacturer name
    /// * `model` - Optional model name
    /// * `sw_version` - Optional software version
    /// * `hw_version` - Optional hardware version
    #[pyo3(signature = (config_entry_id, identifiers, connections, name, manufacturer=None, model=None, sw_version=None, hw_version=None))]
    fn register_device(
        &self,
        config_entry_id: &str,
        identifiers: &Bound<'_, PyList>,
        connections: &Bound<'_, PyList>,
        name: &str,
        manufacturer: Option<&str>,
        model: Option<&str>,
        sw_version: Option<&str>,
        hw_version: Option<&str>,
    ) -> PyResult<String> {
        // Convert identifiers from Python list of tuples to Vec<DeviceIdentifier>
        let mut device_identifiers = Vec::new();
        for item in identifiers.iter() {
            if let Ok(tuple) = item.downcast::<PyTuple>() {
                if tuple.len() >= 2 {
                    let domain: String = tuple.get_item(0)?.extract()?;
                    let id: String = tuple.get_item(1)?.extract()?;
                    device_identifiers.push(DeviceIdentifier::new(domain, id));
                }
            } else if let Ok(list) = item.downcast::<PyList>() {
                if list.len() >= 2 {
                    let domain: String = list.get_item(0)?.extract()?;
                    let id: String = list.get_item(1)?.extract()?;
                    device_identifiers.push(DeviceIdentifier::new(domain, id));
                }
            }
        }

        // Convert connections from Python list of tuples to Vec<DeviceConnection>
        let mut device_connections = Vec::new();
        for item in connections.iter() {
            if let Ok(tuple) = item.downcast::<PyTuple>() {
                if tuple.len() >= 2 {
                    let conn_type: String = tuple.get_item(0)?.extract()?;
                    let id: String = tuple.get_item(1)?.extract()?;
                    device_connections.push(DeviceConnection::new(conn_type, id));
                }
            } else if let Ok(list) = item.downcast::<PyList>() {
                if list.len() >= 2 {
                    let conn_type: String = list.get_item(0)?.extract()?;
                    let id: String = list.get_item(1)?.extract()?;
                    device_connections.push(DeviceConnection::new(conn_type, id));
                }
            }
        }

        // Register the device
        let mut entry = self.registries.devices.get_or_create(
            &device_identifiers,
            &device_connections,
            Some(config_entry_id),
            name,
        );

        // Update additional fields
        if manufacturer.is_some() || model.is_some() || sw_version.is_some() || hw_version.is_some()
        {
            if let Some(updated) = self.registries.devices.update(&entry.id, |e| {
                if let Some(m) = manufacturer {
                    e.manufacturer = Some(m.to_string());
                }
                if let Some(m) = model {
                    e.model = Some(m.to_string());
                }
                if let Some(v) = sw_version {
                    e.sw_version = Some(v.to_string());
                }
                if let Some(v) = hw_version {
                    e.hw_version = Some(v.to_string());
                }
            }) {
                entry = updated;
            }
        }

        tracing::info!(
            device_id = %entry.id,
            name = %name,
            "Registered device in Rust registry"
        );

        Ok(entry.id)
    }

    /// Register an entity and return its entry info
    ///
    /// # Arguments
    /// * `platform` - Platform name (e.g., "light", "sensor")
    /// * `entity_id` - The entity ID (e.g., "light.living_room")
    /// * `unique_id` - Optional unique identifier for the entity
    /// * `config_entry_id` - The config entry that owns this entity
    /// * `device_id` - Optional device ID to link this entity to
    /// * `name` - Optional entity name
    #[pyo3(signature = (platform, entity_id, unique_id=None, config_entry_id=None, device_id=None, name=None))]
    fn register_entity(
        &self,
        py: Python<'_>,
        platform: &str,
        entity_id: &str,
        unique_id: Option<&str>,
        config_entry_id: Option<&str>,
        device_id: Option<&str>,
        name: Option<&str>,
    ) -> PyResult<PyObject> {
        let mut entry = self.registries.entities.get_or_create(
            platform,
            entity_id,
            unique_id,
            config_entry_id,
            device_id,
        );

        // Update name if provided
        if let Some(n) = name {
            entry = self.registries.entities.update(&entry.entity_id, |e| {
                e.name = Some(n.to_string());
            });
        }

        tracing::info!(
            entity_id = %entity_id,
            platform = %platform,
            device_id = ?device_id,
            "Registered entity in Rust registry"
        );

        // Return entry info as a dict
        let dict = PyDict::new_bound(py);
        dict.set_item("entity_id", &entry.entity_id)?;
        dict.set_item("unique_id", &entry.unique_id)?;
        dict.set_item("platform", &entry.platform)?;
        dict.set_item("config_entry_id", &entry.config_entry_id)?;
        dict.set_item("device_id", &entry.device_id)?;
        dict.set_item("name", &entry.name)?;
        dict.set_item("id", &entry.id)?;

        Ok(dict.into())
    }

    /// Get device count
    fn device_count(&self) -> usize {
        self.registries.devices.len()
    }

    /// Get entity count
    fn entity_count(&self) -> usize {
        self.registries.entities.len()
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Convert serde_json::Value to Python object
fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
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
            let list = PyList::empty_bound(py);
            for item in arr {
                list.append(json_to_py(py, item)?)?;
            }
            Ok(list.into())
        }
        serde_json::Value::Object(obj) => {
            let dict = PyDict::new_bound(py);
            for (k, v) in obj {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
            Ok(dict.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_states_wrapper() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let bus = Arc::new(EventBus::new());
            let states = Arc::new(StateMachine::new(bus));
            let wrapper = StatesWrapper::new(states);

            // Test set and get
            let attrs = PyDict::new_bound(py);
            attrs.set_item("friendly_name", "Test Light").unwrap();

            wrapper
                .set("light.test", "on", Some(&attrs), None, None)
                .unwrap();

            let result = wrapper.get(py, "light.test").unwrap();
            assert!(!result.is_none(py));

            let dict = result.bind(py).downcast::<PyDict>().unwrap();
            let state: String = dict.get_item("state").unwrap().unwrap().extract().unwrap();
            assert_eq!(state, "on");
        });
    }

    #[test]
    fn test_bus_wrapper() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let bus = Arc::new(EventBus::new());
            let wrapper = BusWrapper::new(bus);

            let data = PyDict::new_bound(py);
            data.set_item("test", "value").unwrap();

            // Should not panic
            let _ = wrapper.async_fire(py, "test_event", Some(&data), None, None);
        });
    }

    #[test]
    fn test_config_wrapper() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let config = ConfigWrapper::new(py).unwrap();
            assert_eq!(config.latitude, 32.87336);
            assert_eq!(config.time_zone, "UTC");
        });
    }
}
