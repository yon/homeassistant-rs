//! Label Registry
//!
//! Tracks all registered labels for organizing entities and devices.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

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
    ) -> Result<Arc<LabelEntry>, String> {
        let normalized = normalize_name(name);
        if self.by_name.contains_key(&normalized) {
            return Err(format!(
                "The name {} ({}) is already in use",
                name, normalized
            ));
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
    pub fn create_with(&self, entry: LabelEntry) -> Result<Arc<LabelEntry>, String> {
        let normalized = normalize_name(&entry.name);
        if self.by_name.contains_key(&normalized) {
            return Err(format!(
                "The name {} ({}) is already in use",
                entry.name, normalized
            ));
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
    ) -> Result<Arc<LabelEntry>, String>
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
                    return Err(format!(
                        "The name {} ({}) is already in use",
                        entry.name, new_normalized
                    ));
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
            Err(format!("Label not found: {}", label_id))
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

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive LabelRegistry testing through Python bindings
