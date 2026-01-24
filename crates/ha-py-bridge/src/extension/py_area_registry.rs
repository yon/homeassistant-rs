//! Python wrappers for AreaRegistry

use chrono::{DateTime, Utc};
use ha_registries::area_registry::{AreaEntry, AreaRegistry};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tokio::runtime::Handle;

/// Get the current time from Python's datetime.now(UTC) (respects freezer in tests)
fn py_utc_now(py: Python<'_>) -> DateTime<Utc> {
    let datetime_mod = py
        .import_bound("datetime")
        .expect("Failed to import datetime module");
    let timezone = datetime_mod
        .getattr("timezone")
        .expect("Failed to get timezone");
    let utc = timezone.getattr("utc").expect("Failed to get UTC");
    let now = datetime_mod
        .getattr("datetime")
        .expect("Failed to get datetime class")
        .call_method1("now", (utc,))
        .expect("Failed to call datetime.now(UTC)");
    let year: i32 = now.getattr("year").unwrap().extract().unwrap();
    let month: u32 = now.getattr("month").unwrap().extract().unwrap();
    let day: u32 = now.getattr("day").unwrap().extract().unwrap();
    let hour: u32 = now.getattr("hour").unwrap().extract().unwrap();
    let minute: u32 = now.getattr("minute").unwrap().extract().unwrap();
    let second: u32 = now.getattr("second").unwrap().extract().unwrap();
    let microsecond: u32 = now.getattr("microsecond").unwrap().extract().unwrap();
    chrono::NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|d| d.and_hms_micro_opt(hour, minute, second, microsecond))
        .map(|dt| dt.and_utc())
        .unwrap_or_else(Utc::now)
}

/// Python wrapper for AreaEntry
#[pyclass(name = "AreaEntry")]
#[derive(Clone)]
pub struct PyAreaEntry {
    inner: AreaEntry,
}

#[pymethods]
impl PyAreaEntry {
    #[getter]
    fn id(&self) -> &str {
        &self.inner.id
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[getter]
    fn normalized_name(&self) -> Option<&str> {
        self.inner.normalized_name.as_deref()
    }

    #[getter]
    fn picture(&self) -> Option<&str> {
        self.inner.picture.as_deref()
    }

    #[getter]
    fn icon(&self) -> Option<&str> {
        self.inner.icon.as_deref()
    }

    #[getter]
    fn floor_id(&self) -> Option<&str> {
        self.inner.floor_id.as_deref()
    }

    #[getter]
    fn humidity_entity_id(&self) -> Option<&str> {
        self.inner.humidity_entity_id.as_deref()
    }

    #[getter]
    fn temperature_entity_id(&self) -> Option<&str> {
        self.inner.temperature_entity_id.as_deref()
    }

    #[getter]
    fn aliases(&self) -> Vec<String> {
        self.inner.aliases.clone()
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

    fn __repr__(&self) -> String {
        format!(
            "AreaEntry(id='{}', name='{}')",
            self.inner.id, self.inner.name
        )
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> bool {
        // If other is also a PyAreaEntry, compare all fields via PartialEq
        if let Ok(other_entry) = other.extract::<PyAreaEntry>() {
            return self.inner == other_entry.inner;
        }
        // Otherwise try field-by-field comparison for cross-type compatibility
        // Compare key fields via Python attribute access
        let id_match = other
            .getattr("id")
            .and_then(|v| v.extract::<String>())
            .map(|v| v == self.inner.id)
            .unwrap_or(false);
        let name_match = other
            .getattr("name")
            .and_then(|v| v.extract::<String>())
            .map(|v| v == self.inner.name)
            .unwrap_or(false);
        if !id_match || !name_match {
            return false;
        }
        // Compare optional string fields
        let fields_match = self.compare_optional_str(py, other, "icon", &self.inner.icon)
            && self.compare_optional_str(py, other, "picture", &self.inner.picture)
            && self.compare_optional_str(py, other, "floor_id", &self.inner.floor_id)
            && self.compare_optional_str(
                py,
                other,
                "humidity_entity_id",
                &self.inner.humidity_entity_id,
            )
            && self.compare_optional_str(
                py,
                other,
                "temperature_entity_id",
                &self.inner.temperature_entity_id,
            );
        if !fields_match {
            return false;
        }
        // Compare aliases and labels (as sets)
        let aliases_match = other
            .getattr("aliases")
            .ok()
            .map(|v| {
                if let Ok(s) = v.extract::<std::collections::HashSet<String>>() {
                    s == self
                        .inner
                        .aliases
                        .iter()
                        .cloned()
                        .collect::<std::collections::HashSet<_>>()
                } else if let Ok(v) = v.extract::<Vec<String>>() {
                    let mut a = v.clone();
                    a.sort();
                    let mut b = self.inner.aliases.clone();
                    b.sort();
                    a == b
                } else {
                    self.inner.aliases.is_empty()
                }
            })
            .unwrap_or(self.inner.aliases.is_empty());
        let labels_match = other
            .getattr("labels")
            .ok()
            .map(|v| {
                if let Ok(s) = v.extract::<std::collections::HashSet<String>>() {
                    s == self
                        .inner
                        .labels
                        .iter()
                        .cloned()
                        .collect::<std::collections::HashSet<_>>()
                } else if let Ok(v) = v.extract::<Vec<String>>() {
                    let mut a = v.clone();
                    a.sort();
                    let mut b = self.inner.labels.clone();
                    b.sort();
                    a == b
                } else {
                    self.inner.labels.is_empty()
                }
            })
            .unwrap_or(self.inner.labels.is_empty());
        aliases_match && labels_match
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.id.hash(&mut hasher);
        hasher.finish()
    }
}

impl PyAreaEntry {
    /// Create from Arc<AreaEntry> - clones the inner value for Python ownership
    pub fn from_inner(inner: Arc<AreaEntry>) -> Self {
        Self {
            inner: (*inner).clone(),
        }
    }

    pub fn inner(&self) -> &AreaEntry {
        &self.inner
    }

    /// Compare an optional string field with a Python attribute
    fn compare_optional_str(
        &self,
        _py: Python<'_>,
        other: &Bound<'_, PyAny>,
        field: &str,
        our_value: &Option<String>,
    ) -> bool {
        match other.getattr(field) {
            Ok(val) => {
                if val.is_none() {
                    our_value.is_none()
                } else if let Ok(s) = val.extract::<String>() {
                    our_value.as_deref() == Some(s.as_str())
                } else {
                    our_value.is_none()
                }
            }
            Err(_) => our_value.is_none(),
        }
    }
}

/// Python wrapper for AreaRegistry
#[pyclass(name = "AreaRegistry")]
pub struct PyAreaRegistry {
    inner: Arc<AreaRegistry>,
    #[pyo3(get)]
    hass: PyObject,
}

#[pymethods]
impl PyAreaRegistry {
    #[new]
    fn new(py: Python<'_>, hass: PyObject) -> PyResult<Self> {
        // Extract config directory path from hass.config.path()
        // Note: Storage::new() adds ".storage" internally, so we pass the config dir
        let config = hass.getattr(py, "config")?;
        let config_dir: String = config.call_method1(py, "path", ("",))?.extract(py)?;

        // Create Rust storage and registry
        let storage = Arc::new(ha_registries::storage::Storage::new(&config_dir));
        let registry = AreaRegistry::new(storage);

        Ok(Self {
            inner: Arc::new(registry),
            hass,
        })
    }

    /// Load areas from storage
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

    /// Save areas to storage
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

    /// Get area by ID
    fn async_get_area(&self, area_id: &str) -> Option<PyAreaEntry> {
        self.inner.get(area_id).map(PyAreaEntry::from_inner)
    }

    /// Get area by name
    fn async_get_area_by_name(&self, name: &str) -> Option<PyAreaEntry> {
        self.inner.get_by_name(name).map(PyAreaEntry::from_inner)
    }

    /// Get all areas on a floor
    fn async_get_areas_for_floor(&self, floor_id: &str) -> Vec<PyAreaEntry> {
        self.inner
            .get_by_floor_id(floor_id)
            .into_iter()
            .map(PyAreaEntry::from_inner)
            .collect()
    }

    /// Get all areas with a label
    fn async_get_areas_for_label(&self, label_id: &str) -> Vec<PyAreaEntry> {
        self.inner
            .get_by_label_id(label_id)
            .into_iter()
            .map(PyAreaEntry::from_inner)
            .collect()
    }

    /// Create a new area
    #[pyo3(signature = (name, *, aliases=None, floor_id=None, humidity_entity_id=None, icon=None, labels=None, picture=None, temperature_entity_id=None))]
    #[allow(clippy::too_many_arguments)]
    fn async_create(
        &self,
        py: Python<'_>,
        name: &str,
        aliases: Option<Vec<String>>,
        floor_id: Option<String>,
        humidity_entity_id: Option<String>,
        icon: Option<String>,
        labels: Option<Vec<String>>,
        picture: Option<String>,
        temperature_entity_id: Option<String>,
    ) -> PyResult<PyAreaEntry> {
        let now = py_utc_now(py);
        let entry = self
            .inner
            .create(name, Some(now))
            .map_err(PyErr::new::<pyo3::exceptions::PyValueError, _>)?;

        // Update with optional fields if provided
        if aliases.is_some()
            || floor_id.is_some()
            || humidity_entity_id.is_some()
            || icon.is_some()
            || labels.is_some()
            || picture.is_some()
            || temperature_entity_id.is_some()
        {
            if let Ok(updated) = self.inner.update(
                &entry.id,
                |e| {
                    if let Some(ref a) = aliases {
                        e.aliases = a.clone();
                    }
                    if floor_id.is_some() {
                        e.floor_id = floor_id.clone();
                    }
                    if humidity_entity_id.is_some() {
                        e.humidity_entity_id = humidity_entity_id.clone();
                    }
                    if icon.is_some() {
                        e.icon = icon.clone();
                    }
                    if let Some(ref l) = labels {
                        e.labels = l.clone();
                    }
                    if picture.is_some() {
                        e.picture = picture.clone();
                    }
                    if temperature_entity_id.is_some() {
                        e.temperature_entity_id = temperature_entity_id.clone();
                    }
                },
                Some(now),
            ) {
                return Ok(PyAreaEntry::from_inner(updated));
            }
        }

        Ok(PyAreaEntry::from_inner(entry))
    }

    /// Update an area
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (area_id, *, name=None, aliases=None, floor_id=None, humidity_entity_id=None, icon=None, labels=None, picture=None, temperature_entity_id=None))]
    fn async_update(
        &self,
        py: Python<'_>,
        area_id: &str,
        name: Option<String>,
        aliases: Option<Vec<String>>,
        floor_id: Option<String>,
        humidity_entity_id: Option<String>,
        icon: Option<String>,
        labels: Option<Vec<String>>,
        picture: Option<String>,
        temperature_entity_id: Option<String>,
    ) -> PyResult<PyAreaEntry> {
        let now = py_utc_now(py);
        self.inner
            .update(
                area_id,
                |entry| {
                    if let Some(ref n) = name {
                        entry.name = n.clone();
                    }
                    if let Some(ref a) = aliases {
                        entry.aliases = a.clone();
                    }
                    if let Some(ref fid) = floor_id {
                        // Empty string = clear the field
                        entry.floor_id = if fid.is_empty() {
                            None
                        } else {
                            Some(fid.clone())
                        };
                    }
                    if let Some(ref hid) = humidity_entity_id {
                        entry.humidity_entity_id = if hid.is_empty() {
                            None
                        } else {
                            Some(hid.clone())
                        };
                    }
                    if let Some(ref ic) = icon {
                        entry.icon = if ic.is_empty() {
                            None
                        } else {
                            Some(ic.clone())
                        };
                    }
                    if let Some(ref l) = labels {
                        entry.labels = l.clone();
                    }
                    if let Some(ref pic) = picture {
                        entry.picture = if pic.is_empty() {
                            None
                        } else {
                            Some(pic.clone())
                        };
                    }
                    if let Some(ref tid) = temperature_entity_id {
                        entry.temperature_entity_id = if tid.is_empty() {
                            None
                        } else {
                            Some(tid.clone())
                        };
                    }
                },
                Some(now),
            )
            .map(PyAreaEntry::from_inner)
            .map_err(PyErr::new::<pyo3::exceptions::PyValueError, _>)
    }

    /// Clear floor_id from all areas that reference this floor (cascade on floor delete)
    fn async_clear_floor_id(&self, floor_id: &str) {
        self.inner.clear_floor_id(floor_id);
    }

    /// Remove a label from all areas that reference it (cascade on label delete)
    fn async_clear_label_id(&self, label_id: &str) {
        self.inner.clear_label_id(label_id);
    }

    /// Clear floor_id from an area (set to None)
    fn async_clear_area_floor_id(&self, py: Python<'_>, area_id: &str) -> PyResult<PyAreaEntry> {
        let now = py_utc_now(py);
        self.inner
            .update(
                area_id,
                |entry| {
                    entry.floor_id = None;
                },
                Some(now),
            )
            .map(PyAreaEntry::from_inner)
            .map_err(PyErr::new::<pyo3::exceptions::PyValueError, _>)
    }

    /// Delete an area
    fn async_delete(&self, area_id: &str) -> PyResult<()> {
        self.inner.remove(area_id).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!("Area not found: {}", area_id))
        })?;
        Ok(())
    }

    /// List all areas
    fn async_list_areas(&self) -> Vec<PyAreaEntry> {
        self.inner.iter().map(PyAreaEntry::from_inner).collect()
    }

    /// Get all areas as a dict (area_id -> AreaEntry)
    #[getter]
    fn areas(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new_bound(py);
        for entry in self.inner.iter() {
            let id = entry.id.clone();
            dict.set_item(&id, PyAreaEntry::from_inner(entry).into_py(py))?;
        }
        Ok(dict.unbind())
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __repr__(&self) -> String {
        format!("AreaRegistry(count={})", self.inner.len())
    }
}
