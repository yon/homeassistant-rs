//! AreaRegistryWrapper - wraps Rust AreaRegistry for Python access
//!
//! This provides the `hass.data["area_registry"]` object that Python
//! integrations expect. It wraps the Rust `ha_registries::AreaRegistry`
//! and exposes the same interface as native HA's `AreaRegistry`.

use ha_registries::area_registry::AreaEntry;
use ha_registries::Registries;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;

/// Python-visible AreaEntry wrapper
#[pyclass(name = "AreaEntry")]
#[derive(Clone)]
pub struct PyBridgeAreaEntry {
    inner: Arc<AreaEntry>,
}

impl PyBridgeAreaEntry {
    fn from_arc(entry: Arc<AreaEntry>) -> Self {
        Self { inner: entry }
    }
}

#[pymethods]
impl PyBridgeAreaEntry {
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
    fn aliases(&self) -> Vec<String> {
        self.inner.aliases.clone()
    }

    #[getter]
    fn floor_id(&self) -> Option<&str> {
        self.inner.floor_id.as_deref()
    }

    #[getter]
    fn icon(&self) -> Option<&str> {
        self.inner.icon.as_deref()
    }

    #[getter]
    fn labels(&self) -> Vec<String> {
        self.inner.labels.clone()
    }

    #[getter]
    fn picture(&self) -> Option<&str> {
        self.inner.picture.as_deref()
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
    fn created_at(&self) -> f64 {
        self.inner.created_at.timestamp() as f64
    }

    #[getter]
    fn modified_at(&self) -> f64 {
        self.inner.modified_at.timestamp() as f64
    }

    fn __repr__(&self) -> String {
        format!(
            "AreaEntry(id='{}', name='{}')",
            self.inner.id, self.inner.name
        )
    }

    fn __eq__(&self, other: &PyBridgeAreaEntry) -> bool {
        self.inner.id == other.inner.id
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.id.hash(&mut hasher);
        hasher.finish()
    }
}

/// Dict-like wrapper for areas that supports get_by_name()
///
/// This wraps `AreaRegistry` to provide the `AreaRegistryItems`-like interface
/// that Python code expects from `area_reg.areas`.
#[pyclass(name = "AreaRegistryItems")]
pub struct AreaRegistryItemsWrapper {
    registries: Arc<Registries>,
}

#[pymethods]
impl AreaRegistryItemsWrapper {
    /// Get area by normalized name
    fn get_by_name(&self, name: &str) -> Option<PyBridgeAreaEntry> {
        self.registries
            .areas
            .get_by_name(name)
            .map(PyBridgeAreaEntry::from_arc)
    }

    /// Check if area_id is in the registry
    fn __contains__(&self, area_id: &str) -> bool {
        self.registries.areas.get(area_id).is_some()
    }

    /// Get area by area_id
    fn __getitem__(&self, area_id: &str) -> PyResult<PyBridgeAreaEntry> {
        self.registries
            .areas
            .get(area_id)
            .map(PyBridgeAreaEntry::from_arc)
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!("Area not found: {area_id}"))
            })
    }

    /// Get all area entries
    fn values(&self) -> Vec<PyBridgeAreaEntry> {
        self.registries
            .areas
            .iter()
            .map(PyBridgeAreaEntry::from_arc)
            .collect()
    }

    fn __len__(&self) -> usize {
        self.registries.areas.len()
    }

    fn __repr__(&self) -> String {
        format!("AreaRegistryItems(count={})", self.registries.areas.len())
    }

    /// The underlying data dict (used by `_area_data` / `_data_to_save`)
    #[getter]
    fn data(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new_bound(py);
        for entry in self.registries.areas.iter() {
            let id = entry.id.clone();
            dict.set_item(id, PyBridgeAreaEntry::from_arc(entry).into_py(py))?;
        }
        Ok(dict.unbind())
    }
}

/// Python wrapper for the Rust AreaRegistry
///
/// Stored in `hass.data["area_registry"]` to provide the same interface
/// as native HA's `AreaRegistry` class. Holds `Arc<Registries>` and
/// delegates to `registries.areas`.
#[pyclass(name = "AreaRegistry")]
pub struct AreaRegistryWrapper {
    registries: Arc<Registries>,
}

impl AreaRegistryWrapper {
    pub fn new(registries: Arc<Registries>) -> Self {
        Self { registries }
    }
}

#[pymethods]
impl AreaRegistryWrapper {
    /// Get the areas collection (AreaRegistryItems-like)
    #[getter]
    fn areas(&self) -> AreaRegistryItemsWrapper {
        AreaRegistryItemsWrapper {
            registries: self.registries.clone(),
        }
    }

    /// Get the underlying area data dict
    #[getter]
    fn _area_data(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new_bound(py);
        for entry in self.registries.areas.iter() {
            let id = entry.id.clone();
            dict.set_item(id, PyBridgeAreaEntry::from_arc(entry).into_py(py))?;
        }
        Ok(dict.unbind())
    }

    /// Get area by ID
    fn async_get_area(&self, area_id: &str) -> Option<PyBridgeAreaEntry> {
        self.registries
            .areas
            .get(area_id)
            .map(PyBridgeAreaEntry::from_arc)
    }

    /// Get area by name
    fn async_get_area_by_name(&self, name: &str) -> Option<PyBridgeAreaEntry> {
        self.registries
            .areas
            .get_by_name(name)
            .map(PyBridgeAreaEntry::from_arc)
    }

    /// Get or create an area by name
    ///
    /// Called by `device_registry.async_get_or_create` when a device has
    /// a `suggested_area`. Returns existing area if found, creates new one otherwise.
    fn async_get_or_create(&self, name: &str) -> PyResult<PyBridgeAreaEntry> {
        // Try to find by name first
        if let Some(entry) = self.registries.areas.get_by_name(name) {
            return Ok(PyBridgeAreaEntry::from_arc(entry));
        }

        // Create new area
        let entry = self
            .registries
            .areas
            .create(name, None)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(PyBridgeAreaEntry::from_arc(entry))
    }

    /// Create a new area
    #[pyo3(signature = (name, *, aliases=None, floor_id=None, humidity_entity_id=None, icon=None, labels=None, picture=None, temperature_entity_id=None))]
    #[allow(clippy::too_many_arguments)]
    fn async_create(
        &self,
        name: &str,
        aliases: Option<Vec<String>>,
        floor_id: Option<String>,
        humidity_entity_id: Option<String>,
        icon: Option<String>,
        labels: Option<Vec<String>>,
        picture: Option<String>,
        temperature_entity_id: Option<String>,
    ) -> PyResult<PyBridgeAreaEntry> {
        let entry = self
            .registries
            .areas
            .create(name, None)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        // Update optional fields if provided
        let needs_update = aliases.is_some()
            || floor_id.is_some()
            || humidity_entity_id.is_some()
            || icon.is_some()
            || labels.is_some()
            || picture.is_some()
            || temperature_entity_id.is_some();

        if needs_update {
            let updated = self
                .registries
                .areas
                .update(
                    &entry.id,
                    |e| {
                        if let Some(a) = aliases {
                            e.aliases = a;
                        }
                        if let Some(f) = floor_id {
                            e.floor_id = Some(f);
                        }
                        if let Some(h) = humidity_entity_id {
                            e.humidity_entity_id = Some(h);
                        }
                        if let Some(i) = icon {
                            e.icon = Some(i);
                        }
                        if let Some(l) = labels {
                            e.labels = l;
                        }
                        if let Some(p) = picture {
                            e.picture = Some(p);
                        }
                        if let Some(t) = temperature_entity_id {
                            e.temperature_entity_id = Some(t);
                        }
                    },
                    None,
                )
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
            return Ok(PyBridgeAreaEntry::from_arc(updated));
        }

        Ok(PyBridgeAreaEntry::from_arc(entry))
    }

    /// List all areas
    fn async_list_areas(&self) -> Vec<PyBridgeAreaEntry> {
        self.registries
            .areas
            .iter()
            .map(PyBridgeAreaEntry::from_arc)
            .collect()
    }

    /// Delete an area
    fn async_delete(&self, area_id: &str) -> PyResult<()> {
        self.registries.areas.remove(area_id).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!("Area not found: {area_id}"))
        })?;
        Ok(())
    }

    fn __len__(&self) -> usize {
        self.registries.areas.len()
    }

    fn __repr__(&self) -> String {
        format!("AreaRegistry(count={})", self.registries.areas.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_registries() -> (TempDir, Arc<Registries>) {
        let tmpdir = TempDir::new().unwrap();
        let registries = Arc::new(Registries::new(tmpdir.path()));
        (tmpdir, registries)
    }

    #[test]
    fn test_area_registry_wrapper_empty() {
        let (_tmpdir, registries) = create_test_registries();
        let wrapper = AreaRegistryWrapper::new(registries);
        assert_eq!(wrapper.__len__(), 0);
    }

    #[test]
    fn test_area_registry_wrapper_create_and_get() {
        pyo3::prepare_freethreaded_python();
        let (_tmpdir, registries) = create_test_registries();

        // Create an area directly in Rust
        registries.areas.create("Living Room", None).unwrap();

        let wrapper = AreaRegistryWrapper::new(registries);
        assert_eq!(wrapper.__len__(), 1);

        // Find by name
        let found = wrapper.async_get_area_by_name("Living Room");
        assert!(found.is_some());
        assert_eq!(found.unwrap().inner.name, "Living Room");
    }

    #[test]
    fn test_area_registry_wrapper_get_or_create() {
        pyo3::prepare_freethreaded_python();
        let (_tmpdir, registries) = create_test_registries();
        let wrapper = AreaRegistryWrapper::new(registries);

        Python::with_gil(|_py| {
            // First call creates
            let area = wrapper.async_get_or_create("Kitchen").unwrap();
            assert_eq!(area.inner.name, "Kitchen");

            // Second call returns existing
            let area2 = wrapper.async_get_or_create("Kitchen").unwrap();
            assert_eq!(area.inner.id, area2.inner.id);
        });
    }

    #[test]
    fn test_areas_items_wrapper() {
        pyo3::prepare_freethreaded_python();
        let (_tmpdir, registries) = create_test_registries();
        registries.areas.create("Room A", None).unwrap();
        registries.areas.create("Room B", None).unwrap();

        let items = AreaRegistryItemsWrapper {
            registries: registries.clone(),
        };

        assert_eq!(items.__len__(), 2);
        assert!(items.__contains__("room_a") || items.__contains__("Room A"));

        let by_name = items.get_by_name("Room A");
        assert!(by_name.is_some());

        let vals = items.values();
        assert_eq!(vals.len(), 2);
    }
}
