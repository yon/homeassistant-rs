//! Event types for the Home Assistant event bus

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Context;

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

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive Event testing through Python bindings
