//! Area Registry
//!
//! Tracks all registered areas (rooms, zones) in the home.

use crate::storage::{Storage, StorageFile, StorageResult, Storable};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, info};

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
pub struct AreaRegistry {
    /// Storage backend
    storage: Arc<Storage>,

    /// Primary index: area_id -> AreaEntry
    by_id: DashMap<String, AreaEntry>,

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
                self.index_entry(&entry);
            }
        }
        Ok(())
    }

    /// Save to storage
    pub async fn save(&self) -> StorageResult<()> {
        let data = AreaRegistryData {
            areas: self.by_id.iter().map(|r| r.value().clone()).collect(),
        };

        let storage_file =
            StorageFile::new(STORAGE_KEY, data, STORAGE_VERSION, STORAGE_MINOR_VERSION);

        self.storage.save(&storage_file).await?;
        debug!("Saved {} areas to storage", self.by_id.len());
        Ok(())
    }

    /// Index an entry
    fn index_entry(&self, entry: &AreaEntry) {
        let area_id = entry.id.clone();

        self.by_id.insert(area_id.clone(), entry.clone());

        if let Some(ref normalized) = entry.normalized_name {
            self.by_name.insert(normalized.clone(), area_id.clone());
        }

        if let Some(ref floor_id) = entry.floor_id {
            self.by_floor_id
                .entry(floor_id.clone())
                .or_default()
                .insert(area_id);
        }
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
    pub fn get(&self, area_id: &str) -> Option<AreaEntry> {
        self.by_id.get(area_id).map(|r| r.value().clone())
    }

    /// Get area by name
    pub fn get_by_name(&self, name: &str) -> Option<AreaEntry> {
        let normalized = normalize_name(name);
        self.by_name
            .get(&normalized)
            .and_then(|area_id| self.get(&area_id))
    }

    /// Get all areas on a floor
    pub fn get_by_floor_id(&self, floor_id: &str) -> Vec<AreaEntry> {
        self.by_floor_id
            .get(floor_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Create a new area
    pub fn create(&self, name: &str) -> AreaEntry {
        let entry = AreaEntry::new(name);
        self.index_entry(&entry);
        info!("Created area: {} ({})", name, entry.id);
        entry
    }

    /// Update an area
    pub fn update<F>(&self, area_id: &str, f: F) -> Option<AreaEntry>
    where
        F: FnOnce(&mut AreaEntry),
    {
        // Remove first to avoid deadlock
        if let Some((_, mut entry)) = self.by_id.remove(area_id) {
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

            // Re-index
            self.index_entry(&entry);

            Some(entry)
        } else {
            None
        }
    }

    /// Remove an area
    pub fn remove(&self, area_id: &str) -> Option<AreaEntry> {
        if let Some((_, entry)) = self.by_id.remove(area_id) {
            self.unindex_entry(&entry);
            info!("Removed area: {}", area_id);
            Some(entry)
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
    pub fn iter(&self) -> impl Iterator<Item = AreaEntry> + '_ {
        self.by_id.iter().map(|r| r.value().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_registry() -> (TempDir, AreaRegistry) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path()));
        let registry = AreaRegistry::new(storage);
        (temp_dir, registry)
    }

    #[test]
    fn test_create_area() {
        let (_dir, registry) = create_test_registry();

        let area = registry.create("Living Room");
        assert_eq!(area.name, "Living Room");
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_get_by_name() {
        let (_dir, registry) = create_test_registry();

        registry.create("Living Room");

        // Should match case-insensitively
        let area = registry.get_by_name("living room").unwrap();
        assert_eq!(area.name, "Living Room");

        let area = registry.get_by_name("LIVING ROOM").unwrap();
        assert_eq!(area.name, "Living Room");
    }

    #[test]
    fn test_floor_index() {
        let (_dir, registry) = create_test_registry();

        let living = registry.create("Living Room");
        let bedroom = registry.create("Bedroom");

        registry.update(&living.id, |a| {
            a.floor_id = Some("floor1".to_string());
        });
        registry.update(&bedroom.id, |a| {
            a.floor_id = Some("floor1".to_string());
        });

        let floor_areas = registry.get_by_floor_id("floor1");
        assert_eq!(floor_areas.len(), 2);
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path()));

        {
            let registry = AreaRegistry::new(storage.clone());
            registry.create("Living Room");
            registry.save().await.unwrap();
        }

        {
            let registry = AreaRegistry::new(storage);
            registry.load().await.unwrap();

            assert_eq!(registry.len(), 1);
            let area = registry.get_by_name("living room").unwrap();
            assert_eq!(area.name, "Living Room");
        }
    }
}
