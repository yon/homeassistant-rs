//! Config Entry types
//!
//! A ConfigEntry represents a single instance of an integration's configuration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::state_machine::InvalidTransition;

/// Config entry lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConfigEntryState {
    /// Initial state, not yet set up
    #[default]
    NotLoaded,
    /// Currently being configured (non-recoverable)
    SetupInProgress,
    /// Successfully set up (recoverable)
    Loaded,
    /// Setup failed (recoverable)
    SetupError,
    /// Waiting to retry setup (recoverable)
    SetupRetry,
    /// Version migration failed (not recoverable)
    MigrationError,
    /// Currently unloading (non-recoverable)
    UnloadInProgress,
    /// Unload failed (not recoverable)
    FailedUnload,
}

impl ConfigEntryState {
    /// Check if the entry can be unloaded/reloaded from this state
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            ConfigEntryState::Loaded
                | ConfigEntryState::SetupError
                | ConfigEntryState::SetupRetry
                | ConfigEntryState::NotLoaded
        )
    }
}

/// Source of the config entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConfigEntrySource {
    /// Configured via UI/API
    #[default]
    User,
    /// Imported from YAML config
    Import,
    /// Generic discovery
    Discovery,
    /// DHCP discovery
    Dhcp,
    /// UPnP/SSDP discovery
    Ssdp,
    /// mDNS/Bonjour discovery
    Zeroconf,
    /// Bluetooth device discovery
    Bluetooth,
    /// MQTT announcement discovery
    Mqtt,
    /// Philips Hue-style discovery
    Nupnp,
    /// Home Assistant add-on
    Hassio,
    /// HomeKit accessory discovery
    Homekit,
    /// User hiding a discovery
    Ignore,
    /// Re-authentication flow
    Reauth,
    /// User reconfiguring existing entry
    Reconfigure,
    /// System-created entry
    System,
    /// Device registration (e.g., mobile app)
    Registration,
    /// Integration-triggered discovery
    IntegrationDiscovery,
}

/// Reason an entry was disabled
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigEntryDisabledBy {
    /// Disabled by the user
    User,
}

/// A configuration entry for an integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    /// Unique identifier (ULID)
    pub entry_id: String,

    /// Integration domain (e.g., "hue", "mqtt")
    pub domain: String,

    /// Human-readable display name
    pub title: String,

    /// Immutable configuration data
    #[serde(default)]
    pub data: HashMap<String, serde_json::Value>,

    /// User-configurable options
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,

    /// Major schema version
    #[serde(default = "default_version")]
    pub version: u32,

    /// Minor schema version
    #[serde(default = "default_minor_version")]
    pub minor_version: u32,

    /// Optional unique identifier for duplicate prevention
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,

    /// Origin type
    #[serde(default)]
    pub source: ConfigEntrySource,

    /// Current lifecycle state (not persisted)
    #[serde(skip, default)]
    pub state: ConfigEntryState,

    /// Human-readable explanation for failed states
    #[serde(skip, default)]
    pub reason: Option<String>,

    /// Per-entry setup/unload lock (not persisted)
    /// Wrapped in Arc so ConfigEntry can still be Clone
    #[serde(skip)]
    pub setup_lock: Arc<Mutex<()>>,

    /// Number of setup retry attempts (not persisted)
    #[serde(skip, default)]
    pub tries: u32,

    /// Prevent auto-entity creation
    #[serde(default)]
    pub pref_disable_new_entities: bool,

    /// Disable background polling
    #[serde(default)]
    pub pref_disable_polling: bool,

    /// What disabled this entry
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_by: Option<ConfigEntryDisabledBy>,

    /// Maps discovery protocols to their identifiers
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub discovery_keys: HashMap<String, serde_json::Value>,

    /// Hierarchical sub-configurations
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subentries: Vec<serde_json::Value>,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last modification timestamp
    #[serde(default = "Utc::now")]
    pub modified_at: DateTime<Utc>,
}

fn default_version() -> u32 {
    1
}

fn default_minor_version() -> u32 {
    1
}

impl ConfigEntry {
    /// Create a new config entry
    pub fn new(domain: impl Into<String>, title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            entry_id: ulid::Ulid::new().to_string(),
            domain: domain.into(),
            title: title.into(),
            data: HashMap::new(),
            options: HashMap::new(),
            version: 1,
            minor_version: 1,
            unique_id: None,
            source: ConfigEntrySource::User,
            state: ConfigEntryState::NotLoaded,
            reason: None,
            setup_lock: Arc::new(Mutex::new(())),
            tries: 0,
            pref_disable_new_entities: false,
            pref_disable_polling: false,
            disabled_by: None,
            discovery_keys: HashMap::new(),
            subentries: Vec::new(),
            created_at: now,
            modified_at: now,
        }
    }

    /// Set entry data
    pub fn with_data(mut self, data: HashMap<String, serde_json::Value>) -> Self {
        self.data = data;
        self
    }

    /// Set entry options
    pub fn with_options(mut self, options: HashMap<String, serde_json::Value>) -> Self {
        self.options = options;
        self
    }

    /// Set unique_id
    pub fn with_unique_id(mut self, unique_id: impl Into<String>) -> Self {
        self.unique_id = Some(unique_id.into());
        self
    }

    /// Set source
    pub fn with_source(mut self, source: ConfigEntrySource) -> Self {
        self.source = source;
        self
    }

    /// Set version
    pub fn with_version(mut self, version: u32, minor_version: u32) -> Self {
        self.version = version;
        self.minor_version = minor_version;
        self
    }

    /// Check if entry is disabled
    pub fn is_disabled(&self) -> bool {
        self.disabled_by.is_some()
    }

    /// Check if entry is loaded
    pub fn is_loaded(&self) -> bool {
        self.state == ConfigEntryState::Loaded
    }

    /// Check if entry supports unload
    pub fn supports_unload(&self) -> bool {
        self.state.is_recoverable()
    }

    /// Attempt to transition to a new state with validation.
    ///
    /// Returns an error if the transition is invalid according to the FSM rules.
    /// On success, updates the state and reason fields.
    pub fn try_set_state(
        &mut self,
        new_state: ConfigEntryState,
        reason: Option<String>,
    ) -> Result<(), InvalidTransition> {
        // Validate the transition
        self.state.try_transition(new_state)?;

        // Apply the transition
        self.state = new_state;
        self.reason = reason;

        // Reset tries counter on non-retry states
        if !matches!(
            new_state,
            ConfigEntryState::SetupRetry | ConfigEntryState::SetupInProgress
        ) {
            self.tries = 0;
        }

        Ok(())
    }

    /// Increment the retry counter and return the new count
    pub fn increment_tries(&mut self) -> u32 {
        self.tries += 1;
        self.tries
    }
}

/// Update data for a config entry
#[derive(Debug, Default)]
pub struct ConfigEntryUpdate {
    pub title: Option<String>,
    pub data: Option<HashMap<String, serde_json::Value>>,
    pub options: Option<HashMap<String, serde_json::Value>>,
    pub unique_id: Option<Option<String>>,
    pub version: Option<u32>,
    pub minor_version: Option<u32>,
    pub pref_disable_new_entities: Option<bool>,
    pub pref_disable_polling: Option<bool>,
}

impl ConfigEntryUpdate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn data(mut self, data: HashMap<String, serde_json::Value>) -> Self {
        self.data = Some(data);
        self
    }

    pub fn options(mut self, options: HashMap<String, serde_json::Value>) -> Self {
        self.options = Some(options);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_entry_new() {
        let entry = ConfigEntry::new("hue", "Philips Hue");
        assert_eq!(entry.domain, "hue");
        assert_eq!(entry.title, "Philips Hue");
        assert_eq!(entry.state, ConfigEntryState::NotLoaded);
        assert_eq!(entry.version, 1);
        assert!(!entry.entry_id.is_empty());
    }

    #[test]
    fn test_config_entry_builder() {
        let mut data = HashMap::new();
        data.insert("host".to_string(), serde_json::json!("192.168.1.1"));

        let entry = ConfigEntry::new("hue", "Philips Hue")
            .with_data(data)
            .with_unique_id("bridge-001")
            .with_source(ConfigEntrySource::Discovery);

        assert_eq!(entry.unique_id, Some("bridge-001".to_string()));
        assert_eq!(entry.source, ConfigEntrySource::Discovery);
        assert!(entry.data.contains_key("host"));
    }

    #[test]
    fn test_state_recoverable() {
        assert!(ConfigEntryState::NotLoaded.is_recoverable());
        assert!(ConfigEntryState::Loaded.is_recoverable());
        assert!(ConfigEntryState::SetupError.is_recoverable());
        assert!(ConfigEntryState::SetupRetry.is_recoverable());

        assert!(!ConfigEntryState::SetupInProgress.is_recoverable());
        assert!(!ConfigEntryState::MigrationError.is_recoverable());
        assert!(!ConfigEntryState::UnloadInProgress.is_recoverable());
        assert!(!ConfigEntryState::FailedUnload.is_recoverable());
    }

    #[test]
    fn test_serde_roundtrip() {
        let entry = ConfigEntry::new("test", "Test Entry")
            .with_unique_id("test-123")
            .with_source(ConfigEntrySource::Import);

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: ConfigEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.domain, "test");
        assert_eq!(parsed.title, "Test Entry");
        assert_eq!(parsed.unique_id, Some("test-123".to_string()));
        assert_eq!(parsed.source, ConfigEntrySource::Import);
    }
}
