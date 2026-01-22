//! Python wrappers for LabelRegistry

use ha_registries::label_registry::{LabelEntry, LabelRegistry};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tokio::runtime::Handle;

use super::py_storage::PyStorage;

/// Python wrapper for LabelEntry
#[pyclass(name = "LabelEntry")]
#[derive(Clone)]
pub struct PyLabelEntry {
    inner: LabelEntry,
}

#[pymethods]
impl PyLabelEntry {
    #[getter]
    fn label_id(&self) -> &str {
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
    fn color(&self) -> Option<&str> {
        self.inner.color.as_deref()
    }

    #[getter]
    fn description(&self) -> Option<&str> {
        self.inner.description.as_deref()
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
            "LabelEntry(id='{}', name='{}')",
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

impl PyLabelEntry {
    /// Create from Arc<LabelEntry> - clones the inner value for Python ownership
    pub fn from_inner(inner: Arc<LabelEntry>) -> Self {
        Self {
            inner: (*inner).clone(),
        }
    }

    pub fn inner(&self) -> &LabelEntry {
        &self.inner
    }
}

/// Python wrapper for LabelRegistry
#[pyclass(name = "LabelRegistry")]
pub struct PyLabelRegistry {
    inner: Arc<LabelRegistry>,
}

#[pymethods]
impl PyLabelRegistry {
    #[new]
    fn new(storage: &PyStorage) -> Self {
        Self {
            inner: Arc::new(LabelRegistry::new(storage.inner().clone())),
        }
    }

    /// Load labels from storage
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

    /// Save labels to storage
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

    /// Get label by ID
    fn async_get_label(&self, label_id: &str) -> Option<PyLabelEntry> {
        self.inner.get(label_id).map(PyLabelEntry::from_inner)
    }

    /// Get label by name
    fn async_get_label_by_name(&self, name: &str) -> Option<PyLabelEntry> {
        self.inner.get_by_name(name).map(PyLabelEntry::from_inner)
    }

    /// Create a new label
    #[pyo3(signature = (name, *, icon=None, color=None, description=None))]
    fn async_create(
        &self,
        name: &str,
        icon: Option<String>,
        color: Option<String>,
        description: Option<String>,
    ) -> PyLabelEntry {
        let mut entry = LabelEntry::new(name);

        if let Some(i) = icon {
            entry = entry.with_icon(i);
        }
        if let Some(c) = color {
            entry = entry.with_color(c);
        }
        if let Some(d) = description {
            entry = entry.with_description(d);
        }

        let created = self.inner.create_with(entry);
        PyLabelEntry::from_inner(created)
    }

    /// Update a label
    #[pyo3(signature = (label_id, *, name=None, icon=None, color=None, description=None))]
    fn async_update(
        &self,
        label_id: &str,
        name: Option<String>,
        icon: Option<String>,
        color: Option<String>,
        description: Option<String>,
    ) -> PyResult<PyLabelEntry> {
        self.inner
            .update(label_id, |entry| {
                if let Some(ref n) = name {
                    entry.name = n.clone();
                }
                if icon.is_some() {
                    entry.icon = icon.clone();
                }
                if color.is_some() {
                    entry.color = color.clone();
                }
                if description.is_some() {
                    entry.description = description.clone();
                }
            })
            .map(PyLabelEntry::from_inner)
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!(
                    "Label not found: {}",
                    label_id
                ))
            })
    }

    /// Delete a label
    fn async_delete(&self, label_id: &str) -> PyResult<()> {
        self.inner.remove(label_id).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!("Label not found: {}", label_id))
        })?;
        Ok(())
    }

    /// List all labels
    fn async_list_labels(&self) -> Vec<PyLabelEntry> {
        self.inner.iter().map(PyLabelEntry::from_inner).collect()
    }

    /// Get labels sorted by name
    fn sorted_by_name(&self) -> Vec<PyLabelEntry> {
        self.inner
            .sorted_by_name()
            .into_iter()
            .map(PyLabelEntry::from_inner)
            .collect()
    }

    /// Get all labels as a dict (label_id -> LabelEntry)
    #[getter]
    fn labels(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new_bound(py);
        for entry in self.inner.iter() {
            let id = entry.id.clone();
            dict.set_item(&id, PyLabelEntry::from_inner(entry).into_py(py))?;
        }
        Ok(dict.unbind())
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __repr__(&self) -> String {
        format!("LabelRegistry(count={})", self.inner.len())
    }
}

impl PyLabelRegistry {
    pub fn from_arc(inner: Arc<LabelRegistry>) -> Self {
        Self { inner }
    }
}
