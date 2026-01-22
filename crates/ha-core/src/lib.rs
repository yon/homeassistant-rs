//! Core types for Home Assistant
//!
//! This crate provides the fundamental types used throughout the Home Assistant
//! Rust implementation: EntityId, State, Event, Context, and ServiceCall.

mod context;
mod entity_id;
mod event;
mod service_call;
mod state;

pub use context::Context;
pub use entity_id::{EntityId, EntityIdError};
pub use event::{Event, EventData, EventType};
pub use service_call::{ServiceCall, SupportsResponse};
pub use state::State;

/// Maximum length for a state value (matches Python HA)
pub const MAX_STATE_LENGTH: usize = 255;

/// State value used when actual state exceeds MAX_STATE_LENGTH
pub const STATE_UNKNOWN: &str = "unknown";

/// Standard event types used by Home Assistant
pub mod events {
    use super::*;

    /// Event type for state changes
    pub const STATE_CHANGED: &str = "state_changed";

    /// Event type for state reported (unchanged state was written)
    pub const STATE_REPORTED: &str = "state_reported";

    /// Event type for service calls
    pub const CALL_SERVICE: &str = "call_service";

    /// Event type for Home Assistant start
    pub const HOMEASSISTANT_START: &str = "homeassistant_start";

    /// Event type for Home Assistant stop
    pub const HOMEASSISTANT_STOP: &str = "homeassistant_stop";

    /// Event type for Home Assistant close
    pub const HOMEASSISTANT_CLOSE: &str = "homeassistant_close";

    /// Event type for core config update
    pub const CORE_CONFIG_UPDATE: &str = "core_config_update";

    /// Data for STATE_CHANGED events
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct StateChangedData {
        pub entity_id: EntityId,
        pub old_state: Option<State>,
        pub new_state: Option<State>,
    }

    impl EventData for StateChangedData {
        fn event_type() -> &'static str {
            STATE_CHANGED
        }
    }

    /// Data for STATE_REPORTED events (when state is unchanged but reported)
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct StateReportedData {
        pub entity_id: EntityId,
        pub new_state: State,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub old_last_reported: Option<chrono::DateTime<chrono::Utc>>,
        pub last_reported: chrono::DateTime<chrono::Utc>,
    }

    impl EventData for StateReportedData {
        fn event_type() -> &'static str {
            STATE_REPORTED
        }
    }

    /// Data for CALL_SERVICE events
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct CallServiceData {
        pub domain: String,
        pub service: String,
        pub service_data: serde_json::Value,
    }

    impl EventData for CallServiceData {
        fn event_type() -> &'static str {
            CALL_SERVICE
        }
    }
}
