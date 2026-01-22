//! Python wrappers for DeviceRegistry

use ha_registries::device_registry::{
    DeviceConnection, DeviceEntry, DeviceEntryType, DeviceIdentifier, DeviceRegistry,
};
use ha_registries::entity_registry::DisabledBy;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PySet, PyTuple};
use std::sync::Arc;
use tokio::runtime::Handle;

/// Python wrapper for DeviceEntry
#[pyclass(name = "DeviceEntry")]
#[derive(Clone)]
pub struct PyDeviceEntry {
    inner: DeviceEntry,
}

#[pymethods]
impl PyDeviceEntry {
    #[getter]
    fn id(&self) -> &str {
        &self.inner.id
    }

    #[getter]
    fn identifiers(&self, py: Python<'_>) -> PyResult<Py<PySet>> {
        let set = PySet::empty_bound(py)?;
        for ident in &self.inner.identifiers {
            let tuple = PyTuple::new_bound(py, [ident.domain(), ident.id()]);
            set.add(tuple)?;
        }
        Ok(set.unbind())
    }

    #[getter]
    fn connections(&self, py: Python<'_>) -> PyResult<Py<PySet>> {
        let set = PySet::empty_bound(py)?;
        for conn in &self.inner.connections {
            let tuple = PyTuple::new_bound(py, [conn.connection_type(), conn.id()]);
            set.add(tuple)?;
        }
        Ok(set.unbind())
    }

    #[getter]
    fn config_entries(&self) -> Vec<String> {
        self.inner.config_entries.clone()
    }

    #[getter]
    fn primary_config_entry(&self) -> Option<&str> {
        self.inner.primary_config_entry.as_deref()
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[getter]
    fn name_by_user(&self) -> Option<&str> {
        self.inner.name_by_user.as_deref()
    }

    #[getter]
    fn manufacturer(&self) -> Option<&str> {
        self.inner.manufacturer.as_deref()
    }

    #[getter]
    fn model(&self) -> Option<&str> {
        self.inner.model.as_deref()
    }

    #[getter]
    fn model_id(&self) -> Option<&str> {
        self.inner.model_id.as_deref()
    }

    #[getter]
    fn hw_version(&self) -> Option<&str> {
        self.inner.hw_version.as_deref()
    }

    #[getter]
    fn sw_version(&self) -> Option<&str> {
        self.inner.sw_version.as_deref()
    }

    #[getter]
    fn serial_number(&self) -> Option<&str> {
        self.inner.serial_number.as_deref()
    }

    #[getter]
    fn via_device_id(&self) -> Option<&str> {
        self.inner.via_device_id.as_deref()
    }

    #[getter]
    fn entry_type(&self) -> Option<&str> {
        self.inner.entry_type.as_ref().map(|t| match t {
            DeviceEntryType::Service => "service",
        })
    }

    #[getter]
    fn disabled_by(&self) -> Option<&str> {
        self.inner.disabled_by.as_ref().map(|d| match d {
            DisabledBy::ConfigEntry => "config_entry",
            DisabledBy::Device => "device",
            DisabledBy::Hass => "hass",
            DisabledBy::Integration => "integration",
            DisabledBy::User => "user",
        })
    }

    #[getter]
    fn configuration_url(&self) -> Option<&str> {
        self.inner.configuration_url.as_deref()
    }

    #[getter]
    fn area_id(&self) -> Option<&str> {
        self.inner.area_id.as_deref()
    }

    #[getter]
    fn labels(&self) -> Vec<String> {
        self.inner.labels.clone()
    }

    #[getter]
    fn created_at(&self) -> String {
        self.inner.created_at.to_rfc3339()
    }

    #[getter]
    fn modified_at(&self) -> String {
        self.inner.modified_at.to_rfc3339()
    }

    fn is_disabled(&self) -> bool {
        self.inner.is_disabled()
    }

    fn __repr__(&self) -> String {
        format!(
            "DeviceEntry(id='{}', name='{}')",
            self.inner.id, self.inner.name
        )
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner.id == other.inner.id
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.id.hash(&mut hasher);
        hasher.finish()
    }
}

impl PyDeviceEntry {
    /// Create from Arc<DeviceEntry> - clones the inner value for Python ownership
    pub fn from_inner(inner: Arc<DeviceEntry>) -> Self {
        Self {
            inner: (*inner).clone(),
        }
    }

    pub fn inner(&self) -> &DeviceEntry {
        &self.inner
    }
}

fn parse_disabled_by(s: Option<&str>) -> Option<DisabledBy> {
    s.and_then(|s| match s {
        "config_entry" => Some(DisabledBy::ConfigEntry),
        "device" => Some(DisabledBy::Device),
        "hass" => Some(DisabledBy::Hass),
        "integration" => Some(DisabledBy::Integration),
        "user" => Some(DisabledBy::User),
        _ => None,
    })
}

fn parse_identifiers(py_set: &Bound<'_, PySet>) -> PyResult<Vec<DeviceIdentifier>> {
    let mut result = Vec::new();
    for item in py_set.iter() {
        let tuple = item.downcast::<PyTuple>()?;
        if tuple.len() == 2 {
            let domain: String = tuple.get_item(0)?.extract()?;
            let id: String = tuple.get_item(1)?.extract()?;
            result.push(DeviceIdentifier::new(domain, id));
        }
    }
    Ok(result)
}

fn parse_connections(py_set: &Bound<'_, PySet>) -> PyResult<Vec<DeviceConnection>> {
    let mut result = Vec::new();
    for item in py_set.iter() {
        let tuple = item.downcast::<PyTuple>()?;
        if tuple.len() == 2 {
            let conn_type: String = tuple.get_item(0)?.extract()?;
            let id: String = tuple.get_item(1)?.extract()?;
            result.push(DeviceConnection::new(conn_type, id));
        }
    }
    Ok(result)
}

/// Parse identifiers from any iterable (set, list, or frozenset)
fn parse_identifiers_any(py_obj: &Bound<'_, PyAny>) -> PyResult<Vec<DeviceIdentifier>> {
    let mut result = Vec::new();
    // Try as a set first
    if let Ok(set) = py_obj.downcast::<PySet>() {
        return parse_identifiers(set);
    }
    // Try as a list
    if let Ok(list) = py_obj.downcast::<pyo3::types::PyList>() {
        for item in list.iter() {
            if let Ok(tuple) = item.downcast::<PyTuple>() {
                if tuple.len() == 2 {
                    let domain: String = tuple.get_item(0)?.extract()?;
                    let id: String = tuple.get_item(1)?.extract()?;
                    result.push(DeviceIdentifier::new(domain, id));
                }
            }
        }
    }
    Ok(result)
}

/// Parse connections from any iterable (set, list, or frozenset)
fn parse_connections_any(py_obj: &Bound<'_, PyAny>) -> PyResult<Vec<DeviceConnection>> {
    let mut result = Vec::new();
    // Try as a set first
    if let Ok(set) = py_obj.downcast::<PySet>() {
        return parse_connections(set);
    }
    // Try as a list
    if let Ok(list) = py_obj.downcast::<pyo3::types::PyList>() {
        for item in list.iter() {
            if let Ok(tuple) = item.downcast::<PyTuple>() {
                if tuple.len() == 2 {
                    let conn_type: String = tuple.get_item(0)?.extract()?;
                    let id: String = tuple.get_item(1)?.extract()?;
                    result.push(DeviceConnection::new(conn_type, id));
                }
            }
        }
    }
    Ok(result)
}

/// Python wrapper for DeviceRegistry
#[pyclass(name = "DeviceRegistry")]
pub struct PyDeviceRegistry {
    inner: Arc<DeviceRegistry>,
    #[pyo3(get)]
    hass: PyObject,
}

#[pymethods]
impl PyDeviceRegistry {
    #[new]
    fn new(py: Python<'_>, hass: PyObject) -> PyResult<Self> {
        // Extract storage path from hass.config.path('.storage')
        let config = hass.getattr(py, "config")?;
        let storage_path: String = config
            .call_method1(py, "path", (".storage",))?
            .extract(py)?;

        // Create Rust storage and registry
        let storage = Arc::new(ha_registries::storage::Storage::new(&storage_path));
        let registry = DeviceRegistry::new(storage);

        Ok(Self {
            inner: Arc::new(registry),
            hass,
        })
    }

    /// Load devices from storage
    fn async_load(&self) -> PyResult<()> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let inner = self.inner.clone();
        tokio::task::block_in_place(|| handle.block_on(async { inner.load().await }))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Save devices to storage
    fn async_save(&self) -> PyResult<()> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let inner = self.inner.clone();
        tokio::task::block_in_place(|| handle.block_on(async { inner.save().await }))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Get device by ID
    fn async_get(&self, device_id: &str) -> Option<PyDeviceEntry> {
        self.inner.get(device_id).map(PyDeviceEntry::from_inner)
    }

    /// Get device by identifiers or connections
    #[pyo3(signature = (identifiers=None, connections=None))]
    fn async_get_device(
        &self,
        identifiers: Option<&Bound<'_, PySet>>,
        connections: Option<&Bound<'_, PySet>>,
    ) -> PyResult<Option<PyDeviceEntry>> {
        // Check identifiers first
        if let Some(idents) = identifiers {
            let parsed = parse_identifiers(idents)?;
            for ident in &parsed {
                if let Some(entry) = self.inner.get_by_identifier(ident.domain(), ident.id()) {
                    return Ok(Some(PyDeviceEntry::from_inner(entry)));
                }
            }
        }

        // Check connections
        if let Some(conns) = connections {
            let parsed = parse_connections(conns)?;
            for conn in &parsed {
                if let Some(entry) = self
                    .inner
                    .get_by_connection(conn.connection_type(), conn.id())
                {
                    return Ok(Some(PyDeviceEntry::from_inner(entry)));
                }
            }
        }

        Ok(None)
    }

    /// Get all devices for a config entry
    fn async_entries_for_config_entry(&self, config_entry_id: &str) -> Vec<PyDeviceEntry> {
        self.inner
            .get_by_config_entry_id(config_entry_id)
            .into_iter()
            .map(PyDeviceEntry::from_inner)
            .collect()
    }

    /// Get all devices in an area
    fn async_entries_for_area(&self, area_id: &str) -> Vec<PyDeviceEntry> {
        self.inner
            .get_by_area_id(area_id)
            .into_iter()
            .map(PyDeviceEntry::from_inner)
            .collect()
    }

    /// Get child devices (connected via this device)
    fn async_get_children(&self, device_id: &str) -> Vec<PyDeviceEntry> {
        self.inner
            .get_children(device_id)
            .into_iter()
            .map(PyDeviceEntry::from_inner)
            .collect()
    }

    /// Get or create a device
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        *,
        config_entry_id,
        identifiers=None,
        connections=None,
        manufacturer=None,
        model=None,
        model_id=None,
        name=None,
        serial_number=None,
        suggested_area=None,
        sw_version=None,
        hw_version=None,
        via_device=None,
        configuration_url=None,
        entry_type=None,
        // Accept but ignore these - they're HA-specific
        config_subentry_id=None,
        default_manufacturer=None,
        default_model=None,
        default_name=None,
        disabled_by=None,
        translation_key=None,
        translation_placeholders=None,
        created_at=None,
        modified_at=None
    ))]
    fn async_get_or_create(
        &self,
        config_entry_id: &str,
        identifiers: Option<&Bound<'_, PyAny>>,
        connections: Option<&Bound<'_, PyAny>>,
        manufacturer: Option<&str>,
        model: Option<&str>,
        model_id: Option<&str>,
        name: Option<&str>,
        serial_number: Option<&str>,
        suggested_area: Option<&str>,
        sw_version: Option<&str>,
        hw_version: Option<&str>,
        via_device: Option<&Bound<'_, PyAny>>,
        configuration_url: Option<&str>,
        entry_type: Option<&str>,
        // Accepted but ignored
        #[allow(unused_variables)] config_subentry_id: Option<&str>,
        #[allow(unused_variables)] default_manufacturer: Option<&str>,
        #[allow(unused_variables)] default_model: Option<&str>,
        #[allow(unused_variables)] default_name: Option<&str>,
        #[allow(unused_variables)] disabled_by: Option<&Bound<'_, PyAny>>,
        #[allow(unused_variables)] translation_key: Option<&str>,
        #[allow(unused_variables)] translation_placeholders: Option<&Bound<'_, PyAny>>,
        #[allow(unused_variables)] created_at: Option<&Bound<'_, PyAny>>,
        #[allow(unused_variables)] modified_at: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyDeviceEntry> {
        // Parse identifiers (can be set or list of tuples)
        let idents = if let Some(i) = identifiers {
            parse_identifiers_any(i)?
        } else {
            Vec::new()
        };

        // Parse connections (can be set or list of tuples)
        let conns = if let Some(c) = connections {
            parse_connections_any(c)?
        } else {
            Vec::new()
        };

        let device_name = name.unwrap_or("Unknown Device");

        // Get or create the base entry
        let entry = self
            .inner
            .get_or_create(&idents, &conns, Some(config_entry_id), device_name);

        // Update with additional fields if provided
        let needs_update = manufacturer.is_some()
            || model.is_some()
            || model_id.is_some()
            || serial_number.is_some()
            || suggested_area.is_some()
            || sw_version.is_some()
            || hw_version.is_some()
            || via_device.is_some()
            || configuration_url.is_some()
            || entry_type.is_some();

        if needs_update {
            // Parse via_device tuple to get device ID
            let via_device_id = if let Some(vd) = via_device {
                // via_device is a tuple (domain, identifier) - we need to look up the device
                if let Ok(tuple) = vd.downcast::<pyo3::types::PyTuple>() {
                    if tuple.len() == 2 {
                        let domain: String = tuple.get_item(0)?.extract()?;
                        let identifier: String = tuple.get_item(1)?.extract()?;
                        // Look up the device by identifier
                        self.inner
                            .get_by_identifier(&domain, &identifier)
                            .map(|d| d.id.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let updated = self.inner.update(&entry.id, |e| {
                if let Some(v) = manufacturer {
                    e.manufacturer = Some(v.to_string());
                }
                if let Some(v) = model {
                    e.model = Some(v.to_string());
                }
                if let Some(v) = model_id {
                    e.model_id = Some(v.to_string());
                }
                if let Some(v) = serial_number {
                    e.serial_number = Some(v.to_string());
                }
                if let Some(v) = suggested_area {
                    e.area_id = Some(v.to_string());
                }
                if let Some(v) = sw_version {
                    e.sw_version = Some(v.to_string());
                }
                if let Some(v) = hw_version {
                    e.hw_version = Some(v.to_string());
                }
                if via_device_id.is_some() {
                    e.via_device_id = via_device_id.clone();
                }
                if let Some(v) = configuration_url {
                    e.configuration_url = Some(v.to_string());
                }
                if let Some(v) = entry_type {
                    e.entry_type = match v {
                        "service" => Some(ha_registries::device_registry::DeviceEntryType::Service),
                        _ => None,
                    };
                }
            });

            if let Some(updated_entry) = updated {
                return Ok(PyDeviceEntry::from_inner(updated_entry));
            }
        }

        Ok(PyDeviceEntry::from_inner(entry))
    }

    /// Update a device
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        device_id,
        *,
        name=None,
        name_by_user=None,
        manufacturer=None,
        model=None,
        model_id=None,
        hw_version=None,
        sw_version=None,
        serial_number=None,
        via_device_id=None,
        area_id=None,
        disabled_by=None,
        configuration_url=None,
        labels=None,
        identifiers=None,
        connections=None
    ))]
    fn async_update_device(
        &self,
        device_id: &str,
        name: Option<String>,
        name_by_user: Option<String>,
        manufacturer: Option<String>,
        model: Option<String>,
        model_id: Option<String>,
        hw_version: Option<String>,
        sw_version: Option<String>,
        serial_number: Option<String>,
        via_device_id: Option<String>,
        area_id: Option<String>,
        disabled_by: Option<String>,
        configuration_url: Option<String>,
        labels: Option<Vec<String>>,
        identifiers: Option<&Bound<'_, PySet>>,
        connections: Option<&Bound<'_, PySet>>,
    ) -> PyResult<PyDeviceEntry> {
        // Parse identifiers/connections outside the closure
        let parsed_identifiers = if let Some(i) = identifiers {
            Some(parse_identifiers(i)?)
        } else {
            None
        };

        let parsed_connections = if let Some(c) = connections {
            Some(parse_connections(c)?)
        } else {
            None
        };

        let entry = self.inner.update(device_id, |entry| {
            if let Some(ref n) = name {
                entry.name = n.clone();
            }
            if name_by_user.is_some() {
                entry.name_by_user = name_by_user.clone();
            }
            if manufacturer.is_some() {
                entry.manufacturer = manufacturer.clone();
            }
            if model.is_some() {
                entry.model = model.clone();
            }
            if model_id.is_some() {
                entry.model_id = model_id.clone();
            }
            if hw_version.is_some() {
                entry.hw_version = hw_version.clone();
            }
            if sw_version.is_some() {
                entry.sw_version = sw_version.clone();
            }
            if serial_number.is_some() {
                entry.serial_number = serial_number.clone();
            }
            if via_device_id.is_some() {
                entry.via_device_id = via_device_id.clone();
            }
            if area_id.is_some() {
                entry.area_id = area_id.clone();
            }
            if disabled_by.is_some() {
                entry.disabled_by = parse_disabled_by(disabled_by.as_deref());
            }
            if configuration_url.is_some() {
                entry.configuration_url = configuration_url.clone();
            }
            if let Some(ref l) = labels {
                entry.labels = l.clone();
            }
            if let Some(ref i) = parsed_identifiers {
                entry.identifiers = i.clone();
            }
            if let Some(ref c) = parsed_connections {
                entry.connections = c.clone();
            }
        });

        entry.map(PyDeviceEntry::from_inner).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!(
                "Device not found: {}",
                device_id
            ))
        })
    }

    /// Remove a device
    fn async_remove_device(&self, device_id: &str) -> PyResult<()> {
        self.inner.remove(device_id).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!(
                "Device not found: {}",
                device_id
            ))
        })?;
        Ok(())
    }

    /// Check if a device is registered
    fn async_is_registered(&self, device_id: &str) -> bool {
        self.inner.get(device_id).is_some()
    }

    /// Get all device IDs
    fn device_ids(&self) -> Vec<String> {
        self.inner.device_ids()
    }

    /// Get all devices as a dict (device_id -> DeviceEntry)
    #[getter]
    fn devices(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new_bound(py);
        for entry in self.inner.iter() {
            let id = entry.id.clone();
            dict.set_item(&id, PyDeviceEntry::from_inner(entry).into_py(py))?;
        }
        Ok(dict.unbind())
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __repr__(&self) -> String {
        format!("DeviceRegistry(count={})", self.inner.len())
    }
}
