//! Entity Registry
//!
//! Tracks all registered entities with unique_id tracking, device linking,
//! and multiple indexes for fast lookups.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};

use crate::storage::{Storable, Storage, StorageFile, StorageResult};

/// Errors that can occur in the entity registry
#[derive(Debug, Error, Clone)]
pub enum EntityRegistryError {
    /// Entity was not found
    #[error("Entity not found: {0}")]
    NotFound(String),
}

/// Storage key for entity registry
pub const STORAGE_KEY: &str = "core.entity_registry";
/// Current storage version
pub const STORAGE_VERSION: u32 = 1;
/// Current minor version
pub const STORAGE_MINOR_VERSION: u32 = 19;

/// Reason an entity was disabled
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisabledBy {
    /// Disabled by a config entry
    ConfigEntry,
    /// Disabled by device
    Device,
    /// Disabled by Home Assistant itself
    Hass,
    /// Disabled by the integration
    Integration,
    /// Disabled by the user
    User,
}

/// Reason an entity was hidden
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HiddenBy {
    /// Hidden by the integration
    Integration,
    /// Hidden by the user
    User,
}

/// Entity category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityCategory {
    /// Configuration entity
    Config,
    /// Diagnostic entity
    Diagnostic,
}

/// A registered entity entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityEntry {
    /// Internal UUID
    pub id: String,
    /// Full entity ID (domain.object_id)
    pub entity_id: String,
    /// Platform-specific unique identifier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
    /// Previous unique_id (for tracking renames)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_unique_id: Option<String>,

    /// Parent device ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// Config entry that created this entity
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_entry_id: Option<String>,
    /// Config subentry ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_subentry_id: Option<String>,

    /// User-set name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Platform default name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    /// Suggested object_id for naming
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_object_id: Option<String>,
    /// If true, name is auto-derived from device
    /// Can be None when not explicitly set
    #[serde(default)]
    pub has_entity_name: Option<bool>,

    /// Component/platform that provides this entity
    pub platform: String,

    /// Entity category (config, diagnostic, or none)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_category: Option<EntityCategory>,
    /// Device class (e.g., "temperature", "humidity")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_class: Option<String>,
    /// Platform default device class
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_device_class: Option<String>,

    /// Disable reason
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_by: Option<DisabledBy>,
    /// Hidden reason
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden_by: Option<HiddenBy>,

    /// Custom icon
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Platform default icon
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_icon: Option<String>,
    /// Unit of measurement
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_of_measurement: Option<String>,
    /// Translation key for i18n
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation_key: Option<String>,

    /// Bitmask of supported features
    #[serde(default)]
    pub supported_features: u32,
    /// Feature capabilities
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<serde_json::Value>,
    /// Per-platform options
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,

    /// Assigned area
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_id: Option<String>,
    /// Label IDs
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub labels: HashSet<String>,
    /// Alternative names/IDs
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub aliases: HashSet<String>,
    /// Category assignments by scope (e.g., "helpers" -> category_id)
    /// Stored as serde_json::Value to support both dict and set from Python
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<serde_json::Value>,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    /// Last modified timestamp
    #[serde(default = "Utc::now")]
    pub modified_at: DateTime<Utc>,
    /// Timestamp when deleted entity became orphaned (no config entry)
    /// Only used for deleted entities
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orphaned_timestamp: Option<f64>,
}

impl EntityEntry {
    /// Create a new entity entry with minimal required fields
    pub fn new(
        entity_id: impl Into<String>,
        platform: impl Into<String>,
        unique_id: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            entity_id: entity_id.into(),
            unique_id,
            previous_unique_id: None,
            device_id: None,
            config_entry_id: None,
            config_subentry_id: None,
            name: None,
            original_name: None,
            suggested_object_id: None,
            has_entity_name: None,
            platform: platform.into(),
            entity_category: None,
            device_class: None,
            original_device_class: None,
            disabled_by: None,
            hidden_by: None,
            icon: None,
            original_icon: None,
            unit_of_measurement: None,
            translation_key: None,
            supported_features: 0,
            capabilities: None,
            options: None,
            area_id: None,
            labels: HashSet::new(),
            aliases: HashSet::new(),
            categories: None,
            created_at: now,
            modified_at: now,
            orphaned_timestamp: None,
        }
    }

    /// Get the domain from entity_id
    pub fn domain(&self) -> &str {
        self.entity_id.split('.').next().unwrap_or(&self.entity_id)
    }

    /// Get the object_id from entity_id
    pub fn object_id(&self) -> &str {
        self.entity_id.split('.').nth(1).unwrap_or(&self.entity_id)
    }

    /// Check if entity is disabled
    pub fn is_disabled(&self) -> bool {
        self.disabled_by.is_some()
    }

    /// Check if entity is hidden
    pub fn is_hidden(&self) -> bool {
        self.hidden_by.is_some()
    }
}

/// Entity registry data for storage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityRegistryData {
    /// All registered entities
    pub entities: Vec<EntityEntry>,
    /// Soft-deleted entities (for tracking)
    #[serde(default)]
    pub deleted_entities: Vec<EntityEntry>,
}

impl Storable for EntityRegistryData {
    const KEY: &'static str = STORAGE_KEY;
    const VERSION: u32 = STORAGE_VERSION;
    const MINOR_VERSION: u32 = STORAGE_MINOR_VERSION;
}

/// Entity Registry with multi-index support
///
/// Provides O(1) lookups by:
/// - entity_id (primary)
/// - unique_id
/// - device_id (multi)
/// - config_entry_id (multi)
/// - area_id (multi)
/// - platform (multi)
///
/// Entries are stored as `Arc<EntityEntry>` to avoid cloning on reads.
/// The `Arc` reference counting is atomic and very fast.
pub struct EntityRegistry {
    /// Storage backend
    storage: Arc<Storage>,

    /// Primary index: entity_id -> EntityEntry (Arc-wrapped to avoid clones)
    /// Uses IndexMap + RwLock to preserve insertion order (important for Python dict compatibility)
    by_entity_id: RwLock<IndexMap<String, Arc<EntityEntry>>>,

    /// Index: unique_id -> entity_id
    by_unique_id: DashMap<String, String>,

    /// Index: device_id -> set of entity_ids
    by_device_id: DashMap<String, HashSet<String>>,

    /// Index: config_entry_id -> set of entity_ids
    by_config_entry_id: DashMap<String, HashSet<String>>,

    /// Index: area_id -> set of entity_ids
    by_area_id: DashMap<String, HashSet<String>>,

    /// Index: platform -> set of entity_ids
    by_platform: DashMap<String, HashSet<String>>,

    /// Deleted entities (soft delete, Arc-wrapped)
    /// Keyed by (domain, platform, unique_id) to match native HA semantics
    /// Uses IndexMap + RwLock to preserve insertion order (important for test compatibility)
    deleted: RwLock<IndexMap<(String, String, String), Arc<EntityEntry>>>,
}

impl EntityRegistry {
    /// Create a new entity registry
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            by_entity_id: RwLock::new(IndexMap::new()),
            by_unique_id: DashMap::new(),
            by_device_id: DashMap::new(),
            by_config_entry_id: DashMap::new(),
            by_area_id: DashMap::new(),
            by_platform: DashMap::new(),
            deleted: RwLock::new(IndexMap::new()),
        }
    }

    /// Load from storage
    pub async fn load(&self) -> StorageResult<()> {
        if let Some(storage_file) = self.storage.load::<EntityRegistryData>(STORAGE_KEY).await? {
            info!(
                "Loading {} entities from storage (v{}.{})",
                storage_file.data.entities.len(),
                storage_file.version,
                storage_file.minor_version
            );

            for entry in storage_file.data.entities {
                self.index_entry(Arc::new(entry));
            }

            for entry in storage_file.data.deleted_entities {
                // Key by (domain, platform, unique_id) to match native HA semantics
                let key = (
                    entry.domain().to_string(),
                    entry.platform.clone(),
                    entry.unique_id.clone().unwrap_or_default(),
                );
                if let Ok(mut deleted) = self.deleted.write() {
                    deleted.insert(key, Arc::new(entry));
                }
            }
        }
        Ok(())
    }

    /// Save to storage
    pub async fn save(&self) -> StorageResult<()> {
        // IndexMap preserves insertion order, no need to sort
        let deleted_entries: Vec<EntityEntry> = self
            .deleted
            .read()
            .map(|d| d.values().map(|v| (**v).clone()).collect())
            .unwrap_or_default();

        let entities: Vec<EntityEntry> = self
            .by_entity_id
            .read()
            .map(|e| e.values().map(|v| (**v).clone()).collect())
            .unwrap_or_default();

        let data = EntityRegistryData {
            entities,
            deleted_entities: deleted_entries,
        };

        let storage_file =
            StorageFile::new(STORAGE_KEY, data, STORAGE_VERSION, STORAGE_MINOR_VERSION);

        self.storage.save(&storage_file).await?;
        debug!(
            "Saved {} entities to storage",
            self.by_entity_id.read().map(|e| e.len()).unwrap_or(0)
        );
        Ok(())
    }

    /// Index an entry in all indexes
    ///
    /// Takes an `Arc<EntityEntry>` to avoid cloning - the Arc is stored directly.
    fn index_entry(&self, entry: Arc<EntityEntry>) {
        let entity_id = entry.entity_id.clone();

        // unique_id index (keyed by "platform\0unique_id" for uniqueness)
        if let Some(ref unique_id) = entry.unique_id {
            let key = format!("{}\0{}", entry.platform, unique_id);
            self.by_unique_id.insert(key, entity_id.clone());
        }

        // device_id index
        if let Some(ref device_id) = entry.device_id {
            self.by_device_id
                .entry(device_id.clone())
                .or_default()
                .insert(entity_id.clone());
        }

        // config_entry_id index
        if let Some(ref config_entry_id) = entry.config_entry_id {
            self.by_config_entry_id
                .entry(config_entry_id.clone())
                .or_default()
                .insert(entity_id.clone());
        }

        // area_id index
        if let Some(ref area_id) = entry.area_id {
            self.by_area_id
                .entry(area_id.clone())
                .or_default()
                .insert(entity_id.clone());
        }

        // platform index
        self.by_platform
            .entry(entry.platform.clone())
            .or_default()
            .insert(entity_id.clone());

        // Primary index (insert Arc directly, no clone)
        if let Ok(mut idx) = self.by_entity_id.write() {
            idx.insert(entity_id, entry);
        }
    }

    /// Remove an entry from all indexes
    fn unindex_entry(&self, entry: &EntityEntry) {
        let entity_id = &entry.entity_id;

        // Remove from unique_id index
        if let Some(ref unique_id) = entry.unique_id {
            let key = format!("{}\0{}", entry.platform, unique_id);
            self.by_unique_id.remove(&key);
        }

        // Remove from device_id index
        if let Some(ref device_id) = entry.device_id {
            if let Some(mut ids) = self.by_device_id.get_mut(device_id) {
                ids.remove(entity_id);
            }
        }

        // Remove from config_entry_id index
        if let Some(ref config_entry_id) = entry.config_entry_id {
            if let Some(mut ids) = self.by_config_entry_id.get_mut(config_entry_id) {
                ids.remove(entity_id);
            }
        }

        // Remove from area_id index
        if let Some(ref area_id) = entry.area_id {
            if let Some(mut ids) = self.by_area_id.get_mut(area_id) {
                ids.remove(entity_id);
            }
        }

        // Remove from platform index
        if let Some(mut ids) = self.by_platform.get_mut(&entry.platform) {
            ids.remove(entity_id);
        }

        // Remove from primary index
        if let Ok(mut idx) = self.by_entity_id.write() {
            idx.shift_remove(entity_id);
        }
    }

    /// Get entity by entity_id
    ///
    /// Returns an `Arc<EntityEntry>` - cheap to clone (atomic increment).
    pub fn get(&self, entity_id: &str) -> Option<Arc<EntityEntry>> {
        self.by_entity_id
            .read()
            .ok()
            .and_then(|idx| idx.get(entity_id).cloned())
    }

    /// Get entity by (platform, unique_id) composite key
    pub fn get_by_unique_id(&self, unique_id: &str) -> Option<Arc<EntityEntry>> {
        // Search all platform+unique_id combinations (backward compat)
        // This iterates but is only used when platform is unknown
        for entry in self.by_unique_id.iter() {
            if entry.key().ends_with(&format!("\0{}", unique_id)) {
                return self.get(entry.value());
            }
        }
        None
    }

    /// Get entity by platform and unique_id (preferred - uses index directly)
    pub fn get_by_platform_unique_id(
        &self,
        platform: &str,
        unique_id: &str,
    ) -> Option<Arc<EntityEntry>> {
        let key = format!("{}\0{}", platform, unique_id);
        self.by_unique_id
            .get(&key)
            .and_then(|entity_id| self.get(&entity_id))
    }

    /// Get all entities for a device
    pub fn get_by_device_id(&self, device_id: &str) -> Vec<Arc<EntityEntry>> {
        self.by_device_id
            .get(device_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all entities for a config entry
    pub fn get_by_config_entry_id(&self, config_entry_id: &str) -> Vec<Arc<EntityEntry>> {
        self.by_config_entry_id
            .get(config_entry_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all entities in an area
    pub fn get_by_area_id(&self, area_id: &str) -> Vec<Arc<EntityEntry>> {
        self.by_area_id
            .get(area_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all entities for a platform
    pub fn get_by_platform(&self, platform: &str) -> Vec<Arc<EntityEntry>> {
        self.by_platform
            .get(platform)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get or create an entity entry
    ///
    /// This is the main registration method. If an entity with the same
    /// unique_id exists, returns it. Otherwise creates a new entry.
    ///
    /// Returns an `Arc<EntityEntry>` - cheap to clone (atomic increment).
    pub fn get_or_create(
        &self,
        platform: &str,
        entity_id: &str,
        unique_id: Option<&str>,
        config_entry_id: Option<&str>,
        device_id: Option<&str>,
    ) -> Arc<EntityEntry> {
        // Check if unique_id exists for this platform
        if let Some(uid) = unique_id {
            if let Some(existing) = self.get_by_platform_unique_id(platform, uid) {
                debug!("Found existing entity by unique_id: {}", existing.entity_id);
                return existing;
            }
        }

        // Check if entity_id exists
        if let Some(existing) = self.get(entity_id) {
            // Update with unique_id if not set
            if existing.unique_id.is_none() && unique_id.is_some() {
                return self
                    .update(entity_id, |entry| {
                        entry.unique_id = unique_id.map(String::from);
                        entry.modified_at = Utc::now();
                    })
                    .expect("Entity should exist after get check");
            }
            return existing;
        }

        // Check if entity was previously deleted and can be restored
        // Key is (domain, platform, unique_id)
        let domain = entity_id.split('.').next().unwrap_or("");
        if let Some(uid) = unique_id {
            let deleted_key = (domain.to_string(), platform.to_string(), uid.to_string());
            let deleted_entry = self
                .deleted
                .write()
                .ok()
                .and_then(|mut d| d.shift_remove(&deleted_key));
            if let Some(deleted_entry) = deleted_entry {
                // Restore the deleted entity with updated modified_at
                let mut restored = (*deleted_entry).clone();
                restored.entity_id = entity_id.to_string();
                restored.modified_at = Utc::now();
                // Keep original id and created_at from deleted entry

                let arc_entry = Arc::new(restored);
                self.index_entry(Arc::clone(&arc_entry));

                info!("Restored deleted entity: {}", entity_id);
                return arc_entry;
            }
        }

        // Create new entry
        let mut entry = EntityEntry::new(entity_id, platform, unique_id.map(String::from));
        entry.config_entry_id = config_entry_id.map(String::from);
        entry.device_id = device_id.map(String::from);

        let arc_entry = Arc::new(entry);
        self.index_entry(Arc::clone(&arc_entry));

        info!("Registered new entity: {}", entity_id);
        arc_entry
    }

    /// Update an entity entry
    ///
    /// Returns the updated entry as `Arc<EntityEntry>`, or an error if not found.
    /// The closure receives a mutable reference to a cloned entry, which is then
    /// wrapped in a new Arc and stored.
    pub fn update<F>(&self, entity_id: &str, f: F) -> Result<Arc<EntityEntry>, EntityRegistryError>
    where
        F: FnOnce(&mut EntityEntry),
    {
        // Remove first to avoid deadlock (don't hold ref while modifying indexes)
        let arc_entry = self
            .by_entity_id
            .write()
            .ok()
            .and_then(|mut idx| idx.shift_remove(entity_id));

        if let Some(arc_entry) = arc_entry {
            // Clone the inner entry for modification
            let mut entry = (*arc_entry).clone();

            // Unindex the old entry from secondary indexes
            if let Some(ref unique_id) = entry.unique_id {
                let key = format!("{}\0{}", entry.platform, unique_id);
                self.by_unique_id.remove(&key);
            }
            if let Some(ref device_id) = entry.device_id {
                if let Some(mut ids) = self.by_device_id.get_mut(device_id) {
                    ids.remove(&entry.entity_id);
                }
            }
            if let Some(ref config_entry_id) = entry.config_entry_id {
                if let Some(mut ids) = self.by_config_entry_id.get_mut(config_entry_id) {
                    ids.remove(&entry.entity_id);
                }
            }
            if let Some(ref area_id) = entry.area_id {
                if let Some(mut ids) = self.by_area_id.get_mut(area_id) {
                    ids.remove(&entry.entity_id);
                }
            }
            if let Some(mut ids) = self.by_platform.get_mut(&entry.platform) {
                ids.remove(&entry.entity_id);
            }

            // Apply update
            f(&mut entry);
            // Note: modified_at should be set by the caller in the closure if needed

            // Re-index with new Arc
            let new_arc = Arc::new(entry);
            self.index_entry(Arc::clone(&new_arc));

            Ok(new_arc)
        } else {
            Err(EntityRegistryError::NotFound(entity_id.to_string()))
        }
    }

    /// Remove an entity
    ///
    /// Returns the removed entry as `Arc<EntityEntry>`.
    pub fn remove(&self, entity_id: &str) -> Option<Arc<EntityEntry>> {
        let arc_entry = self
            .by_entity_id
            .write()
            .ok()
            .and_then(|mut idx| idx.shift_remove(entity_id));

        if let Some(arc_entry) = arc_entry {
            self.unindex_entry(&arc_entry);
            // Add to deleted for tracking, keyed by (domain, platform, unique_id)
            let key = (
                arc_entry.domain().to_string(),
                arc_entry.platform.clone(),
                arc_entry.unique_id.clone().unwrap_or_default(),
            );
            if let Ok(mut deleted) = self.deleted.write() {
                deleted.insert(key, Arc::clone(&arc_entry));
            }
            info!("Removed entity: {}", entity_id);
            Some(arc_entry)
        } else {
            None
        }
    }

    /// Get all entity IDs
    pub fn entity_ids(&self) -> Vec<String> {
        self.by_entity_id
            .read()
            .map(|idx| idx.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Get count of registered entities
    pub fn len(&self) -> usize {
        self.by_entity_id.read().map(|idx| idx.len()).unwrap_or(0)
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.by_entity_id
            .read()
            .map(|idx| idx.is_empty())
            .unwrap_or(true)
    }

    /// Check if an entity_id is registered
    pub fn is_registered(&self, entity_id: &str) -> bool {
        self.by_entity_id
            .read()
            .map(|idx| idx.contains_key(entity_id))
            .unwrap_or(false)
    }

    /// Generate a unique entity_id that doesn't conflict with existing registrations
    ///
    /// Takes a domain and suggested object_id, and returns an entity_id that is
    /// guaranteed not to conflict with any existing registered entity or reserved IDs.
    /// If the preferred entity_id is taken, appends `_2`, `_3`, etc. until
    /// finding an available one.
    ///
    /// # Arguments
    /// * `domain` - The entity domain (e.g., "light", "sensor")
    /// * `suggested_object_id` - The preferred object_id part
    /// * `current_entity_id` - Optional: the entity's current entity_id (excluded from conflict check)
    /// * `reserved_ids` - Optional: additional IDs to consider as unavailable (e.g., from state machine)
    ///
    /// # Returns
    /// A unique entity_id in the format `{domain}.{object_id}`
    pub fn generate_entity_id(
        &self,
        domain: &str,
        suggested_object_id: &str,
        current_entity_id: Option<&str>,
        reserved_ids: Option<&[String]>,
    ) -> String {
        const MAX_LENGTH_STATE_ENTITY_ID: usize = 255;

        let slugified = slugify(suggested_object_id);
        let preferred_full = format!("{}.{}", domain, slugified);
        // Truncate to max entity_id length for the initial check
        let preferred = if preferred_full.len() > MAX_LENGTH_STATE_ENTITY_ID {
            &preferred_full[..MAX_LENGTH_STATE_ENTITY_ID]
        } else {
            &preferred_full[..]
        };

        // Helper to check if an entity_id is available
        let is_available = |entity_id: &str| -> bool {
            // Not available if registered in entity registry
            if self.is_registered(entity_id) {
                return false;
            }
            // Not available if in reserved IDs list
            if let Some(reserved) = reserved_ids {
                if reserved.iter().any(|r| r == entity_id) {
                    return false;
                }
            }
            true
        };

        // Check if preferred is available
        if is_available(preferred) {
            return preferred.to_string();
        }

        // If current_entity_id matches preferred, it's available for this entity
        if let Some(current) = current_entity_id {
            if current == preferred {
                return preferred.to_string();
            }
        }

        // Find available entity_id with suffix
        let mut tries = 1;
        loop {
            tries += 1;
            let len_suffix = format!("{}", tries).len() + 1; // "_N"
            let base_len = MAX_LENGTH_STATE_ENTITY_ID - len_suffix;
            let base = if preferred_full.len() > base_len {
                &preferred_full[..base_len]
            } else {
                &preferred_full[..]
            };
            let test_id = format!("{}_{}", base, tries);

            // Check if available
            if is_available(&test_id) {
                return test_id;
            }

            // Check if it's the entity's current ID
            if let Some(current) = current_entity_id {
                if current == test_id {
                    return test_id;
                }
            }

            // Safety: prevent infinite loops
            if tries > 10000 {
                // Highly unlikely, but return a unique ID based on timestamp
                return format!(
                    "{}.{}_{}",
                    domain,
                    suggested_object_id,
                    chrono::Utc::now().timestamp_millis()
                );
            }
        }
    }

    /// Iterate over all entries
    ///
    /// Returns `Arc<EntityEntry>` references - cheap to clone.
    /// Iterate over all entities (preserves insertion order)
    ///
    /// Returns a Vec to avoid holding the lock during iteration.
    pub fn iter(&self) -> Vec<Arc<EntityEntry>> {
        self.by_entity_id
            .read()
            .map(|idx| idx.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all deleted entries as a vector (preserves insertion order)
    ///
    /// Returns `Arc<EntityEntry>` references for soft-deleted entities.
    pub fn deleted_iter(&self) -> Vec<Arc<EntityEntry>> {
        self.deleted
            .read()
            .map(|d| d.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get count of deleted entities
    pub fn deleted_len(&self) -> usize {
        self.deleted.read().map(|d| d.len()).unwrap_or(0)
    }

    /// Check if an entity with the given (domain, platform, unique_id) is in deleted_entities
    pub fn is_deleted(&self, domain: &str, platform: &str, unique_id: &str) -> bool {
        let key = (
            domain.to_string(),
            platform.to_string(),
            unique_id.to_string(),
        );
        self.deleted
            .read()
            .map(|d| d.contains_key(&key))
            .unwrap_or(false)
    }

    /// Clear config_entry_id from deleted entities that match the given config_entry_id.
    /// Sets orphaned_timestamp to the provided timestamp.
    pub fn clear_deleted_config_entry(&self, config_entry_id: &str, orphaned_timestamp: f64) {
        if let Ok(mut deleted) = self.deleted.write() {
            for entry in deleted.values_mut() {
                if entry.config_entry_id.as_deref() == Some(config_entry_id) {
                    let mut updated = (**entry).clone();
                    updated.config_entry_id = None;
                    updated.orphaned_timestamp = Some(orphaned_timestamp);
                    *entry = Arc::new(updated);
                }
            }
        }
    }

    /// Clear area_id from deleted entities that match.
    pub fn clear_deleted_area_id(&self, area_id: &str) {
        if let Ok(mut deleted) = self.deleted.write() {
            for entry in deleted.values_mut() {
                if entry.area_id.as_deref() == Some(area_id) {
                    let mut updated = (**entry).clone();
                    updated.area_id = None;
                    *entry = Arc::new(updated);
                }
            }
        }
    }

    /// Clear a label from deleted entities that have it.
    pub fn clear_deleted_label_id(&self, label_id: &str) {
        if let Ok(mut deleted) = self.deleted.write() {
            for entry in deleted.values_mut() {
                if entry.labels.contains(label_id) {
                    let mut updated = (**entry).clone();
                    updated.labels.remove(label_id);
                    *entry = Arc::new(updated);
                }
            }
        }
    }

    /// Clear a category from deleted entities matching scope and category_id.
    pub fn clear_deleted_category_id(&self, scope: &str, category_id: &str) {
        if let Ok(mut deleted) = self.deleted.write() {
            for entry in deleted.values_mut() {
                if let Some(serde_json::Value::Object(ref cats)) = entry.categories {
                    if cats.get(scope).and_then(|v| v.as_str()) == Some(category_id) {
                        let mut updated = (**entry).clone();
                        if let Some(serde_json::Value::Object(ref mut map)) = updated.categories {
                            map.remove(scope);
                            if map.is_empty() {
                                updated.categories = None;
                            }
                        }
                        *entry = Arc::new(updated);
                    }
                }
            }
        }
    }

    /// Clear config_subentry_id from deleted entities that match.
    /// Also clears config_entry_id and sets orphaned_timestamp.
    pub fn clear_deleted_config_subentry(
        &self,
        _config_entry_id: &str,
        config_subentry_id: &str,
        orphaned_timestamp: f64,
    ) {
        if let Ok(mut deleted) = self.deleted.write() {
            for entry in deleted.values_mut() {
                if entry.config_subentry_id.as_deref() == Some(config_subentry_id) {
                    let mut updated = (**entry).clone();
                    updated.config_entry_id = None;
                    updated.config_subentry_id = None;
                    updated.orphaned_timestamp = Some(orphaned_timestamp);
                    *entry = Arc::new(updated);
                }
            }
        }
    }
}

/// Slugify a string for use as an entity object_id.
/// Converts to lowercase, replaces non-alphanumeric chars with underscore,
/// collapses consecutive underscores, and strips leading/trailing underscores.
fn slugify(name: &str) -> String {
    let mut result = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            result.extend(c.to_lowercase());
        } else if !result.is_empty() && !result.ends_with('_') {
            result.push('_');
        }
    }
    result.trim_end_matches('_').to_string()
}

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive EntityRegistry testing through Python bindings
