//! Area Registry
//!
//! Tracks all registered areas (rooms, zones) in the home.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::storage::{Storable, Storage, StorageFile, StorageResult};

/// Storage key for area registry
pub const STORAGE_KEY: &str = "core.area_registry";
/// Current storage version
pub const STORAGE_VERSION: u32 = 1;
/// Current minor version
pub const STORAGE_MINOR_VERSION: u32 = 6;

/// A registered area entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AreaEntry {
    /// Internal UUID
    pub id: String,

    /// Area name (e.g., "Living Room")
    pub name: String,

    /// Normalized name for searching
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_name: Option<String>,

    /// Area picture URL/path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,

    /// Area icon (e.g., "mdi:sofa")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Alternative names
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,

    /// Floor this area belongs to
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor_id: Option<String>,

    /// Label IDs
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last modified timestamp
    #[serde(default = "Utc::now")]
    pub modified_at: DateTime<Utc>,
}

impl AreaEntry {
    /// Create a new area entry
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let now = Utc::now();
        Self {
            id: ulid::Ulid::new().to_string().to_lowercase(),
            normalized_name: Some(normalize_name(&name)),
            name,
            picture: None,
            icon: None,
            aliases: Vec::new(),
            floor_id: None,
            labels: Vec::new(),
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

/// Area registry data for storage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AreaRegistryData {
    /// All registered areas
    pub areas: Vec<AreaEntry>,
}

impl Storable for AreaRegistryData {
    const KEY: &'static str = STORAGE_KEY;
    const VERSION: u32 = STORAGE_VERSION;
    const MINOR_VERSION: u32 = STORAGE_MINOR_VERSION;
}

/// Area Registry
///
/// Entries are stored as `Arc<AreaEntry>` to avoid cloning on reads.
pub struct AreaRegistry {
    /// Storage backend
    storage: Arc<Storage>,

    /// Primary index: area_id -> AreaEntry (Arc-wrapped)
    by_id: DashMap<String, Arc<AreaEntry>>,

    /// Index: normalized_name -> area_id
    by_name: DashMap<String, String>,

    /// Index: floor_id -> set of area_ids
    by_floor_id: DashMap<String, HashSet<String>>,
}

impl AreaRegistry {
    /// Create a new area registry
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            by_id: DashMap::new(),
            by_name: DashMap::new(),
            by_floor_id: DashMap::new(),
        }
    }

    /// Load from storage
    pub async fn load(&self) -> StorageResult<()> {
        if let Some(storage_file) = self.storage.load::<AreaRegistryData>(STORAGE_KEY).await? {
            info!(
                "Loading {} areas from storage (v{}.{})",
                storage_file.data.areas.len(),
                storage_file.version,
                storage_file.minor_version
            );

            for entry in storage_file.data.areas {
                self.index_entry(Arc::new(entry));
            }
        }
        Ok(())
    }

    /// Save to storage
    pub async fn save(&self) -> StorageResult<()> {
        let data = AreaRegistryData {
            areas: self.by_id.iter().map(|r| (**r.value()).clone()).collect(),
        };

        let storage_file =
            StorageFile::new(STORAGE_KEY, data, STORAGE_VERSION, STORAGE_MINOR_VERSION);

        self.storage.save(&storage_file).await?;
        debug!("Saved {} areas to storage", self.by_id.len());
        Ok(())
    }

    /// Index an entry (takes Arc to avoid cloning)
    fn index_entry(&self, entry: Arc<AreaEntry>) {
        let area_id = entry.id.clone();

        if let Some(ref normalized) = entry.normalized_name {
            self.by_name.insert(normalized.clone(), area_id.clone());
        }

        if let Some(ref floor_id) = entry.floor_id {
            self.by_floor_id
                .entry(floor_id.clone())
                .or_default()
                .insert(area_id.clone());
        }

        self.by_id.insert(area_id, entry);
    }

    /// Remove an entry from indexes
    fn unindex_entry(&self, entry: &AreaEntry) {
        if let Some(ref normalized) = entry.normalized_name {
            self.by_name.remove(normalized);
        }

        if let Some(ref floor_id) = entry.floor_id {
            if let Some(mut ids) = self.by_floor_id.get_mut(floor_id) {
                ids.remove(&entry.id);
            }
        }

        self.by_id.remove(&entry.id);
    }

    /// Get area by ID
    ///
    /// Returns an `Arc<AreaEntry>` - cheap to clone.
    pub fn get(&self, area_id: &str) -> Option<Arc<AreaEntry>> {
        self.by_id.get(area_id).map(|r| Arc::clone(r.value()))
    }

    /// Get area by name
    pub fn get_by_name(&self, name: &str) -> Option<Arc<AreaEntry>> {
        let normalized = normalize_name(name);
        self.by_name
            .get(&normalized)
            .and_then(|area_id| self.get(&area_id))
    }

    /// Get all areas on a floor
    pub fn get_by_floor_id(&self, floor_id: &str) -> Vec<Arc<AreaEntry>> {
        self.by_floor_id
            .get(floor_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Create a new area
    ///
    /// Returns an `Arc<AreaEntry>` - cheap to clone.
    pub fn create(&self, name: &str) -> Arc<AreaEntry> {
        let entry = AreaEntry::new(name);
        let arc_entry = Arc::new(entry);
        info!("Created area: {} ({})", name, arc_entry.id);
        self.index_entry(Arc::clone(&arc_entry));
        arc_entry
    }

    /// Update an area
    ///
    /// Returns the updated entry as `Arc<AreaEntry>`.
    pub fn update<F>(&self, area_id: &str, f: F) -> Option<Arc<AreaEntry>>
    where
        F: FnOnce(&mut AreaEntry),
    {
        // Remove first to avoid deadlock
        if let Some((_, arc_entry)) = self.by_id.remove(area_id) {
            // Clone the inner entry for modification
            let mut entry = (*arc_entry).clone();

            // Unindex from secondary indexes
            if let Some(ref normalized) = entry.normalized_name {
                self.by_name.remove(normalized);
            }
            if let Some(ref floor_id) = entry.floor_id {
                if let Some(mut ids) = self.by_floor_id.get_mut(floor_id) {
                    ids.remove(&entry.id);
                }
            }

            // Apply update
            f(&mut entry);
            entry.normalized_name = Some(normalize_name(&entry.name));
            entry.modified_at = Utc::now();

            // Re-index with new Arc
            let new_arc = Arc::new(entry);
            self.index_entry(Arc::clone(&new_arc));

            Some(new_arc)
        } else {
            None
        }
    }

    /// Remove an area
    ///
    /// Returns the removed entry as `Arc<AreaEntry>`.
    pub fn remove(&self, area_id: &str) -> Option<Arc<AreaEntry>> {
        if let Some((_, arc_entry)) = self.by_id.remove(area_id) {
            self.unindex_entry(&arc_entry);
            info!("Removed area: {}", area_id);
            Some(arc_entry)
        } else {
            None
        }
    }

    /// Get count of areas
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Iterate over all areas
    ///
    /// Returns `Arc<AreaEntry>` references - cheap to clone.
    pub fn iter(&self) -> impl Iterator<Item = Arc<AreaEntry>> + '_ {
        self.by_id.iter().map(|r| Arc::clone(r.value()))
    }
}

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive AreaRegistry testing through Python bindings
