//! System Log Component
//!
//! Captures and stores system log entries for debugging and monitoring.
//! Compatible with Home Assistant's system_log component API.
//!
//! ## Features
//! - Captures WARNING and ERROR level logs
//! - Deduplicates repeated log entries
//! - Stores up to 5 unique messages per log source
//! - Configurable max entries (default: 50)
//! - Optional event firing on new log entries
//!
//! ## Services
//! - `system_log.clear`: Clear all log entries
//! - `system_log.write`: Write a log entry programmatically
//!
//! ## WebSocket
//! - `system_log/list`: List all log entries (admin only)

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::Level;

/// Domain name for the system_log component
pub const DOMAIN: &str = "system_log";

/// Default maximum number of log entries to store
pub const DEFAULT_MAX_ENTRIES: usize = 50;

/// Maximum number of unique messages to store per log entry
const MAX_MESSAGES_PER_ENTRY: usize = 5;

/// Event fired when a new log entry is added (if fire_event is enabled)
pub const EVENT_SYSTEM_LOG: &str = "system_log_event";

/// Log level for entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl LogLevel {
    /// Convert from tracing::Level
    pub fn from_tracing_level(level: Level) -> Self {
        match level {
            Level::DEBUG | Level::TRACE => LogLevel::Debug,
            Level::ERROR => LogLevel::Error,
            Level::INFO => LogLevel::Info,
            Level::WARN => LogLevel::Warning,
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = ();

    /// Convert from string (case-insensitive)
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "debug" => Ok(LogLevel::Debug),
            "info" => Ok(LogLevel::Info),
            "warning" | "warn" => Ok(LogLevel::Warning),
            "error" => Ok(LogLevel::Error),
            "critical" | "fatal" => Ok(LogLevel::Critical),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Critical => write!(f, "CRITICAL"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warning => write!(f, "WARNING"),
        }
    }
}

/// Key for deduplicating log entries
/// Composed of: (logger_name, source_file, source_line, root_cause)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LogKey {
    /// Logger name (e.g., "homeassistant.components.light")
    pub name: String,
    /// Source file
    pub source_file: String,
    /// Source line number
    pub source_line: u32,
    /// Root cause location (file, line, function) for exceptions
    pub root_cause: Option<(String, u32, String)>,
}

/// A single log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Unique key for deduplication
    pub key: LogKey,
    /// Logger name
    pub name: String,
    /// Log level
    pub level: LogLevel,
    /// Unique messages (max 5)
    pub messages: VecDeque<String>,
    /// Source file path
    pub source_file: String,
    /// Source line number
    pub source_line: u32,
    /// Exception/error trace (if any)
    pub exception: String,
    /// Number of occurrences
    pub count: u64,
    /// First occurrence timestamp
    pub first_occurred: DateTime<Utc>,
    /// Most recent occurrence timestamp
    pub timestamp: DateTime<Utc>,
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(
        name: String,
        level: LogLevel,
        message: String,
        source_file: String,
        source_line: u32,
        exception: Option<String>,
        root_cause: Option<(String, u32, String)>,
    ) -> Self {
        let now = Utc::now();
        let key = LogKey {
            name: name.clone(),
            source_file: source_file.clone(),
            source_line,
            root_cause,
        };

        let mut messages = VecDeque::with_capacity(MAX_MESSAGES_PER_ENTRY);
        messages.push_back(message);

        Self {
            key,
            name,
            level,
            messages,
            source_file,
            source_line,
            exception: exception.unwrap_or_default(),
            count: 1,
            first_occurred: now,
            timestamp: now,
        }
    }

    /// Update entry with a new occurrence
    pub fn update(&mut self, message: &str) {
        self.count += 1;
        self.timestamp = Utc::now();

        // Add message if unique and within limit
        if !self.messages.contains(&message.to_string()) {
            if self.messages.len() >= MAX_MESSAGES_PER_ENTRY {
                self.messages.pop_front();
            }
            self.messages.push_back(message.to_string());
        }
    }

    /// Convert to dictionary format for API responses
    pub fn to_dict(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "message": self.messages.iter().collect::<Vec<_>>(),
            "level": self.level.to_string(),
            "source": [self.source_file, self.source_line],
            "timestamp": self.timestamp.timestamp_micros() as f64 / 1_000_000.0,
            "exception": self.exception,
            "count": self.count,
            "first_occurred": self.first_occurred.timestamp_micros() as f64 / 1_000_000.0,
        })
    }
}

/// Configuration for the system log component
#[derive(Debug, Clone)]
pub struct SystemLogConfig {
    /// Maximum number of entries to store
    pub max_entries: usize,
    /// Whether to fire events on new log entries
    pub fire_event: bool,
}

impl Default for SystemLogConfig {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_MAX_ENTRIES,
            fire_event: false,
        }
    }
}

/// Deduplicating log store
///
/// Stores log entries with deduplication based on logger name, source location,
/// and root cause. Maintains insertion order (most recent last) using a Vec.
#[derive(Debug)]
pub struct DedupStore {
    /// Log entries indexed by key for fast lookup
    index: HashMap<LogKey, usize>,
    /// Log entries in insertion order
    entries: Vec<LogEntry>,
    /// Maximum number of entries
    max_entries: usize,
}

impl DedupStore {
    /// Create a new dedup store
    pub fn new(max_entries: usize) -> Self {
        Self {
            index: HashMap::new(),
            entries: Vec::new(),
            max_entries,
        }
    }

    /// Add or update a log entry
    pub fn add_entry(&mut self, entry: LogEntry) {
        let key = entry.key.clone();
        let message = entry.messages.front().cloned().unwrap_or_default();

        if let Some(&idx) = self.index.get(&key) {
            // Update existing entry
            if let Some(existing) = self.entries.get_mut(idx) {
                existing.update(&message);
            }
            // Move to end (most recent) by removing and re-adding
            let updated = self.entries.remove(idx);
            // Update indices for entries after the removed one
            for (k, v) in self.index.iter_mut() {
                if *v > idx {
                    *v -= 1;
                } else if k == &key {
                    *v = self.entries.len();
                }
            }
            self.entries.push(updated);
            *self.index.get_mut(&key).unwrap() = self.entries.len() - 1;
        } else {
            // Insert new entry
            let new_idx = self.entries.len();
            self.entries.push(entry);
            self.index.insert(key, new_idx);

            // Remove oldest if over limit
            while self.entries.len() > self.max_entries {
                if let Some(oldest) = self.entries.first() {
                    let oldest_key = oldest.key.clone();
                    self.index.remove(&oldest_key);
                    self.entries.remove(0);
                    // Update all indices
                    for v in self.index.values_mut() {
                        *v -= 1;
                    }
                }
            }
        }
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
    }

    /// Get all entries as a list (most recent first)
    pub fn to_list(&self) -> Vec<serde_json::Value> {
        self.entries.iter().rev().map(|e| e.to_dict()).collect()
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Thread-safe system log handler
#[derive(Debug)]
pub struct SystemLog {
    /// Configuration
    config: SystemLogConfig,
    /// Log store
    store: RwLock<DedupStore>,
}

impl SystemLog {
    /// Create a new system log handler
    pub fn new(config: SystemLogConfig) -> Self {
        Self {
            store: RwLock::new(DedupStore::new(config.max_entries)),
            config,
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(SystemLogConfig::default())
    }

    /// Add a log entry
    pub fn add(&self, entry: LogEntry) {
        if let Ok(mut store) = self.store.write() {
            store.add_entry(entry);
        }
    }

    /// Log a message (convenience method)
    pub fn log(
        &self,
        name: &str,
        level: LogLevel,
        message: &str,
        source_file: Option<&str>,
        source_line: Option<u32>,
    ) {
        let entry = LogEntry::new(
            name.to_string(),
            level,
            message.to_string(),
            source_file.unwrap_or("unknown").to_string(),
            source_line.unwrap_or(0),
            None,
            None,
        );
        self.add(entry);
    }

    /// Clear all log entries
    pub fn clear(&self) {
        if let Ok(mut store) = self.store.write() {
            store.clear();
        }
    }

    /// Get all log entries
    pub fn list(&self) -> Vec<serde_json::Value> {
        self.store.read().map(|s| s.to_list()).unwrap_or_default()
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.store.read().map(|s| s.len()).unwrap_or(0)
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.store.read().map(|s| s.is_empty()).unwrap_or(true)
    }

    /// Whether to fire events on new log entries
    pub fn fire_event(&self) -> bool {
        self.config.fire_event
    }
}

impl Default for SystemLog {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Register system_log services with the service registry
pub fn register_services(
    services: &ha_service_registry::ServiceRegistry,
    system_log: Arc<SystemLog>,
) {
    use ha_core::SupportsResponse;

    // system_log.clear - Clear all log entries
    {
        let log = Arc::clone(&system_log);
        services.register(
            DOMAIN,
            "clear",
            move |_call| {
                let log = Arc::clone(&log);
                async move {
                    log.clear();
                    tracing::debug!("System log cleared");
                    Ok(None)
                }
            },
            None,
            SupportsResponse::None,
        );
    }

    // system_log.write - Write a log entry
    {
        let log = Arc::clone(&system_log);
        services.register(
            DOMAIN,
            "write",
            move |call| {
                let log = Arc::clone(&log);
                async move {
                    let message = call
                        .service_data
                        .get("message")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            ha_service_registry::ServiceError::InvalidData(
                                "message is required".to_string(),
                            )
                        })?;

                    let level_str = call
                        .service_data
                        .get("level")
                        .and_then(|v| v.as_str())
                        .unwrap_or("error");

                    let level = level_str.parse().unwrap_or(LogLevel::Error);

                    let logger = call
                        .service_data
                        .get("logger")
                        .and_then(|v| v.as_str())
                        .unwrap_or("system_log.external");

                    // Log via tracing so it appears in normal log output
                    // Note: tracing requires compile-time constant targets, so we use a fixed target
                    // and include the logger name in the message
                    match level {
                        LogLevel::Critical | LogLevel::Error => {
                            tracing::error!("[{}] {}", logger, message)
                        }
                        LogLevel::Debug => tracing::debug!("[{}] {}", logger, message),
                        LogLevel::Info => tracing::info!("[{}] {}", logger, message),
                        LogLevel::Warning => tracing::warn!("[{}] {}", logger, message),
                    }

                    // Also add to system log store
                    log.log(logger, level, message, None, None);

                    Ok(None)
                }
            },
            Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string"},
                    "level": {
                        "type": "string",
                        "enum": ["debug", "info", "warning", "error", "critical"],
                        "default": "error"
                    },
                    "logger": {"type": "string"}
                },
                "required": ["message"]
            })),
            SupportsResponse::None,
        );
    }

    tracing::info!("System log services registered");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_from_str() {
        assert_eq!("debug".parse(), Ok(LogLevel::Debug));
        assert_eq!("INFO".parse(), Ok(LogLevel::Info));
        assert_eq!("Warning".parse(), Ok(LogLevel::Warning));
        assert_eq!("warn".parse(), Ok(LogLevel::Warning));
        assert_eq!("error".parse(), Ok(LogLevel::Error));
        assert_eq!("CRITICAL".parse(), Ok(LogLevel::Critical));
        assert_eq!("fatal".parse(), Ok(LogLevel::Critical));
        assert!("invalid".parse::<LogLevel>().is_err());
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry::new(
            "test.logger".to_string(),
            LogLevel::Error,
            "Test message".to_string(),
            "test.rs".to_string(),
            42,
            None,
            None,
        );

        assert_eq!(entry.name, "test.logger");
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.messages.len(), 1);
        assert_eq!(entry.messages[0], "Test message");
        assert_eq!(entry.source_file, "test.rs");
        assert_eq!(entry.source_line, 42);
        assert_eq!(entry.count, 1);
    }

    #[test]
    fn test_log_entry_update() {
        let mut entry = LogEntry::new(
            "test.logger".to_string(),
            LogLevel::Error,
            "Message 1".to_string(),
            "test.rs".to_string(),
            42,
            None,
            None,
        );

        entry.update("Message 2");
        assert_eq!(entry.count, 2);
        assert_eq!(entry.messages.len(), 2);

        // Duplicate message should not be added
        entry.update("Message 1");
        assert_eq!(entry.count, 3);
        assert_eq!(entry.messages.len(), 2);
    }

    #[test]
    fn test_log_entry_max_messages() {
        let mut entry = LogEntry::new(
            "test.logger".to_string(),
            LogLevel::Error,
            "Message 1".to_string(),
            "test.rs".to_string(),
            42,
            None,
            None,
        );

        for i in 2..=10 {
            entry.update(&format!("Message {}", i));
        }

        assert_eq!(entry.messages.len(), MAX_MESSAGES_PER_ENTRY);
        assert_eq!(entry.count, 10);
        // Oldest messages should have been removed
        assert!(!entry.messages.contains(&"Message 1".to_string()));
    }

    #[test]
    fn test_dedup_store_add_entry() {
        let mut store = DedupStore::new(10);

        let entry1 = LogEntry::new(
            "logger1".to_string(),
            LogLevel::Error,
            "Error 1".to_string(),
            "file1.rs".to_string(),
            10,
            None,
            None,
        );
        store.add_entry(entry1);

        assert_eq!(store.len(), 1);

        // Add entry with same key (should update)
        let entry2 = LogEntry::new(
            "logger1".to_string(),
            LogLevel::Error,
            "Error 2".to_string(),
            "file1.rs".to_string(),
            10,
            None,
            None,
        );
        store.add_entry(entry2);

        assert_eq!(store.len(), 1);
        let entries = store.to_list();
        assert_eq!(entries[0]["count"], 2);
    }

    #[test]
    fn test_dedup_store_max_entries() {
        let mut store = DedupStore::new(3);

        for i in 0..5 {
            let entry = LogEntry::new(
                format!("logger{}", i),
                LogLevel::Error,
                format!("Error {}", i),
                format!("file{}.rs", i),
                i as u32,
                None,
                None,
            );
            store.add_entry(entry);
        }

        assert_eq!(store.len(), 3);
        // Oldest entries should have been removed
        let entries = store.to_list();
        assert!(entries
            .iter()
            .all(|e| !e["name"].as_str().unwrap().contains("logger0")));
        assert!(entries
            .iter()
            .all(|e| !e["name"].as_str().unwrap().contains("logger1")));
    }

    #[test]
    fn test_system_log() {
        let log = SystemLog::with_defaults();

        log.log(
            "test",
            LogLevel::Error,
            "Test error",
            Some("test.rs"),
            Some(1),
        );
        assert_eq!(log.len(), 1);

        log.clear();
        assert!(log.is_empty());
    }
}
