//! Python wrappers for FloorRegistry

use ha_registries::floor_registry::{FloorEntry, FloorRegistry};
use ha_registries::storage::Storage;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tokio::runtime::Handle;

use super::py_storage::PyStorage;

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
    fn level(&self) -> i32 {
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
            "FloorEntry(id='{}', name='{}', level={})",
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
    pub fn from_inner(inner: FloorEntry) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &FloorEntry {
        &self.inner
    }
}

/// Python wrapper for FloorRegistry
#[pyclass(name = "FloorRegistry")]
pub struct PyFloorRegistry {
    inner: Arc<FloorRegistry>,
    storage: Arc<Storage>,
}

#[pymethods]
impl PyFloorRegistry {
    #[new]
    fn new(storage: &PyStorage) -> Self {
        let storage_arc = storage.inner().clone();
        Self {
            inner: Arc::new(FloorRegistry::new(storage_arc.clone())),
            storage: storage_arc,
        }
    }

    /// Load floors from storage
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

    /// Save floors to storage
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
    #[pyo3(signature = (name, *, level=0, aliases=None, icon=None))]
    fn async_create(
        &self,
        name: &str,
        level: i32,
        aliases: Option<Vec<String>>,
        icon: Option<String>,
    ) -> PyFloorEntry {
        let entry = self.inner.create(name, level);

        // Update with optional fields if provided
        if aliases.is_some() || icon.is_some() {
            if let Some(updated) = self.inner.update(&entry.id, |e| {
                if let Some(ref a) = aliases {
                    e.aliases = a.clone();
                }
                if icon.is_some() {
                    e.icon = icon.clone();
                }
            }) {
                return PyFloorEntry::from_inner(updated);
            }
        }

        PyFloorEntry::from_inner(entry)
    }

    /// Update a floor
    #[pyo3(signature = (floor_id, *, name=None, level=None, aliases=None, icon=None))]
    fn async_update(
        &self,
        floor_id: &str,
        name: Option<String>,
        level: Option<i32>,
        aliases: Option<Vec<String>>,
        icon: Option<String>,
    ) -> PyResult<PyFloorEntry> {
        self.inner
            .update(floor_id, |entry| {
                if let Some(ref n) = name {
                    entry.name = n.clone();
                }
                if let Some(l) = level {
                    entry.level = l;
                }
                if let Some(ref a) = aliases {
                    entry.aliases = a.clone();
                }
                if icon.is_some() {
                    entry.icon = icon.clone();
                }
            })
            .map(PyFloorEntry::from_inner)
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!(
                    "Floor not found: {}",
                    floor_id
                ))
            })
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

impl PyFloorRegistry {
    pub fn from_arc(inner: Arc<FloorRegistry>, storage: Arc<Storage>) -> Self {
        Self { inner, storage }
    }
}
