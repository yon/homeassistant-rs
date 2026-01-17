//! Config Entries Manager
//!
//! Manages the lifecycle of configuration entries.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;
use ha_registries::{Storable, Storage, StorageFile, StorageResult};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::entry::{ConfigEntry, ConfigEntryState, ConfigEntryUpdate};

/// Storage key for config entries
pub const STORAGE_KEY: &str = "core.config_entries";
/// Current storage version
pub const STORAGE_VERSION: u32 = 1;
/// Current minor version
pub const STORAGE_MINOR_VERSION: u32 = 5;

/// Config entries errors
#[derive(Debug, Error)]
pub enum ConfigEntriesError {
    #[error("Entry not found: {0}")]
    NotFound(String),

    #[error("Entry already exists for domain {domain} with unique_id {unique_id}")]
    AlreadyExists { domain: String, unique_id: String },

    #[error("Cannot unload entry in state {0:?}")]
    CannotUnload(ConfigEntryState),

    #[error("Setup failed: {0}")]
    SetupFailed(String),

    #[error("Storage error: {0}")]
    Storage(#[from] ha_registries::StorageError),
}

pub type ConfigEntriesResult<T> = Result<T, ConfigEntriesError>;

/// Config entries data for storage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigEntriesData {
    /// All config entries
    pub entries: Vec<ConfigEntry>,
}

impl Storable for ConfigEntriesData {
    const KEY: &'static str = STORAGE_KEY;
    const VERSION: u32 = STORAGE_VERSION;
    const MINOR_VERSION: u32 = STORAGE_MINOR_VERSION;
}

/// Setup handler function type
pub type SetupHandler = Arc<dyn Fn(&ConfigEntry) -> Result<(), String> + Send + Sync + 'static>;

/// Config Entries Manager
///
/// Manages the lifecycle of configuration entries including:
/// - Loading/saving from storage
/// - Entry creation and removal
/// - State management
/// - Integration with registries
pub struct ConfigEntries {
    /// Storage backend
    storage: Arc<Storage>,

    /// Primary index: entry_id -> ConfigEntry
    entries: DashMap<String, ConfigEntry>,

    /// Index: domain -> set of entry_ids
    by_domain: DashMap<String, HashSet<String>>,

    /// Index: (domain, unique_id) -> entry_id
    by_unique_id: DashMap<(String, String), String>,

    /// Setup lock to prevent concurrent setup/unload
    setup_lock: Mutex<()>,

    /// Setup handlers by domain
    setup_handlers: DashMap<String, SetupHandler>,
}

impl ConfigEntries {
    /// Create a new config entries manager
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            entries: DashMap::new(),
            by_domain: DashMap::new(),
            by_unique_id: DashMap::new(),
            setup_lock: Mutex::new(()),
            setup_handlers: DashMap::new(),
        }
    }

    /// Load entries from storage
    pub async fn load(&self) -> StorageResult<()> {
        if let Some(storage_file) = self.storage.load::<ConfigEntriesData>(STORAGE_KEY).await? {
            info!(
                "Loading {} config entries from storage (v{}.{})",
                storage_file.data.entries.len(),
                storage_file.version,
                storage_file.minor_version
            );

            for entry in storage_file.data.entries {
                self.index_entry(&entry);
            }
        }
        Ok(())
    }

    /// Save entries to storage
    pub async fn save(&self) -> StorageResult<()> {
        let data = ConfigEntriesData {
            entries: self.entries.iter().map(|r| r.value().clone()).collect(),
        };

        let storage_file =
            StorageFile::new(STORAGE_KEY, data, STORAGE_VERSION, STORAGE_MINOR_VERSION);

        self.storage.save(&storage_file).await?;
        debug!("Saved {} config entries to storage", self.entries.len());
        Ok(())
    }

    /// Index an entry
    fn index_entry(&self, entry: &ConfigEntry) {
        let entry_id = entry.entry_id.clone();

        // Primary index
        self.entries.insert(entry_id.clone(), entry.clone());

        // Domain index
        self.by_domain
            .entry(entry.domain.clone())
            .or_default()
            .insert(entry_id.clone());

        // Unique ID index
        if let Some(ref unique_id) = entry.unique_id {
            self.by_unique_id
                .insert((entry.domain.clone(), unique_id.clone()), entry_id);
        }
    }

    /// Remove an entry from indexes
    fn unindex_entry(&self, entry: &ConfigEntry) {
        // Remove from domain index
        if let Some(mut ids) = self.by_domain.get_mut(&entry.domain) {
            ids.remove(&entry.entry_id);
        }

        // Remove from unique_id index
        if let Some(ref unique_id) = entry.unique_id {
            self.by_unique_id
                .remove(&(entry.domain.clone(), unique_id.clone()));
        }

        // Remove from primary index
        self.entries.remove(&entry.entry_id);
    }

    /// Get an entry by ID
    pub fn get(&self, entry_id: &str) -> Option<ConfigEntry> {
        self.entries.get(entry_id).map(|r| r.value().clone())
    }

    /// Get all entries for a domain
    pub fn get_by_domain(&self, domain: &str) -> Vec<ConfigEntry> {
        self.by_domain
            .get(domain)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get loaded entries for a domain
    pub fn get_loaded_by_domain(&self, domain: &str) -> Vec<ConfigEntry> {
        self.get_by_domain(domain)
            .into_iter()
            .filter(|e| e.is_loaded())
            .collect()
    }

    /// Get entry by unique_id
    pub fn get_by_unique_id(&self, domain: &str, unique_id: &str) -> Option<ConfigEntry> {
        self.by_unique_id
            .get(&(domain.to_string(), unique_id.to_string()))
            .and_then(|entry_id| self.get(&entry_id))
    }

    /// Add a new config entry
    pub async fn add(&self, entry: ConfigEntry) -> ConfigEntriesResult<ConfigEntry> {
        // Check for duplicate unique_id
        if let Some(ref unique_id) = entry.unique_id {
            if self.get_by_unique_id(&entry.domain, unique_id).is_some() {
                return Err(ConfigEntriesError::AlreadyExists {
                    domain: entry.domain.clone(),
                    unique_id: unique_id.clone(),
                });
            }
        }

        self.index_entry(&entry);
        self.save().await?;

        info!(
            "Added config entry: {} ({}) [{}]",
            entry.title, entry.domain, entry.entry_id
        );

        Ok(entry)
    }

    /// Update an existing entry
    pub async fn update(
        &self,
        entry_id: &str,
        update: ConfigEntryUpdate,
    ) -> ConfigEntriesResult<ConfigEntry> {
        let entry = self
            .get(entry_id)
            .ok_or_else(|| ConfigEntriesError::NotFound(entry_id.to_string()))?;

        // Remove from indexes
        self.unindex_entry(&entry);

        // Apply updates
        let mut updated = entry;
        if let Some(title) = update.title {
            updated.title = title;
        }
        if let Some(data) = update.data {
            updated.data = data;
        }
        if let Some(options) = update.options {
            updated.options = options;
        }
        if let Some(unique_id) = update.unique_id {
            updated.unique_id = unique_id;
        }
        if let Some(version) = update.version {
            updated.version = version;
        }
        if let Some(minor_version) = update.minor_version {
            updated.minor_version = minor_version;
        }
        if let Some(pref) = update.pref_disable_new_entities {
            updated.pref_disable_new_entities = pref;
        }
        if let Some(pref) = update.pref_disable_polling {
            updated.pref_disable_polling = pref;
        }
        updated.modified_at = Utc::now();

        // Re-index
        self.index_entry(&updated);
        self.save().await?;

        debug!("Updated config entry: {}", entry_id);
        Ok(updated)
    }

    /// Remove an entry
    pub async fn remove(&self, entry_id: &str) -> ConfigEntriesResult<ConfigEntry> {
        let entry = self
            .get(entry_id)
            .ok_or_else(|| ConfigEntriesError::NotFound(entry_id.to_string()))?;

        self.unindex_entry(&entry);
        self.save().await?;

        info!(
            "Removed config entry: {} ({}) [{}]",
            entry.title, entry.domain, entry_id
        );

        Ok(entry)
    }

    /// Set entry state
    pub fn set_state(&self, entry_id: &str, state: ConfigEntryState, reason: Option<String>) {
        if let Some(mut entry) = self.entries.get_mut(entry_id) {
            entry.state = state;
            entry.reason = reason;
            debug!("Entry {} state changed to {:?}", entry_id, state);
        }
    }

    /// Register a setup handler for a domain
    pub fn register_setup_handler(&self, domain: &str, handler: SetupHandler) {
        self.setup_handlers.insert(domain.to_string(), handler);
        debug!("Registered setup handler for domain: {}", domain);
    }

    /// Setup an entry (call integration's setup)
    pub async fn setup(&self, entry_id: &str) -> ConfigEntriesResult<()> {
        let _lock = self.setup_lock.lock().await;

        let entry = self
            .get(entry_id)
            .ok_or_else(|| ConfigEntriesError::NotFound(entry_id.to_string()))?;

        if entry.is_disabled() {
            debug!("Skipping setup for disabled entry: {}", entry_id);
            return Ok(());
        }

        self.set_state(entry_id, ConfigEntryState::SetupInProgress, None);

        // Call setup handler if registered
        if let Some(handler) = self.setup_handlers.get(&entry.domain) {
            match handler(&entry) {
                Ok(()) => {
                    self.set_state(entry_id, ConfigEntryState::Loaded, None);
                    info!("Setup completed for entry: {} ({})", entry.title, entry_id);
                }
                Err(reason) => {
                    warn!("Setup failed for entry {}: {}", entry_id, reason);
                    self.set_state(entry_id, ConfigEntryState::SetupError, Some(reason.clone()));
                    return Err(ConfigEntriesError::SetupFailed(reason));
                }
            }
        } else {
            // No handler, mark as loaded
            self.set_state(entry_id, ConfigEntryState::Loaded, None);
            debug!(
                "No setup handler for domain {}, marking as loaded",
                entry.domain
            );
        }

        Ok(())
    }

    /// Unload an entry
    pub async fn unload(&self, entry_id: &str) -> ConfigEntriesResult<()> {
        let _lock = self.setup_lock.lock().await;

        let entry = self
            .get(entry_id)
            .ok_or_else(|| ConfigEntriesError::NotFound(entry_id.to_string()))?;

        if !entry.state.is_recoverable() {
            return Err(ConfigEntriesError::CannotUnload(entry.state));
        }

        self.set_state(entry_id, ConfigEntryState::UnloadInProgress, None);

        // TODO: Call unload handler
        // For now, just mark as not loaded
        self.set_state(entry_id, ConfigEntryState::NotLoaded, None);

        info!("Unloaded entry: {} ({})", entry.title, entry_id);
        Ok(())
    }

    /// Reload an entry (unload + setup)
    pub async fn reload(&self, entry_id: &str) -> ConfigEntriesResult<()> {
        self.unload(entry_id).await?;
        self.setup(entry_id).await
    }

    /// Get all entry IDs
    pub fn entry_ids(&self) -> Vec<String> {
        self.entries.iter().map(|r| r.key().clone()).collect()
    }

    /// Get all domains with entries
    pub fn domains(&self) -> Vec<String> {
        self.by_domain.iter().map(|r| r.key().clone()).collect()
    }

    /// Get count of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all entries
    pub fn iter(&self) -> impl Iterator<Item = ConfigEntry> + '_ {
        self.entries.iter().map(|r| r.value().clone())
    }

    /// Setup all entries
    pub async fn setup_all(&self) -> Vec<ConfigEntriesResult<()>> {
        let entry_ids: Vec<_> = self.entry_ids();
        let mut results = Vec::new();

        for entry_id in entry_ids {
            results.push(self.setup(&entry_id).await);
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::ConfigEntrySource;

    use tempfile::TempDir;

    fn create_test_manager() -> (TempDir, ConfigEntries) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path()));
        let manager = ConfigEntries::new(storage);
        (temp_dir, manager)
    }

    #[tokio::test]
    async fn test_add_entry() {
        let (_dir, manager) = create_test_manager();

        let entry = ConfigEntry::new("hue", "Philips Hue")
            .with_unique_id("bridge-001")
            .with_source(ConfigEntrySource::Discovery);

        let added = manager.add(entry).await.unwrap();
        assert_eq!(added.domain, "hue");
        assert_eq!(manager.len(), 1);
    }

    #[tokio::test]
    async fn test_duplicate_unique_id_rejected() {
        let (_dir, manager) = create_test_manager();

        let entry1 = ConfigEntry::new("hue", "Bridge 1").with_unique_id("same-id");
        let entry2 = ConfigEntry::new("hue", "Bridge 2").with_unique_id("same-id");

        manager.add(entry1).await.unwrap();
        let result = manager.add(entry2).await;

        assert!(matches!(
            result,
            Err(ConfigEntriesError::AlreadyExists { .. })
        ));
    }

    #[tokio::test]
    async fn test_get_by_domain() {
        let (_dir, manager) = create_test_manager();

        manager.add(ConfigEntry::new("hue", "Hue 1")).await.unwrap();
        manager.add(ConfigEntry::new("hue", "Hue 2")).await.unwrap();
        manager.add(ConfigEntry::new("mqtt", "MQTT")).await.unwrap();

        let hue_entries = manager.get_by_domain("hue");
        assert_eq!(hue_entries.len(), 2);

        let mqtt_entries = manager.get_by_domain("mqtt");
        assert_eq!(mqtt_entries.len(), 1);
    }

    #[tokio::test]
    async fn test_update_entry() {
        let (_dir, manager) = create_test_manager();

        let entry = manager
            .add(ConfigEntry::new("hue", "Old Name"))
            .await
            .unwrap();

        let updated = manager
            .update(&entry.entry_id, ConfigEntryUpdate::new().title("New Name"))
            .await
            .unwrap();

        assert_eq!(updated.title, "New Name");
    }

    #[tokio::test]
    async fn test_remove_entry() {
        let (_dir, manager) = create_test_manager();

        let entry = manager.add(ConfigEntry::new("hue", "Test")).await.unwrap();
        assert_eq!(manager.len(), 1);

        manager.remove(&entry.entry_id).await.unwrap();
        assert_eq!(manager.len(), 0);
    }

    #[tokio::test]
    async fn test_setup_and_unload() {
        let (_dir, manager) = create_test_manager();

        let entry = manager.add(ConfigEntry::new("hue", "Test")).await.unwrap();
        assert_eq!(
            manager.get(&entry.entry_id).unwrap().state,
            ConfigEntryState::NotLoaded
        );

        manager.setup(&entry.entry_id).await.unwrap();
        assert_eq!(
            manager.get(&entry.entry_id).unwrap().state,
            ConfigEntryState::Loaded
        );

        manager.unload(&entry.entry_id).await.unwrap();
        assert_eq!(
            manager.get(&entry.entry_id).unwrap().state,
            ConfigEntryState::NotLoaded
        );
    }

    #[tokio::test]
    async fn test_setup_handler() {
        let (_dir, manager) = create_test_manager();

        // Register a handler that always succeeds
        manager.register_setup_handler("hue", Arc::new(|_entry| Ok(())));

        let entry = manager.add(ConfigEntry::new("hue", "Test")).await.unwrap();
        manager.setup(&entry.entry_id).await.unwrap();

        assert!(manager.get(&entry.entry_id).unwrap().is_loaded());
    }

    #[tokio::test]
    async fn test_setup_handler_failure() {
        let (_dir, manager) = create_test_manager();

        // Register a handler that always fails
        manager.register_setup_handler(
            "hue",
            Arc::new(|_entry| Err("Connection failed".to_string())),
        );

        let entry = manager.add(ConfigEntry::new("hue", "Test")).await.unwrap();
        let result = manager.setup(&entry.entry_id).await;

        assert!(matches!(result, Err(ConfigEntriesError::SetupFailed(_))));
        assert_eq!(
            manager.get(&entry.entry_id).unwrap().state,
            ConfigEntryState::SetupError
        );
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path()));

        // Create and populate
        {
            let manager = ConfigEntries::new(storage.clone());
            manager
                .add(
                    ConfigEntry::new("hue", "Test")
                        .with_unique_id("test-123")
                        .with_source(ConfigEntrySource::Import),
                )
                .await
                .unwrap();
        }

        // Load into new manager
        {
            let manager = ConfigEntries::new(storage);
            manager.load().await.unwrap();

            assert_eq!(manager.len(), 1);
            let entry = manager.get_by_unique_id("hue", "test-123").unwrap();
            assert_eq!(entry.title, "Test");
            assert_eq!(entry.source, ConfigEntrySource::Import);
        }
    }
}
