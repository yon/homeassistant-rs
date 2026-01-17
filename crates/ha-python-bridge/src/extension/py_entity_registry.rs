//! Python wrappers for EntityRegistry

use ha_registries::entity_registry::{
    DisabledBy, EntityCategory, EntityEntry, EntityRegistry, HiddenBy,
};
use ha_registries::storage::Storage;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tokio::runtime::Handle;

use super::py_storage::PyStorage;
use super::py_types::json_to_py;

/// Python wrapper for EntityEntry
#[pyclass(name = "EntityEntry")]
#[derive(Clone)]
pub struct PyEntityEntry {
    inner: EntityEntry,
}

#[pymethods]
impl PyEntityEntry {
    #[getter]
    fn id(&self) -> &str {
        &self.inner.id
    }

    #[getter]
    fn entity_id(&self) -> &str {
        &self.inner.entity_id
    }

    #[getter]
    fn unique_id(&self) -> Option<&str> {
        self.inner.unique_id.as_deref()
    }

    #[getter]
    fn previous_unique_id(&self) -> Option<&str> {
        self.inner.previous_unique_id.as_deref()
    }

    #[getter]
    fn platform(&self) -> &str {
        &self.inner.platform
    }

    #[getter]
    fn device_id(&self) -> Option<&str> {
        self.inner.device_id.as_deref()
    }

    #[getter]
    fn config_entry_id(&self) -> Option<&str> {
        self.inner.config_entry_id.as_deref()
    }

    #[getter]
    fn config_subentry_id(&self) -> Option<&str> {
        self.inner.config_subentry_id.as_deref()
    }

    #[getter]
    fn name(&self) -> Option<&str> {
        self.inner.name.as_deref()
    }

    #[getter]
    fn original_name(&self) -> Option<&str> {
        self.inner.original_name.as_deref()
    }

    #[getter]
    fn suggested_object_id(&self) -> Option<&str> {
        self.inner.suggested_object_id.as_deref()
    }

    #[getter]
    fn has_entity_name(&self) -> bool {
        self.inner.has_entity_name
    }

    #[getter]
    fn domain(&self) -> &str {
        self.inner.domain()
    }

    #[getter]
    fn object_id(&self) -> &str {
        self.inner.object_id()
    }

    #[getter]
    fn disabled_by(&self) -> Option<&str> {
        self.inner.disabled_by.as_ref().map(|d| match d {
            DisabledBy::Integration => "integration",
            DisabledBy::User => "user",
            DisabledBy::ConfigEntry => "config_entry",
            DisabledBy::Device => "device",
        })
    }

    #[getter]
    fn hidden_by(&self) -> Option<&str> {
        self.inner.hidden_by.as_ref().map(|h| match h {
            HiddenBy::Integration => "integration",
            HiddenBy::User => "user",
        })
    }

    #[getter]
    fn entity_category(&self) -> Option<&str> {
        self.inner.entity_category.as_ref().map(|c| match c {
            EntityCategory::Config => "config",
            EntityCategory::Diagnostic => "diagnostic",
        })
    }

    #[getter]
    fn device_class(&self) -> Option<&str> {
        self.inner.device_class.as_deref()
    }

    #[getter]
    fn original_device_class(&self) -> Option<&str> {
        self.inner.original_device_class.as_deref()
    }

    #[getter]
    fn icon(&self) -> Option<&str> {
        self.inner.icon.as_deref()
    }

    #[getter]
    fn original_icon(&self) -> Option<&str> {
        self.inner.original_icon.as_deref()
    }

    #[getter]
    fn unit_of_measurement(&self) -> Option<&str> {
        self.inner.unit_of_measurement.as_deref()
    }

    #[getter]
    fn translation_key(&self) -> Option<&str> {
        self.inner.translation_key.as_deref()
    }

    #[getter]
    fn supported_features(&self) -> u32 {
        self.inner.supported_features
    }

    #[getter]
    fn capabilities(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        match &self.inner.capabilities {
            Some(c) => Ok(Some(json_to_py(py, c)?)),
            None => Ok(None),
        }
    }

    #[getter]
    fn options(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        match &self.inner.options {
            Some(o) => Ok(Some(json_to_py(py, o)?)),
            None => Ok(None),
        }
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

    fn is_disabled(&self) -> bool {
        self.inner.is_disabled()
    }

    fn is_hidden(&self) -> bool {
        self.inner.is_hidden()
    }

    fn __repr__(&self) -> String {
        format!(
            "EntityEntry(entity_id='{}', platform='{}')",
            self.inner.entity_id, self.inner.platform
        )
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner.entity_id == other.inner.entity_id
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.entity_id.hash(&mut hasher);
        hasher.finish()
    }
}

impl PyEntityEntry {
    pub fn from_inner(inner: EntityEntry) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &EntityEntry {
        &self.inner
    }
}

fn parse_disabled_by(s: Option<&str>) -> Option<DisabledBy> {
    s.and_then(|s| match s {
        "integration" => Some(DisabledBy::Integration),
        "user" => Some(DisabledBy::User),
        "config_entry" => Some(DisabledBy::ConfigEntry),
        "device" => Some(DisabledBy::Device),
        _ => None,
    })
}

fn parse_hidden_by(s: Option<&str>) -> Option<HiddenBy> {
    s.and_then(|s| match s {
        "integration" => Some(HiddenBy::Integration),
        "user" => Some(HiddenBy::User),
        _ => None,
    })
}

fn parse_entity_category(s: Option<&str>) -> Option<EntityCategory> {
    s.and_then(|s| match s {
        "config" => Some(EntityCategory::Config),
        "diagnostic" => Some(EntityCategory::Diagnostic),
        _ => None,
    })
}

/// Python wrapper for EntityRegistry
#[pyclass(name = "EntityRegistry")]
pub struct PyEntityRegistry {
    inner: Arc<EntityRegistry>,
    storage: Arc<Storage>,
}

#[pymethods]
impl PyEntityRegistry {
    #[new]
    fn new(storage: &PyStorage) -> Self {
        let storage_arc = storage.inner().clone();
        Self {
            inner: Arc::new(EntityRegistry::new(storage_arc.clone())),
            storage: storage_arc,
        }
    }

    /// Load entities from storage
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

    /// Save entities to storage
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

    /// Get entity by entity_id
    fn async_get(&self, entity_id: &str) -> Option<PyEntityEntry> {
        self.inner.get(entity_id).map(PyEntityEntry::from_inner)
    }

    /// Get entity ID by unique_id lookup
    fn async_get_entity_id(
        &self,
        domain: &str,
        platform: &str,
        unique_id: &str,
    ) -> Option<String> {
        self.inner
            .get_by_unique_id(unique_id)
            .filter(|e| e.domain() == domain && e.platform == platform)
            .map(|e| e.entity_id.clone())
    }

    /// Get entity by unique_id
    fn async_get_by_unique_id(&self, unique_id: &str) -> Option<PyEntityEntry> {
        self.inner
            .get_by_unique_id(unique_id)
            .map(PyEntityEntry::from_inner)
    }

    /// Get all entities for a device
    fn async_entries_for_device(&self, device_id: &str) -> Vec<PyEntityEntry> {
        self.inner
            .get_by_device_id(device_id)
            .into_iter()
            .map(PyEntityEntry::from_inner)
            .collect()
    }

    /// Get all entities for a config entry
    fn async_entries_for_config_entry(&self, config_entry_id: &str) -> Vec<PyEntityEntry> {
        self.inner
            .get_by_config_entry_id(config_entry_id)
            .into_iter()
            .map(PyEntityEntry::from_inner)
            .collect()
    }

    /// Get all entities in an area
    fn async_entries_for_area(&self, area_id: &str) -> Vec<PyEntityEntry> {
        self.inner
            .get_by_area_id(area_id)
            .into_iter()
            .map(PyEntityEntry::from_inner)
            .collect()
    }

    /// Get all entities for a platform
    fn async_entries_for_platform(&self, platform: &str) -> Vec<PyEntityEntry> {
        self.inner
            .get_by_platform(platform)
            .into_iter()
            .map(PyEntityEntry::from_inner)
            .collect()
    }

    /// Get or create an entity
    #[pyo3(signature = (domain, platform, unique_id, *, config_entry_id=None, device_id=None, suggested_object_id=None))]
    fn async_get_or_create(
        &self,
        domain: &str,
        platform: &str,
        unique_id: &str,
        config_entry_id: Option<&str>,
        device_id: Option<&str>,
        suggested_object_id: Option<&str>,
    ) -> PyEntityEntry {
        // Build entity_id from domain and suggested_object_id or unique_id
        let object_id = suggested_object_id.unwrap_or(unique_id);
        let entity_id = format!("{}.{}", domain, object_id);

        let entry = self.inner.get_or_create(
            platform,
            &entity_id,
            Some(unique_id),
            config_entry_id,
            device_id,
        );

        PyEntityEntry::from_inner(entry)
    }

    /// Update an entity
    #[pyo3(signature = (
        entity_id,
        *,
        name=None,
        icon=None,
        area_id=None,
        disabled_by=None,
        hidden_by=None,
        device_class=None,
        unit_of_measurement=None,
        labels=None,
        aliases=None
    ))]
    fn async_update_entity(
        &self,
        entity_id: &str,
        name: Option<String>,
        icon: Option<String>,
        area_id: Option<String>,
        disabled_by: Option<String>,
        hidden_by: Option<String>,
        device_class: Option<String>,
        unit_of_measurement: Option<String>,
        labels: Option<Vec<String>>,
        aliases: Option<Vec<String>>,
    ) -> PyResult<PyEntityEntry> {
        let entry = self.inner.update(entity_id, |entry| {
            if name.is_some() {
                entry.name = name.clone();
            }
            if icon.is_some() {
                entry.icon = icon.clone();
            }
            if area_id.is_some() {
                entry.area_id = area_id.clone();
            }
            if disabled_by.is_some() {
                entry.disabled_by = parse_disabled_by(disabled_by.as_deref());
            }
            if hidden_by.is_some() {
                entry.hidden_by = parse_hidden_by(hidden_by.as_deref());
            }
            if device_class.is_some() {
                entry.device_class = device_class.clone();
            }
            if unit_of_measurement.is_some() {
                entry.unit_of_measurement = unit_of_measurement.clone();
            }
            if let Some(ref l) = labels {
                entry.labels = l.clone();
            }
            if let Some(ref a) = aliases {
                entry.aliases = a.clone();
            }
        });

        Ok(PyEntityEntry::from_inner(entry))
    }

    /// Remove an entity
    fn async_remove(&self, entity_id: &str) -> PyResult<()> {
        self.inner.remove(entity_id).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!(
                "Entity not found: {}",
                entity_id
            ))
        })?;
        Ok(())
    }

    /// Check if an entity is registered
    fn async_is_registered(&self, entity_id: &str) -> bool {
        self.inner.get(entity_id).is_some()
    }

    /// Get all entity IDs
    fn entity_ids(&self) -> Vec<String> {
        self.inner.entity_ids()
    }

    /// Get all entities as a dict (entity_id -> EntityEntry)
    #[getter]
    fn entities(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new_bound(py);
        for entry in self.inner.iter() {
            let entity_id = entry.entity_id.clone();
            dict.set_item(&entity_id, PyEntityEntry::from_inner(entry).into_py(py))?;
        }
        Ok(dict.unbind())
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __repr__(&self) -> String {
        format!("EntityRegistry(count={})", self.inner.len())
    }
}

impl PyEntityRegistry {
    pub fn from_arc(inner: Arc<EntityRegistry>, storage: Arc<Storage>) -> Self {
        Self { inner, storage }
    }
}
