//! Mock config entry for testing
//!
//! Provides a configurable mock config entry similar to Python HA's MockConfigEntry.

use serde_json::Value;
use std::collections::HashMap;
use ulid::Ulid;

/// State of a config entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigEntryState {
    /// Entry has not been loaded yet
    NotLoaded,
    /// Entry is being set up
    SetupInProgress,
    /// Entry has been loaded successfully
    Loaded,
    /// Entry setup failed
    SetupError,
    /// Entry setup is being retried
    SetupRetry,
    /// Entry is being unloaded
    UnloadInProgress,
    /// Entry failed to unload
    FailedUnload,
}

impl Default for ConfigEntryState {
    fn default() -> Self {
        Self::NotLoaded
    }
}

/// A mock config entry for testing
#[derive(Debug, Clone)]
pub struct MockConfigEntry {
    /// Unique identifier for this entry
    pub entry_id: String,
    /// Integration domain
    pub domain: String,
    /// Display title
    pub title: String,
    /// Configuration data
    pub data: HashMap<String, Value>,
    /// User-configurable options
    pub options: HashMap<String, Value>,
    /// Unique ID for identifying the same config
    pub unique_id: Option<String>,
    /// Current state of the entry
    pub state: ConfigEntryState,
    /// Version of the entry schema
    pub version: u32,
    /// Minor version of the entry schema
    pub minor_version: u32,
    /// Source of the config entry
    pub source: String,
    /// Whether the entry is disabled
    pub disabled_by: Option<String>,
}

impl MockConfigEntry {
    /// Create a new mock config entry for a domain
    pub fn new(domain: impl Into<String>) -> Self {
        Self {
            entry_id: Ulid::new().to_string(),
            domain: domain.into(),
            title: "Mock Title".to_string(),
            data: HashMap::new(),
            options: HashMap::new(),
            unique_id: None,
            state: ConfigEntryState::NotLoaded,
            version: 1,
            minor_version: 1,
            source: "user".to_string(),
            disabled_by: None,
        }
    }

    /// Set the entry ID
    pub fn with_entry_id(mut self, entry_id: impl Into<String>) -> Self {
        self.entry_id = entry_id.into();
        self
    }

    /// Set the title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set the configuration data
    pub fn with_data(mut self, data: HashMap<String, Value>) -> Self {
        self.data = data;
        self
    }

    /// Set a single data value
    pub fn with_data_value(mut self, key: impl Into<String>, value: Value) -> Self {
        self.data.insert(key.into(), value);
        self
    }

    /// Set the options
    pub fn with_options(mut self, options: HashMap<String, Value>) -> Self {
        self.options = options;
        self
    }

    /// Set a single option value
    pub fn with_option(mut self, key: impl Into<String>, value: Value) -> Self {
        self.options.insert(key.into(), value);
        self
    }

    /// Set the unique ID
    pub fn with_unique_id(mut self, unique_id: impl Into<String>) -> Self {
        self.unique_id = Some(unique_id.into());
        self
    }

    /// Set the state
    pub fn with_state(mut self, state: ConfigEntryState) -> Self {
        self.state = state;
        self
    }

    /// Set the version
    pub fn with_version(mut self, version: u32, minor_version: u32) -> Self {
        self.version = version;
        self.minor_version = minor_version;
        self
    }

    /// Set the source
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    /// Disable the entry
    pub fn disabled(mut self, disabled_by: impl Into<String>) -> Self {
        self.disabled_by = Some(disabled_by.into());
        self
    }
}

impl Default for MockConfigEntry {
    fn default() -> Self {
        Self::new("test")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mock_config_entry_defaults() {
        let entry = MockConfigEntry::new("test_domain");

        assert_eq!(entry.domain, "test_domain");
        assert_eq!(entry.title, "Mock Title");
        assert_eq!(entry.state, ConfigEntryState::NotLoaded);
        assert!(!entry.entry_id.is_empty());
    }

    #[test]
    fn test_mock_config_entry_builder() {
        let entry = MockConfigEntry::new("hue")
            .with_title("Philips Hue")
            .with_data_value("host", json!("192.168.1.100"))
            .with_option("transition_time", json!(400))
            .with_unique_id("abc123")
            .with_state(ConfigEntryState::Loaded);

        assert_eq!(entry.domain, "hue");
        assert_eq!(entry.title, "Philips Hue");
        assert_eq!(entry.data.get("host"), Some(&json!("192.168.1.100")));
        assert_eq!(entry.options.get("transition_time"), Some(&json!(400)));
        assert_eq!(entry.unique_id, Some("abc123".to_string()));
        assert_eq!(entry.state, ConfigEntryState::Loaded);
    }
}
