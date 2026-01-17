//! Floor Registry
//!
//! Tracks all registered floors in the home.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::storage::{Storable, Storage, StorageFile, StorageResult};

/// Storage key for floor registry
pub const STORAGE_KEY: &str = "core.floor_registry";
/// Current storage version
pub const STORAGE_VERSION: u32 = 1;
/// Current minor version
pub const STORAGE_MINOR_VERSION: u32 = 2;

/// A registered floor entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorEntry {
    /// Internal UUID
    pub id: String,

    /// Floor name (e.g., "Ground Floor", "First Floor")
    pub name: String,

    /// Normalized name for searching
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_name: Option<String>,

    /// Floor icon (e.g., "mdi:home-floor-1")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Floor level (0 = ground, positive = above, negative = below)
    #[serde(default)]
    pub level: i32,

    /// Alternative names
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last modified timestamp
    #[serde(default = "Utc::now")]
    pub modified_at: DateTime<Utc>,
}

impl FloorEntry {
    /// Create a new floor entry
    pub fn new(name: impl Into<String>, level: i32) -> Self {
        let name = name.into();
        let now = Utc::now();
        Self {
            id: ulid::Ulid::new().to_string().to_lowercase(),
            normalized_name: Some(normalize_name(&name)),
            name,
            icon: None,
            level,
            aliases: Vec::new(),
            created_at: now,
            modified_at: now,
        }
    }
}

/// Normalize a name for searching
fn normalize_name(name: &str) -> String {
    name.to_lowercase()
        .trim()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
}

/// Floor registry data for storage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FloorRegistryData {
    /// All registered floors
    pub floors: Vec<FloorEntry>,
}

impl Storable for FloorRegistryData {
    const KEY: &'static str = STORAGE_KEY;
    const VERSION: u32 = STORAGE_VERSION;
    const MINOR_VERSION: u32 = STORAGE_MINOR_VERSION;
}

/// Floor Registry
pub struct FloorRegistry {
    /// Storage backend
    storage: Arc<Storage>,

    /// Primary index: floor_id -> FloorEntry
    by_id: DashMap<String, FloorEntry>,

    /// Index: normalized_name -> floor_id
    by_name: DashMap<String, String>,

    /// Index: level -> floor_id
    by_level: DashMap<i32, String>,
}

impl FloorRegistry {
    /// Create a new floor registry
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            by_id: DashMap::new(),
            by_name: DashMap::new(),
            by_level: DashMap::new(),
        }
    }

    /// Load from storage
    pub async fn load(&self) -> StorageResult<()> {
        if let Some(storage_file) = self.storage.load::<FloorRegistryData>(STORAGE_KEY).await? {
            info!(
                "Loading {} floors from storage (v{}.{})",
                storage_file.data.floors.len(),
                storage_file.version,
                storage_file.minor_version
            );

            for entry in storage_file.data.floors {
                self.index_entry(&entry);
            }
        }
        Ok(())
    }

    /// Save to storage
    pub async fn save(&self) -> StorageResult<()> {
        let data = FloorRegistryData {
            floors: self.by_id.iter().map(|r| r.value().clone()).collect(),
        };

        let storage_file =
            StorageFile::new(STORAGE_KEY, data, STORAGE_VERSION, STORAGE_MINOR_VERSION);

        self.storage.save(&storage_file).await?;
        debug!("Saved {} floors to storage", self.by_id.len());
        Ok(())
    }

    /// Index an entry
    fn index_entry(&self, entry: &FloorEntry) {
        let floor_id = entry.id.clone();

        self.by_id.insert(floor_id.clone(), entry.clone());

        if let Some(ref normalized) = entry.normalized_name {
            self.by_name.insert(normalized.clone(), floor_id.clone());
        }

        self.by_level.insert(entry.level, floor_id);
    }

    /// Remove an entry from indexes
    fn unindex_entry(&self, entry: &FloorEntry) {
        if let Some(ref normalized) = entry.normalized_name {
            self.by_name.remove(normalized);
        }
        self.by_level.remove(&entry.level);
        self.by_id.remove(&entry.id);
    }

    /// Get floor by ID
    pub fn get(&self, floor_id: &str) -> Option<FloorEntry> {
        self.by_id.get(floor_id).map(|r| r.value().clone())
    }

    /// Get floor by name
    pub fn get_by_name(&self, name: &str) -> Option<FloorEntry> {
        let normalized = normalize_name(name);
        self.by_name
            .get(&normalized)
            .and_then(|floor_id| self.get(&floor_id))
    }

    /// Get floor by level
    pub fn get_by_level(&self, level: i32) -> Option<FloorEntry> {
        self.by_level
            .get(&level)
            .and_then(|floor_id| self.get(&floor_id))
    }

    /// Create a new floor
    pub fn create(&self, name: &str, level: i32) -> FloorEntry {
        let entry = FloorEntry::new(name, level);
        self.index_entry(&entry);
        info!("Created floor: {} (level {}, {})", name, level, entry.id);
        entry
    }

    /// Update a floor
    pub fn update<F>(&self, floor_id: &str, f: F) -> Option<FloorEntry>
    where
        F: FnOnce(&mut FloorEntry),
    {
        // Remove first to avoid deadlock
        if let Some((_, mut entry)) = self.by_id.remove(floor_id) {
            // Unindex from secondary indexes
            if let Some(ref normalized) = entry.normalized_name {
                self.by_name.remove(normalized);
            }
            self.by_level.remove(&entry.level);

            // Apply update
            f(&mut entry);
            entry.normalized_name = Some(normalize_name(&entry.name));
            entry.modified_at = Utc::now();

            // Re-index
            self.index_entry(&entry);

            Some(entry)
        } else {
            None
        }
    }

    /// Remove a floor
    pub fn remove(&self, floor_id: &str) -> Option<FloorEntry> {
        if let Some((_, entry)) = self.by_id.remove(floor_id) {
            self.unindex_entry(&entry);
            info!("Removed floor: {}", floor_id);
            Some(entry)
        } else {
            None
        }
    }

    /// Get count of floors
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Iterate over all floors (sorted by level)
    pub fn iter(&self) -> impl Iterator<Item = FloorEntry> + '_ {
        self.by_id.iter().map(|r| r.value().clone())
    }

    /// Get all floors sorted by level
    pub fn sorted_by_level(&self) -> Vec<FloorEntry> {
        let mut floors: Vec<_> = self.iter().collect();
        floors.sort_by_key(|f| f.level);
        floors
    }
}

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive FloorRegistry testing through Python bindings
