//! Event types for the Home Assistant event bus

use crate::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Trait for typed event data
///
/// Implement this trait for any data type that should be carried by events.
pub trait EventData: Clone + Send + Sync + 'static {
    /// The event type string for this data type
    fn event_type() -> &'static str;
}

/// Event type identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventType(String);

impl EventType {
    /// Create a new event type
    pub fn new(event_type: impl Into<String>) -> Self {
        Self(event_type.into())
    }

    /// Get the event type as a string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Special event type that matches all events
    pub fn match_all() -> Self {
        Self("*".to_string())
    }

    /// Check if this is the MATCH_ALL event type
    pub fn is_match_all(&self) -> bool {
        self.0 == "*"
    }
}

impl From<&str> for EventType {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for EventType {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An event that can be fired on the event bus
///
/// Events carry typed data and context about their origin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event<T = serde_json::Value> {
    /// The type of event
    pub event_type: EventType,

    /// The event data
    pub data: T,

    /// Origin of the event (local, remote, etc.)
    pub origin: EventOrigin,

    /// When the event was fired
    pub time_fired: DateTime<Utc>,

    /// Context tracking the origin and causality
    pub context: Context,
}

impl<T> Event<T> {
    /// Create a new event with current timestamp
    pub fn new(event_type: impl Into<EventType>, data: T, context: Context) -> Self {
        Self {
            event_type: event_type.into(),
            data,
            origin: EventOrigin::Local,
            time_fired: Utc::now(),
            context,
        }
    }

    /// Create an event with a specific origin
    pub fn with_origin(mut self, origin: EventOrigin) -> Self {
        self.origin = origin;
        self
    }
}

impl<T: EventData> Event<T> {
    /// Create a typed event from EventData
    pub fn typed(data: T, context: Context) -> Self {
        Self::new(T::event_type(), data, context)
    }
}

/// Origin of an event
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventOrigin {
    /// Event originated locally
    #[default]
    Local,
    /// Event came from a remote source
    Remote,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug, Clone)]
    struct TestEventData {
        message: String,
    }

    impl EventData for TestEventData {
        fn event_type() -> &'static str {
            "test_event"
        }
    }

    #[test]
    fn test_event_type() {
        let et = EventType::new("state_changed");
        assert_eq!(et.as_str(), "state_changed");
        assert!(!et.is_match_all());

        let match_all = EventType::match_all();
        assert!(match_all.is_match_all());
    }

    #[test]
    fn test_event_creation() {
        let ctx = Context::new();
        let event: Event<serde_json::Value> =
            Event::new("test_event", json!({"key": "value"}), ctx.clone());

        assert_eq!(event.event_type.as_str(), "test_event");
        assert_eq!(event.origin, EventOrigin::Local);
        assert_eq!(event.context.id, ctx.id);
    }

    #[test]
    fn test_typed_event() {
        let data = TestEventData {
            message: "hello".to_string(),
        };
        let ctx = Context::new();
        let event = Event::typed(data, ctx);

        assert_eq!(event.event_type.as_str(), "test_event");
        assert_eq!(event.data.message, "hello");
    }

    #[test]
    fn test_event_origin() {
        let ctx = Context::new();
        let event: Event<()> = Event::new("test", (), ctx).with_origin(EventOrigin::Remote);

        assert_eq!(event.origin, EventOrigin::Remote);
    }

    #[test]
    fn test_event_serde() {
        let ctx = Context::new();
        let event: Event<serde_json::Value> = Event::new("test_event", json!({"data": 123}), ctx);

        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event<serde_json::Value> = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.event_type, event.event_type);
        assert_eq!(parsed.data, event.data);
    }
}
