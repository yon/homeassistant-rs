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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloorEntry {
    /// Internal UUID (stored as "floor_id" in HA storage)
    #[serde(alias = "floor_id")]
    pub id: String,

    /// Floor name (e.g., "Ground Floor", "First Floor")
    pub name: String,

    /// Normalized name for searching
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_name: Option<String>,

    /// Floor icon (e.g., "mdi:home-floor-1")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Floor level (None = unset, 0 = ground, positive = above, negative = below)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<i32>,

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
    /// Create a new floor entry with an explicit ID and timestamp
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        level: Option<i32>,
        now: Option<DateTime<Utc>>,
    ) -> Self {
        let name = name.into();
        let now = now.unwrap_or_else(Utc::now);
        Self {
            id: id.into(),
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

/// Normalize a name by removing whitespace and case folding (matches HA behavior)
fn normalize_name(name: &str) -> String {
    name.to_lowercase().replace(' ', "")
}

/// Slugify a name for use as an ID (matches HA's slugify behavior)
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
///
/// Entries are stored as `Arc<FloorEntry>` to avoid cloning on reads.
pub struct FloorRegistry {
    /// Storage backend
    storage: Arc<Storage>,

    /// Primary index: floor_id -> FloorEntry (Arc-wrapped)
    by_id: DashMap<String, Arc<FloorEntry>>,

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
                self.index_entry(Arc::new(entry));
            }
        }
        Ok(())
    }

    /// Save to storage
    pub async fn save(&self) -> StorageResult<()> {
        let data = FloorRegistryData {
            floors: self.by_id.iter().map(|r| (**r.value()).clone()).collect(),
        };

        let storage_file =
            StorageFile::new(STORAGE_KEY, data, STORAGE_VERSION, STORAGE_MINOR_VERSION);

        self.storage.save(&storage_file).await?;
        debug!("Saved {} floors to storage", self.by_id.len());
        Ok(())
    }

    /// Index an entry (takes Arc to avoid cloning)
    fn index_entry(&self, entry: Arc<FloorEntry>) {
        let floor_id = entry.id.clone();

        if let Some(ref normalized) = entry.normalized_name {
            self.by_name.insert(normalized.clone(), floor_id.clone());
        }

        if let Some(level) = entry.level {
            self.by_level.insert(level, floor_id.clone());
        }
        self.by_id.insert(floor_id, entry);
    }

    /// Remove an entry from indexes
    fn unindex_entry(&self, entry: &FloorEntry) {
        if let Some(ref normalized) = entry.normalized_name {
            self.by_name.remove(normalized);
        }
        if let Some(level) = entry.level {
            self.by_level.remove(&level);
        }
        self.by_id.remove(&entry.id);
    }

    /// Get floor by ID
    ///
    /// Returns an `Arc<FloorEntry>` - cheap to clone.
    pub fn get(&self, floor_id: &str) -> Option<Arc<FloorEntry>> {
        self.by_id.get(floor_id).map(|r| Arc::clone(r.value()))
    }

    /// Get floor by name
    pub fn get_by_name(&self, name: &str) -> Option<Arc<FloorEntry>> {
        let normalized = normalize_name(name);
        self.by_name
            .get(&normalized)
            .and_then(|floor_id| self.get(&floor_id))
    }

    /// Get floor by level
    pub fn get_by_level(&self, level: i32) -> Option<Arc<FloorEntry>> {
        self.by_level
            .get(&level)
            .and_then(|floor_id| self.get(&floor_id))
    }

    /// Create a new floor
    ///
    /// Returns an `Arc<FloorEntry>` - cheap to clone.
    /// Returns an error if a floor with the same name already exists.
    /// If `now` is None, uses the current system time.
    pub fn create(
        &self,
        name: &str,
        level: Option<i32>,
        now: Option<DateTime<Utc>>,
    ) -> Result<Arc<FloorEntry>, String> {
        let normalized = normalize_name(name);
        if self.by_name.contains_key(&normalized) {
            return Err(format!(
                "The name {} ({}) is already in use",
                name, normalized
            ));
        }

        let id = self.generate_id(name);
        let entry = FloorEntry::new(id, name, level, now);
        let arc_entry = Arc::new(entry);
        info!(
            "Created floor: {} (level {:?}, {})",
            name, level, arc_entry.id
        );
        self.index_entry(Arc::clone(&arc_entry));
        Ok(arc_entry)
    }

    /// Generate a unique ID from a name (slugified, with suffix for conflicts)
    fn generate_id(&self, name: &str) -> String {
        let base = slugify(name);
        if !self.by_id.contains_key(&base) {
            return base;
        }
        let mut tries = 2;
        loop {
            let candidate = format!("{}_{}", base, tries);
            if !self.by_id.contains_key(&candidate) {
                return candidate;
            }
            tries += 1;
        }
    }

    /// Update a floor
    ///
    /// Returns the updated entry as `Arc<FloorEntry>`.
    /// Returns `Err` if the new name conflicts with another floor.
    /// Only updates `modified_at` if the entry actually changed.
    /// If `now` is None, uses the current system time for modified_at.
    pub fn update<F>(
        &self,
        floor_id: &str,
        f: F,
        now: Option<DateTime<Utc>>,
    ) -> Result<Arc<FloorEntry>, String>
    where
        F: FnOnce(&mut FloorEntry),
    {
        // Remove first to avoid deadlock
        if let Some((_, arc_entry)) = self.by_id.remove(floor_id) {
            // Clone the inner entry for modification
            let mut entry = (*arc_entry).clone();
            let old_entry = entry.clone();

            // Unindex from secondary indexes
            if let Some(ref normalized) = entry.normalized_name {
                self.by_name.remove(normalized);
            }
            if let Some(level) = entry.level {
                self.by_level.remove(&level);
            }

            // Apply update
            f(&mut entry);
            entry.normalized_name = Some(normalize_name(&entry.name));

            // Check for name conflict with another floor
            if entry.name != old_entry.name {
                let new_normalized = normalize_name(&entry.name);
                if self.by_name.contains_key(&new_normalized) {
                    // Name conflict - re-index the old entry and return error
                    self.index_entry(arc_entry);
                    return Err(format!(
                        "The name {} ({}) is already in use",
                        entry.name, new_normalized
                    ));
                }
            }

            // Only update modified_at if something actually changed
            let changed = entry.name != old_entry.name
                || entry.aliases != old_entry.aliases
                || entry.icon != old_entry.icon
                || entry.level != old_entry.level;
            if changed {
                entry.modified_at = now.unwrap_or_else(Utc::now);
            }

            // Re-index with new Arc
            let new_arc = Arc::new(entry);
            self.index_entry(Arc::clone(&new_arc));

            Ok(new_arc)
        } else {
            Err(format!("Floor not found: {}", floor_id))
        }
    }

    /// Remove a floor
    ///
    /// Returns the removed entry as `Arc<FloorEntry>`.
    pub fn remove(&self, floor_id: &str) -> Option<Arc<FloorEntry>> {
        if let Some((_, arc_entry)) = self.by_id.remove(floor_id) {
            self.unindex_entry(&arc_entry);
            info!("Removed floor: {}", floor_id);
            Some(arc_entry)
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

    /// Iterate over all floors
    ///
    /// Returns `Arc<FloorEntry>` references - cheap to clone.
    pub fn iter(&self) -> impl Iterator<Item = Arc<FloorEntry>> + '_ {
        self.by_id.iter().map(|r| Arc::clone(r.value()))
    }

    /// Get all floors sorted by level
    ///
    /// Returns `Arc<FloorEntry>` references - cheap to clone.
    pub fn sorted_by_level(&self) -> Vec<Arc<FloorEntry>> {
        let mut floors: Vec<_> = self.iter().collect();
        floors.sort_by_key(|f| f.level);
        floors
    }
}

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive FloorRegistry testing through Python bindings
