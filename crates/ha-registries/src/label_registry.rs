//! Label Registry
//!
//! Tracks all registered labels for organizing entities and devices.

use crate::storage::{Storage, StorageFile, StorageResult, Storable};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

/// Storage key for label registry
pub const STORAGE_KEY: &str = "core.label_registry";
/// Current storage version
pub const STORAGE_VERSION: u32 = 1;
/// Current minor version
pub const STORAGE_MINOR_VERSION: u32 = 2;

/// A registered label entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelEntry {
    /// Internal UUID (also used as label_id)
    pub id: String,

    /// Label name (e.g., "Critical", "Outdoor")
    pub name: String,

    /// Normalized name for searching
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_name: Option<String>,

    /// Label icon (e.g., "mdi:tag")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Label color (hex color code)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    /// Label description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last modified timestamp
    #[serde(default = "Utc::now")]
    pub modified_at: DateTime<Utc>,
}

impl LabelEntry {
    /// Create a new label entry
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let now = Utc::now();
        Self {
            id: ulid::Ulid::new().to_string().to_lowercase(),
            normalized_name: Some(normalize_name(&name)),
            name,
            icon: None,
            color: None,
            description: None,
            created_at: now,
            modified_at: now,
        }
    }

    /// Set icon
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Set color
    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Normalize a name for searching
fn normalize_name(name: &str) -> String {
    name.to_lowercase()
        .trim()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
}

/// Label registry data for storage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LabelRegistryData {
    /// All registered labels
    pub labels: Vec<LabelEntry>,
}

impl Storable for LabelRegistryData {
    const KEY: &'static str = STORAGE_KEY;
    const VERSION: u32 = STORAGE_VERSION;
    const MINOR_VERSION: u32 = STORAGE_MINOR_VERSION;
}

/// Label Registry
pub struct LabelRegistry {
    /// Storage backend
    storage: Arc<Storage>,

    /// Primary index: label_id -> LabelEntry
    by_id: DashMap<String, LabelEntry>,

    /// Index: normalized_name -> label_id
    by_name: DashMap<String, String>,
}

impl LabelRegistry {
    /// Create a new label registry
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            by_id: DashMap::new(),
            by_name: DashMap::new(),
        }
    }

    /// Load from storage
    pub async fn load(&self) -> StorageResult<()> {
        if let Some(storage_file) = self.storage.load::<LabelRegistryData>(STORAGE_KEY).await? {
            info!(
                "Loading {} labels from storage (v{}.{})",
                storage_file.data.labels.len(),
                storage_file.version,
                storage_file.minor_version
            );

            for entry in storage_file.data.labels {
                self.index_entry(&entry);
            }
        }
        Ok(())
    }

    /// Save to storage
    pub async fn save(&self) -> StorageResult<()> {
        let data = LabelRegistryData {
            labels: self.by_id.iter().map(|r| r.value().clone()).collect(),
        };

        let storage_file =
            StorageFile::new(STORAGE_KEY, data, STORAGE_VERSION, STORAGE_MINOR_VERSION);

        self.storage.save(&storage_file).await?;
        debug!("Saved {} labels to storage", self.by_id.len());
        Ok(())
    }

    /// Index an entry
    fn index_entry(&self, entry: &LabelEntry) {
        let label_id = entry.id.clone();

        self.by_id.insert(label_id.clone(), entry.clone());

        if let Some(ref normalized) = entry.normalized_name {
            self.by_name.insert(normalized.clone(), label_id);
        }
    }

    /// Remove an entry from indexes
    fn unindex_entry(&self, entry: &LabelEntry) {
        if let Some(ref normalized) = entry.normalized_name {
            self.by_name.remove(normalized);
        }
        self.by_id.remove(&entry.id);
    }

    /// Get label by ID
    pub fn get(&self, label_id: &str) -> Option<LabelEntry> {
        self.by_id.get(label_id).map(|r| r.value().clone())
    }

    /// Get label by name
    pub fn get_by_name(&self, name: &str) -> Option<LabelEntry> {
        let normalized = normalize_name(name);
        self.by_name
            .get(&normalized)
            .and_then(|label_id| self.get(&label_id))
    }

    /// Create a new label
    pub fn create(&self, name: &str) -> LabelEntry {
        let entry = LabelEntry::new(name);
        self.index_entry(&entry);
        info!("Created label: {} ({})", name, entry.id);
        entry
    }

    /// Create a new label with builder pattern
    pub fn create_with(&self, entry: LabelEntry) -> LabelEntry {
        self.index_entry(&entry);
        info!("Created label: {} ({})", entry.name, entry.id);
        entry
    }

    /// Update a label
    pub fn update<F>(&self, label_id: &str, f: F) -> Option<LabelEntry>
    where
        F: FnOnce(&mut LabelEntry),
    {
        // Remove first to avoid deadlock
        if let Some((_, mut entry)) = self.by_id.remove(label_id) {
            // Unindex from secondary indexes
            if let Some(ref normalized) = entry.normalized_name {
                self.by_name.remove(normalized);
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

    /// Remove a label
    pub fn remove(&self, label_id: &str) -> Option<LabelEntry> {
        if let Some((_, entry)) = self.by_id.remove(label_id) {
            self.unindex_entry(&entry);
            info!("Removed label: {}", label_id);
            Some(entry)
        } else {
            None
        }
    }

    /// Get count of labels
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Iterate over all labels
    pub fn iter(&self) -> impl Iterator<Item = LabelEntry> + '_ {
        self.by_id.iter().map(|r| r.value().clone())
    }

    /// Get all labels sorted by name
    pub fn sorted_by_name(&self) -> Vec<LabelEntry> {
        let mut labels: Vec<_> = self.iter().collect();
        labels.sort_by(|a, b| a.name.cmp(&b.name));
        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_registry() -> (TempDir, LabelRegistry) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path()));
        let registry = LabelRegistry::new(storage);
        (temp_dir, registry)
    }

    #[test]
    fn test_create_label() {
        let (_dir, registry) = create_test_registry();

        let label = registry.create("Critical");
        assert_eq!(label.name, "Critical");
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_create_with_builder() {
        let (_dir, registry) = create_test_registry();

        let label = LabelEntry::new("Critical")
            .with_icon("mdi:alert")
            .with_color("#FF0000")
            .with_description("Critical systems");

        let created = registry.create_with(label);
        assert_eq!(created.icon, Some("mdi:alert".to_string()));
        assert_eq!(created.color, Some("#FF0000".to_string()));
    }

    #[test]
    fn test_get_by_name() {
        let (_dir, registry) = create_test_registry();

        registry.create("Critical");

        let label = registry.get_by_name("critical").unwrap();
        assert_eq!(label.name, "Critical");
    }

    #[test]
    fn test_sorted_by_name() {
        let (_dir, registry) = create_test_registry();

        registry.create("Zebra");
        registry.create("Alpha");
        registry.create("Middle");

        let sorted = registry.sorted_by_name();
        assert_eq!(sorted[0].name, "Alpha");
        assert_eq!(sorted[1].name, "Middle");
        assert_eq!(sorted[2].name, "Zebra");
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path()));

        {
            let registry = LabelRegistry::new(storage.clone());
            let label = LabelEntry::new("Critical")
                .with_color("#FF0000");
            registry.create_with(label);
            registry.save().await.unwrap();
        }

        {
            let registry = LabelRegistry::new(storage);
            registry.load().await.unwrap();

            assert_eq!(registry.len(), 1);
            let label = registry.get_by_name("critical").unwrap();
            assert_eq!(label.color, Some("#FF0000".to_string()));
        }
    }
}
