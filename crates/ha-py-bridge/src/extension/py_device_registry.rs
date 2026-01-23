//! Python wrappers for DeviceRegistry

use std::collections::HashMap;
use std::sync::Arc;

/// Config entry domains that are considered low priority for primary_config_entry promotion.
/// When these domains hold the primary position, other integrations can take over.
const LOW_PRIO_CONFIG_ENTRY_DOMAINS: &[&str] = &["homekit_controller", "matter", "mqtt", "upnp"];

use ha_registries::device_registry::{
    DeviceConnection, DeviceEntry, DeviceEntryType, DeviceIdentifier, DeviceRegistry,
};
use ha_registries::entity_registry::DisabledBy;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PySet, PyTuple};
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
    fn name(&self) -> Option<&str> {
        self.inner.name.as_deref()
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
    fn suggested_area(&self) -> Option<&str> {
        self.inner.suggested_area.as_deref()
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
    fn config_entries_subentries(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new_bound(py);
        for (config_entry_id, subentries) in &self.inner.config_entries_subentries {
            let py_list: Vec<Option<String>> = subentries.clone();
            dict.set_item(config_entry_id, py_list)?;
        }
        Ok(dict.unbind())
    }

    #[getter]
    fn created_at(&self) -> String {
        self.inner.created_at.to_rfc3339()
    }

    #[getter]
    fn modified_at(&self) -> String {
        self.inner.modified_at.to_rfc3339()
    }

    #[getter]
    fn created_at_timestamp(&self) -> f64 {
        self.inner.created_at.timestamp() as f64
            + self.inner.created_at.timestamp_subsec_nanos() as f64 / 1_000_000_000.0
    }

    #[getter]
    fn modified_at_timestamp(&self) -> f64 {
        self.inner.modified_at.timestamp() as f64
            + self.inner.modified_at.timestamp_subsec_nanos() as f64 / 1_000_000_000.0
    }

    #[getter]
    fn insertion_order(&self) -> u64 {
        self.inner.insertion_order
    }

    fn is_disabled(&self) -> bool {
        self.inner.is_disabled()
    }

    fn __repr__(&self) -> String {
        format!(
            "DeviceEntry(id='{}', name='{}')",
            self.inner.id,
            self.inner.name.as_deref().unwrap_or("<unnamed>")
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
            result.push(DeviceConnection::normalized(conn_type, id));
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
                    result.push(DeviceConnection::normalized(conn_type, id));
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
        // Extract config directory path from hass.config.path()
        // Note: Storage::new() adds ".storage" internally, so we pass the config dir
        let config = hass.getattr(py, "config")?;
        let config_dir: String = config.call_method1(py, "path", ("",))?.extract(py)?;

        // Create Rust storage and registry
        let storage = Arc::new(ha_registries::storage::Storage::new(&config_dir));
        let registry = DeviceRegistry::new(storage);

        Ok(Self {
            inner: Arc::new(registry),
            hass,
        })
    }

    /// Load devices from storage
    fn async_load(&self) -> PyResult<()> {
        let inner = self.inner.clone();
        if let Ok(handle) = Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(async { inner.load().await }))
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
        } else {
            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to create Tokio runtime: {}",
                    e
                ))
            })?;
            rt.block_on(async { inner.load().await })
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
        }
    }

    /// Save devices to storage
    fn async_save(&self) -> PyResult<()> {
        let inner = self.inner.clone();
        if let Ok(handle) = Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(async { inner.save().await }))
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
        } else {
            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to create Tokio runtime: {}",
                    e
                ))
            })?;
            rt.block_on(async { inner.save().await })
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
        }
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
    ///
    /// Handles field updates, primary_config_entry promotion, disabled_by on
    /// creation, and suggested_area storage.
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
        config_subentry_id=None,
        default_manufacturer=None,
        default_model=None,
        default_name=None,
        disabled_by=None,
        translation_key=None,
        translation_placeholders=None,
        created_at=None,
        modified_at=None,
        current_primary_domain=None,
        initial_disabled_by=None
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
        config_subentry_id: Option<&str>,
        default_manufacturer: Option<&str>,
        default_model: Option<&str>,
        default_name: Option<&str>,
        #[allow(unused_variables)] disabled_by: Option<&Bound<'_, PyAny>>,
        #[allow(unused_variables)] translation_key: Option<&str>,
        #[allow(unused_variables)] translation_placeholders: Option<&Bound<'_, PyAny>>,
        created_at: Option<f64>,
        #[allow(unused_variables)] modified_at: Option<f64>,
        // Domain of the current primary config entry (for promotion decision)
        current_primary_domain: Option<String>,
        // Applied only on newly created devices
        initial_disabled_by: Option<String>,
    ) -> PyResult<PyDeviceEntry> {
        // Parse timestamp (seconds since epoch from Python's time.time())
        let timestamp = created_at.and_then(|ts| {
            chrono::DateTime::from_timestamp(ts as i64, ((ts % 1.0) * 1_000_000_000.0) as u32)
        });

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

        // Check if device already exists (for is_new detection)
        let existing = self
            .inner
            .get_by_identifiers_or_connections(&idents, &conns);
        let is_new = existing.is_none();

        // Get or create the base entry
        let entry = self.inner.get_or_create(
            &idents,
            &conns,
            Some(config_entry_id),
            Some(config_subentry_id),
            name,
            timestamp,
        );

        // Parse via_device tuple to get device ID
        let via_device_id = if let Some(vd) = via_device {
            if let Ok(tuple) = vd.downcast::<pyo3::types::PyTuple>() {
                if tuple.len() == 2 {
                    let domain: String = tuple.get_item(0)?.extract()?;
                    let identifier: String = tuple.get_item(1)?.extract()?;
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

        // Determine if we need to update fields
        let needs_field_update = name.is_some()
            || manufacturer.is_some()
            || model.is_some()
            || model_id.is_some()
            || serial_number.is_some()
            || sw_version.is_some()
            || hw_version.is_some()
            || via_device.is_some()
            || configuration_url.is_some()
            || entry_type.is_some()
            || default_name.is_some()
            || default_manufacturer.is_some()
            || default_model.is_some()
            || suggested_area.is_some();

        // Primary config entry promotion decision
        // "Primary keys" are fields that indicate the integration provides
        // substantive device info (excludes default_* fields)
        let has_primary_keys = name.is_some()
            || manufacturer.is_some()
            || model.is_some()
            || model_id.is_some()
            || serial_number.is_some()
            || sw_version.is_some()
            || hw_version.is_some()
            || via_device.is_some()
            || configuration_url.is_some()
            || entry_type.is_some()
            || suggested_area.is_some();

        let needs_primary = if has_primary_keys {
            let current_primary = existing
                .as_ref()
                .and_then(|e| e.primary_config_entry.as_deref());
            match current_primary {
                None => true,                               // No current primary
                Some(cp) if cp == config_entry_id => false, // Already the primary
                Some(_) => {
                    // Check if current primary is low-priority or doesn't exist
                    match &current_primary_domain {
                        None => true, // Config entry doesn't exist
                        Some(domain) => LOW_PRIO_CONFIG_ENTRY_DOMAINS.contains(&domain.as_str()),
                    }
                }
            }
        } else {
            false
        };
        let needs_disabled = is_new && initial_disabled_by.is_some();

        if needs_field_update || needs_primary || needs_disabled {
            let ce_id = config_entry_id.to_string();
            let initial_db = initial_disabled_by.clone();

            let updated = self.inner.update_at(
                &entry.id,
                |e| {
                    // Field updates
                    if let Some(n) = name {
                        e.name = Some(n.to_string());
                    } else if let Some(dn) = default_name {
                        if e.name.is_none() {
                            e.name = Some(dn.to_string());
                        }
                    }
                    if let Some(v) = manufacturer {
                        e.manufacturer = Some(v.to_string());
                    } else if let Some(dm) = default_manufacturer {
                        if e.manufacturer.is_none() {
                            e.manufacturer = Some(dm.to_string());
                        }
                    }
                    if let Some(v) = model {
                        e.model = Some(v.to_string());
                    } else if let Some(dm) = default_model {
                        if e.model.is_none() {
                            e.model = Some(dm.to_string());
                        }
                    }
                    if let Some(v) = model_id {
                        e.model_id = Some(v.to_string());
                    }
                    if let Some(v) = serial_number {
                        e.serial_number = Some(v.to_string());
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
                            "service" => {
                                Some(ha_registries::device_registry::DeviceEntryType::Service)
                            }
                            _ => None,
                        };
                    }
                    if let Some(sa) = suggested_area {
                        e.suggested_area = Some(sa.to_string());
                    }

                    // Primary config entry promotion
                    if needs_primary {
                        e.primary_config_entry = Some(ce_id.clone());
                    }

                    // Disabled_by on creation
                    if let Some(ref db) = initial_db {
                        e.disabled_by = parse_disabled_by(Some(db.as_str()));
                    }
                },
                timestamp,
            );

            if let Some(updated_entry) = updated {
                return Ok(PyDeviceEntry::from_inner(updated_entry));
            }
        }

        Ok(PyDeviceEntry::from_inner(entry))
    }

    /// Update a device
    ///
    /// Supports add/remove of config entries with subentry management,
    /// automatic disabled_by propagation based on config entry disabled status,
    /// merge/new connections/identifiers with collision detection, and
    /// via_device_id cleanup on device removal.
    ///
    /// Returns None if the device was removed (last config entry removed).
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
        suggested_area=None,
        via_device_id=None,
        area_id=None,
        disabled_by=None,
        configuration_url=None,
        labels=None,
        identifiers=None,
        connections=None,
        config_entries=None,
        config_entries_subentries=None,
        modified_at=None,
        entry_type=None,
        primary_config_entry=None,
        add_config_entry_id=None,
        add_config_subentry_id=None,
        add_config_entry_disabled=None,
        remove_config_entry_id=None,
        remove_config_subentry_only=None,
        remove_config_subentry_id=None,
        config_entry_disabled_map=None,
        merge_connections=None,
        new_connections=None,
        merge_identifiers=None,
        new_identifiers=None
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
        suggested_area: Option<String>,
        via_device_id: Option<String>,
        area_id: Option<String>,
        disabled_by: Option<String>,
        configuration_url: Option<String>,
        labels: Option<Vec<String>>,
        identifiers: Option<&Bound<'_, PySet>>,
        connections: Option<&Bound<'_, PySet>>,
        config_entries: Option<Vec<String>>,
        config_entries_subentries: Option<&Bound<'_, PyDict>>,
        modified_at: Option<f64>,
        entry_type: Option<String>,
        primary_config_entry: Option<String>,
        // Config entry add/remove parameters
        add_config_entry_id: Option<String>,
        add_config_subentry_id: Option<String>,
        add_config_entry_disabled: Option<bool>,
        remove_config_entry_id: Option<String>,
        remove_config_subentry_only: Option<bool>,
        remove_config_subentry_id: Option<String>,
        config_entry_disabled_map: Option<&Bound<'_, PyDict>>,
        // Merge/new connections/identifiers
        merge_connections: Option<&Bound<'_, PySet>>,
        new_connections: Option<&Bound<'_, PySet>>,
        merge_identifiers: Option<&Bound<'_, PySet>>,
        new_identifiers: Option<&Bound<'_, PySet>>,
    ) -> PyResult<Option<PyDeviceEntry>> {
        // Validate: can't specify both merge and new
        if merge_connections.is_some() && new_connections.is_some() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Cannot specify both new_connections and merge_connections",
            ));
        }
        if merge_identifiers.is_some() && new_identifiers.is_some() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Cannot specify both new_identifiers and merge_identifiers",
            ));
        }

        // Resolve final connections (merge with existing or replace)
        let resolved_connections = if let Some(nc) = new_connections {
            let parsed = parse_connections(nc)?;
            Some(parsed)
        } else if let Some(mc) = merge_connections {
            let merge_parsed = parse_connections(mc)?;
            // Merge with existing device connections
            let mut result = if let Some(current) = self.inner.get(device_id) {
                current.connections.clone()
            } else {
                Vec::new()
            };
            for conn in merge_parsed {
                if !result
                    .iter()
                    .any(|c| c.connection_type() == conn.connection_type() && c.id() == conn.id())
                {
                    result.push(conn);
                }
            }
            Some(result)
        } else {
            None
        };

        // Resolve final identifiers (merge with existing or replace)
        let resolved_identifiers = if let Some(ni) = new_identifiers {
            let parsed = parse_identifiers(ni)?;
            Some(parsed)
        } else if let Some(mi) = merge_identifiers {
            let merge_parsed = parse_identifiers(mi)?;
            // Merge with existing device identifiers
            let mut result = if let Some(current) = self.inner.get(device_id) {
                current.identifiers.clone()
            } else {
                Vec::new()
            };
            for ident in merge_parsed {
                if !result
                    .iter()
                    .any(|i| i.domain() == ident.domain() && i.id() == ident.id())
                {
                    result.push(ident);
                }
            }
            Some(result)
        } else {
            None
        };

        // Validate: can't clear both connections and identifiers
        if let (Some(ref conns), Some(ref idents)) = (&resolved_connections, &resolved_identifiers)
        {
            if conns.is_empty() && idents.is_empty() {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "A device must have at least one of identifiers or connections",
                ));
            }
        }

        // Collision detection for connections
        let connections_to_validate = if new_connections.is_some() || merge_connections.is_some() {
            resolved_connections.as_ref()
        } else {
            None
        };
        if let Some(conns) = connections_to_validate {
            for conn in conns {
                if let Some(existing) = self
                    .inner
                    .get_by_connection(conn.connection_type(), conn.id())
                {
                    if existing.id != device_id {
                        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                            "Connections already registered with {}",
                            existing.id
                        )));
                    }
                }
            }
        }

        // Collision detection for identifiers
        let identifiers_to_validate = if new_identifiers.is_some() || merge_identifiers.is_some() {
            resolved_identifiers.as_ref()
        } else {
            None
        };
        if let Some(idents) = identifiers_to_validate {
            for ident in idents {
                if let Some(existing) = self.inner.get_by_identifier(ident.domain(), ident.id()) {
                    if existing.id != device_id {
                        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                            "Identifiers already registered with {}",
                            existing.id
                        )));
                    }
                }
            }
        }

        // Parse direct identifiers/connections params (lower priority than merge/new)
        let parsed_identifiers = if resolved_identifiers.is_some() {
            resolved_identifiers
        } else if let Some(i) = identifiers {
            Some(parse_identifiers(i)?)
        } else {
            None
        };

        let parsed_connections = if resolved_connections.is_some() {
            resolved_connections
        } else if let Some(c) = connections {
            Some(parse_connections(c)?)
        } else {
            None
        };

        let parsed_subentries = if let Some(ces_dict) = config_entries_subentries {
            let mut map = std::collections::HashMap::new();
            for (key, value) in ces_dict.iter() {
                let config_entry_id: String = key.extract()?;
                let subentries: Vec<Option<String>> = value.extract()?;
                map.insert(config_entry_id, subentries);
            }
            Some(map)
        } else {
            None
        };

        // Parse config entry disabled map for disabled_by computation
        let disabled_map: HashMap<String, bool> = if let Some(dm) = config_entry_disabled_map {
            let mut map = HashMap::new();
            for (key, value) in dm.iter() {
                let ce_id: String = key.extract()?;
                let is_disabled: bool = value.extract()?;
                map.insert(ce_id, is_disabled);
            }
            map
        } else {
            HashMap::new()
        };

        let timestamp = modified_at.and_then(|ts| {
            chrono::DateTime::from_timestamp(ts as i64, ((ts % 1.0) * 1_000_000_000.0) as u32)
        });

        // Handle add_config_entry_id: check for no-op before calling update
        if let Some(ref add_ce_id) = add_config_entry_id {
            let subentry_to_add = add_config_subentry_id.clone();
            if let Some(current) = self.inner.get(device_id) {
                // Check if this is a no-op (CE + subentry already present)
                if current.config_entries.contains(add_ce_id) {
                    if let Some(existing_subs) = current.config_entries_subentries.get(add_ce_id) {
                        if existing_subs.contains(&subentry_to_add) {
                            // No-op: config entry and subentry already present
                            return Ok(Some(PyDeviceEntry::from_inner(current)));
                        }
                    }
                }
            }
        }

        // Handle remove_config_entry_id: determine what to remove
        let mut should_remove_device = false;
        if let Some(ref remove_ce_id) = remove_config_entry_id {
            if let Some(current) = self.inner.get(device_id) {
                let subentry_only = remove_config_subentry_only.unwrap_or(false);
                if subentry_only {
                    // Remove specific subentry
                    let sub_to_remove = remove_config_subentry_id.clone();
                    if let Some(existing_subs) = current.config_entries_subentries.get(remove_ce_id)
                    {
                        if !existing_subs.contains(&sub_to_remove) {
                            // Subentry not present - no-op
                            return Ok(Some(PyDeviceEntry::from_inner(current)));
                        }
                        // Check if removing this subentry empties the CE
                        let remaining_subs: Vec<_> = existing_subs
                            .iter()
                            .filter(|s| *s != &sub_to_remove)
                            .collect();
                        if remaining_subs.is_empty() {
                            // This CE will be fully removed
                            if current.config_entries.len() <= 1 {
                                should_remove_device = true;
                            }
                        }
                    } else if !current.config_entries.contains(remove_ce_id) {
                        // CE not on device - no-op
                        return Ok(Some(PyDeviceEntry::from_inner(current)));
                    }
                } else {
                    // Remove entire config entry
                    if !current.config_entries.contains(remove_ce_id) {
                        // CE not on device - no-op
                        return Ok(Some(PyDeviceEntry::from_inner(current)));
                    }
                    if current.config_entries.len() <= 1 {
                        should_remove_device = true;
                    }
                }
            }
        }

        // If device should be removed, do it and return None
        if should_remove_device {
            self.inner.remove(device_id);
            // Clear via_device_id on devices that referenced the removed device
            self.inner.clear_via_device_id(device_id);
            return Ok(None);
        }

        let entry = self.inner.update_at(
            device_id,
            |entry| {
                // Handle add_config_entry_id
                if let Some(ref add_ce_id) = add_config_entry_id {
                    let subentry_to_add = add_config_subentry_id.clone();
                    let is_new_ce = !entry.config_entries.contains(add_ce_id);

                    if is_new_ce {
                        // Add new config entry
                        entry.config_entries.push(add_ce_id.clone());
                        entry
                            .config_entries_subentries
                            .insert(add_ce_id.clone(), vec![subentry_to_add]);

                        // Disabled_by logic: if adding enabled CE and device disabled by CONFIG_ENTRY → clear
                        if let Some(false) = add_config_entry_disabled {
                            if entry.disabled_by == Some(DisabledBy::ConfigEntry) {
                                entry.disabled_by = None;
                            }
                        }
                    } else {
                        // CE already exists - add subentry if new
                        let subs = entry
                            .config_entries_subentries
                            .entry(add_ce_id.clone())
                            .or_default();
                        if !subs.contains(&subentry_to_add) {
                            subs.push(subentry_to_add);
                        }
                    }
                }

                // Handle remove_config_entry_id
                if let Some(ref remove_ce_id) = remove_config_entry_id {
                    let subentry_only = remove_config_subentry_only.unwrap_or(false);
                    let mut ce_fully_removed = false;

                    if subentry_only {
                        // Remove specific subentry
                        let sub_to_remove = remove_config_subentry_id.clone();
                        if let Some(subs) = entry.config_entries_subentries.get_mut(remove_ce_id) {
                            subs.retain(|s| s != &sub_to_remove);
                            if subs.is_empty() {
                                // No subentries left → remove the config entry
                                entry.config_entries_subentries.remove(remove_ce_id);
                                entry.config_entries.retain(|id| id != remove_ce_id);
                                ce_fully_removed = true;
                            }
                        }
                    } else {
                        // Remove entire config entry
                        entry.config_entries.retain(|id| id != remove_ce_id);
                        entry.config_entries_subentries.remove(remove_ce_id);
                        ce_fully_removed = true;
                    }

                    if ce_fully_removed {
                        // Update primary_config_entry if it was removed
                        if entry.primary_config_entry.as_deref() == Some(remove_ce_id) {
                            entry.primary_config_entry = None;
                        }

                        // Disabled_by logic: if device not disabled and all remaining CEs are disabled → set CONFIG_ENTRY
                        if entry.disabled_by.is_none() && !entry.config_entries.is_empty() {
                            let all_remaining_disabled = entry
                                .config_entries
                                .iter()
                                .all(|ce_id| disabled_map.get(ce_id).copied().unwrap_or(false));
                            if all_remaining_disabled {
                                entry.disabled_by = Some(DisabledBy::ConfigEntry);
                            }
                        }
                    }
                }

                // Apply standard field updates
                if let Some(ref n) = name {
                    entry.name = Some(n.clone());
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
                if let Some(ref sa) = suggested_area {
                    entry.suggested_area = if sa.is_empty() {
                        None
                    } else {
                        Some(sa.clone())
                    };
                }
                if let Some(ref vid) = via_device_id {
                    entry.via_device_id = if vid.is_empty() {
                        None
                    } else {
                        Some(vid.clone())
                    };
                }
                if let Some(ref aid) = area_id {
                    entry.area_id = if aid.is_empty() {
                        None
                    } else {
                        Some(aid.clone())
                    };
                }
                if disabled_by.is_some() {
                    entry.disabled_by = parse_disabled_by(disabled_by.as_deref());
                }
                if let Some(ref curl) = configuration_url {
                    entry.configuration_url = if curl.is_empty() {
                        None
                    } else {
                        Some(curl.clone())
                    };
                }
                if let Some(ref et) = entry_type {
                    entry.entry_type = if et.is_empty() {
                        None
                    } else {
                        match et.as_str() {
                            "service" => Some(DeviceEntryType::Service),
                            _ => None,
                        }
                    };
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
                if let Some(ref ces) = config_entries {
                    // Add default subentry for new config entries
                    for ce_id in ces {
                        if !entry.config_entries.contains(ce_id) {
                            entry
                                .config_entries_subentries
                                .entry(ce_id.clone())
                                .or_insert_with(|| vec![None]);
                        }
                    }
                    entry.config_entries = ces.clone();
                    // Clean up config_entries_subentries for removed entries
                    entry
                        .config_entries_subentries
                        .retain(|k, _| ces.contains(k));
                    // Update primary_config_entry if it was removed
                    if let Some(ref pce) = entry.primary_config_entry {
                        if !ces.contains(pce) {
                            entry.primary_config_entry = None;
                        }
                    }
                }
                if let Some(ref ces_map) = parsed_subentries {
                    entry.config_entries_subentries = ces_map.clone();
                }
                if let Some(ref pce) = primary_config_entry {
                    entry.primary_config_entry = if pce.is_empty() {
                        None
                    } else {
                        Some(pce.clone())
                    };
                }
            },
            timestamp,
        );

        Ok(entry.map(PyDeviceEntry::from_inner))
    }

    /// Clear a config entry from all devices
    fn async_clear_config_entry(&self, config_entry_id: &str) {
        self.inner.clear_config_entry(config_entry_id);
    }

    /// Remove a device and clean up via_device_id references
    fn async_remove_device(&self, device_id: &str) -> PyResult<()> {
        self.inner.remove(device_id).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!(
                "Device not found: {}",
                device_id
            ))
        })?;
        // Clear via_device_id on devices that referenced the removed device
        self.inner.clear_via_device_id(device_id);
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
