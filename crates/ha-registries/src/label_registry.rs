//! Label Registry
//!
//! Tracks all registered labels for organizing entities and devices.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::error::{RegistryError, RegistryResult};
use crate::storage::{Storable, Storage, StorageFile, StorageResult};

/// Storage key for label registry
pub const STORAGE_KEY: &str = "core.label_registry";
/// Current storage version
pub const STORAGE_VERSION: u32 = 1;
/// Current minor version
pub const STORAGE_MINOR_VERSION: u32 = 2;

/// A registered label entry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelEntry {
    /// Internal UUID (stored as "label_id" in HA storage)
    #[serde(alias = "label_id")]
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
    /// Create a new label entry with an explicit ID and timestamp
    pub fn new(id: impl Into<String>, name: impl Into<String>, now: Option<DateTime<Utc>>) -> Self {
        let name = name.into();
        let now = now.unwrap_or_else(Utc::now);
        Self {
            id: id.into(),
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
///
/// Entries are stored as `Arc<LabelEntry>` to avoid cloning on reads.
pub struct LabelRegistry {
    /// Storage backend
    storage: Arc<Storage>,

    /// Primary index: label_id -> LabelEntry (Arc-wrapped)
    by_id: DashMap<String, Arc<LabelEntry>>,

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
                self.index_entry(Arc::new(entry));
            }
        }
        Ok(())
    }

    /// Save to storage
    pub async fn save(&self) -> StorageResult<()> {
        let data = LabelRegistryData {
            labels: self.by_id.iter().map(|r| (**r.value()).clone()).collect(),
        };

        let storage_file =
            StorageFile::new(STORAGE_KEY, data, STORAGE_VERSION, STORAGE_MINOR_VERSION);

        self.storage.save(&storage_file).await?;
        debug!("Saved {} labels to storage", self.by_id.len());
        Ok(())
    }

    /// Index an entry (takes Arc to avoid cloning)
    fn index_entry(&self, entry: Arc<LabelEntry>) {
        let label_id = entry.id.clone();

        if let Some(ref normalized) = entry.normalized_name {
            self.by_name.insert(normalized.clone(), label_id.clone());
        }

        self.by_id.insert(label_id, entry);
    }

    /// Remove an entry from indexes
    fn unindex_entry(&self, entry: &LabelEntry) {
        if let Some(ref normalized) = entry.normalized_name {
            self.by_name.remove(normalized);
        }
        self.by_id.remove(&entry.id);
    }

    /// Get label by ID
    ///
    /// Returns an `Arc<LabelEntry>` - cheap to clone.
    pub fn get(&self, label_id: &str) -> Option<Arc<LabelEntry>> {
        self.by_id.get(label_id).map(|r| Arc::clone(r.value()))
    }

    /// Get label by name
    pub fn get_by_name(&self, name: &str) -> Option<Arc<LabelEntry>> {
        let normalized = normalize_name(name);
        self.by_name
            .get(&normalized)
            .and_then(|label_id| self.get(&label_id))
    }

    /// Create a new label
    ///
    /// Returns an `Arc<LabelEntry>` - cheap to clone.
    /// Returns `Err` if a label with the same name already exists.
    /// If `now` is None, uses the current system time.
    pub fn create(
        &self,
        name: &str,
        now: Option<DateTime<Utc>>,
    ) -> RegistryResult<Arc<LabelEntry>> {
        let normalized = normalize_name(name);
        if self.by_name.contains_key(&normalized) {
            return Err(RegistryError::DuplicateName {
                name: name.to_owned(),
                normalized,
            });
        }

        let id = self.generate_id(name);
        let entry = LabelEntry::new(id, name, now);
        let arc_entry = Arc::new(entry);
        info!("Created label: {} ({})", name, arc_entry.id);
        self.index_entry(Arc::clone(&arc_entry));
        Ok(arc_entry)
    }

    /// Generate a unique ID from a name (slugified, with suffix for conflicts)
    pub fn generate_id(&self, name: &str) -> String {
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

    /// Create a new label with builder pattern
    ///
    /// Returns an `Arc<LabelEntry>` - cheap to clone.
    /// Returns `Err` if a label with the same name already exists.
    pub fn create_with(&self, entry: LabelEntry) -> RegistryResult<Arc<LabelEntry>> {
        let normalized = normalize_name(&entry.name);
        if self.by_name.contains_key(&normalized) {
            return Err(RegistryError::DuplicateName {
                name: entry.name.clone(),
                normalized,
            });
        }

        let arc_entry = Arc::new(entry);
        info!("Created label: {} ({})", arc_entry.name, arc_entry.id);
        self.index_entry(Arc::clone(&arc_entry));
        Ok(arc_entry)
    }

    /// Update a label
    ///
    /// Returns the updated entry as `Arc<LabelEntry>`.
    /// Returns `Err` if the new name conflicts with another label.
    /// Only updates `modified_at` if the entry actually changed.
    /// If `now` is None, uses the current system time for modified_at.
    pub fn update<F>(
        &self,
        label_id: &str,
        f: F,
        now: Option<DateTime<Utc>>,
    ) -> RegistryResult<Arc<LabelEntry>>
    where
        F: FnOnce(&mut LabelEntry),
    {
        // Remove first to avoid deadlock
        if let Some((_, arc_entry)) = self.by_id.remove(label_id) {
            // Clone the inner entry for modification
            let mut entry = (*arc_entry).clone();
            let old_entry = entry.clone();

            // Unindex from secondary indexes
            if let Some(ref normalized) = entry.normalized_name {
                self.by_name.remove(normalized);
            }

            // Apply update
            f(&mut entry);
            entry.normalized_name = Some(normalize_name(&entry.name));

            // Check for name conflict with another label
            if entry.name != old_entry.name {
                let new_normalized = normalize_name(&entry.name);
                if self.by_name.contains_key(&new_normalized) {
                    // Name conflict - re-index the old entry and return error
                    self.index_entry(arc_entry);
                    return Err(RegistryError::DuplicateName {
                        name: entry.name.clone(),
                        normalized: new_normalized,
                    });
                }
            }

            // Only update modified_at if something actually changed
            let changed = entry.name != old_entry.name
                || entry.icon != old_entry.icon
                || entry.color != old_entry.color
                || entry.description != old_entry.description;
            if changed {
                entry.modified_at = now.unwrap_or_else(Utc::now);
            }

            // Re-index with new Arc
            let new_arc = Arc::new(entry);
            self.index_entry(Arc::clone(&new_arc));

            Ok(new_arc)
        } else {
            Err(RegistryError::NotFound {
                kind: "Label",
                id: label_id.to_owned(),
            })
        }
    }

    /// Remove a label
    ///
    /// Returns the removed entry as `Arc<LabelEntry>`.
    pub fn remove(&self, label_id: &str) -> Option<Arc<LabelEntry>> {
        if let Some((_, arc_entry)) = self.by_id.remove(label_id) {
            self.unindex_entry(&arc_entry);
            info!("Removed label: {}", label_id);
            Some(arc_entry)
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
    ///
    /// Returns `Arc<LabelEntry>` references - cheap to clone.
    pub fn iter(&self) -> impl Iterator<Item = Arc<LabelEntry>> + '_ {
        self.by_id.iter().map(|r| Arc::clone(r.value()))
    }

    /// Get all labels sorted by name
    ///
    /// Returns `Arc<LabelEntry>` references - cheap to clone.
    pub fn sorted_by_name(&self) -> Vec<Arc<LabelEntry>> {
        let mut labels: Vec<_> = self.iter().collect();
        labels.sort_by(|a, b| a.name.cmp(&b.name));
        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::RegistryError;
    use crate::storage::Storage;

    fn test_registry() -> LabelRegistry {
        let storage = Arc::new(Storage::new("/tmp/ha-test-label"));
        LabelRegistry::new(storage)
    }

    #[test]
    fn create_label_returns_entry() {
        let registry = test_registry();
        let result = registry.create("Important", None);
        assert!(result.is_ok(), "create should succeed: {:?}", result.err());
        let entry = result.unwrap();
        assert_eq!(entry.name, "Important");
    }

    #[test]
    fn create_duplicate_name_returns_duplicate_error() {
        let registry = test_registry();
        registry.create("Outdoor", None).unwrap();
        let err = registry.create("Outdoor", None).unwrap_err();
        assert!(
            matches!(err, RegistryError::DuplicateName { ref name, .. } if name == "Outdoor"),
            "Expected DuplicateName(Outdoor), got: {:?}",
            err
        );
    }

    #[test]
    fn create_with_duplicate_name_returns_duplicate_error() {
        let registry = test_registry();
        registry.create("Indoor", None).unwrap();
        let entry = LabelEntry::new("custom_id".to_string(), "Indoor", None);
        let err = registry.create_with(entry).unwrap_err();
        assert!(
            matches!(err, RegistryError::DuplicateName { ref name, .. } if name == "Indoor"),
            "Expected DuplicateName(Indoor), got: {:?}",
            err
        );
    }

    #[test]
    fn update_nonexistent_label_returns_not_found() {
        let registry = test_registry();
        let err = registry.update("nonexistent", |_| {}, None).unwrap_err();
        assert!(
            matches!(err, RegistryError::NotFound { kind: "Label", ref id } if id == "nonexistent"),
            "Expected NotFound(Label, nonexistent), got: {:?}",
            err
        );
    }

    #[test]
    fn update_with_conflicting_name_returns_duplicate_error() {
        let registry = test_registry();
        registry.create("Tag A", None).unwrap();
        registry.create("Tag B", None).unwrap();
        let err = registry
            .update("tag_b", |entry| entry.name = "Tag A".to_string(), None)
            .unwrap_err();
        assert!(
            matches!(err, RegistryError::DuplicateName { ref name, .. } if name == "Tag A"),
            "Expected DuplicateName(Tag A), got: {:?}",
            err
        );
    }

    #[test]
    fn get_returns_label_by_id() {
        let registry = test_registry();
        let created = registry.create("Critical", None).unwrap();
        let fetched = registry.get(&created.id).expect("should find label");
        assert_eq!(fetched.name, "Critical");
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let registry = test_registry();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn get_by_name_is_case_insensitive() {
        let registry = test_registry();
        registry.create("Urgent", None).unwrap();
        let found = registry.get_by_name("urgent").expect("should find by name");
        assert_eq!(found.name, "Urgent");
        let found2 = registry
            .get_by_name("URGENT")
            .expect("should find by uppercase");
        assert_eq!(found2.name, "Urgent");
    }

    #[test]
    fn create_with_builder_succeeds() {
        let registry = test_registry();
        let entry = LabelEntry::new("custom_id", "Custom Label", None)
            .with_icon("mdi:star")
            .with_color("#ff0000")
            .with_description("A custom label");
        let created = registry.create_with(entry).unwrap();
        assert_eq!(created.id, "custom_id");
        assert_eq!(created.name, "Custom Label");
        assert_eq!(created.icon.as_deref(), Some("mdi:star"));
        assert_eq!(created.color.as_deref(), Some("#ff0000"));
        assert_eq!(created.description.as_deref(), Some("A custom label"));
    }

    #[test]
    fn update_changes_fields_and_modified_at() {
        let registry = test_registry();
        let created = registry.create("Info", None).unwrap();
        let original_modified = created.modified_at;

        std::thread::sleep(std::time::Duration::from_millis(10));
        let updated = registry
            .update(
                &created.id,
                |entry| entry.color = Some("#0000ff".to_string()),
                None,
            )
            .unwrap();

        assert_eq!(updated.color.as_deref(), Some("#0000ff"));
        assert!(
            updated.modified_at > original_modified,
            "modified_at should advance on change"
        );
    }

    #[test]
    fn update_unchanged_does_not_update_modified_at() {
        let registry = test_registry();
        let created = registry.create("Static", None).unwrap();
        let original_modified = created.modified_at;

        let updated = registry.update(&created.id, |_| {}, None).unwrap();
        assert_eq!(
            updated.modified_at, original_modified,
            "modified_at should not change when nothing changed"
        );
    }

    #[test]
    fn remove_returns_entry_and_decrements_count() {
        let registry = test_registry();
        let created = registry.create("Temporary", None).unwrap();
        assert_eq!(registry.len(), 1);

        let removed = registry.remove(&created.id).expect("should remove");
        assert_eq!(removed.name, "Temporary");
        assert_eq!(registry.len(), 0);
        assert!(registry.get(&created.id).is_none());
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let registry = test_registry();
        assert!(registry.remove("nonexistent").is_none());
    }

    #[test]
    fn len_and_is_empty() {
        let registry = test_registry();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        registry.create("X", None).unwrap();
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn sorted_by_name_returns_alphabetical() {
        let registry = test_registry();
        registry.create("Zebra", None).unwrap();
        registry.create("Alpha", None).unwrap();
        registry.create("Middle", None).unwrap();

        let sorted = registry.sorted_by_name();
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].name, "Alpha");
        assert_eq!(sorted[1].name, "Middle");
        assert_eq!(sorted[2].name, "Zebra");
    }

    #[test]
    fn generate_id_handles_conflicts() {
        let registry = test_registry();
        let first = registry.create("My Label", None).unwrap();
        assert_eq!(first.id, "my_label");

        // Force second entry with conflicting slug
        let entry2 = LabelEntry::new("my_label_2", "My Label 2", None);
        registry.index_entry(Arc::new(entry2));

        let id = registry.generate_id("My Label");
        assert!(
            id.starts_with("my_label_"),
            "should generate suffixed id, got: {}",
            id
        );
    }

    #[test]
    fn iter_yields_all_entries() {
        let registry = test_registry();
        registry.create("A", None).unwrap();
        registry.create("B", None).unwrap();
        registry.create("C", None).unwrap();

        let all: Vec<_> = registry.iter().collect();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn normalize_name_lowercases_and_removes_spaces() {
        assert_eq!(normalize_name("My Label"), "mylabel");
        assert_eq!(normalize_name("URGENT"), "urgent");
    }

    #[test]
    fn slugify_creates_url_safe_ids() {
        assert_eq!(slugify("My Label"), "my_label");
        assert_eq!(slugify("Tag!@#"), "tag");
    }

    #[test]
    fn label_entry_builder_methods() {
        let entry = LabelEntry::new("id1", "Test", None)
            .with_icon("mdi:tag")
            .with_color("#aabbcc")
            .with_description("A test label");
        assert_eq!(entry.icon.as_deref(), Some("mdi:tag"));
        assert_eq!(entry.color.as_deref(), Some("#aabbcc"));
        assert_eq!(entry.description.as_deref(), Some("A test label"));
    }
}
