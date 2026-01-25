//! Python wrappers for ConfigEntries

// Allow unexpected_cfgs from PyO3's create_exception macro (gil-refs feature check)
#![allow(unexpected_cfgs)]

use ha_config_entries::{
    ConfigEntries, ConfigEntry, ConfigEntryDisabledBy, ConfigEntrySource, ConfigEntryState,
    ConfigEntryUpdate,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tokio::runtime::Handle;

use super::py_storage::PyStorage;
use super::py_types::{json_to_py, py_to_json};

/// Python enum for ConfigEntryState
/// Matches Python HA's ConfigEntryState exactly
#[pyclass(name = "ConfigEntryState", eq)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PyConfigEntryState {
    #[pyo3(name = "NOT_LOADED")]
    NotLoaded,
    #[pyo3(name = "SETUP_IN_PROGRESS")]
    SetupInProgress,
    #[pyo3(name = "LOADED")]
    Loaded,
    #[pyo3(name = "SETUP_ERROR")]
    SetupError,
    #[pyo3(name = "SETUP_RETRY")]
    SetupRetry,
    #[pyo3(name = "MIGRATION_ERROR")]
    MigrationError,
    #[pyo3(name = "UNLOAD_IN_PROGRESS")]
    UnloadInProgress,
    #[pyo3(name = "FAILED_UNLOAD")]
    FailedUnload,
}

#[pymethods]
impl PyConfigEntryState {
    /// Check if the entry can be unloaded/reloaded from this state
    fn is_recoverable(&self) -> bool {
        matches!(
            self,
            PyConfigEntryState::Loaded
                | PyConfigEntryState::SetupError
                | PyConfigEntryState::SetupRetry
                | PyConfigEntryState::NotLoaded
        )
    }

    fn __repr__(&self) -> &'static str {
        match self {
            PyConfigEntryState::FailedUnload => "<ConfigEntryState.FAILED_UNLOAD>",
            PyConfigEntryState::Loaded => "<ConfigEntryState.LOADED>",
            PyConfigEntryState::MigrationError => "<ConfigEntryState.MIGRATION_ERROR>",
            PyConfigEntryState::NotLoaded => "<ConfigEntryState.NOT_LOADED>",
            PyConfigEntryState::SetupError => "<ConfigEntryState.SETUP_ERROR>",
            PyConfigEntryState::SetupInProgress => "<ConfigEntryState.SETUP_IN_PROGRESS>",
            PyConfigEntryState::SetupRetry => "<ConfigEntryState.SETUP_RETRY>",
            PyConfigEntryState::UnloadInProgress => "<ConfigEntryState.UNLOAD_IN_PROGRESS>",
        }
    }

    fn __str__(&self) -> &'static str {
        match self {
            PyConfigEntryState::FailedUnload => "failed_unload",
            PyConfigEntryState::Loaded => "loaded",
            PyConfigEntryState::MigrationError => "migration_error",
            PyConfigEntryState::NotLoaded => "not_loaded",
            PyConfigEntryState::SetupError => "setup_error",
            PyConfigEntryState::SetupInProgress => "setup_in_progress",
            PyConfigEntryState::SetupRetry => "setup_retry",
            PyConfigEntryState::UnloadInProgress => "unload_in_progress",
        }
    }
}

impl From<ConfigEntryState> for PyConfigEntryState {
    fn from(state: ConfigEntryState) -> Self {
        match state {
            ConfigEntryState::FailedUnload => PyConfigEntryState::FailedUnload,
            ConfigEntryState::Loaded => PyConfigEntryState::Loaded,
            ConfigEntryState::MigrationError => PyConfigEntryState::MigrationError,
            ConfigEntryState::NotLoaded => PyConfigEntryState::NotLoaded,
            ConfigEntryState::SetupError => PyConfigEntryState::SetupError,
            ConfigEntryState::SetupInProgress => PyConfigEntryState::SetupInProgress,
            ConfigEntryState::SetupRetry => PyConfigEntryState::SetupRetry,
            ConfigEntryState::UnloadInProgress => PyConfigEntryState::UnloadInProgress,
        }
    }
}

impl From<PyConfigEntryState> for ConfigEntryState {
    fn from(state: PyConfigEntryState) -> Self {
        match state {
            PyConfigEntryState::FailedUnload => ConfigEntryState::FailedUnload,
            PyConfigEntryState::Loaded => ConfigEntryState::Loaded,
            PyConfigEntryState::MigrationError => ConfigEntryState::MigrationError,
            PyConfigEntryState::NotLoaded => ConfigEntryState::NotLoaded,
            PyConfigEntryState::SetupError => ConfigEntryState::SetupError,
            PyConfigEntryState::SetupInProgress => ConfigEntryState::SetupInProgress,
            PyConfigEntryState::SetupRetry => ConfigEntryState::SetupRetry,
            PyConfigEntryState::UnloadInProgress => ConfigEntryState::UnloadInProgress,
        }
    }
}

// Exception raised when an invalid state transition is attempted
pyo3::create_exception!(
    ha_core_rs,
    InvalidStateTransition,
    pyo3::exceptions::PyException
);

fn source_to_str(source: &ConfigEntrySource) -> &'static str {
    match source {
        ConfigEntrySource::Bluetooth => "bluetooth",
        ConfigEntrySource::Dhcp => "dhcp",
        ConfigEntrySource::Discovery => "discovery",
        ConfigEntrySource::Hassio => "hassio",
        ConfigEntrySource::Homekit => "homekit",
        ConfigEntrySource::Ignore => "ignore",
        ConfigEntrySource::Import => "import",
        ConfigEntrySource::IntegrationDiscovery => "integration_discovery",
        ConfigEntrySource::Mqtt => "mqtt",
        ConfigEntrySource::Nupnp => "nupnp",
        ConfigEntrySource::Reauth => "reauth",
        ConfigEntrySource::Reconfigure => "reconfigure",
        ConfigEntrySource::Registration => "registration",
        ConfigEntrySource::Ssdp => "ssdp",
        ConfigEntrySource::System => "system",
        ConfigEntrySource::User => "user",
        ConfigEntrySource::Zeroconf => "zeroconf",
    }
}

fn parse_source(s: &str) -> ConfigEntrySource {
    match s {
        "bluetooth" => ConfigEntrySource::Bluetooth,
        "dhcp" => ConfigEntrySource::Dhcp,
        "discovery" => ConfigEntrySource::Discovery,
        "hassio" => ConfigEntrySource::Hassio,
        "homekit" => ConfigEntrySource::Homekit,
        "ignore" => ConfigEntrySource::Ignore,
        "import" => ConfigEntrySource::Import,
        "integration_discovery" => ConfigEntrySource::IntegrationDiscovery,
        "mqtt" => ConfigEntrySource::Mqtt,
        "nupnp" => ConfigEntrySource::Nupnp,
        "reauth" => ConfigEntrySource::Reauth,
        "reconfigure" => ConfigEntrySource::Reconfigure,
        "registration" => ConfigEntrySource::Registration,
        "ssdp" => ConfigEntrySource::Ssdp,
        "system" => ConfigEntrySource::System,
        "user" => ConfigEntrySource::User,
        "zeroconf" => ConfigEntrySource::Zeroconf,
        _ => ConfigEntrySource::User,
    }
}

/// Python wrapper for ConfigEntry
#[pyclass(name = "ConfigEntry", subclass)]
#[derive(Clone)]
pub struct PyConfigEntry {
    inner: ConfigEntry,
}

#[pymethods]
impl PyConfigEntry {
    #[getter]
    fn entry_id(&self) -> &str {
        &self.inner.entry_id
    }

    #[getter]
    fn domain(&self) -> &str {
        &self.inner.domain
    }

    #[getter]
    fn title(&self) -> &str {
        &self.inner.title
    }

    #[getter]
    fn data(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json_val = serde_json::to_value(&self.inner.data)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        json_to_py(py, &json_val)
    }

    #[getter]
    fn options(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json_val = serde_json::to_value(&self.inner.options)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        json_to_py(py, &json_val)
    }

    #[getter]
    fn version(&self) -> u32 {
        self.inner.version
    }

    #[getter]
    fn minor_version(&self) -> u32 {
        self.inner.minor_version
    }

    #[getter]
    fn unique_id(&self) -> Option<&str> {
        self.inner.unique_id.as_deref()
    }

    #[getter]
    fn source(&self) -> &str {
        source_to_str(&self.inner.source)
    }

    #[getter]
    fn state(&self) -> PyConfigEntryState {
        self.inner.state.into()
    }

    #[getter]
    fn reason(&self) -> Option<&str> {
        self.inner.reason.as_deref()
    }

    #[getter]
    fn pref_disable_new_entities(&self) -> bool {
        self.inner.pref_disable_new_entities
    }

    #[getter]
    fn pref_disable_polling(&self) -> bool {
        self.inner.pref_disable_polling
    }

    #[getter]
    fn disabled_by(&self) -> Option<&str> {
        self.inner.disabled_by.as_ref().map(|d| match d {
            ConfigEntryDisabledBy::User => "user",
        })
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

    fn is_loaded(&self) -> bool {
        self.inner.is_loaded()
    }

    fn supports_unload(&self) -> bool {
        self.inner.supports_unload()
    }

    fn __repr__(&self) -> String {
        format!(
            "ConfigEntry(entry_id='{}', domain='{}', title='{}')",
            self.inner.entry_id, self.inner.domain, self.inner.title
        )
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner.entry_id == other.inner.entry_id
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.entry_id.hash(&mut hasher);
        hasher.finish()
    }
}

impl PyConfigEntry {
    pub fn from_inner(inner: ConfigEntry) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &ConfigEntry {
        &self.inner
    }
}

/// Python wrapper for ConfigEntries manager
#[pyclass(name = "ConfigEntries")]
pub struct PyConfigEntries {
    inner: Arc<ConfigEntries>,
}

#[pymethods]
impl PyConfigEntries {
    #[new]
    fn new(storage: &PyStorage) -> Self {
        Self {
            inner: Arc::new(ConfigEntries::new(storage.inner().clone())),
        }
    }

    /// Load entries from storage
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

    /// Save entries to storage
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

    /// Get an entry by ID
    fn async_get_entry(&self, entry_id: &str) -> Option<PyConfigEntry> {
        self.inner.get(entry_id).map(PyConfigEntry::from_inner)
    }

    /// Get all entries for a domain
    #[pyo3(signature = (domain=None))]
    fn async_entries(&self, domain: Option<&str>) -> Vec<PyConfigEntry> {
        match domain {
            Some(d) => self
                .inner
                .get_by_domain(d)
                .into_iter()
                .map(PyConfigEntry::from_inner)
                .collect(),
            None => self.inner.iter().map(PyConfigEntry::from_inner).collect(),
        }
    }

    /// Get loaded entries for a domain
    fn async_loaded_entries(&self, domain: &str) -> Vec<PyConfigEntry> {
        self.inner
            .get_loaded_by_domain(domain)
            .into_iter()
            .map(PyConfigEntry::from_inner)
            .collect()
    }

    /// Get entry by unique_id
    fn async_get_entry_by_unique_id(&self, domain: &str, unique_id: &str) -> Option<PyConfigEntry> {
        self.inner
            .get_by_unique_id(domain, unique_id)
            .map(PyConfigEntry::from_inner)
    }

    /// Add a new config entry
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (domain, title, *, data=None, options=None, unique_id=None, source=None, version=None, minor_version=None))]
    fn async_add(
        &self,
        domain: &str,
        title: &str,
        data: Option<&Bound<'_, PyDict>>,
        options: Option<&Bound<'_, PyDict>>,
        unique_id: Option<&str>,
        source: Option<&str>,
        version: Option<u32>,
        minor_version: Option<u32>,
    ) -> PyResult<PyConfigEntry> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let mut entry = ConfigEntry::new(domain, title);

        if let Some(d) = data {
            let json_data = py_to_json(d.as_any())?;
            if let Some(obj) = json_data.as_object() {
                entry.data = obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            }
        }

        if let Some(o) = options {
            let json_opts = py_to_json(o.as_any())?;
            if let Some(obj) = json_opts.as_object() {
                entry.options = obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            }
        }

        if let Some(uid) = unique_id {
            entry.unique_id = Some(uid.to_string());
        }

        if let Some(src) = source {
            entry.source = parse_source(src);
        }

        if let Some(v) = version {
            entry.version = v;
        }

        if let Some(mv) = minor_version {
            entry.minor_version = mv;
        }

        let inner = self.inner.clone();
        tokio::task::block_in_place(|| handle.block_on(async { inner.add(entry).await }))
            .map(PyConfigEntry::from_inner)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Update an existing entry
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (entry_id, *, title=None, data=None, options=None, unique_id=None, version=None, minor_version=None))]
    fn async_update_entry(
        &self,
        entry_id: &str,
        title: Option<String>,
        data: Option<&Bound<'_, PyDict>>,
        options: Option<&Bound<'_, PyDict>>,
        unique_id: Option<String>,
        version: Option<u32>,
        minor_version: Option<u32>,
    ) -> PyResult<PyConfigEntry> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let mut update = ConfigEntryUpdate::default();

        if let Some(t) = title {
            update.title = Some(t);
        }

        if let Some(d) = data {
            let json_data = py_to_json(d.as_any())?;
            if let Some(obj) = json_data.as_object() {
                update.data = Some(obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
            }
        }

        if let Some(o) = options {
            let json_opts = py_to_json(o.as_any())?;
            if let Some(obj) = json_opts.as_object() {
                update.options = Some(obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
            }
        }

        if unique_id.is_some() {
            update.unique_id = Some(unique_id);
        }

        if let Some(v) = version {
            update.version = Some(v);
        }

        if let Some(mv) = minor_version {
            update.minor_version = Some(mv);
        }

        let inner = self.inner.clone();
        let entry_id = entry_id.to_string();
        tokio::task::block_in_place(|| {
            handle.block_on(async { inner.update(&entry_id, update).await })
        })
        .map(PyConfigEntry::from_inner)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Remove an entry
    fn async_remove(&self, entry_id: &str) -> PyResult<PyConfigEntry> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let inner = self.inner.clone();
        let entry_id = entry_id.to_string();
        tokio::task::block_in_place(|| handle.block_on(async { inner.remove(&entry_id).await }))
            .map(PyConfigEntry::from_inner)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Setup an entry
    fn async_setup(&self, entry_id: &str) -> PyResult<()> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let inner = self.inner.clone();
        let entry_id = entry_id.to_string();
        tokio::task::block_in_place(|| handle.block_on(async { inner.setup(&entry_id).await }))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Unload an entry
    fn async_unload(&self, entry_id: &str) -> PyResult<()> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let inner = self.inner.clone();
        let entry_id = entry_id.to_string();
        tokio::task::block_in_place(|| handle.block_on(async { inner.unload(&entry_id).await }))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Reload an entry
    fn async_reload(&self, entry_id: &str) -> PyResult<()> {
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let inner = self.inner.clone();
        let entry_id = entry_id.to_string();
        tokio::task::block_in_place(|| handle.block_on(async { inner.reload(&entry_id).await }))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Get all entry IDs
    fn entry_ids(&self) -> Vec<String> {
        self.inner.entry_ids()
    }

    /// Get all domains
    fn domains(&self) -> Vec<String> {
        self.inner.domains()
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __repr__(&self) -> String {
        format!("ConfigEntries(count={})", self.inner.len())
    }
}

impl PyConfigEntries {
    pub fn from_arc(inner: Arc<ConfigEntries>) -> Self {
        Self { inner }
    }
}
