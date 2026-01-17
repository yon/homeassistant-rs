//! Python wrappers for AreaRegistry

use ha_registries::area_registry::{AreaEntry, AreaRegistry};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tokio::runtime::Handle;

use super::py_storage::PyStorage;

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
        format!("AreaEntry(id='{}', name='{}')", self.inner.id, self.inner.name)
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

impl PyAreaEntry {
    pub fn from_inner(inner: AreaEntry) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &AreaEntry {
        &self.inner
    }
}

/// Python wrapper for AreaRegistry
#[pyclass(name = "AreaRegistry")]
pub struct PyAreaRegistry {
    inner: Arc<AreaRegistry>,
}

#[pymethods]
impl PyAreaRegistry {
    #[new]
    fn new(storage: &PyStorage) -> Self {
        Self {
            inner: Arc::new(AreaRegistry::new(storage.inner().clone())),
        }
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

    /// Create a new area
    #[pyo3(signature = (name, *, aliases=None, floor_id=None, icon=None, picture=None, labels=None))]
    fn async_create(
        &self,
        name: &str,
        aliases: Option<Vec<String>>,
        floor_id: Option<String>,
        icon: Option<String>,
        picture: Option<String>,
        labels: Option<Vec<String>>,
    ) -> PyAreaEntry {
        let entry = self.inner.create(name);

        // Update with optional fields if provided
        if aliases.is_some() || floor_id.is_some() || icon.is_some() || picture.is_some() || labels.is_some() {
            if let Some(updated) = self.inner.update(&entry.id, |e| {
                if let Some(ref a) = aliases {
                    e.aliases = a.clone();
                }
                if floor_id.is_some() {
                    e.floor_id = floor_id.clone();
                }
                if icon.is_some() {
                    e.icon = icon.clone();
                }
                if picture.is_some() {
                    e.picture = picture.clone();
                }
                if let Some(ref l) = labels {
                    e.labels = l.clone();
                }
            }) {
                return PyAreaEntry::from_inner(updated);
            }
        }

        PyAreaEntry::from_inner(entry)
    }

    /// Update an area
    #[pyo3(signature = (area_id, *, name=None, aliases=None, floor_id=None, icon=None, picture=None, labels=None))]
    fn async_update(
        &self,
        area_id: &str,
        name: Option<String>,
        aliases: Option<Vec<String>>,
        floor_id: Option<String>,
        icon: Option<String>,
        picture: Option<String>,
        labels: Option<Vec<String>>,
    ) -> PyResult<PyAreaEntry> {
        self.inner
            .update(area_id, |entry| {
                if let Some(ref n) = name {
                    entry.name = n.clone();
                }
                if let Some(ref a) = aliases {
                    entry.aliases = a.clone();
                }
                if floor_id.is_some() {
                    entry.floor_id = floor_id.clone();
                }
                if icon.is_some() {
                    entry.icon = icon.clone();
                }
                if picture.is_some() {
                    entry.picture = picture.clone();
                }
                if let Some(ref l) = labels {
                    entry.labels = l.clone();
                }
            })
            .map(PyAreaEntry::from_inner)
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!(
                    "Area not found: {}",
                    area_id
                ))
            })
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

impl PyAreaRegistry {
    pub fn from_arc(inner: Arc<AreaRegistry>) -> Self {
        Self { inner }
    }
}
