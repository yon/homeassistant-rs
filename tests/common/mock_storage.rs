//! Mock storage for testing
//!
//! Provides an in-memory storage implementation for tests,
//! avoiding file I/O during testing.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// In-memory storage mock for testing
pub struct MockStorage {
    data: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

impl MockStorage {
    /// Create a new empty mock storage
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a mock storage with initial data
    pub fn with_data(initial: HashMap<String, serde_json::Value>) -> Self {
        Self {
            data: Arc::new(RwLock::new(initial)),
        }
    }

    /// Get a value from storage
    pub fn get(&self, key: &str) -> Option<serde_json::Value> {
        self.data.read().unwrap().get(key).cloned()
    }

    /// Set a value in storage
    pub fn set(&self, key: impl Into<String>, value: serde_json::Value) {
        self.data.write().unwrap().insert(key.into(), value);
    }

    /// Check if a key exists in storage
    pub fn contains(&self, key: &str) -> bool {
        self.data.read().unwrap().contains_key(key)
    }

    /// Remove a value from storage
    pub fn remove(&self, key: &str) -> Option<serde_json::Value> {
        self.data.write().unwrap().remove(key)
    }

    /// Clear all stored data
    pub fn clear(&self) {
        self.data.write().unwrap().clear();
    }

    /// Get all keys in storage
    pub fn keys(&self) -> Vec<String> {
        self.data.read().unwrap().keys().cloned().collect()
    }

    /// Assert that a key has been saved
    pub fn assert_saved(&self, key: &str) {
        assert!(
            self.contains(key),
            "Expected key '{}' to be saved in storage",
            key
        );
    }

    /// Assert that a key has not been saved
    pub fn assert_not_saved(&self, key: &str) {
        assert!(
            !self.contains(key),
            "Expected key '{}' to NOT be saved in storage",
            key
        );
    }
}

impl Default for MockStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MockStorage {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_get_and_set() {
        let storage = MockStorage::new();

        storage.set("test_key", json!({"value": 123}));

        let value = storage.get("test_key").unwrap();
        assert_eq!(value["value"], 123);
    }

    #[test]
    fn test_with_initial_data() {
        let initial = HashMap::from([
            ("key1".to_string(), json!(1)),
            ("key2".to_string(), json!(2)),
        ]);

        let storage = MockStorage::with_data(initial);

        assert_eq!(storage.get("key1"), Some(json!(1)));
        assert_eq!(storage.get("key2"), Some(json!(2)));
    }

    #[test]
    fn test_remove() {
        let storage = MockStorage::new();
        storage.set("key", json!("value"));

        let removed = storage.remove("key");
        assert_eq!(removed, Some(json!("value")));
        assert!(!storage.contains("key"));
    }

    #[test]
    fn test_assert_saved() {
        let storage = MockStorage::new();
        storage.set("saved_key", json!(null));
        storage.assert_saved("saved_key");
    }

    #[test]
    #[should_panic(expected = "Expected key 'missing' to be saved")]
    fn test_assert_saved_fails() {
        let storage = MockStorage::new();
        storage.assert_saved("missing");
    }
}
