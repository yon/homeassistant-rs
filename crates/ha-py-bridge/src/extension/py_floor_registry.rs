//! Python wrappers for FloorRegistry

use chrono::{DateTime, Utc};
use ha_registries::floor_registry::{FloorEntry, FloorRegistry};
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

/// Python wrapper for FloorEntry
#[pyclass(name = "FloorEntry")]
#[derive(Clone)]
pub struct PyFloorEntry {
    inner: FloorEntry,
}

#[pymethods]
impl PyFloorEntry {
    #[getter]
    fn floor_id(&self) -> &str {
        &self.inner.id
    }

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
    fn icon(&self) -> Option<&str> {
        self.inner.icon.as_deref()
    }

    #[getter]
    fn level(&self) -> Option<i32> {
        self.inner.level
    }

    #[getter]
    fn aliases(&self) -> Vec<String> {
        self.inner.aliases.clone()
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
            "FloorEntry(id='{}', name='{}', level={:?})",
            self.inner.id, self.inner.name, self.inner.level
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

impl PyFloorEntry {
    /// Create from Arc<FloorEntry> - clones the inner value for Python ownership
    pub fn from_inner(inner: Arc<FloorEntry>) -> Self {
        Self {
            inner: (*inner).clone(),
        }
    }

    pub fn inner(&self) -> &FloorEntry {
        &self.inner
    }
}

/// Python wrapper for FloorRegistry
#[pyclass(name = "FloorRegistry")]
pub struct PyFloorRegistry {
    inner: Arc<FloorRegistry>,
    #[pyo3(get)]
    hass: PyObject,
}

#[pymethods]
impl PyFloorRegistry {
    #[new]
    fn new(py: Python<'_>, hass: PyObject) -> PyResult<Self> {
        // Extract config directory path from hass.config.path()
        // Note: Storage::new() adds ".storage" internally, so we pass the config dir
        let config = hass.getattr(py, "config")?;
        let config_dir: String = config.call_method1(py, "path", ("",))?.extract(py)?;

        // Create Rust storage and registry
        let storage = Arc::new(ha_registries::storage::Storage::new(&config_dir));
        let registry = FloorRegistry::new(storage);

        Ok(Self {
            inner: Arc::new(registry),
            hass,
        })
    }

    /// Load floors from storage
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

    /// Save floors to storage
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

    /// Get floor by ID
    fn async_get_floor(&self, floor_id: &str) -> Option<PyFloorEntry> {
        self.inner.get(floor_id).map(PyFloorEntry::from_inner)
    }

    /// Get floor by name
    fn async_get_floor_by_name(&self, name: &str) -> Option<PyFloorEntry> {
        self.inner.get_by_name(name).map(PyFloorEntry::from_inner)
    }

    /// Get floor by level
    fn async_get_floor_by_level(&self, level: i32) -> Option<PyFloorEntry> {
        self.inner.get_by_level(level).map(PyFloorEntry::from_inner)
    }

    /// Create a new floor
    #[pyo3(signature = (name, *, level=None, aliases=None, icon=None))]
    fn async_create(
        &self,
        py: Python<'_>,
        name: &str,
        level: Option<i32>,
        aliases: Option<Vec<String>>,
        icon: Option<String>,
    ) -> PyResult<PyFloorEntry> {
        let now = py_utc_now(py);
        let entry = self
            .inner
            .create(name, level, Some(now))
            .map_err(PyErr::new::<pyo3::exceptions::PyValueError, _>)?;

        // Update with optional fields if provided
        if aliases.is_some() || icon.is_some() {
            if let Ok(updated) = self.inner.update(
                &entry.id,
                |e| {
                    if let Some(ref a) = aliases {
                        e.aliases = a.clone();
                    }
                    if icon.is_some() {
                        e.icon = icon.clone();
                    }
                },
                Some(now),
            ) {
                return Ok(PyFloorEntry::from_inner(updated));
            }
        }

        Ok(PyFloorEntry::from_inner(entry))
    }

    /// Update a floor
    #[pyo3(signature = (floor_id, *, name=None, level=None, aliases=None, icon=None))]
    fn async_update(
        &self,
        py: Python<'_>,
        floor_id: &str,
        name: Option<String>,
        level: Option<i32>,
        aliases: Option<Vec<String>>,
        icon: Option<String>,
    ) -> PyResult<PyFloorEntry> {
        let now = py_utc_now(py);
        self.inner
            .update(
                floor_id,
                |entry| {
                    if let Some(ref n) = name {
                        entry.name = n.clone();
                    }
                    if let Some(l) = level {
                        entry.level = Some(l);
                    }
                    if let Some(ref a) = aliases {
                        entry.aliases = a.clone();
                    }
                    if icon.is_some() {
                        entry.icon = icon.clone();
                    }
                },
                Some(now),
            )
            .map(PyFloorEntry::from_inner)
            .map_err(PyErr::new::<pyo3::exceptions::PyValueError, _>)
    }

    /// Update a floor, always setting all fields (None means "clear the field")
    /// This is different from async_update where None means "don't change"
    #[pyo3(signature = (floor_id, name, *, level=None, aliases=None, icon=None))]
    fn async_set_fields(
        &self,
        py: Python<'_>,
        floor_id: &str,
        name: String,
        level: Option<i32>,
        aliases: Option<Vec<String>>,
        icon: Option<String>,
    ) -> PyResult<PyFloorEntry> {
        let now = py_utc_now(py);
        self.inner
            .update(
                floor_id,
                |entry| {
                    entry.name = name.clone();
                    entry.level = level;
                    entry.aliases = aliases.clone().unwrap_or_default();
                    entry.icon = icon.clone();
                },
                Some(now),
            )
            .map(PyFloorEntry::from_inner)
            .map_err(PyErr::new::<pyo3::exceptions::PyValueError, _>)
    }

    /// Delete a floor
    fn async_delete(&self, floor_id: &str) -> PyResult<()> {
        self.inner.remove(floor_id).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!("Floor not found: {}", floor_id))
        })?;
        Ok(())
    }

    /// List all floors
    fn async_list_floors(&self) -> Vec<PyFloorEntry> {
        self.inner.iter().map(PyFloorEntry::from_inner).collect()
    }

    /// Get floors sorted by level
    fn sorted_by_level(&self) -> Vec<PyFloorEntry> {
        self.inner
            .sorted_by_level()
            .into_iter()
            .map(PyFloorEntry::from_inner)
            .collect()
    }

    /// Get all floors as a dict (floor_id -> FloorEntry)
    #[getter]
    fn floors(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new_bound(py);
        for entry in self.inner.iter() {
            let id = entry.id.clone();
            dict.set_item(&id, PyFloorEntry::from_inner(entry).into_py(py))?;
        }
        Ok(dict.unbind())
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __repr__(&self) -> String {
        format!("FloorRegistry(count={})", self.inner.len())
    }
}
