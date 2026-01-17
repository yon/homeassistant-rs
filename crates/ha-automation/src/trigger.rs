//! Trigger types
//!
//! Triggers are event detectors that initiate automations.
//! When a trigger matches, it provides TriggerData with context variables.

use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;

/// Trigger errors
#[derive(Debug, Error)]
pub enum TriggerError {
    #[error("Invalid trigger configuration: {0}")]
    InvalidConfig(String),

    #[error("Template error: {0}")]
    Template(String),

    #[error("Entity not found: {0}")]
    EntityNotFound(String),
}

/// Result type for trigger operations
pub type TriggerResult<T> = Result<T, TriggerError>;

/// Data provided when a trigger fires
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerData {
    /// Optional trigger ID for referencing in conditions/actions
    pub id: Option<String>,

    /// Trigger platform type (e.g., "state", "time", "event")
    pub platform: String,

    /// Additional variables available in templates
    #[serde(flatten)]
    pub variables: HashMap<String, serde_json::Value>,

    /// When the trigger matched
    pub triggered_at: DateTime<Utc>,
}

impl TriggerData {
    /// Create new trigger data
    pub fn new(platform: impl Into<String>) -> Self {
        Self {
            id: None,
            platform: platform.into(),
            variables: HashMap::new(),
            triggered_at: Utc::now(),
        }
    }

    /// Set trigger ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Add a variable
    pub fn with_var(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.variables.insert(key.into(), value);
        self
    }
}

/// Trigger definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "trigger", rename_all = "snake_case")]
pub enum Trigger {
    /// Fires when an entity's state changes
    State(StateTrigger),

    /// Fires on any event with optional data matching
    Event(EventTrigger),

    /// Fires at a specific time
    Time(TimeTrigger),

    /// Fires on a time pattern (e.g., every 5 minutes)
    TimePattern(TimePatternTrigger),

    /// Fires when a numeric value crosses a threshold
    NumericState(NumericStateTrigger),

    /// Fires when a template evaluates to true
    Template(TemplateTrigger),

    /// Fires when an entity enters/leaves a zone
    Zone(ZoneTrigger),

    /// Fires at sunrise/sunset
    Sun(SunTrigger),

    /// Fires on Home Assistant start/stop
    Homeassistant(HomeassistantTrigger),

    /// Fires on webhook request
    Webhook(WebhookTrigger),
}

impl Trigger {
    /// Get the trigger's ID if set
    pub fn id(&self) -> Option<&str> {
        match self {
            Trigger::State(t) => t.id.as_deref(),
            Trigger::Event(t) => t.id.as_deref(),
            Trigger::Time(t) => t.id.as_deref(),
            Trigger::TimePattern(t) => t.id.as_deref(),
            Trigger::NumericState(t) => t.id.as_deref(),
            Trigger::Template(t) => t.id.as_deref(),
            Trigger::Zone(t) => t.id.as_deref(),
            Trigger::Sun(t) => t.id.as_deref(),
            Trigger::Homeassistant(t) => t.id.as_deref(),
            Trigger::Webhook(t) => t.id.as_deref(),
        }
    }

    /// Get the trigger platform name
    pub fn platform(&self) -> &'static str {
        match self {
            Trigger::State(_) => "state",
            Trigger::Event(_) => "event",
            Trigger::Time(_) => "time",
            Trigger::TimePattern(_) => "time_pattern",
            Trigger::NumericState(_) => "numeric_state",
            Trigger::Template(_) => "template",
            Trigger::Zone(_) => "zone",
            Trigger::Sun(_) => "sun",
            Trigger::Homeassistant(_) => "homeassistant",
            Trigger::Webhook(_) => "webhook",
        }
    }
}

/// State change trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTrigger {
    /// Optional trigger ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Entity IDs to monitor (can be single or list)
    pub entity_id: EntityIdSpec,

    /// Previous state to match (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<StateMatch>,

    /// New state to match (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<StateMatch>,

    /// Attribute to monitor instead of state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,

    /// Duration the state must be held before triggering
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        with = "option_duration_serde"
    )]
    pub r#for: Option<Duration>,

    /// Don't trigger if coming from these states
    #[serde(default)]
    pub not_from: Vec<String>,

    /// Don't trigger if going to these states
    #[serde(default)]
    pub not_to: Vec<String>,
}

/// Event trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTrigger {
    /// Optional trigger ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Event type to match
    pub event_type: String,

    /// Optional event data to match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_data: Option<serde_json::Value>,

    /// Context filters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<EventContextFilter>,
}

/// Time trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeTrigger {
    /// Optional trigger ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Time to trigger at (HH:MM:SS or input_datetime entity)
    pub at: TimeSpec,
}

/// Time pattern trigger (cron-like)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimePatternTrigger {
    /// Optional trigger ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Hours pattern (0-23 or */N)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hours: Option<String>,

    /// Minutes pattern (0-59 or */N)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minutes: Option<String>,

    /// Seconds pattern (0-59 or */N)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seconds: Option<String>,
}

/// Numeric state trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericStateTrigger {
    /// Optional trigger ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Entity IDs to monitor
    pub entity_id: EntityIdSpec,

    /// Attribute to monitor (uses state if not set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,

    /// Trigger when value goes above this
    #[serde(skip_serializing_if = "Option::is_none")]
    pub above: Option<NumericValue>,

    /// Trigger when value goes below this
    #[serde(skip_serializing_if = "Option::is_none")]
    pub below: Option<NumericValue>,

    /// Duration the value must be held
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        with = "option_duration_serde"
    )]
    pub r#for: Option<Duration>,

    /// Template to extract value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_template: Option<String>,
}

/// Template trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateTrigger {
    /// Optional trigger ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Template that evaluates to true/false
    pub value_template: String,

    /// Duration the template must be true
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        with = "option_duration_serde"
    )]
    pub r#for: Option<Duration>,
}

/// Zone trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneTrigger {
    /// Optional trigger ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Person/device tracker entity
    pub entity_id: EntityIdSpec,

    /// Zone entity
    pub zone: String,

    /// Event type: enter or leave
    pub event: ZoneEvent,
}

/// Sun trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SunTrigger {
    /// Optional trigger ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// sunrise or sunset
    pub event: SunEvent,

    /// Offset from the event (e.g., "-00:30:00" for 30 min before)
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        with = "option_duration_serde"
    )]
    pub offset: Option<Duration>,
}

/// Home Assistant trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeassistantTrigger {
    /// Optional trigger ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Event: start or shutdown
    pub event: HassEvent,
}

/// Webhook trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookTrigger {
    /// Optional trigger ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Webhook ID
    pub webhook_id: String,

    /// Allowed HTTP methods
    #[serde(default)]
    pub allowed_methods: Vec<String>,

    /// Whether to use local only
    #[serde(default)]
    pub local_only: bool,
}

// --- Supporting types ---

/// Entity ID specification (single or list)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EntityIdSpec {
    Single(String),
    List(Vec<String>),
}

impl EntityIdSpec {
    /// Get all entity IDs
    pub fn ids(&self) -> Vec<&str> {
        match self {
            EntityIdSpec::Single(id) => vec![id.as_str()],
            EntityIdSpec::List(ids) => ids.iter().map(|s| s.as_str()).collect(),
        }
    }
}

/// State match specification (single value or list)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StateMatch {
    Single(String),
    List(Vec<String>),
}

impl StateMatch {
    /// Check if a state matches
    pub fn matches(&self, state: &str) -> bool {
        match self {
            StateMatch::Single(s) => s == state,
            StateMatch::List(list) => list.iter().any(|s| s == state),
        }
    }
}

/// Time specification (fixed time or entity)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TimeSpec {
    /// Fixed time (HH:MM:SS)
    Fixed(NaiveTime),
    /// Entity ID (input_datetime, sensor, etc.)
    Entity(String),
}

/// Numeric value (literal or entity reference)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NumericValue {
    Literal(f64),
    Entity(String),
}

/// Event context filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventContextFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

/// Zone event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneEvent {
    Enter,
    Leave,
}

/// Sun event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SunEvent {
    Sunrise,
    Sunset,
}

/// Home Assistant event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HassEvent {
    Start,
    Shutdown,
}

// --- Duration serde helpers ---

pub(crate) mod option_duration_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(value: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(d) => {
                let secs = d.as_secs();
                let hours = secs / 3600;
                let mins = (secs % 3600) / 60;
                let secs = secs % 60;
                serializer.serialize_str(&format!("{:02}:{:02}:{:02}", hours, mins, secs))
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            None => Ok(None),
            Some(s) => parse_duration(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }

    fn parse_duration(s: &str) -> Result<Duration, String> {
        // Parse HH:MM:SS or MM:SS or SS format
        let parts: Vec<&str> = s.split(':').collect();
        match parts.len() {
            1 => {
                let secs: u64 = parts[0].parse().map_err(|_| "invalid seconds")?;
                Ok(Duration::from_secs(secs))
            }
            2 => {
                let mins: u64 = parts[0].parse().map_err(|_| "invalid minutes")?;
                let secs: u64 = parts[1].parse().map_err(|_| "invalid seconds")?;
                Ok(Duration::from_secs(mins * 60 + secs))
            }
            3 => {
                let hours: u64 = parts[0].parse().map_err(|_| "invalid hours")?;
                let mins: u64 = parts[1].parse().map_err(|_| "invalid minutes")?;
                let secs: u64 = parts[2].parse().map_err(|_| "invalid seconds")?;
                Ok(Duration::from_secs(hours * 3600 + mins * 60 + secs))
            }
            _ => Err("invalid duration format".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_trigger_deserialize() {
        let json = r#"{
            "trigger": "state",
            "entity_id": "light.living_room",
            "to": "on"
        }"#;

        let trigger: Trigger = serde_json::from_str(json).unwrap();
        assert!(matches!(trigger, Trigger::State(_)));
        assert_eq!(trigger.platform(), "state");
    }

    #[test]
    fn test_event_trigger_deserialize() {
        let json = r#"{
            "trigger": "event",
            "event_type": "mobile_app_notification_action",
            "event_data": {"action": "confirm"}
        }"#;

        let trigger: Trigger = serde_json::from_str(json).unwrap();
        assert!(matches!(trigger, Trigger::Event(_)));
    }

    #[test]
    fn test_time_pattern_trigger() {
        let json = r#"{
            "trigger": "time_pattern",
            "minutes": "/5"
        }"#;

        let trigger: Trigger = serde_json::from_str(json).unwrap();
        if let Trigger::TimePattern(t) = trigger {
            assert_eq!(t.minutes, Some("/5".to_string()));
        } else {
            panic!("Expected TimePattern trigger");
        }
    }

    #[test]
    fn test_entity_id_spec() {
        let single: EntityIdSpec = serde_json::from_str(r#""light.test""#).unwrap();
        assert_eq!(single.ids(), vec!["light.test"]);

        let list: EntityIdSpec = serde_json::from_str(r#"["light.one", "light.two"]"#).unwrap();
        assert_eq!(list.ids(), vec!["light.one", "light.two"]);
    }

    #[test]
    fn test_state_match() {
        let single = StateMatch::Single("on".to_string());
        assert!(single.matches("on"));
        assert!(!single.matches("off"));

        let list = StateMatch::List(vec!["on".to_string(), "home".to_string()]);
        assert!(list.matches("on"));
        assert!(list.matches("home"));
        assert!(!list.matches("off"));
    }

    #[test]
    fn test_trigger_data() {
        let data = TriggerData::new("state")
            .with_id("motion_detected")
            .with_var("entity_id", serde_json::json!("binary_sensor.motion"));

        assert_eq!(data.platform, "state");
        assert_eq!(data.id, Some("motion_detected".to_string()));
        assert!(data.variables.contains_key("entity_id"));
    }
}
