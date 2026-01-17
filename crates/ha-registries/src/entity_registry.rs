//! Entity Registry
//!
//! Tracks all registered entities with unique_id tracking, device linking,
//! and multiple indexes for fast lookups.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::storage::{Storable, Storage, StorageFile, StorageResult};

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
    /// Disabled by the integration
    Integration,
    /// Disabled by the user
    User,
    /// Disabled by a config entry
    ConfigEntry,
    /// Disabled by device
    Device,
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
    #[serde(default)]
    pub has_entity_name: bool,

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Alternative names/IDs
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    /// Last modified timestamp
    #[serde(default = "Utc::now")]
    pub modified_at: DateTime<Utc>,
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
            id: ulid::Ulid::new().to_string().to_lowercase(),
            entity_id: entity_id.into(),
            unique_id,
            previous_unique_id: None,
            device_id: None,
            config_entry_id: None,
            config_subentry_id: None,
            name: None,
            original_name: None,
            suggested_object_id: None,
            has_entity_name: false,
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
            labels: Vec::new(),
            aliases: Vec::new(),
            created_at: now,
            modified_at: now,
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
pub struct EntityRegistry {
    /// Storage backend
    storage: Arc<Storage>,

    /// Primary index: entity_id -> EntityEntry
    by_entity_id: DashMap<String, EntityEntry>,

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

    /// Deleted entities (soft delete)
    deleted: DashMap<String, EntityEntry>,
}

impl EntityRegistry {
    /// Create a new entity registry
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            by_entity_id: DashMap::new(),
            by_unique_id: DashMap::new(),
            by_device_id: DashMap::new(),
            by_config_entry_id: DashMap::new(),
            by_area_id: DashMap::new(),
            by_platform: DashMap::new(),
            deleted: DashMap::new(),
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
                self.index_entry(&entry);
            }

            for entry in storage_file.data.deleted_entities {
                self.deleted.insert(entry.entity_id.clone(), entry);
            }
        }
        Ok(())
    }

    /// Save to storage
    pub async fn save(&self) -> StorageResult<()> {
        let data = EntityRegistryData {
            entities: self
                .by_entity_id
                .iter()
                .map(|r| r.value().clone())
                .collect(),
            deleted_entities: self.deleted.iter().map(|r| r.value().clone()).collect(),
        };

        let storage_file =
            StorageFile::new(STORAGE_KEY, data, STORAGE_VERSION, STORAGE_MINOR_VERSION);

        self.storage.save(&storage_file).await?;
        debug!("Saved {} entities to storage", self.by_entity_id.len());
        Ok(())
    }

    /// Index an entry in all indexes
    fn index_entry(&self, entry: &EntityEntry) {
        let entity_id = entry.entity_id.clone();

        // Primary index
        self.by_entity_id.insert(entity_id.clone(), entry.clone());

        // unique_id index
        if let Some(ref unique_id) = entry.unique_id {
            self.by_unique_id
                .insert(unique_id.clone(), entity_id.clone());
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
            .insert(entity_id);
    }

    /// Remove an entry from all indexes
    fn unindex_entry(&self, entry: &EntityEntry) {
        let entity_id = &entry.entity_id;

        // Remove from unique_id index
        if let Some(ref unique_id) = entry.unique_id {
            self.by_unique_id.remove(unique_id);
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
        self.by_entity_id.remove(entity_id);
    }

    /// Get entity by entity_id
    pub fn get(&self, entity_id: &str) -> Option<EntityEntry> {
        self.by_entity_id.get(entity_id).map(|r| r.value().clone())
    }

    /// Get entity by unique_id
    pub fn get_by_unique_id(&self, unique_id: &str) -> Option<EntityEntry> {
        self.by_unique_id
            .get(unique_id)
            .and_then(|entity_id| self.get(&entity_id))
    }

    /// Get all entities for a device
    pub fn get_by_device_id(&self, device_id: &str) -> Vec<EntityEntry> {
        self.by_device_id
            .get(device_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all entities for a config entry
    pub fn get_by_config_entry_id(&self, config_entry_id: &str) -> Vec<EntityEntry> {
        self.by_config_entry_id
            .get(config_entry_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all entities in an area
    pub fn get_by_area_id(&self, area_id: &str) -> Vec<EntityEntry> {
        self.by_area_id
            .get(area_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all entities for a platform
    pub fn get_by_platform(&self, platform: &str) -> Vec<EntityEntry> {
        self.by_platform
            .get(platform)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get or create an entity entry
    ///
    /// This is the main registration method. If an entity with the same
    /// unique_id exists, returns it. Otherwise creates a new entry.
    pub fn get_or_create(
        &self,
        platform: &str,
        entity_id: &str,
        unique_id: Option<&str>,
        config_entry_id: Option<&str>,
        device_id: Option<&str>,
    ) -> EntityEntry {
        // Check if unique_id exists
        if let Some(uid) = unique_id {
            if let Some(existing) = self.get_by_unique_id(uid) {
                debug!("Found existing entity by unique_id: {}", existing.entity_id);
                return existing;
            }
        }

        // Check if entity_id exists
        if let Some(existing) = self.get(entity_id) {
            // Update with unique_id if not set
            if existing.unique_id.is_none() && unique_id.is_some() {
                return self.update(entity_id, |entry| {
                    entry.unique_id = unique_id.map(String::from);
                    entry.modified_at = Utc::now();
                });
            }
            return existing;
        }

        // Create new entry
        let mut entry = EntityEntry::new(entity_id, platform, unique_id.map(String::from));
        entry.config_entry_id = config_entry_id.map(String::from);
        entry.device_id = device_id.map(String::from);

        self.index_entry(&entry);

        info!("Registered new entity: {}", entity_id);
        entry
    }

    /// Update an entity entry
    pub fn update<F>(&self, entity_id: &str, f: F) -> EntityEntry
    where
        F: FnOnce(&mut EntityEntry),
    {
        // Remove first to avoid deadlock (don't hold ref while modifying indexes)
        if let Some((_, mut entry)) = self.by_entity_id.remove(entity_id) {
            // Unindex the old entry from secondary indexes
            if let Some(ref unique_id) = entry.unique_id {
                self.by_unique_id.remove(unique_id);
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
            entry.modified_at = Utc::now();

            // Re-index
            self.index_entry(&entry);

            entry
        } else {
            panic!("Entity not found: {}", entity_id);
        }
    }

    /// Remove an entity
    pub fn remove(&self, entity_id: &str) -> Option<EntityEntry> {
        if let Some((_, entry)) = self.by_entity_id.remove(entity_id) {
            self.unindex_entry(&entry);
            // Add to deleted for tracking
            self.deleted.insert(entity_id.to_string(), entry.clone());
            info!("Removed entity: {}", entity_id);
            Some(entry)
        } else {
            None
        }
    }

    /// Get all entity IDs
    pub fn entity_ids(&self) -> Vec<String> {
        self.by_entity_id.iter().map(|r| r.key().clone()).collect()
    }

    /// Get count of registered entities
    pub fn len(&self) -> usize {
        self.by_entity_id.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.by_entity_id.is_empty()
    }

    /// Iterate over all entries
    pub fn iter(&self) -> impl Iterator<Item = EntityEntry> + '_ {
        self.by_entity_id.iter().map(|r| r.value().clone())
    }
}

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive EntityRegistry testing through Python bindings
