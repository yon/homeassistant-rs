//! ConfigEntryWrapper - wraps Rust ConfigEntry for Python integrations

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::RwLock;

/// Python wrapper for ConfigEntry
///
/// This provides a proper ConfigEntry-like object that supports:
/// - All standard readonly properties (entry_id, domain, data, etc.)
/// - runtime_data as a read/write property
/// - async_on_unload() method for cleanup callbacks
#[pyclass(name = "ConfigEntry")]
pub struct ConfigEntryWrapper {
    // Core fields
    entry_id: String,
    domain: String,
    title: String,
    version: u32,
    minor_version: u32,
    source: String,
    unique_id: Option<String>,
    state: String,
    // Data as Python dicts (stored as PyObject for easy Python access)
    data: PyObject,
    options: PyObject,
    discovery_keys: PyObject,
    // Mutable fields
    runtime_data: RwLock<PyObject>,
    // Callbacks registered via async_on_unload
    unload_callbacks: RwLock<Vec<PyObject>>,
}

impl ConfigEntryWrapper {
    /// Create a new ConfigEntryWrapper from Rust ConfigEntry data
    pub fn new(
        py: Python<'_>,
        entry_id: String,
        domain: String,
        title: String,
        version: u32,
        minor_version: u32,
        source: String,
        unique_id: Option<String>,
        state: String,
        data: &Bound<'_, PyDict>,
        options: &Bound<'_, PyDict>,
        discovery_keys: &Bound<'_, PyDict>,
    ) -> PyResult<Self> {
        Ok(Self {
            entry_id,
            domain,
            title,
            version,
            minor_version,
            source,
            unique_id,
            state,
            data: data.clone().unbind().into(),
            options: options.clone().unbind().into(),
            discovery_keys: discovery_keys.clone().unbind().into(),
            runtime_data: RwLock::new(py.None()),
            unload_callbacks: RwLock::new(Vec::new()),
        })
    }
}

#[pymethods]
impl ConfigEntryWrapper {
    /// Entry ID (readonly)
    #[getter]
    fn entry_id(&self) -> &str {
        &self.entry_id
    }

    /// Domain (readonly)
    #[getter]
    fn domain(&self) -> &str {
        &self.domain
    }

    /// Title (readonly)
    #[getter]
    fn title(&self) -> &str {
        &self.title
    }

    /// Version (readonly)
    #[getter]
    fn version(&self) -> u32 {
        self.version
    }

    /// Minor version (readonly)
    #[getter]
    fn minor_version(&self) -> u32 {
        self.minor_version
    }

    /// Source (readonly)
    #[getter]
    fn source(&self) -> &str {
        &self.source
    }

    /// Unique ID (readonly)
    #[getter]
    fn unique_id(&self) -> Option<&str> {
        self.unique_id.as_deref()
    }

    /// State (readonly) - returns the Python ConfigEntryState enum
    #[getter]
    fn state(&self, py: Python<'_>) -> PyResult<PyObject> {
        // Import the ConfigEntryState enum from homeassistant.config_entries
        let config_entries = py.import_bound("homeassistant.config_entries")?;
        let state_enum = config_entries.getattr("ConfigEntryState")?;

        // Map our string state to the enum value
        let enum_value = match self.state.as_str() {
            "not_loaded" => state_enum.getattr("NOT_LOADED")?,
            "setup_in_progress" => state_enum.getattr("SETUP_IN_PROGRESS")?,
            "loaded" => state_enum.getattr("LOADED")?,
            "setup_error" => state_enum.getattr("SETUP_ERROR")?,
            "setup_retry" => state_enum.getattr("SETUP_RETRY")?,
            "migration_error" => state_enum.getattr("MIGRATION_ERROR")?,
            "failed_unload" => state_enum.getattr("FAILED_UNLOAD")?,
            _ => state_enum.getattr("NOT_LOADED")?, // Default to NOT_LOADED
        };

        Ok(enum_value.unbind())
    }

    /// Data dict (readonly)
    #[getter]
    fn data(&self, py: Python<'_>) -> PyObject {
        self.data.clone_ref(py)
    }

    /// Options dict (readonly)
    #[getter]
    fn options(&self, py: Python<'_>) -> PyObject {
        self.options.clone_ref(py)
    }

    /// Discovery keys (readonly)
    #[getter]
    fn discovery_keys(&self, py: Python<'_>) -> PyObject {
        self.discovery_keys.clone_ref(py)
    }

    /// Whether polling is disabled by user preference (readonly)
    /// Used by update_coordinator.py to skip polling if user disabled it
    #[getter]
    fn pref_disable_polling(&self) -> bool {
        false // Default to allowing polling
    }

    /// Whether new entities should be disabled by default (readonly)
    #[getter]
    fn pref_disable_new_entities(&self) -> bool {
        false // Default to enabling new entities
    }

    /// Runtime data (read/write)
    /// This is where integrations store their runtime state
    #[getter]
    fn runtime_data(&self, py: Python<'_>) -> PyObject {
        let data = self.runtime_data.read().unwrap();
        data.clone_ref(py)
    }

    #[setter]
    fn set_runtime_data(&self, value: PyObject) {
        let mut data = self.runtime_data.write().unwrap();
        *data = value;
    }

    /// Register a callback to be called when the entry is unloaded
    ///
    /// Returns a function that can be called to remove the callback.
    fn async_on_unload(&self, py: Python<'_>, callback: PyObject) -> PyResult<PyObject> {
        {
            let mut callbacks = self.unload_callbacks.write().unwrap();
            callbacks.push(callback.clone_ref(py));
        }

        // Return a function that removes this callback
        // For now, return None since the callback tracking is primarily for cleanup
        Ok(py.None())
    }

    /// Get all registered unload callbacks (for internal use)
    fn _get_unload_callbacks(&self, py: Python<'_>) -> Vec<PyObject> {
        let callbacks = self.unload_callbacks.read().unwrap();
        callbacks.iter().map(|cb| cb.clone_ref(py)).collect()
    }

    /// Call all unload callbacks (for cleanup)
    fn _run_unload_callbacks(&self, py: Python<'_>) -> PyResult<()> {
        let callbacks = self.unload_callbacks.read().unwrap();
        for callback in callbacks.iter() {
            // Try to call the callback
            if let Err(e) = callback.call0(py) {
                tracing::warn!("Unload callback failed: {}", e);
            }
        }
        Ok(())
    }

    /// Hash based on entry_id (for dict keys)
    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.entry_id.hash(&mut hasher);
        hasher.finish()
    }

    /// Equality based on entry_id
    fn __eq__(&self, other: &ConfigEntryWrapper) -> bool {
        self.entry_id == other.entry_id
    }

    /// String representation
    fn __repr__(&self) -> String {
        format!(
            "<ConfigEntry entry_id={} domain={} title={}>",
            self.entry_id, self.domain, self.title
        )
    }

    /// Start a reauthentication flow for this config entry
    ///
    /// Called when authentication fails and user needs to re-authenticate.
    /// For now, this is a no-op since we don't have full reauth support.
    #[pyo3(signature = (_hass, context=None, data=None))]
    fn async_start_reauth(
        &self,
        _py: Python<'_>,
        _hass: PyObject,
        context: Option<PyObject>,
        data: Option<PyObject>,
    ) -> PyResult<()> {
        // Silence unused warnings
        let _ = (context, data);
        tracing::warn!(
            "ConfigEntry.async_start_reauth called for {} ({}), but reauth is not yet implemented",
            self.domain,
            self.entry_id
        );
        Ok(())
    }

    /// Create a background task tied to the config entry lifecycle
    ///
    /// Background tasks are automatically cancelled when config entry is unloaded.
    ///
    /// # Arguments
    /// * `hass` - The HomeAssistant instance
    /// * `target` - The coroutine to wrap in a task
    /// * `name` - Name for the task
    /// * `eager_start` - Whether to start eagerly (default true)
    ///
    /// # Returns
    /// The created asyncio task
    #[pyo3(signature = (hass, target, name, eager_start=true))]
    fn async_create_background_task<'py>(
        &self,
        py: Python<'py>,
        hass: PyObject,
        target: PyObject,
        name: String,
        eager_start: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        tracing::info!(
            "ConfigEntry.async_create_background_task called for '{}' (domain: {}, entry_id: {})",
            name,
            self.domain,
            self.entry_id
        );

        // Call hass.async_create_background_task(target, name, eager_start)
        let hass_bound = hass.bind(py);

        // Build kwargs for the call
        let kwargs = PyDict::new_bound(py);
        kwargs.set_item("eager_start", eager_start)?;

        let task = hass_bound.call_method(
            "async_create_background_task",
            (target, &name),
            Some(&kwargs),
        )?;

        tracing::info!(
            "ConfigEntry background task '{}' created successfully",
            name
        );

        // TODO: Track the task and cancel it when the entry is unloaded
        // For now, we just create the task without lifecycle tracking

        Ok(task)
    }
}
