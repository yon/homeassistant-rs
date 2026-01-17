//! Device Registry
//!
//! Tracks all registered devices with identifiers, connections,
//! and multiple indexes for fast lookups.

use crate::entity_registry::DisabledBy;
use crate::storage::{Storage, StorageFile, StorageResult, Storable};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, info};

/// Storage key for device registry
pub const STORAGE_KEY: &str = "core.device_registry";
/// Current storage version
pub const STORAGE_VERSION: u32 = 1;
/// Current minor version
pub const STORAGE_MINOR_VERSION: u32 = 12;

/// Device entry type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceEntryType {
    /// Service device (virtual)
    Service,
}

/// A device identifier (domain, id) pair
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceIdentifier(pub String, pub String);

impl DeviceIdentifier {
    pub fn new(domain: impl Into<String>, id: impl Into<String>) -> Self {
        Self(domain.into(), id.into())
    }

    pub fn domain(&self) -> &str {
        &self.0
    }

    pub fn id(&self) -> &str {
        &self.1
    }

    /// Create a key for indexing
    pub fn key(&self) -> String {
        format!("{}:{}", self.0, self.1)
    }
}

/// A device connection (type, id) pair
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceConnection(pub String, pub String);

impl DeviceConnection {
    pub fn new(conn_type: impl Into<String>, id: impl Into<String>) -> Self {
        Self(conn_type.into(), id.into())
    }

    pub fn connection_type(&self) -> &str {
        &self.0
    }

    pub fn id(&self) -> &str {
        &self.1
    }

    /// Create a key for indexing
    pub fn key(&self) -> String {
        format!("{}:{}", self.0, self.1)
    }
}

/// A registered device entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEntry {
    /// Internal UUID
    pub id: String,

    /// Unique identifiers by domain (e.g., [["hue", "bridge123"]])
    #[serde(default)]
    pub identifiers: Vec<DeviceIdentifier>,

    /// Connection info (e.g., [["mac", "AA:BB:CC:DD:EE:FF"]])
    #[serde(default)]
    pub connections: Vec<DeviceConnection>,

    /// Associated config entries
    #[serde(default)]
    pub config_entries: Vec<String>,

    /// Config entry to subentries mapping
    #[serde(default)]
    pub config_entries_subentries: HashMap<String, Vec<Option<String>>>,

    /// Primary config entry ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_config_entry: Option<String>,

    /// Device name
    #[serde(default)]
    pub name: String,

    /// User-set name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_by_user: Option<String>,

    /// Manufacturer name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,

    /// Model name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Manufacturer model ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,

    /// Hardware version
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hw_version: Option<String>,

    /// Software/firmware version
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sw_version: Option<String>,

    /// Serial number
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,

    /// Parent device (for nested devices)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_device_id: Option<String>,

    /// Entry type (service, virtual, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_type: Option<DeviceEntryType>,

    /// Disable reason
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_by: Option<DisabledBy>,

    /// URL for device configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_url: Option<String>,

    /// Assigned area
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_id: Option<String>,

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

impl DeviceEntry {
    /// Create a new device entry
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: ulid::Ulid::new().to_string().to_lowercase(),
            identifiers: Vec::new(),
            connections: Vec::new(),
            config_entries: Vec::new(),
            config_entries_subentries: HashMap::new(),
            primary_config_entry: None,
            name: name.into(),
            name_by_user: None,
            manufacturer: None,
            model: None,
            model_id: None,
            hw_version: None,
            sw_version: None,
            serial_number: None,
            via_device_id: None,
            entry_type: None,
            disabled_by: None,
            configuration_url: None,
            area_id: None,
            labels: Vec::new(),
            created_at: now,
            modified_at: now,
        }
    }

    /// Get display name (user name or device name)
    pub fn display_name(&self) -> &str {
        self.name_by_user.as_deref().unwrap_or(&self.name)
    }

    /// Check if device is disabled
    pub fn is_disabled(&self) -> bool {
        self.disabled_by.is_some()
    }

    /// Add an identifier
    pub fn with_identifier(mut self, domain: impl Into<String>, id: impl Into<String>) -> Self {
        self.identifiers.push(DeviceIdentifier::new(domain, id));
        self
    }

    /// Add a connection
    pub fn with_connection(
        mut self,
        conn_type: impl Into<String>,
        id: impl Into<String>,
    ) -> Self {
        self.connections.push(DeviceConnection::new(conn_type, id));
        self
    }

    /// Add a config entry
    pub fn with_config_entry(mut self, config_entry_id: impl Into<String>) -> Self {
        let id = config_entry_id.into();
        if self.primary_config_entry.is_none() {
            self.primary_config_entry = Some(id.clone());
        }
        if !self.config_entries.contains(&id) {
            self.config_entries.push(id);
        }
        self
    }
}

/// Device registry data for storage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceRegistryData {
    /// All registered devices
    pub devices: Vec<DeviceEntry>,
    /// Soft-deleted devices
    #[serde(default)]
    pub deleted_devices: Vec<DeviceEntry>,
}

impl Storable for DeviceRegistryData {
    const KEY: &'static str = STORAGE_KEY;
    const VERSION: u32 = STORAGE_VERSION;
    const MINOR_VERSION: u32 = STORAGE_MINOR_VERSION;
}

/// Device Registry with multi-index support
///
/// Provides O(1) lookups by:
/// - id (primary)
/// - identifier
/// - connection
/// - config_entry_id (multi)
/// - area_id (multi)
/// - via_device_id (multi)
pub struct DeviceRegistry {
    /// Storage backend
    storage: Arc<Storage>,

    /// Primary index: device_id -> DeviceEntry
    by_id: DashMap<String, DeviceEntry>,

    /// Index: identifier key -> device_id
    by_identifier: DashMap<String, String>,

    /// Index: connection key -> device_id
    by_connection: DashMap<String, String>,

    /// Index: config_entry_id -> set of device_ids
    by_config_entry_id: DashMap<String, HashSet<String>>,

    /// Index: area_id -> set of device_ids
    by_area_id: DashMap<String, HashSet<String>>,

    /// Index: via_device_id -> set of device_ids (child devices)
    by_via_device_id: DashMap<String, HashSet<String>>,

    /// Deleted devices (soft delete)
    deleted: DashMap<String, DeviceEntry>,
}

impl DeviceRegistry {
    /// Create a new device registry
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            by_id: DashMap::new(),
            by_identifier: DashMap::new(),
            by_connection: DashMap::new(),
            by_config_entry_id: DashMap::new(),
            by_area_id: DashMap::new(),
            by_via_device_id: DashMap::new(),
            deleted: DashMap::new(),
        }
    }

    /// Load from storage
    pub async fn load(&self) -> StorageResult<()> {
        if let Some(storage_file) = self.storage.load::<DeviceRegistryData>(STORAGE_KEY).await? {
            info!(
                "Loading {} devices from storage (v{}.{})",
                storage_file.data.devices.len(),
                storage_file.version,
                storage_file.minor_version
            );

            for entry in storage_file.data.devices {
                self.index_entry(&entry);
            }

            for entry in storage_file.data.deleted_devices {
                self.deleted.insert(entry.id.clone(), entry);
            }
        }
        Ok(())
    }

    /// Save to storage
    pub async fn save(&self) -> StorageResult<()> {
        let data = DeviceRegistryData {
            devices: self.by_id.iter().map(|r| r.value().clone()).collect(),
            deleted_devices: self.deleted.iter().map(|r| r.value().clone()).collect(),
        };

        let storage_file =
            StorageFile::new(STORAGE_KEY, data, STORAGE_VERSION, STORAGE_MINOR_VERSION);

        self.storage.save(&storage_file).await?;
        debug!("Saved {} devices to storage", self.by_id.len());
        Ok(())
    }

    /// Index an entry in all indexes
    fn index_entry(&self, entry: &DeviceEntry) {
        let device_id = entry.id.clone();

        // Primary index
        self.by_id.insert(device_id.clone(), entry.clone());

        // Identifier indexes
        for identifier in &entry.identifiers {
            self.by_identifier
                .insert(identifier.key(), device_id.clone());
        }

        // Connection indexes
        for connection in &entry.connections {
            self.by_connection
                .insert(connection.key(), device_id.clone());
        }

        // config_entry_id index
        for config_entry_id in &entry.config_entries {
            self.by_config_entry_id
                .entry(config_entry_id.clone())
                .or_default()
                .insert(device_id.clone());
        }

        // area_id index
        if let Some(ref area_id) = entry.area_id {
            self.by_area_id
                .entry(area_id.clone())
                .or_default()
                .insert(device_id.clone());
        }

        // via_device_id index
        if let Some(ref via_device_id) = entry.via_device_id {
            self.by_via_device_id
                .entry(via_device_id.clone())
                .or_default()
                .insert(device_id);
        }
    }

    /// Remove an entry from all indexes
    fn unindex_entry(&self, entry: &DeviceEntry) {
        let device_id = &entry.id;

        // Remove from identifier indexes
        for identifier in &entry.identifiers {
            self.by_identifier.remove(&identifier.key());
        }

        // Remove from connection indexes
        for connection in &entry.connections {
            self.by_connection.remove(&connection.key());
        }

        // Remove from config_entry_id index
        for config_entry_id in &entry.config_entries {
            if let Some(mut ids) = self.by_config_entry_id.get_mut(config_entry_id) {
                ids.remove(device_id);
            }
        }

        // Remove from area_id index
        if let Some(ref area_id) = entry.area_id {
            if let Some(mut ids) = self.by_area_id.get_mut(area_id) {
                ids.remove(device_id);
            }
        }

        // Remove from via_device_id index
        if let Some(ref via_device_id) = entry.via_device_id {
            if let Some(mut ids) = self.by_via_device_id.get_mut(via_device_id) {
                ids.remove(device_id);
            }
        }

        // Remove from primary index
        self.by_id.remove(device_id);
    }

    /// Get device by ID
    pub fn get(&self, device_id: &str) -> Option<DeviceEntry> {
        self.by_id.get(device_id).map(|r| r.value().clone())
    }

    /// Get device by identifier
    pub fn get_by_identifier(&self, domain: &str, id: &str) -> Option<DeviceEntry> {
        let key = format!("{}:{}", domain, id);
        self.by_identifier
            .get(&key)
            .and_then(|device_id| self.get(&device_id))
    }

    /// Get device by connection
    pub fn get_by_connection(&self, conn_type: &str, id: &str) -> Option<DeviceEntry> {
        let key = format!("{}:{}", conn_type, id);
        self.by_connection
            .get(&key)
            .and_then(|device_id| self.get(&device_id))
    }

    /// Get all devices for a config entry
    pub fn get_by_config_entry_id(&self, config_entry_id: &str) -> Vec<DeviceEntry> {
        self.by_config_entry_id
            .get(config_entry_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all devices in an area
    pub fn get_by_area_id(&self, area_id: &str) -> Vec<DeviceEntry> {
        self.by_area_id
            .get(area_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all child devices (connected via this device)
    pub fn get_children(&self, device_id: &str) -> Vec<DeviceEntry> {
        self.by_via_device_id
            .get(device_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get or create a device
    ///
    /// Looks up by identifiers first, then connections. Creates new if not found.
    pub fn get_or_create(
        &self,
        identifiers: &[DeviceIdentifier],
        connections: &[DeviceConnection],
        config_entry_id: Option<&str>,
        name: &str,
    ) -> DeviceEntry {
        // Check by identifiers
        for identifier in identifiers {
            if let Some(existing) = self.get_by_identifier(identifier.domain(), identifier.id()) {
                debug!("Found existing device by identifier: {}", existing.id);
                return existing;
            }
        }

        // Check by connections
        for connection in connections {
            if let Some(existing) =
                self.get_by_connection(connection.connection_type(), connection.id())
            {
                debug!("Found existing device by connection: {}", existing.id);
                return existing;
            }
        }

        // Create new device
        let mut entry = DeviceEntry::new(name);
        entry.identifiers = identifiers.to_vec();
        entry.connections = connections.to_vec();

        if let Some(config_id) = config_entry_id {
            entry.config_entries.push(config_id.to_string());
            entry.primary_config_entry = Some(config_id.to_string());
        }

        self.index_entry(&entry);

        info!("Registered new device: {} ({})", name, entry.id);
        entry
    }

    /// Update a device entry
    pub fn update<F>(&self, device_id: &str, f: F) -> Option<DeviceEntry>
    where
        F: FnOnce(&mut DeviceEntry),
    {
        // Remove first to avoid deadlock
        if let Some((_, mut entry)) = self.by_id.remove(device_id) {
            // Unindex from secondary indexes
            for identifier in &entry.identifiers {
                self.by_identifier.remove(&identifier.key());
            }
            for connection in &entry.connections {
                self.by_connection.remove(&connection.key());
            }
            for config_entry_id in &entry.config_entries {
                if let Some(mut ids) = self.by_config_entry_id.get_mut(config_entry_id) {
                    ids.remove(&entry.id);
                }
            }
            if let Some(ref area_id) = entry.area_id {
                if let Some(mut ids) = self.by_area_id.get_mut(area_id) {
                    ids.remove(&entry.id);
                }
            }
            if let Some(ref via_device_id) = entry.via_device_id {
                if let Some(mut ids) = self.by_via_device_id.get_mut(via_device_id) {
                    ids.remove(&entry.id);
                }
            }

            // Apply update
            f(&mut entry);
            entry.modified_at = Utc::now();

            // Re-index
            self.index_entry(&entry);

            Some(entry)
        } else {
            None
        }
    }

    /// Remove a device
    pub fn remove(&self, device_id: &str) -> Option<DeviceEntry> {
        if let Some((_, entry)) = self.by_id.remove(device_id) {
            self.unindex_entry(&entry);
            // Add to deleted for tracking
            self.deleted.insert(device_id.to_string(), entry.clone());
            info!("Removed device: {}", device_id);
            Some(entry)
        } else {
            None
        }
    }

    /// Get all device IDs
    pub fn device_ids(&self) -> Vec<String> {
        self.by_id.iter().map(|r| r.key().clone()).collect()
    }

    /// Get count of registered devices
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Iterate over all entries
    pub fn iter(&self) -> impl Iterator<Item = DeviceEntry> + '_ {
        self.by_id.iter().map(|r| r.value().clone())
    }

    /// Clear all entries (for testing)
    #[cfg(test)]
    pub fn clear(&self) {
        self.by_id.clear();
        self.by_identifier.clear();
        self.by_connection.clear();
        self.by_config_entry_id.clear();
        self.by_area_id.clear();
        self.by_via_device_id.clear();
        self.deleted.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_registry() -> (TempDir, DeviceRegistry) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path()));
        let registry = DeviceRegistry::new(storage);
        (temp_dir, registry)
    }

    #[test]
    fn test_device_entry_new() {
        let entry = DeviceEntry::new("Test Device")
            .with_identifier("hue", "bridge123")
            .with_connection("mac", "AA:BB:CC:DD:EE:FF")
            .with_config_entry("config1");

        assert_eq!(entry.name, "Test Device");
        assert_eq!(entry.identifiers.len(), 1);
        assert_eq!(entry.connections.len(), 1);
        assert_eq!(entry.config_entries.len(), 1);
        assert_eq!(entry.primary_config_entry, Some("config1".to_string()));
    }

    #[test]
    fn test_get_or_create_new() {
        let (_dir, registry) = create_test_registry();

        let identifiers = vec![DeviceIdentifier::new("hue", "bridge123")];
        let connections = vec![DeviceConnection::new("mac", "AA:BB:CC:DD:EE:FF")];

        let entry = registry.get_or_create(&identifiers, &connections, Some("config1"), "Hue Bridge");

        assert_eq!(entry.name, "Hue Bridge");
        assert_eq!(entry.identifiers.len(), 1);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_get_or_create_existing_identifier() {
        let (_dir, registry) = create_test_registry();

        let identifiers = vec![DeviceIdentifier::new("hue", "bridge123")];

        // Create first device
        let first = registry.get_or_create(&identifiers, &[], Some("config1"), "First");

        // Try to create with same identifier
        let second = registry.get_or_create(&identifiers, &[], Some("config2"), "Second");

        // Should return existing
        assert_eq!(first.id, second.id);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_get_or_create_existing_connection() {
        let (_dir, registry) = create_test_registry();

        let connections = vec![DeviceConnection::new("mac", "AA:BB:CC:DD:EE:FF")];

        // Create first device
        let first = registry.get_or_create(&[], &connections, Some("config1"), "First");

        // Try to create with same connection
        let second = registry.get_or_create(&[], &connections, Some("config2"), "Second");

        // Should return existing
        assert_eq!(first.id, second.id);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_indexes() {
        let (_dir, registry) = create_test_registry();

        // Create devices with various relationships
        let dev1 = registry.get_or_create(
            &[DeviceIdentifier::new("hue", "bridge1")],
            &[],
            Some("config1"),
            "Bridge 1",
        );

        registry.get_or_create(
            &[DeviceIdentifier::new("hue", "light1")],
            &[],
            Some("config1"),
            "Light 1",
        );

        // Update to set via_device_id and area
        registry.update(&dev1.id, |d| {
            d.area_id = Some("living_room".to_string());
        });

        // Test by_identifier
        let by_id = registry.get_by_identifier("hue", "bridge1").unwrap();
        assert_eq!(by_id.name, "Bridge 1");

        // Test by_config_entry_id
        let config1_devices = registry.get_by_config_entry_id("config1");
        assert_eq!(config1_devices.len(), 2);

        // Test by_area_id
        let area_devices = registry.get_by_area_id("living_room");
        assert_eq!(area_devices.len(), 1);
    }

    #[test]
    fn test_via_device_hierarchy() {
        let (_dir, registry) = create_test_registry();

        // Create hub device
        let hub = registry.get_or_create(
            &[DeviceIdentifier::new("hue", "hub")],
            &[],
            Some("config1"),
            "Hue Hub",
        );

        // Create child devices
        for i in 1..=3 {
            let child = registry.get_or_create(
                &[DeviceIdentifier::new("hue", format!("light{}", i))],
                &[],
                Some("config1"),
                &format!("Light {}", i),
            );

            registry.update(&child.id, |d| {
                d.via_device_id = Some(hub.id.clone());
            });
        }

        // Get children
        let children = registry.get_children(&hub.id);
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn test_remove() {
        let (_dir, registry) = create_test_registry();

        let entry = registry.get_or_create(
            &[DeviceIdentifier::new("hue", "test")],
            &[DeviceConnection::new("mac", "AA:BB")],
            Some("config1"),
            "Test",
        );

        let removed = registry.remove(&entry.id).unwrap();
        assert_eq!(removed.id, entry.id);

        // Should be gone from all indexes
        assert!(registry.get(&entry.id).is_none());
        assert!(registry.get_by_identifier("hue", "test").is_none());
        assert!(registry.get_by_connection("mac", "AA:BB").is_none());
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path()));

        // Create and populate registry
        {
            let registry = DeviceRegistry::new(storage.clone());
            registry.get_or_create(
                &[DeviceIdentifier::new("hue", "bridge")],
                &[DeviceConnection::new("mac", "AA:BB:CC")],
                Some("config1"),
                "Hue Bridge",
            );
            registry.save().await.unwrap();
        }

        // Load into new registry
        {
            let registry = DeviceRegistry::new(storage);
            registry.load().await.unwrap();

            assert_eq!(registry.len(), 1);
            let entry = registry.get_by_identifier("hue", "bridge").unwrap();
            assert_eq!(entry.name, "Hue Bridge");
        }
    }
}
