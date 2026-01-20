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

    /// Check if an entity_id is available (not already in use)
    ///
    /// Returns True if the entity_id is available, False if already taken.
    fn async_available(&self, entity_id: &str) -> bool {
        self.states.get(entity_id).is_none()
    }

    /// Reserve an entity_id so nothing else can use it
    ///
    /// In HA, this sets the state to STATE_UNAVAILABLE to reserve the id.
    fn async_reserve(&self, entity_id: &str) -> PyResult<()> {
        let entity_id: EntityId = entity_id
            .parse()
            .map_err(|e| PyValueError::new_err(format!("Invalid entity_id: {}", e)))?;

        // Reserve by setting to "unavailable"
        let context = Context::new();
        self.states
            .set(entity_id, "unavailable", HashMap::new(), context);
        Ok(())
    }

    /// Remove an entity's state
    #[pyo3(signature = (entity_id, _context=None))]
    fn async_remove<'py>(
        &self,
        py: Python<'py>,
        entity_id: &str,
        _context: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Try to remove the state (if it exists)
        if let Ok(eid) = entity_id.parse() {
            let context = Context::new();
            self.states.remove(&eid, context);
        }

        // Return completed future
        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (true,))?;
        Ok(future)
    }

    /// Internal set method used by Entity._async_write_ha_state
    ///
    /// This is the method that entities call to write their state.
    /// It has more parameters than async_set for internal use.
    #[pyo3(signature = (entity_id, new_state, attributes=None, force_update=None, context=None, state_info=None, timestamp=None))]
    fn async_set_internal<'py>(
        &self,
        py: Python<'py>,
        entity_id: &str,
        new_state: &str,
        attributes: Option<&Bound<'py, PyDict>>,
        force_update: Option<bool>,
        context: Option<PyObject>,
        state_info: Option<PyObject>,
        timestamp: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = (state_info, timestamp); // Suppress unused warnings
                                         // Just delegate to async_set - internal details handled by Rust
        self.set(entity_id, new_state, attributes, force_update, context)?;

        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (py.None(),))?;
        Ok(future)
    }

    /// Get all states with optional domain filter
    #[pyo3(signature = (domain_filter=None))]
    fn async_all<'py>(
        &self,
        py: Python<'py>,
        domain_filter: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let states: Vec<_> = if let Some(domain) = domain_filter {
            self.states
                .all()
                .into_iter()
                .filter(|s| s.entity_id.domain() == domain)
                .collect()
        } else {
            self.states.all()
        };

        // Convert to list of dicts
        let list = PyList::empty_bound(py);
        for state in states {
            let dict = PyDict::new_bound(py);
            dict.set_item("entity_id", state.entity_id.to_string())?;
            dict.set_item("state", &state.state)?;

            let attrs = PyDict::new_bound(py);
            for (k, v) in &state.attributes {
                attrs.set_item(k, json_to_py(py, v)?)?;
            }
            dict.set_item("attributes", attrs)?;
            dict.set_item("last_changed", state.last_changed.to_rfc3339())?;
            dict.set_item("last_updated", state.last_updated.to_rfc3339())?;

            list.append(dict)?;
        }

        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (list,))?;
        Ok(future)
    }

    /// Count entities with optional domain filter
    fn async_entity_ids_count(&self, domain_filter: Option<&str>) -> usize {
        if let Some(domain) = domain_filter {
            self.states
                .all()
                .iter()
                .filter(|s| s.entity_id.domain() == domain)
                .count()
        } else {
            self.states.all().len()
        }
    }

    /// Check if entity is in specific state
    fn is_state(&self, entity_id: &str, state: &str) -> bool {
        match self.states.get(entity_id) {
            Some(s) => s.state == state,
            None => false,
        }
    }

    /// Sync version of entity_ids
    fn entity_ids(&self, domain_filter: Option<&str>) -> Vec<String> {
        if let Some(domain) = domain_filter {
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
        }
    }

    /// Sync version of all
    fn all<'py>(
        &self,
        py: Python<'py>,
        domain_filter: Option<&str>,
    ) -> PyResult<Bound<'py, PyList>> {
        let states: Vec<_> = if let Some(domain) = domain_filter {
            self.states
                .all()
                .into_iter()
                .filter(|s| s.entity_id.domain() == domain)
                .collect()
        } else {
            self.states.all()
        };

        let list = PyList::empty_bound(py);
        for state in states {
            let dict = PyDict::new_bound(py);
            dict.set_item("entity_id", state.entity_id.to_string())?;
            dict.set_item("state", &state.state)?;

            let attrs = PyDict::new_bound(py);
            for (k, v) in &state.attributes {
                attrs.set_item(k, json_to_py(py, v)?)?;
            }
            dict.set_item("attributes", attrs)?;
            list.append(dict)?;
        }
        Ok(list)
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
    #[pyo3(signature = (event_type, _listener, event_filter=None))]
    fn async_listen<'py>(
        &self,
        py: Python<'py>,
        event_type: &str,
        _listener: PyObject,
        event_filter: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = event_filter; // Silence unused warning
        tracing::debug!(event_type = %event_type, "Event listener registered (stub)");

        // Return a dummy unsubscribe function
        let code = "lambda: None";
        let unsub = py.eval_bound(code, None, None)?;

        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (unsub,))?;
        Ok(future)
    }

    /// Listen for an event once (placeholder - returns a dummy unsub function)
    #[pyo3(signature = (event_type, _listener, event_filter=None))]
    fn async_listen_once<'py>(
        &self,
        py: Python<'py>,
        event_type: &str,
        _listener: PyObject,
        event_filter: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = event_filter; // Silence unused warning
        tracing::debug!(event_type = %event_type, "One-time event listener registered (stub)");

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
// UnitSystemWrapper - unit system configuration
// ============================================================================

/// Python wrapper for Home Assistant unit system
#[pyclass(name = "UnitSystemWrapper")]
pub struct UnitSystemWrapper {
    #[pyo3(get)]
    length_unit: String,
    #[pyo3(get)]
    temperature_unit: String,
    #[pyo3(get)]
    mass_unit: String,
    #[pyo3(get)]
    volume_unit: String,
    #[pyo3(get)]
    pressure_unit: String,
    #[pyo3(get)]
    wind_speed_unit: String,
    #[pyo3(get)]
    accumulated_precipitation_unit: String,
    is_metric: bool,
}

impl UnitSystemWrapper {
    pub fn metric() -> Self {
        Self {
            length_unit: "km".to_string(),
            temperature_unit: "°C".to_string(),
            mass_unit: "g".to_string(),
            volume_unit: "L".to_string(),
            pressure_unit: "Pa".to_string(),
            wind_speed_unit: "m/s".to_string(),
            accumulated_precipitation_unit: "mm".to_string(),
            is_metric: true,
        }
    }

    pub fn imperial() -> Self {
        Self {
            length_unit: "mi".to_string(),
            temperature_unit: "°F".to_string(),
            mass_unit: "lb".to_string(),
            volume_unit: "gal".to_string(),
            pressure_unit: "psi".to_string(),
            wind_speed_unit: "mph".to_string(),
            accumulated_precipitation_unit: "in".to_string(),
            is_metric: false,
        }
    }
}

#[pymethods]
impl UnitSystemWrapper {
    #[getter]
    fn is_metric(&self) -> bool {
        self.is_metric
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
    location_name: String,
    #[pyo3(get)]
    internal_url: Option<String>,
    #[pyo3(get)]
    external_url: Option<String>,
    components: Py<PySet>,
    units: Py<UnitSystemWrapper>,
}

impl ConfigWrapper {
    pub fn new(py: Python<'_>) -> PyResult<Self> {
        let units = Py::new(py, UnitSystemWrapper::metric())?;
        Ok(Self {
            config_dir: "/config".to_string(),
            latitude: 32.87336,
            longitude: -117.22743,
            elevation: 0,
            time_zone: "UTC".to_string(),
            location_name: "Home".to_string(),
            internal_url: None,
            external_url: None,
            components: PySet::empty_bound(py)?.unbind(),
            units,
        })
    }
}

#[pymethods]
impl ConfigWrapper {
    #[getter]
    fn components(&self, py: Python<'_>) -> PyResult<Py<PySet>> {
        Ok(self.components.clone_ref(py))
    }

    #[getter]
    fn units(&self, py: Python<'_>) -> PyResult<Py<UnitSystemWrapper>> {
        Ok(self.units.clone_ref(py))
    }

    // Alias for backwards compatibility
    #[getter]
    fn unit_system(&self, py: Python<'_>) -> PyResult<Py<UnitSystemWrapper>> {
        Ok(self.units.clone_ref(py))
    }

    /// Return path to the config directory or a path within it
    ///
    /// If called with no arguments, returns the config directory.
    /// If called with a relative path, returns the joined path.
    #[pyo3(signature = (*args))]
    fn path(&self, args: &Bound<'_, PyTuple>) -> PyResult<String> {
        if args.is_empty() {
            Ok(self.config_dir.clone())
        } else {
            // Join all path segments
            let mut path = std::path::PathBuf::from(&self.config_dir);
            for arg in args.iter() {
                let segment: String = arg.extract()?;
                path.push(segment);
            }
            Ok(path.to_string_lossy().to_string())
        }
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
// ConfigEntryWrapper - wraps Rust ConfigEntry for Python integrations
// ============================================================================

use std::sync::RwLock;

/// Python wrapper for ConfigEntry
///
/// This provides a proper ConfigEntry-like object that supports:
/// - All standard readonly properties (entry_id, domain, data, etc.)
/// - runtime_data as a read/write property
/// - async_on_unload() method for cleanup callbacks
#[pyclass(name = "ConfigEntry")]
pub struct ConfigEntryWrapper {
    // Core fields
    entry_id: String,
    domain: String,
    title: String,
    version: u32,
    minor_version: u32,
    source: String,
    unique_id: Option<String>,
    state: String,
    // Data as Python dicts (stored as PyObject for easy Python access)
    data: PyObject,
    options: PyObject,
    discovery_keys: PyObject,
    // Mutable fields
    runtime_data: RwLock<PyObject>,
    // Callbacks registered via async_on_unload
    unload_callbacks: RwLock<Vec<PyObject>>,
}

impl ConfigEntryWrapper {
    /// Create a new ConfigEntryWrapper from Rust ConfigEntry data
    pub fn new(
        py: Python<'_>,
        entry_id: String,
        domain: String,
        title: String,
        version: u32,
        minor_version: u32,
        source: String,
        unique_id: Option<String>,
        state: String,
        data: &Bound<'_, PyDict>,
        options: &Bound<'_, PyDict>,
        discovery_keys: &Bound<'_, PyDict>,
    ) -> PyResult<Self> {
        Ok(Self {
            entry_id,
            domain,
            title,
            version,
            minor_version,
            source,
            unique_id,
            state,
            data: data.clone().unbind().into(),
            options: options.clone().unbind().into(),
            discovery_keys: discovery_keys.clone().unbind().into(),
            runtime_data: RwLock::new(py.None()),
            unload_callbacks: RwLock::new(Vec::new()),
        })
    }
}

#[pymethods]
impl ConfigEntryWrapper {
    /// Entry ID (readonly)
    #[getter]
    fn entry_id(&self) -> &str {
        &self.entry_id
    }

    /// Domain (readonly)
    #[getter]
    fn domain(&self) -> &str {
        &self.domain
    }

    /// Title (readonly)
    #[getter]
    fn title(&self) -> &str {
        &self.title
    }

    /// Version (readonly)
    #[getter]
    fn version(&self) -> u32 {
        self.version
    }

    /// Minor version (readonly)
    #[getter]
    fn minor_version(&self) -> u32 {
        self.minor_version
    }

    /// Source (readonly)
    #[getter]
    fn source(&self) -> &str {
        &self.source
    }

    /// Unique ID (readonly)
    #[getter]
    fn unique_id(&self) -> Option<&str> {
        self.unique_id.as_deref()
    }

    /// State (readonly) - returns the Python ConfigEntryState enum
    #[getter]
    fn state(&self, py: Python<'_>) -> PyResult<PyObject> {
        // Import the ConfigEntryState enum from homeassistant.config_entries
        let config_entries = py.import_bound("homeassistant.config_entries")?;
        let state_enum = config_entries.getattr("ConfigEntryState")?;

        // Map our string state to the enum value
        let enum_value = match self.state.as_str() {
            "not_loaded" => state_enum.getattr("NOT_LOADED")?,
            "setup_in_progress" => state_enum.getattr("SETUP_IN_PROGRESS")?,
            "loaded" => state_enum.getattr("LOADED")?,
            "setup_error" => state_enum.getattr("SETUP_ERROR")?,
            "setup_retry" => state_enum.getattr("SETUP_RETRY")?,
            "migration_error" => state_enum.getattr("MIGRATION_ERROR")?,
            "failed_unload" => state_enum.getattr("FAILED_UNLOAD")?,
            _ => state_enum.getattr("NOT_LOADED")?, // Default to NOT_LOADED
        };

        Ok(enum_value.unbind())
    }

    /// Data dict (readonly)
    #[getter]
    fn data(&self, py: Python<'_>) -> PyObject {
        self.data.clone_ref(py)
    }

    /// Options dict (readonly)
    #[getter]
    fn options(&self, py: Python<'_>) -> PyObject {
        self.options.clone_ref(py)
    }

    /// Discovery keys (readonly)
    #[getter]
    fn discovery_keys(&self, py: Python<'_>) -> PyObject {
        self.discovery_keys.clone_ref(py)
    }

    /// Runtime data (read/write)
    /// This is where integrations store their runtime state
    #[getter]
    fn runtime_data(&self, py: Python<'_>) -> PyObject {
        let data = self.runtime_data.read().unwrap();
        data.clone_ref(py)
    }

    #[setter]
    fn set_runtime_data(&self, value: PyObject) {
        let mut data = self.runtime_data.write().unwrap();
        *data = value;
    }

    /// Register a callback to be called when the entry is unloaded
    ///
    /// Returns a function that can be called to remove the callback.
    fn async_on_unload(&self, py: Python<'_>, callback: PyObject) -> PyResult<PyObject> {
        {
            let mut callbacks = self.unload_callbacks.write().unwrap();
            callbacks.push(callback.clone_ref(py));
        }

        // Return a function that removes this callback
        // For now, return None since the callback tracking is primarily for cleanup
        Ok(py.None())
    }

    /// Get all registered unload callbacks (for internal use)
    fn _get_unload_callbacks(&self, py: Python<'_>) -> Vec<PyObject> {
        let callbacks = self.unload_callbacks.read().unwrap();
        callbacks.iter().map(|cb| cb.clone_ref(py)).collect()
    }

    /// Call all unload callbacks (for cleanup)
    fn _run_unload_callbacks(&self, py: Python<'_>) -> PyResult<()> {
        let callbacks = self.unload_callbacks.read().unwrap();
        for callback in callbacks.iter() {
            // Try to call the callback
            if let Err(e) = callback.call0(py) {
                tracing::warn!("Unload callback failed: {}", e);
            }
        }
        Ok(())
    }

    /// Hash based on entry_id (for dict keys)
    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.entry_id.hash(&mut hasher);
        hasher.finish()
    }

    /// Equality based on entry_id
    fn __eq__(&self, other: &ConfigEntryWrapper) -> bool {
        self.entry_id == other.entry_id
    }

    /// String representation
    fn __repr__(&self) -> String {
        format!(
            "<ConfigEntry entry_id={} domain={} title={}>",
            self.entry_id, self.domain, self.title
        )
    }
}

// ============================================================================
// HassWrapper - hashable Home Assistant object for Python integrations
// ============================================================================

use std::sync::atomic::{AtomicU64, Ordering};

static HASS_INSTANCE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Python wrapper for the Home Assistant object
///
/// This provides a hashable HomeAssistant-like object that can be used as
/// a dictionary key or set element in Python code. SimpleNamespace isn't
/// hashable, so we need this custom class.
#[pyclass(name = "HomeAssistant")]
pub struct HassWrapper {
    /// Unique instance ID for hashing
    instance_id: u64,
    /// Event bus
    #[pyo3(get)]
    bus: Py<BusWrapper>,
    /// State machine
    #[pyo3(get)]
    states: Py<StatesWrapper>,
    /// Service registry
    #[pyo3(get)]
    services: Py<ServicesWrapper>,
    /// Configuration
    #[pyo3(get)]
    config: Py<ConfigWrapper>,
    /// Data storage dict
    data: Py<PyDict>,
    /// Config entries wrapper
    config_entries: PyObject,
    /// Helpers namespace
    helpers: PyObject,
    /// Event loop
    loop_: PyObject,
    /// Loop thread ID
    loop_thread_id: PyObject,
    /// async_create_task function
    async_create_task: PyObject,
    /// timeout context manager factory
    timeout: PyObject,
}

impl HassWrapper {
    pub fn new(
        py: Python<'_>,
        bus: Py<BusWrapper>,
        states: Py<StatesWrapper>,
        services: Py<ServicesWrapper>,
        config: Py<ConfigWrapper>,
        config_entries: PyObject,
        helpers: PyObject,
        loop_: PyObject,
        loop_thread_id: PyObject,
        async_create_task: PyObject,
        timeout: PyObject,
    ) -> PyResult<Self> {
        let data = PyDict::new_bound(py);
        // Add integrations dict that entities expect
        let integrations = PyDict::new_bound(py);
        data.set_item("integrations", &integrations)?;

        Ok(Self {
            instance_id: HASS_INSTANCE_COUNTER.fetch_add(1, Ordering::SeqCst),
            bus,
            states,
            services,
            config,
            data: data.unbind(),
            config_entries,
            helpers,
            loop_,
            loop_thread_id,
            async_create_task,
            timeout,
        })
    }
}

#[pymethods]
impl HassWrapper {
    /// Get the data dict
    #[getter]
    fn data(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        Ok(self.data.clone_ref(py))
    }

    /// Get config_entries
    #[getter]
    fn config_entries(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.config_entries.clone_ref(py))
    }

    /// Get helpers
    #[getter]
    fn helpers(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.helpers.clone_ref(py))
    }

    /// Get the event loop
    #[pyo3(name = "loop")]
    #[getter]
    fn get_loop(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.loop_.clone_ref(py))
    }

    /// Get the loop thread ID
    #[getter]
    fn get_loop_thread_id(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.loop_thread_id.clone_ref(py))
    }

    /// Get async_create_task
    #[getter]
    fn get_async_create_task(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.async_create_task.clone_ref(py))
    }

    /// Get timeout factory
    #[getter]
    fn get_timeout(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.timeout.clone_ref(py))
    }

    /// Verify we're running in the event loop thread
    ///
    /// In HA, this raises an error if called from wrong thread.
    /// We just no-op since we're always in the same thread context.
    fn verify_event_loop_thread(&self, _func_name: &str) {
        // No-op - we're always running in the right thread context
    }

    /// Hash based on instance ID (identity-based hashing)
    fn __hash__(&self) -> u64 {
        self.instance_id
    }

    /// Equality based on instance ID (identity-based equality)
    fn __eq__(&self, other: &HassWrapper) -> bool {
        self.instance_id == other.instance_id
    }

    /// String representation
    fn __repr__(&self) -> String {
        format!("<HomeAssistant instance_id={}>", self.instance_id)
    }

    /// Run a blocking function in the executor thread pool
    ///
    /// This is the key method that config flows need to run blocking I/O
    /// (like network requests, file operations, etc.) without blocking the event loop.
    ///
    /// # Arguments
    /// * `func` - The blocking function to run
    /// * `args` - Optional positional arguments to pass to the function
    ///
    /// # Returns
    /// A coroutine that will return the result of the function when awaited.
    #[pyo3(signature = (func, *args))]
    fn async_add_executor_job<'py>(
        &self,
        py: Python<'py>,
        func: PyObject,
        args: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Create a coroutine that runs the function in the executor
        let code = r#"
import asyncio
import concurrent.futures

# Create a module-level executor if not already created
if not hasattr(asyncio, '_ha_executor'):
    asyncio._ha_executor = concurrent.futures.ThreadPoolExecutor(max_workers=8)

async def _run_in_executor(func, *args):
    """Run a blocking function in the executor."""
    loop = asyncio.get_running_loop()
    return await loop.run_in_executor(asyncio._ha_executor, func, *args)
"#;
        let globals = pyo3::types::PyDict::new_bound(py);
        py.run_bound(code, Some(&globals), None)?;

        let run_fn = globals.get_item("_run_in_executor")?.unwrap();

        // Build the argument tuple: (func, *args)
        // Collect into a Vec first since chain() doesn't implement ExactSizeIterator
        let call_args: Vec<_> = std::iter::once(func.bind(py).clone())
            .chain(args.iter())
            .collect();
        let call_args = PyTuple::new_bound(py, call_args);

        // Call the async function to get the coroutine
        let coro = run_fn.call1(call_args)?;
        Ok(coro)
    }

    /// Run a blocking function in the executor (alternate signature with target)
    ///
    /// Some code passes the function as target=func, so we support that too.
    #[pyo3(signature = (target, *args))]
    fn add_executor_job<'py>(
        &self,
        py: Python<'py>,
        target: PyObject,
        args: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Delegate to async_add_executor_job
        self.async_add_executor_job(py, target, args)
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
