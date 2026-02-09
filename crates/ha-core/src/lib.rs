//! Core types for Home Assistant
//!
//! This crate provides the fundamental types used throughout the Home Assistant
//! Rust implementation: EntityId, State, Event, Context, and ServiceCall.

mod context;
pub mod domains;
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

/// Standard state values (matches Python HA `homeassistant.const`)
pub const STATE_ALARM_ARMED_AWAY: &str = "armed_away";
pub const STATE_ALARM_ARMED_CUSTOM_BYPASS: &str = "armed_custom_bypass";
pub const STATE_ALARM_ARMED_HOME: &str = "armed_home";
pub const STATE_ALARM_ARMED_NIGHT: &str = "armed_night";
pub const STATE_ALARM_ARMED_VACATION: &str = "armed_vacation";
pub const STATE_ALARM_ARMING: &str = "arming";
pub const STATE_ALARM_DISARMED: &str = "disarmed";
pub const STATE_ALARM_DISARMING: &str = "disarming";
pub const STATE_ALARM_PENDING: &str = "pending";
pub const STATE_ALARM_TRIGGERED: &str = "triggered";
pub const STATE_CLOSED: &str = "closed";
pub const STATE_CLOSING: &str = "closing";
pub const STATE_HOME: &str = "home";
pub const STATE_IDLE: &str = "idle";
pub const STATE_LOCKED: &str = "locked";
pub const STATE_LOCKING: &str = "locking";
pub const STATE_NOT_HOME: &str = "not_home";
pub const STATE_OFF: &str = "off";
pub const STATE_ON: &str = "on";
pub const STATE_OPEN: &str = "open";
pub const STATE_OPENING: &str = "opening";
pub const STATE_PAUSED: &str = "paused";
pub const STATE_PLAYING: &str = "playing";
pub const STATE_STANDBY: &str = "standby";
pub const STATE_UNAVAILABLE: &str = "unavailable";
/// Also used as fallback when state string exceeds MAX_STATE_LENGTH
pub const STATE_UNKNOWN: &str = "unknown";
pub const STATE_UNLOCKED: &str = "unlocked";
pub const STATE_UNLOCKING: &str = "unlocking";

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
