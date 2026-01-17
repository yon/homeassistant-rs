//! Python wrappers for Storage

use ha_registries::storage::{Storage, StorageFile};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tokio::runtime::Handle;

use super::py_types::{json_to_py, py_to_json};

/// Python wrapper for Storage
#[pyclass(name = "Storage")]
#[derive(Clone)]
pub struct PyStorage {
    inner: Arc<Storage>,
}

#[pymethods]
impl PyStorage {
    #[new]
    fn new(config_dir: &str) -> Self {
        Self {
            inner: Arc::new(Storage::new(config_dir)),
        }
    }

    /// Get the storage directory path
    #[getter]
    fn storage_dir(&self) -> String {
        self.inner.storage_dir().to_string_lossy().to_string()
    }

    /// Get the file path for a storage key
    fn file_path(&self, key: &str) -> String {
        self.inner.file_path(key).to_string_lossy().to_string()
    }

    /// Check if a storage key exists
    fn exists(&self, key: &str) -> PyResult<bool> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let inner = self.inner.clone();
        let key = key.to_string();

        tokio::task::block_in_place(|| handle.block_on(async { inner.exists(&key).await }))
            .then_some(true)
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to check existence")
            })
            .or(Ok(false))
    }

    /// Load data from storage
    ///
    /// Returns None if the file doesn't exist.
    fn load(&self, py: Python<'_>, key: &str) -> PyResult<Option<PyObject>> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let inner = self.inner.clone();
        let key = key.to_string();

        let result: Result<Option<StorageFile<serde_json::Value>>, _> =
            tokio::task::block_in_place(|| handle.block_on(async { inner.load(&key).await }));

        match result {
            Ok(Some(storage_file)) => {
                let dict = PyDict::new_bound(py);
                dict.set_item("version", storage_file.version)?;
                dict.set_item("minor_version", storage_file.minor_version)?;
                dict.set_item("key", &storage_file.key)?;
                dict.set_item("data", json_to_py(py, &storage_file.data)?)?;
                Ok(Some(dict.into_any().unbind()))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                e.to_string(),
            )),
        }
    }

    /// Save data to storage
    #[pyo3(signature = (key, data, version=1, minor_version=1))]
    fn save(
        &self,
        key: &str,
        data: &Bound<'_, PyDict>,
        version: u32,
        minor_version: u32,
    ) -> PyResult<()> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let json_data = py_to_json(data.as_any())?;
        let storage_file = StorageFile::new(key, json_data, version, minor_version);

        let inner = self.inner.clone();

        tokio::task::block_in_place(|| handle.block_on(async { inner.save(&storage_file).await }))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Delete a storage file
    fn delete(&self, key: &str) -> PyResult<()> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let inner = self.inner.clone();
        let key = key.to_string();

        tokio::task::block_in_place(|| handle.block_on(async { inner.delete(&key).await }))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// List all storage keys
    fn list_keys(&self) -> PyResult<Vec<String>> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let inner = self.inner.clone();

        tokio::task::block_in_place(|| handle.block_on(async { inner.list_keys().await }))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Ensure the storage directory exists
    fn ensure_dir(&self) -> PyResult<()> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let inner = self.inner.clone();

        tokio::task::block_in_place(|| handle.block_on(async { inner.ensure_dir().await }))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!("Storage(dir='{}')", self.inner.storage_dir().display())
    }
}

impl PyStorage {
    pub fn from_arc(inner: Arc<Storage>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<Storage> {
        &self.inner
    }
}
