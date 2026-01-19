//! Persistent Notification Component
//!
//! In-memory notification system for UI alerts.
//! Compatible with Home Assistant's persistent_notification component.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

/// Domain name for persistent notification services
pub const DOMAIN: &str = "persistent_notification";

/// A persistent notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Unique notification ID
    pub notification_id: String,
    /// Notification message (supports markdown)
    pub message: String,
    /// Optional title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

impl Notification {
    /// Create a new notification
    pub fn new(notification_id: String, message: String, title: Option<String>) -> Self {
        Self {
            notification_id,
            message,
            title,
            created_at: Utc::now(),
        }
    }
}

/// Update type for notification events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateType {
    /// Initial snapshot of current notifications
    Current,
    /// Notification was added
    Added,
    /// Notification was removed
    Removed,
    /// Notification was updated
    Updated,
}

/// Persistent Notification Manager
///
/// Thread-safe in-memory notification storage using DashMap.
/// All operations are idempotent.
#[derive(Debug)]
pub struct PersistentNotificationManager {
    /// Notifications indexed by notification_id
    notifications: DashMap<String, Notification>,
}

impl Default for PersistentNotificationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentNotificationManager {
    /// Create a new notification manager
    pub fn new() -> Self {
        Self {
            notifications: DashMap::new(),
        }
    }

    /// Create or update a notification.
    ///
    /// Returns the created/updated notification and whether it was an update.
    /// Idempotent: creating the same ID updates the existing notification.
    pub fn create(
        &self,
        notification_id: String,
        message: String,
        title: Option<String>,
    ) -> (Notification, UpdateType) {
        let is_update = self.notifications.contains_key(&notification_id);
        let notification = Notification::new(notification_id.clone(), message, title);

        self.notifications
            .insert(notification_id.clone(), notification.clone());

        let update_type = if is_update {
            debug!("Updated notification: {}", notification_id);
            UpdateType::Updated
        } else {
            info!("Created notification: {}", notification_id);
            UpdateType::Added
        };

        (notification, update_type)
    }

    /// Dismiss a notification.
    ///
    /// Returns the dismissed notification if it existed.
    /// Idempotent: dismissing non-existent notification is a no-op.
    pub fn dismiss(&self, notification_id: &str) -> Option<Notification> {
        if let Some((_, notification)) = self.notifications.remove(notification_id) {
            info!("Dismissed notification: {}", notification_id);
            Some(notification)
        } else {
            debug!(
                "Attempted to dismiss non-existent notification: {}",
                notification_id
            );
            None
        }
    }

    /// Dismiss all notifications.
    ///
    /// Returns all dismissed notifications.
    pub fn dismiss_all(&self) -> Vec<Notification> {
        let notifications: Vec<Notification> = self
            .notifications
            .iter()
            .map(|r| r.value().clone())
            .collect();

        self.notifications.clear();

        if !notifications.is_empty() {
            info!("Dismissed all {} notifications", notifications.len());
        }

        notifications
    }

    /// Get a notification by ID
    pub fn get(&self, notification_id: &str) -> Option<Notification> {
        self.notifications
            .get(notification_id)
            .map(|r| r.value().clone())
    }

    /// Get all notifications
    pub fn get_all(&self) -> Vec<Notification> {
        self.notifications
            .iter()
            .map(|r| r.value().clone())
            .collect()
    }

    /// Get all notifications as a map (for WebSocket response)
    pub fn get_all_map(&self) -> std::collections::HashMap<String, Notification> {
        self.notifications
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect()
    }

    /// Get count of notifications
    pub fn len(&self) -> usize {
        self.notifications.len()
    }

    /// Check if there are no notifications
    pub fn is_empty(&self) -> bool {
        self.notifications.is_empty()
    }
}

/// Create a shared notification manager
pub fn create_manager() -> Arc<PersistentNotificationManager> {
    Arc::new(PersistentNotificationManager::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_notification() {
        let manager = PersistentNotificationManager::new();

        let (notification, update_type) =
            manager.create("test_id".to_string(), "Test message".to_string(), None);

        assert_eq!(notification.notification_id, "test_id");
        assert_eq!(notification.message, "Test message");
        assert!(notification.title.is_none());
        assert_eq!(update_type, UpdateType::Added);
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn test_create_notification_with_title() {
        let manager = PersistentNotificationManager::new();

        let (notification, _) = manager.create(
            "test_id".to_string(),
            "Test message".to_string(),
            Some("Test Title".to_string()),
        );

        assert_eq!(notification.title, Some("Test Title".to_string()));
    }

    #[test]
    fn test_update_existing_notification() {
        let manager = PersistentNotificationManager::new();

        // Create initial notification
        manager.create("test_id".to_string(), "Original".to_string(), None);

        // Update it
        let (notification, update_type) =
            manager.create("test_id".to_string(), "Updated".to_string(), None);

        assert_eq!(notification.message, "Updated");
        assert_eq!(update_type, UpdateType::Updated);
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn test_dismiss_notification() {
        let manager = PersistentNotificationManager::new();

        manager.create("test_id".to_string(), "Test".to_string(), None);
        assert_eq!(manager.len(), 1);

        let dismissed = manager.dismiss("test_id");
        assert!(dismissed.is_some());
        assert_eq!(dismissed.unwrap().notification_id, "test_id");
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_dismiss_nonexistent() {
        let manager = PersistentNotificationManager::new();

        let dismissed = manager.dismiss("nonexistent");
        assert!(dismissed.is_none());
    }

    #[test]
    fn test_dismiss_all() {
        let manager = PersistentNotificationManager::new();

        manager.create("id1".to_string(), "Message 1".to_string(), None);
        manager.create("id2".to_string(), "Message 2".to_string(), None);
        manager.create("id3".to_string(), "Message 3".to_string(), None);
        assert_eq!(manager.len(), 3);

        let dismissed = manager.dismiss_all();
        assert_eq!(dismissed.len(), 3);
        assert!(manager.is_empty());
    }

    #[test]
    fn test_get_notification() {
        let manager = PersistentNotificationManager::new();

        manager.create("test_id".to_string(), "Test".to_string(), None);

        let notification = manager.get("test_id");
        assert!(notification.is_some());
        assert_eq!(notification.unwrap().message, "Test");

        let missing = manager.get("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_get_all() {
        let manager = PersistentNotificationManager::new();

        manager.create("id1".to_string(), "Message 1".to_string(), None);
        manager.create("id2".to_string(), "Message 2".to_string(), None);

        let all = manager.get_all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_get_all_map() {
        let manager = PersistentNotificationManager::new();

        manager.create("id1".to_string(), "Message 1".to_string(), None);
        manager.create("id2".to_string(), "Message 2".to_string(), None);

        let map = manager.get_all_map();
        assert_eq!(map.len(), 2);
        assert!(map.contains_key("id1"));
        assert!(map.contains_key("id2"));
    }
}
