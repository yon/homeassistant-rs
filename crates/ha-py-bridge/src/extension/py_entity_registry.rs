//! Python wrappers for EntityRegistry

use ha_registries::entity_registry::{
    DisabledBy, EntityCategory, EntityEntry, EntityRegistry, HiddenBy,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::runtime::Handle;

use super::py_types::{json_to_py, py_to_json};

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
    fn has_entity_name(&self) -> Option<bool> {
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
            DisabledBy::ConfigEntry => "config_entry",
            DisabledBy::Device => "device",
            DisabledBy::Hass => "hass",
            DisabledBy::Integration => "integration",
            DisabledBy::User => "user",
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
    fn labels(&self) -> std::collections::HashSet<String> {
        self.inner.labels.clone()
    }

    #[getter]
    fn aliases(&self) -> std::collections::HashSet<String> {
        self.inner.aliases.clone()
    }

    #[getter]
    fn categories(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.inner.categories {
            Some(v) => json_to_py(py, v),
            None => {
                // Return empty dict for None
                use pyo3::types::PyDict;
                Ok(PyDict::new_bound(py).into())
            }
        }
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
    fn orphaned_timestamp(&self) -> Option<f64> {
        self.inner.orphaned_timestamp
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
    /// Create from Arc<EntityEntry> - clones the inner value for Python ownership
    pub fn from_inner(inner: Arc<EntityEntry>) -> Self {
        Self {
            inner: (*inner).clone(),
        }
    }

    pub fn inner(&self) -> &EntityEntry {
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

fn parse_hidden_by(s: Option<&str>) -> Option<HiddenBy> {
    s.and_then(|s| match s {
        "integration" => Some(HiddenBy::Integration),
        "user" => Some(HiddenBy::User),
        _ => None,
    })
}

/// Python wrapper for EntityRegistry
#[pyclass(name = "EntityRegistry")]
pub struct PyEntityRegistry {
    inner: Arc<EntityRegistry>,
    #[pyo3(get)]
    hass: PyObject,
}

#[pymethods]
impl PyEntityRegistry {
    #[new]
    fn new(py: Python<'_>, hass: PyObject) -> PyResult<Self> {
        // Extract config directory path from hass.config.path()
        // Note: Storage::new() adds ".storage" internally, so we pass the config dir
        let config = hass.getattr(py, "config")?;
        let config_dir: String = config.call_method1(py, "path", ("",))?.extract(py)?;

        // Create Rust storage and registry
        let storage = Arc::new(ha_registries::storage::Storage::new(&config_dir));
        let registry = EntityRegistry::new(storage);

        Ok(Self {
            inner: Arc::new(registry),
            hass,
        })
    }

    /// Load entities from storage
    fn async_load(&self) -> PyResult<()> {
        // Try to use existing Tokio runtime, or create a new one
        let inner = self.inner.clone();
        if let Ok(handle) = Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(async { inner.load().await }))
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
        } else {
            // No runtime available, create a temporary one
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

    /// Save entities to storage
    fn async_save(&self) -> PyResult<()> {
        // Try to use existing Tokio runtime, or create a new one
        let inner = self.inner.clone();
        if let Ok(handle) = Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(async { inner.save().await }))
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
        } else {
            // No runtime available, create a temporary one
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

    /// Get entity by entity_id
    fn async_get(&self, entity_id: &str) -> Option<PyEntityEntry> {
        self.inner.get(entity_id).map(PyEntityEntry::from_inner)
    }

    /// Get entity ID by unique_id lookup
    fn async_get_entity_id(&self, domain: &str, platform: &str, unique_id: &str) -> Option<String> {
        self.inner
            .get_by_platform_unique_id(platform, unique_id)
            .filter(|e| e.domain() == domain)
            .map(|e| e.entity_id.clone())
    }

    /// Check if an entity with the given (domain, platform, unique_id) is in deleted_entities
    fn is_deleted(&self, domain: &str, platform: &str, unique_id: &str) -> bool {
        self.inner.is_deleted(domain, platform, unique_id)
    }

    /// Clear area_id from deleted entities matching the given area_id
    fn clear_deleted_area_id(&self, area_id: &str) {
        self.inner.clear_deleted_area_id(area_id)
    }

    /// Clear label_id from deleted entities that have it
    fn clear_deleted_label_id(&self, label_id: &str) {
        self.inner.clear_deleted_label_id(label_id)
    }

    /// Clear category from deleted entities matching scope and category_id
    fn clear_deleted_category_id(&self, scope: &str, category_id: &str) {
        self.inner.clear_deleted_category_id(scope, category_id)
    }

    /// Clear config_entry_id from deleted entities matching the given config_entry_id
    fn clear_deleted_config_entry(&self, config_entry_id: &str, orphaned_timestamp: f64) {
        self.inner
            .clear_deleted_config_entry(config_entry_id, orphaned_timestamp)
    }

    /// Clear config_subentry_id from deleted entities matching the given config_entry_id and subentry_id
    fn clear_deleted_config_subentry(
        &self,
        config_entry_id: &str,
        config_subentry_id: &str,
        orphaned_timestamp: f64,
    ) {
        self.inner.clear_deleted_config_subentry(
            config_entry_id,
            config_subentry_id,
            orphaned_timestamp,
        )
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
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        domain,
        platform,
        unique_id,
        *,
        config_entry_id=None,
        config_subentry_id=None,
        device_id=None,
        suggested_object_id=None,
        disabled_by=None,
        hidden_by=None,
        has_entity_name=None,
        capabilities=None,
        supported_features=None,
        device_class=None,
        unit_of_measurement=None,
        original_name=None,
        original_icon=None,
        original_device_class=None,
        entity_category=None,
        translation_key=None,
        // Accept but ignore these - they're Python-specific
        known_object_ids=None,
        get_initial_options=None,
        calculated_object_id=None,
        // Entity IDs that should be considered unavailable (e.g., from state machine)
        reserved_ids=None,
        // Timestamp overrides for tests (ISO format strings)
        // created_at: only passed for new entities
        // modified_at: passed for updates (and new entities)
        created_at=None,
        modified_at=None
    ))]
    fn async_get_or_create(
        &self,
        domain: &str,
        platform: &str,
        unique_id: &str,
        config_entry_id: Option<&str>,
        config_subentry_id: Option<&str>,
        device_id: Option<&str>,
        suggested_object_id: Option<&str>,
        disabled_by: Option<&str>,
        hidden_by: Option<&str>,
        has_entity_name: Option<bool>,
        capabilities: Option<&Bound<'_, PyAny>>,
        supported_features: Option<u32>,
        device_class: Option<&str>,
        unit_of_measurement: Option<&str>,
        original_name: Option<&str>,
        original_icon: Option<&str>,
        original_device_class: Option<&str>,
        entity_category: Option<&str>,
        translation_key: Option<&str>,
        // Accepted but ignored
        #[allow(unused_variables)] known_object_ids: Option<&Bound<'_, PyAny>>,
        #[allow(unused_variables)] get_initial_options: Option<&Bound<'_, PyAny>>,
        #[allow(unused_variables)] calculated_object_id: Option<&str>,
        // Entity IDs that should be considered unavailable (e.g., from state machine)
        reserved_ids: Option<Vec<String>>,
        // Timestamp overrides for tests (ISO format strings)
        created_at: Option<&str>,
        modified_at: Option<&str>,
    ) -> PyEntityEntry {
        // Check if entity with this (platform, unique_id) already exists
        let existing = self.inner.get_by_platform_unique_id(platform, unique_id);
        let is_new = existing.is_none();

        // Determine the entity_id to use
        let entity_id = if let Some(ref existing_entry) = existing {
            // Entity exists - use its current entity_id
            existing_entry.entity_id.clone()
        } else {
            // New entity - generate a conflict-free entity_id
            let object_id = suggested_object_id
                .map(String::from)
                .unwrap_or_else(|| format!("{}_{}", platform, unique_id));
            // Use generate_entity_id to handle conflicts (_2, _3, etc.)
            // Pass reserved_ids to also check against state machine entity IDs
            self.inner
                .generate_entity_id(domain, &object_id, None, reserved_ids.as_deref())
        };

        // Get or create the base entry with the resolved entity_id
        let entry = self.inner.get_or_create(
            platform,
            &entity_id,
            Some(unique_id),
            config_entry_id,
            device_id,
        );

        // Parse timestamps if provided (ISO format strings)
        let created_timestamp = created_at.and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        });
        let modified_timestamp = modified_at.and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        });

        // Update with additional fields if provided
        // Note: config_entry_id and device_id can be updated on existing entities
        let needs_update = config_entry_id.is_some()
            || config_subentry_id.is_some()
            || device_id.is_some()
            || disabled_by.is_some()
            || hidden_by.is_some()
            || has_entity_name.is_some()
            || capabilities.is_some()
            || supported_features.is_some()
            || device_class.is_some()
            || unit_of_measurement.is_some()
            || original_name.is_some()
            || original_icon.is_some()
            || original_device_class.is_some()
            || entity_category.is_some()
            || translation_key.is_some()
            || created_timestamp.is_some()
            || modified_timestamp.is_some();

        if needs_update {
            // Convert capabilities from Python dict to JSON
            let caps_json = capabilities.and_then(|c| py_to_json(c).ok());

            // Parse disabled_by and hidden_by enums
            let disabled = disabled_by.and_then(|s| match s {
                "config_entry" => Some(DisabledBy::ConfigEntry),
                "device" => Some(DisabledBy::Device),
                "hass" => Some(DisabledBy::Hass),
                "integration" => Some(DisabledBy::Integration),
                "user" => Some(DisabledBy::User),
                _ => None,
            });
            let hidden = hidden_by.and_then(|s| match s {
                "integration" => Some(HiddenBy::Integration),
                "user" => Some(HiddenBy::User),
                _ => None,
            });
            let category = entity_category.and_then(|s| match s {
                "config" => Some(EntityCategory::Config),
                "diagnostic" => Some(EntityCategory::Diagnostic),
                _ => None,
            });

            // Helper to handle "clear" marker: empty string means clear, non-empty means set
            fn update_optional_field(field: &mut Option<String>, value: Option<&str>) {
                if let Some(v) = value {
                    if v.is_empty() {
                        *field = None; // Empty string = clear
                    } else {
                        *field = Some(v.to_string());
                    }
                }
            }

            let updated = self.inner.update(&entity_id, |e| {
                update_optional_field(&mut e.config_entry_id, config_entry_id);
                update_optional_field(&mut e.config_subentry_id, config_subentry_id);
                update_optional_field(&mut e.device_id, device_id);
                // Store suggested_object_id for new entities
                if is_new {
                    e.suggested_object_id = suggested_object_id.map(|s| s.to_string());
                }
                // disabled_by and hidden_by only affect newly created entities,
                // not existing ones (per Home Assistant's entity_registry.py behavior)
                if is_new && disabled.is_some() {
                    e.disabled_by = disabled;
                }
                if is_new && hidden.is_some() {
                    e.hidden_by = hidden;
                }
                // has_entity_name: always set (None, Some(true), or Some(false))
                // Python wrapper controls when to pass this based on UNDEFINED logic
                e.has_entity_name = has_entity_name;
                // capabilities: always set (Python controls via UNDEFINED)
                e.capabilities = caps_json.clone();
                // supported_features: always set (None means 0)
                e.supported_features = supported_features.unwrap_or(0);
                update_optional_field(&mut e.device_class, device_class);
                update_optional_field(&mut e.unit_of_measurement, unit_of_measurement);
                update_optional_field(&mut e.original_name, original_name);
                update_optional_field(&mut e.original_icon, original_icon);
                update_optional_field(&mut e.original_device_class, original_device_class);
                // entity_category: always set (Python controls via UNDEFINED)
                e.entity_category = category;
                update_optional_field(&mut e.translation_key, translation_key);
                // Set timestamps from Python (respects freezer in tests)
                // created_at: only set for new entities (when provided)
                // modified_at: always set (for new entities and updates)
                if let Some(ts) = created_timestamp {
                    e.created_at = ts;
                }
                if let Some(ts) = modified_timestamp {
                    e.modified_at = ts;
                } else {
                    e.modified_at = chrono::Utc::now();
                }
            });

            if let Ok(updated_entry) = updated {
                return PyEntityEntry::from_inner(updated_entry);
            }
        }

        PyEntityEntry::from_inner(entry)
    }

    /// Update an entity
    ///
    /// Includes business logic:
    /// - Unique ID conflict detection (raises ValueError if new_unique_id conflicts)
    /// - Config entry disabled_by propagation (when config_entry_id changes)
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        entity_id,
        *,
        name=None,
        icon=None,
        area_id=None,
        new_entity_id=None,
        new_unique_id=None,
        disabled_by=None,
        hidden_by=None,
        device_class=None,
        unit_of_measurement=None,
        labels=None,
        aliases=None,
        categories=None,
        capabilities=None,
        config_entry_id=None,
        config_subentry_id=None,
        device_id=None,
        entity_category=None,
        has_entity_name=None,
        original_device_class=None,
        original_icon=None,
        original_name=None,
        supported_features=None,
        translation_key=None,
        config_entry_is_disabled=None
    ))]
    fn async_update_entity(
        &self,
        entity_id: &str,
        name: Option<String>,
        icon: Option<String>,
        area_id: Option<String>,
        new_entity_id: Option<String>,
        new_unique_id: Option<String>,
        disabled_by: Option<String>,
        hidden_by: Option<String>,
        device_class: Option<String>,
        unit_of_measurement: Option<String>,
        labels: Option<HashSet<String>>,
        aliases: Option<HashSet<String>>,
        categories: Option<&Bound<'_, PyAny>>,
        capabilities: Option<&Bound<'_, PyAny>>,
        config_entry_id: Option<String>,
        config_subentry_id: Option<String>,
        device_id: Option<String>,
        entity_category: Option<String>,
        has_entity_name: Option<bool>,
        original_device_class: Option<String>,
        original_icon: Option<String>,
        original_name: Option<String>,
        supported_features: Option<u32>,
        translation_key: Option<String>,
        // Whether the new config entry (if config_entry_id is changing) is disabled.
        // Used for disabled_by propagation logic.
        config_entry_is_disabled: Option<bool>,
    ) -> PyResult<PyEntityEntry> {
        // Unique ID conflict detection
        if let Some(ref new_uid) = new_unique_id {
            if !new_uid.is_empty() {
                // Check if another entity uses this unique_id on the same platform
                let current_entry = self.inner.get(entity_id);
                if let Some(ref current) = current_entry {
                    let platform = &current.platform;
                    // Look up by platform + unique_id
                    if let Some(existing) = self.inner.get_by_platform_unique_id(platform, new_uid)
                    {
                        if existing.entity_id != entity_id {
                            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                                "Unique id '{}' is already in use by '{}'",
                                new_uid, existing.entity_id
                            )));
                        }
                    }
                }
            }
        }

        // Config entry disabled_by propagation logic:
        // When config_entry_id changes and disabled_by isn't explicitly set,
        // propagate disabled state from the new config entry.
        let mut effective_disabled_by = disabled_by.clone();
        if config_entry_id.is_some() && disabled_by.is_none() {
            let ce_id = config_entry_id.as_deref().unwrap_or("");
            if ce_id.is_empty() {
                // Config entry being removed - clear CONFIG_ENTRY disabled_by
                if let Some(current) = self.inner.get(entity_id) {
                    if current.disabled_by == Some(DisabledBy::ConfigEntry) {
                        effective_disabled_by = Some(String::new()); // "" = clear
                    }
                }
            } else if let Some(ce_disabled) = config_entry_is_disabled {
                if ce_disabled {
                    // New config entry is disabled - disable entity unless already disabled
                    if let Some(current) = self.inner.get(entity_id) {
                        if current.disabled_by.is_none() {
                            effective_disabled_by = Some("config_entry".to_string());
                        }
                    }
                } else {
                    // New config entry is not disabled - clear CONFIG_ENTRY disabled_by
                    if let Some(current) = self.inner.get(entity_id) {
                        if current.disabled_by == Some(DisabledBy::ConfigEntry) {
                            effective_disabled_by = Some(String::new()); // "" = clear
                        }
                    }
                }
            }
        }

        // Convert categories/capabilities from Python to JSON
        let categories_json = categories.and_then(|c| py_to_json(c).ok());
        let capabilities_json = capabilities.and_then(|c| py_to_json(c).ok());

        let entry = self
            .inner
            .update(entity_id, |entry| {
                if name.is_some() {
                    entry.name = name.clone();
                }
                if icon.is_some() {
                    entry.icon = icon.clone();
                }
                if let Some(ref aid) = area_id {
                    entry.area_id = if aid.is_empty() {
                        None
                    } else {
                        Some(aid.clone())
                    };
                }
                if let Some(ref new_eid) = new_entity_id {
                    entry.entity_id = new_eid.clone();
                }
                if let Some(ref new_uid) = new_unique_id {
                    entry.previous_unique_id = entry.unique_id.clone();
                    entry.unique_id = Some(new_uid.clone());
                }
                if effective_disabled_by.is_some() {
                    entry.disabled_by = parse_disabled_by(effective_disabled_by.as_deref());
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
                if categories_json.is_some() {
                    entry.categories = categories_json.clone();
                }
                if capabilities_json.is_some() {
                    entry.capabilities = capabilities_json.clone();
                }
                if let Some(ref ceid) = config_entry_id {
                    entry.config_entry_id = if ceid.is_empty() {
                        None
                    } else {
                        Some(ceid.clone())
                    };
                }
                if let Some(ref csid) = config_subentry_id {
                    entry.config_subentry_id = if csid.is_empty() {
                        None
                    } else {
                        Some(csid.clone())
                    };
                }
                if let Some(ref did) = device_id {
                    entry.device_id = if did.is_empty() {
                        None
                    } else {
                        Some(did.clone())
                    };
                }
                if let Some(ref ec) = entity_category {
                    entry.entity_category = if ec.is_empty() {
                        None
                    } else {
                        match ec.as_str() {
                            "config" => Some(EntityCategory::Config),
                            "diagnostic" => Some(EntityCategory::Diagnostic),
                            _ => None,
                        }
                    };
                }
                if let Some(hen) = has_entity_name {
                    entry.has_entity_name = Some(hen);
                }
                if original_device_class.is_some() {
                    entry.original_device_class = original_device_class.clone();
                }
                if original_icon.is_some() {
                    entry.original_icon = original_icon.clone();
                }
                if original_name.is_some() {
                    entry.original_name = original_name.clone();
                }
                if let Some(sf) = supported_features {
                    entry.supported_features = sf;
                }
                if translation_key.is_some() {
                    entry.translation_key = translation_key.clone();
                }
            })
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!("{}", e)))?;

        Ok(PyEntityEntry::from_inner(entry))
    }

    /// Remove an entity
    ///
    /// This is idempotent - removing a non-existent entity is a no-op.
    fn async_remove(&self, entity_id: &str) {
        // Ignore result - removing non-existent entity is a no-op
        let _ = self.inner.remove(entity_id);
    }

    /// Check if an entity is registered
    fn async_is_registered(&self, entity_id: &str) -> bool {
        self.inner.is_registered(entity_id)
    }

    /// Generate a unique entity_id that doesn't conflict with existing registrations
    ///
    /// Takes a domain and suggested object_id, and returns an entity_id that is
    /// guaranteed not to conflict with any existing registered entity or reserved IDs.
    /// If the preferred entity_id is taken, appends `_2`, `_3`, etc.
    #[pyo3(signature = (domain, suggested_object_id, *, current_entity_id=None, reserved_ids=None))]
    fn async_generate_entity_id(
        &self,
        domain: &str,
        suggested_object_id: &str,
        current_entity_id: Option<&str>,
        reserved_ids: Option<Vec<String>>,
    ) -> String {
        self.inner.generate_entity_id(
            domain,
            suggested_object_id,
            current_entity_id,
            reserved_ids.as_deref(),
        )
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

    /// Get deleted entities as a dict ((domain, platform, unique_id) -> EntityEntry)
    #[getter]
    fn deleted_entities(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        // Collect entries, preserves IndexMap insertion order
        let entries: Vec<_> = self
            .inner
            .deleted_iter()  // Now returns Vec, preserves IndexMap insertion order
            .into_iter()
            .map(|entry| {
                let key = (
                    entry.domain().to_string(),
                    entry.platform.clone(),
                    entry.unique_id.clone().unwrap_or_default(),
                );
                (key, entry)
            })
            .collect();
        // No sorting needed - IndexMap preserves insertion order

        let dict = PyDict::new_bound(py);
        for (key, entry) in entries {
            dict.set_item(key, PyEntityEntry::from_inner(entry).into_py(py))?;
        }
        Ok(dict.unbind())
    }

    /// Update entity options for a specific domain.
    ///
    /// If options is None, the domain's options are removed.
    /// This updates the `options` dict keyed by domain name.
    #[pyo3(signature = (entity_id, domain, options))]
    fn async_update_entity_options(
        &self,
        entity_id: &str,
        domain: &str,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyEntityEntry> {
        let entry = self
            .inner
            .update(entity_id, |entry| {
                // Get or create the options object
                let mut opts_map: serde_json::Map<String, serde_json::Value> =
                    if let Some(serde_json::Value::Object(ref existing)) = entry.options {
                        existing.clone()
                    } else {
                        serde_json::Map::new()
                    };

                if let Some(new_opts) = options {
                    // Convert Python dict to JSON and set for this domain
                    if let Ok(json_val) = py_to_json(new_opts) {
                        opts_map.insert(domain.to_string(), json_val);
                    }
                } else {
                    // Remove domain options
                    opts_map.remove(domain);
                }

                // Set the options back (or None if empty)
                if opts_map.is_empty() {
                    entry.options = None;
                } else {
                    entry.options = Some(serde_json::Value::Object(opts_map));
                }
            })
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!("{}", e)))?;

        Ok(PyEntityEntry::from_inner(entry))
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __repr__(&self) -> String {
        format!("EntityRegistry(count={})", self.inner.len())
    }
}
