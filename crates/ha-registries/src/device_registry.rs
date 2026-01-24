//! Device Registry
//!
//! Tracks all registered devices with identifiers, connections,
//! and multiple indexes for fast lookups.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::entity_registry::DisabledBy;
use crate::storage::{Storable, Storage, StorageFile, StorageResult};

/// Storage key for device registry
pub const STORAGE_KEY: &str = "core.device_registry";
pub const CONNECTION_NETWORK_MAC: &str = "mac";
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
/// The id can be either a string or an integer in the JSON, but is stored as String
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct DeviceIdentifier(pub String, pub String);

impl<'de> Deserialize<'de> for DeviceIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, SeqAccess, Visitor};

        struct DeviceIdentifierVisitor;

        impl<'de> Visitor<'de> for DeviceIdentifierVisitor {
            type Value = DeviceIdentifier;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a tuple of [domain, id, ...] where id parts are joined")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let domain: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;

                // Collect all remaining elements as ID parts
                // Some integrations use 3+ element tuples like ["homekit", "id", "bridge"]
                let mut id_parts: Vec<String> = Vec::new();
                while let Some(value) = seq.next_element::<serde_json::Value>()? {
                    let part = match value {
                        serde_json::Value::String(s) => s,
                        serde_json::Value::Number(n) => n.to_string(),
                        _ => return Err(de::Error::custom("id parts must be string or number")),
                    };
                    id_parts.push(part);
                }

                if id_parts.is_empty() {
                    return Err(de::Error::invalid_length(1, &self));
                }

                // Join multiple ID parts with colon separator
                let id = id_parts.join(":");

                Ok(DeviceIdentifier(domain, id))
            }
        }

        deserializer.deserialize_seq(DeviceIdentifierVisitor)
    }
}

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

    /// Create a normalized connection (MAC addresses lowercased and formatted)
    pub fn normalized(conn_type: impl Into<String>, id: impl Into<String>) -> Self {
        let ct = conn_type.into();
        let raw_id = id.into();
        let normalized_id = if ct == CONNECTION_NETWORK_MAC {
            format_mac(&raw_id)
        } else {
            raw_id
        };
        Self(ct, normalized_id)
    }
}

/// Format a MAC address string for storage (matches HA's format_mac).
/// Normalizes to lowercase colon-separated format.
pub fn format_mac(mac: &str) -> String {
    let to_test = mac;

    // Already colon-separated (17 chars, 5 colons) - just lowercase
    if to_test.len() == 17 && to_test.chars().filter(|c| *c == ':').count() == 5 {
        return to_test.to_lowercase();
    }

    // Dash-separated (17 chars, 5 dashes) - remove dashes, format
    let stripped = if to_test.len() == 17 && to_test.chars().filter(|c| *c == '-').count() == 5 {
        to_test.replace('-', "")
    } else if to_test.len() == 14 && to_test.chars().filter(|c| *c == '.').count() == 2 {
        // Dot-separated (14 chars, 2 dots) - remove dots, format
        to_test.replace('.', "")
    } else if to_test.len() == 12 && to_test.chars().all(|c| c.is_ascii_hexdigit()) {
        // No separators (12 hex chars) - format with colons
        to_test.to_string()
    } else {
        // Unknown format - return as-is
        return mac.to_string();
    };

    // Format as colon-separated lowercase
    stripped
        .to_lowercase()
        .as_bytes()
        .chunks(2)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
        .collect::<Vec<_>>()
        .join(":")
}

/// Normalize a slice of connections (MAC addresses formatted to lowercase)
pub fn normalize_connections(connections: &[DeviceConnection]) -> Vec<DeviceConnection> {
    connections
        .iter()
        .map(|c| DeviceConnection::normalized(c.connection_type(), c.id()))
        .collect()
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

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

    /// Suggested area name (informational, used during creation)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_area: Option<String>,

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

    /// Insertion order (for stable iteration when timestamps are equal)
    #[serde(skip)]
    pub insertion_order: u64,
}

impl DeviceEntry {
    /// Create a new device entry with the current time
    pub fn new(name: Option<&str>) -> Self {
        Self::new_at(name, Utc::now())
    }

    /// Create a new device entry with a specific timestamp
    pub fn new_at(name: Option<&str>, now: DateTime<Utc>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            identifiers: Vec::new(),
            connections: Vec::new(),
            config_entries: Vec::new(),
            config_entries_subentries: HashMap::new(),
            primary_config_entry: None,
            name: name.map(|s| s.to_string()),
            name_by_user: None,
            manufacturer: None,
            model: None,
            model_id: None,
            hw_version: None,
            sw_version: None,
            serial_number: None,
            suggested_area: None,
            via_device_id: None,
            entry_type: None,
            disabled_by: None,
            configuration_url: None,
            area_id: None,
            labels: Vec::new(),
            created_at: now,
            modified_at: now,
            insertion_order: 0,
        }
    }

    /// Get display name (user name or device name)
    pub fn display_name(&self) -> &str {
        self.name_by_user
            .as_deref()
            .or(self.name.as_deref())
            .unwrap_or("")
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
    pub fn with_connection(mut self, conn_type: impl Into<String>, id: impl Into<String>) -> Self {
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
///
/// Entries are stored as `Arc<DeviceEntry>` to avoid cloning on reads.
pub struct DeviceRegistry {
    /// Storage backend
    storage: Arc<Storage>,

    /// Primary index: device_id -> DeviceEntry (Arc-wrapped)
    by_id: DashMap<String, Arc<DeviceEntry>>,

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

    /// Deleted devices (soft delete, Arc-wrapped)
    deleted: DashMap<String, Arc<DeviceEntry>>,

    /// Counter for insertion ordering
    insertion_counter: AtomicU64,
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
            insertion_counter: AtomicU64::new(0),
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

            // Sort by created_at for stable insertion order on load
            let mut devices = storage_file.data.devices;
            devices.sort_by_key(|e| e.created_at);
            for mut entry in devices {
                entry.insertion_order = self.insertion_counter.fetch_add(1, Ordering::Relaxed);
                self.index_entry(Arc::new(entry));
            }

            for entry in storage_file.data.deleted_devices {
                self.deleted.insert(entry.id.clone(), Arc::new(entry));
            }
        }
        Ok(())
    }

    /// Save to storage
    pub async fn save(&self) -> StorageResult<()> {
        let data = DeviceRegistryData {
            devices: self.by_id.iter().map(|r| (**r.value()).clone()).collect(),
            deleted_devices: self.deleted.iter().map(|r| (**r.value()).clone()).collect(),
        };

        let storage_file =
            StorageFile::new(STORAGE_KEY, data, STORAGE_VERSION, STORAGE_MINOR_VERSION);

        self.storage.save(&storage_file).await?;
        debug!("Saved {} devices to storage", self.by_id.len());
        Ok(())
    }

    /// Index an entry in all indexes
    ///
    /// Takes an `Arc<DeviceEntry>` to avoid cloning.
    fn index_entry(&self, entry: Arc<DeviceEntry>) {
        let device_id = entry.id.clone();

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
                .insert(device_id.clone());
        }

        // Primary index (insert Arc directly)
        self.by_id.insert(device_id, entry);
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
    ///
    /// Returns an `Arc<DeviceEntry>` - cheap to clone (atomic increment).
    pub fn get(&self, device_id: &str) -> Option<Arc<DeviceEntry>> {
        self.by_id.get(device_id).map(|r| Arc::clone(r.value()))
    }

    /// Get device by identifier
    pub fn get_by_identifier(&self, domain: &str, id: &str) -> Option<Arc<DeviceEntry>> {
        let key = format!("{}:{}", domain, id);
        self.by_identifier
            .get(&key)
            .and_then(|device_id| self.get(&device_id))
    }

    /// Get device by connection
    pub fn get_by_connection(&self, conn_type: &str, id: &str) -> Option<Arc<DeviceEntry>> {
        // Normalize the connection value for lookup (e.g., MAC addresses to lowercase)
        let normalized_id = if conn_type == CONNECTION_NETWORK_MAC {
            format_mac(id)
        } else {
            id.to_string()
        };
        let key = format!("{}:{}", conn_type, normalized_id);
        self.by_connection
            .get(&key)
            .and_then(|device_id| self.get(&device_id))
    }

    /// Get a device by any of its identifiers or connections
    pub fn get_by_identifiers_or_connections(
        &self,
        identifiers: &[DeviceIdentifier],
        connections: &[DeviceConnection],
    ) -> Option<Arc<DeviceEntry>> {
        // Check identifiers first
        for ident in identifiers {
            if let Some(entry) = self.get_by_identifier(ident.domain(), ident.id()) {
                return Some(entry);
            }
        }
        // Check connections
        for conn in connections {
            if let Some(entry) = self.get_by_connection(conn.connection_type(), conn.id()) {
                return Some(entry);
            }
        }
        None
    }

    /// Get all devices for a config entry
    pub fn get_by_config_entry_id(&self, config_entry_id: &str) -> Vec<Arc<DeviceEntry>> {
        self.by_config_entry_id
            .get(config_entry_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all devices in an area
    pub fn get_by_area_id(&self, area_id: &str) -> Vec<Arc<DeviceEntry>> {
        self.by_area_id
            .get(area_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all child devices (connected via this device)
    pub fn get_children(&self, device_id: &str) -> Vec<Arc<DeviceEntry>> {
        self.by_via_device_id
            .get(device_id)
            .map(|ids| ids.iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get or create a device
    ///
    /// Looks up by identifiers first, then connections. Creates new if not found.
    /// Returns an `Arc<DeviceEntry>` - cheap to clone.
    pub fn get_or_create(
        &self,
        identifiers: &[DeviceIdentifier],
        connections: &[DeviceConnection],
        config_entry_id: Option<&str>,
        config_subentry_id: Option<Option<&str>>,
        name: Option<&str>,
        timestamp: Option<DateTime<Utc>>,
    ) -> Arc<DeviceEntry> {
        // Normalize connections (e.g., MAC addresses to lowercase)
        let connections = normalize_connections(connections);
        let connections = connections.as_slice();

        // Determine the subentry value to add
        // config_subentry_id: None = not specified (default to None subentry)
        //                     Some(None) = explicitly None subentry
        //                     Some(Some("id")) = specific subentry id
        let subentry_val: Option<String> = match config_subentry_id {
            None => None,                           // Default: None subentry
            Some(None) => None,                     // Explicit None subentry
            Some(Some(id)) => Some(id.to_string()), // Specific subentry
        };

        // Check by identifiers
        for identifier in identifiers {
            if let Some(existing) = self.get_by_identifier(identifier.domain(), identifier.id()) {
                debug!("Found existing device by identifier: {}", existing.id);
                let device_id = existing.id.clone();
                let (ce_needs_add, subentry_needs_add, conns_to_add, idents_to_add) = self
                    .compute_merge_needs(
                        &existing,
                        config_entry_id,
                        &subentry_val,
                        connections,
                        identifiers,
                    );
                if ce_needs_add
                    || subentry_needs_add
                    || !conns_to_add.is_empty()
                    || !idents_to_add.is_empty()
                {
                    if let Some(updated) = self.update_at(
                        &device_id,
                        |e| {
                            self.apply_merge(
                                e,
                                config_entry_id,
                                &subentry_val,
                                ce_needs_add,
                                subentry_needs_add,
                                conns_to_add.clone(),
                                idents_to_add.clone(),
                            );
                        },
                        timestamp,
                    ) {
                        return updated;
                    }
                }
                return existing;
            }
        }

        // Check by connections
        for connection in connections {
            if let Some(existing) =
                self.get_by_connection(connection.connection_type(), connection.id())
            {
                debug!("Found existing device by connection: {}", existing.id);
                let device_id = existing.id.clone();
                let (ce_needs_add, subentry_needs_add, conns_to_add, idents_to_add) = self
                    .compute_merge_needs(
                        &existing,
                        config_entry_id,
                        &subentry_val,
                        connections,
                        identifiers,
                    );
                if ce_needs_add
                    || subentry_needs_add
                    || !conns_to_add.is_empty()
                    || !idents_to_add.is_empty()
                {
                    if let Some(updated) = self.update_at(
                        &device_id,
                        |e| {
                            self.apply_merge(
                                e,
                                config_entry_id,
                                &subentry_val,
                                ce_needs_add,
                                subentry_needs_add,
                                conns_to_add.clone(),
                                idents_to_add.clone(),
                            );
                        },
                        timestamp,
                    ) {
                        return updated;
                    }
                }
                return existing;
            }
        }

        // Create new device
        let mut entry = DeviceEntry::new_at(name, timestamp.unwrap_or_else(Utc::now));
        entry.insertion_order = self.insertion_counter.fetch_add(1, Ordering::Relaxed);
        entry.identifiers = identifiers.to_vec();
        entry.connections = connections.to_vec();

        if let Some(config_id) = config_entry_id {
            entry.config_entries.push(config_id.to_string());
            // primary_config_entry is set by the Python layer based on device_info_type
            entry
                .config_entries_subentries
                .insert(config_id.to_string(), vec![subentry_val.clone()]);
        }

        let arc_entry = Arc::new(entry);
        self.index_entry(Arc::clone(&arc_entry));

        info!("Registered new device: {:?} ({})", name, arc_entry.id);
        arc_entry
    }

    /// Compute what needs to be merged into an existing device
    fn compute_merge_needs(
        &self,
        existing: &DeviceEntry,
        config_entry_id: Option<&str>,
        subentry_val: &Option<String>,
        connections: &[DeviceConnection],
        identifiers: &[DeviceIdentifier],
    ) -> (bool, bool, Vec<DeviceConnection>, Vec<DeviceIdentifier>) {
        let ce_needs_add = config_entry_id
            .map(|ce| !existing.config_entries.contains(&ce.to_string()))
            .unwrap_or(false);

        // Check if subentry needs to be added (config entry exists but subentry is new)
        let subentry_needs_add = if !ce_needs_add {
            if let Some(ce_id) = config_entry_id {
                existing
                    .config_entries_subentries
                    .get(ce_id)
                    .map(|subs| !subs.contains(subentry_val))
                    .unwrap_or(true)
            } else {
                false
            }
        } else {
            false // Will be added with the config entry
        };

        let conns_to_add: Vec<_> = connections
            .iter()
            .filter(|c| !existing.connections.contains(c))
            .cloned()
            .collect();
        let idents_to_add: Vec<_> = identifiers
            .iter()
            .filter(|i| !existing.identifiers.contains(i))
            .cloned()
            .collect();

        (
            ce_needs_add,
            subentry_needs_add,
            conns_to_add,
            idents_to_add,
        )
    }

    /// Apply merge operations to a device entry
    #[allow(clippy::too_many_arguments)]
    fn apply_merge(
        &self,
        e: &mut DeviceEntry,
        config_entry_id: Option<&str>,
        subentry_val: &Option<String>,
        ce_needs_add: bool,
        subentry_needs_add: bool,
        conns_to_add: Vec<DeviceConnection>,
        idents_to_add: Vec<DeviceIdentifier>,
    ) {
        if let Some(ce_id) = config_entry_id {
            if ce_needs_add {
                e.config_entries.push(ce_id.to_string());
                e.config_entries_subentries
                    .insert(ce_id.to_string(), vec![subentry_val.clone()]);
            } else if subentry_needs_add {
                e.config_entries_subentries
                    .entry(ce_id.to_string())
                    .or_default()
                    .push(subentry_val.clone());
            }
        }
        e.connections.extend(conns_to_add);
        e.identifiers.extend(idents_to_add);
    }

    /// Update a device entry
    ///
    /// Returns the updated entry as `Arc<DeviceEntry>`.
    pub fn update_at<F>(
        &self,
        device_id: &str,
        f: F,
        timestamp: Option<DateTime<Utc>>,
    ) -> Option<Arc<DeviceEntry>>
    where
        F: FnOnce(&mut DeviceEntry),
    {
        // Remove first to avoid deadlock
        if let Some((_, arc_entry)) = self.by_id.remove(device_id) {
            // Clone the inner entry for modification
            let mut entry = (*arc_entry).clone();

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

            // Apply update - only set modified_at if data fields actually changed
            let old = entry.clone();
            f(&mut entry);
            let changed = old.identifiers != entry.identifiers
                || old.connections != entry.connections
                || old.config_entries != entry.config_entries
                || old.config_entries_subentries != entry.config_entries_subentries
                || old.primary_config_entry != entry.primary_config_entry
                || old.name != entry.name
                || old.name_by_user != entry.name_by_user
                || old.manufacturer != entry.manufacturer
                || old.model != entry.model
                || old.model_id != entry.model_id
                || old.hw_version != entry.hw_version
                || old.sw_version != entry.sw_version
                || old.serial_number != entry.serial_number
                || old.suggested_area != entry.suggested_area
                || old.via_device_id != entry.via_device_id
                || old.entry_type != entry.entry_type
                || old.disabled_by != entry.disabled_by
                || old.configuration_url != entry.configuration_url
                || old.area_id != entry.area_id
                || old.labels != entry.labels;
            if changed {
                entry.modified_at = timestamp.unwrap_or_else(Utc::now);
            }

            // Re-index with new Arc
            let new_arc = Arc::new(entry);
            self.index_entry(Arc::clone(&new_arc));

            Some(new_arc)
        } else {
            None
        }
    }

    /// Update a device entry using the current time
    pub fn update<F>(&self, device_id: &str, f: F) -> Option<Arc<DeviceEntry>>
    where
        F: FnOnce(&mut DeviceEntry),
    {
        self.update_at(device_id, f, None)
    }

    /// Remove a device
    ///
    /// Returns the removed entry as `Arc<DeviceEntry>`.
    pub fn remove(&self, device_id: &str) -> Option<Arc<DeviceEntry>> {
        if let Some((_, arc_entry)) = self.by_id.remove(device_id) {
            self.unindex_entry(&arc_entry);
            // Add to deleted for tracking
            self.deleted
                .insert(device_id.to_string(), Arc::clone(&arc_entry));
            info!("Removed device: {}", device_id);
            Some(arc_entry)
        } else {
            None
        }
    }

    /// Clear a config entry from all devices.
    ///
    /// For each device with this config_entry_id:
    /// - Removes the config_entry_id from the device's config_entries
    /// - Removes from config_entries_subentries
    /// - Updates primary_config_entry if it was the removed one
    /// - If the device has no remaining config entries, removes the device
    pub fn clear_config_entry(&self, config_entry_id: &str) {
        // Collect device IDs first to avoid holding locks during modification
        let device_ids: Vec<String> = self
            .get_by_config_entry_id(config_entry_id)
            .iter()
            .map(|d| d.id.clone())
            .collect();

        for device_id in device_ids {
            // Check if this is the only config entry
            let should_remove = if let Some(entry) = self.get(&device_id) {
                entry.config_entries.len() <= 1
            } else {
                continue;
            };

            if should_remove {
                self.remove(&device_id);
            } else {
                let ce_id = config_entry_id.to_string();
                self.update(&device_id, |entry| {
                    entry.config_entries.retain(|id| id != &ce_id);
                    entry.config_entries_subentries.remove(&ce_id);
                    if entry.primary_config_entry.as_deref() == Some(&ce_id) {
                        entry.primary_config_entry = entry.config_entries.first().cloned();
                    }
                });
            }
        }
    }

    /// Clear area_id from all devices that reference the given area_id.
    ///
    /// Returns the list of device IDs that were modified.
    pub fn clear_area_id(&self, area_id: &str) -> Vec<String> {
        let device_ids: Vec<String> = self
            .by_area_id
            .get(area_id)
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default();

        let mut modified = Vec::new();
        for device_id in &device_ids {
            self.update(device_id, |entry| {
                entry.area_id = None;
            });
            modified.push(device_id.clone());
        }
        modified
    }

    /// Clear a label from all devices that have it.
    ///
    /// Returns the list of device IDs that were modified.
    pub fn clear_label_id(&self, label_id: &str) -> Vec<String> {
        let device_ids: Vec<String> = self
            .by_id
            .iter()
            .filter(|r| r.value().labels.contains(&label_id.to_string()))
            .map(|r| r.key().clone())
            .collect();

        let mut modified = Vec::new();
        for device_id in &device_ids {
            self.update(device_id, |entry| {
                entry.labels.retain(|l| l != label_id);
            });
            modified.push(device_id.clone());
        }
        modified
    }

    /// Clear a config entry from all devices, returning change info.
    ///
    /// Returns `(removed_device_ids, updated_devices)` where:
    /// - `removed_device_ids`: devices that were deleted (had only this config entry)
    /// - `updated_devices`: `Vec<(device_id, changed_fields)>` for devices that were modified
    pub fn clear_config_entry_with_changes(
        &self,
        config_entry_id: &str,
    ) -> (Vec<String>, Vec<(String, Vec<String>)>) {
        let device_ids: Vec<String> = self
            .get_by_config_entry_id(config_entry_id)
            .iter()
            .map(|d| d.id.clone())
            .collect();

        let mut removed = Vec::new();
        let mut updated = Vec::new();

        for device_id in device_ids {
            let old_entry = match self.get(&device_id) {
                Some(e) => e,
                None => continue,
            };

            let should_remove = old_entry.config_entries.len() <= 1;

            if should_remove {
                self.remove(&device_id);
                removed.push(device_id);
            } else {
                let old = (*old_entry).clone();
                let ce_id = config_entry_id.to_string();
                self.update(&device_id, |entry| {
                    entry.config_entries.retain(|id| id != &ce_id);
                    entry.config_entries_subentries.remove(&ce_id);
                    if entry.primary_config_entry.as_deref() == Some(&ce_id) {
                        entry.primary_config_entry = entry.config_entries.first().cloned();
                    }
                });

                // Compute changed fields
                if let Some(new_entry) = self.get(&device_id) {
                    let changed = compute_device_changed_fields(&old, &new_entry);
                    if !changed.is_empty() {
                        updated.push((device_id, changed));
                    }
                }
            }
        }

        (removed, updated)
    }

    /// Clear via_device_id from all devices that reference the given device_id
    pub fn clear_via_device_id(&self, removed_device_id: &str) {
        let device_ids: Vec<String> = self
            .by_id
            .iter()
            .filter(|r| r.value().via_device_id.as_deref() == Some(removed_device_id))
            .map(|r| r.key().clone())
            .collect();

        for device_id in device_ids {
            self.update(&device_id, |entry| {
                entry.via_device_id = None;
            });
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
    ///
    /// Returns `Arc<DeviceEntry>` references - cheap to clone.
    pub fn iter(&self) -> impl Iterator<Item = Arc<DeviceEntry>> + '_ {
        self.by_id.iter().map(|r| Arc::clone(r.value()))
    }
}

/// Compare two DeviceEntry instances and return the list of field names that changed.
pub fn compute_device_changed_fields(old: &DeviceEntry, new: &DeviceEntry) -> Vec<String> {
    let mut changed = Vec::new();
    if old.area_id != new.area_id {
        changed.push("area_id".to_string());
    }
    if old.config_entries != new.config_entries {
        changed.push("config_entries".to_string());
    }
    if old.config_entries_subentries != new.config_entries_subentries {
        changed.push("config_entries_subentries".to_string());
    }
    if old.configuration_url != new.configuration_url {
        changed.push("configuration_url".to_string());
    }
    if old.connections != new.connections {
        changed.push("connections".to_string());
    }
    if old.disabled_by != new.disabled_by {
        changed.push("disabled_by".to_string());
    }
    if old.entry_type != new.entry_type {
        changed.push("entry_type".to_string());
    }
    if old.hw_version != new.hw_version {
        changed.push("hw_version".to_string());
    }
    if old.identifiers != new.identifiers {
        changed.push("identifiers".to_string());
    }
    if old.labels != new.labels {
        changed.push("labels".to_string());
    }
    if old.manufacturer != new.manufacturer {
        changed.push("manufacturer".to_string());
    }
    if old.model != new.model {
        changed.push("model".to_string());
    }
    if old.model_id != new.model_id {
        changed.push("model_id".to_string());
    }
    if old.name != new.name {
        changed.push("name".to_string());
    }
    if old.name_by_user != new.name_by_user {
        changed.push("name_by_user".to_string());
    }
    if old.primary_config_entry != new.primary_config_entry {
        changed.push("primary_config_entry".to_string());
    }
    if old.serial_number != new.serial_number {
        changed.push("serial_number".to_string());
    }
    if old.suggested_area != new.suggested_area {
        changed.push("suggested_area".to_string());
    }
    if old.sw_version != new.sw_version {
        changed.push("sw_version".to_string());
    }
    if old.via_device_id != new.via_device_id {
        changed.push("via_device_id".to_string());
    }
    changed
}

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive DeviceRegistry testing through Python bindings
