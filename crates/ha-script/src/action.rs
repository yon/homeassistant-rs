//! Action types
//!
//! Actions are the building blocks of scripts. Each action performs a specific
//! task like calling a service, waiting, or evaluating conditions.

use ha_automation::{Condition, Trigger};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Deserialize a field that can be either a single string or an array of strings
fn string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        String(String),
        Vec(Vec<String>),
    }

    match StringOrVec::deserialize(deserializer)? {
        StringOrVec::String(s) => Ok(vec![s]),
        StringOrVec::Vec(v) => Ok(v),
    }
}

/// Target specification for service calls
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Target {
    /// Target entity IDs
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "string_or_vec"
    )]
    pub entity_id: Vec<String>,

    /// Target device IDs
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "string_or_vec"
    )]
    pub device_id: Vec<String>,

    /// Target area IDs
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "string_or_vec"
    )]
    pub area_id: Vec<String>,

    /// Target floor IDs
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "string_or_vec"
    )]
    pub floor_id: Vec<String>,

    /// Target label IDs
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "string_or_vec"
    )]
    pub label_id: Vec<String>,
}

impl Target {
    /// Check if target is empty
    pub fn is_empty(&self) -> bool {
        self.entity_id.is_empty()
            && self.device_id.is_empty()
            && self.area_id.is_empty()
            && self.floor_id.is_empty()
            && self.label_id.is_empty()
    }
}

/// Script action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Action {
    /// Call a service
    Service(ServiceAction),

    /// Wait/delay
    Delay(DelayAction),

    /// Wait for a trigger
    WaitForTrigger(WaitForTriggerAction),

    /// Wait for a template to become true
    WaitTemplate(WaitTemplateAction),

    /// Set variables
    Variables(VariablesAction),

    /// Conditional branching
    Choose(ChooseAction),

    /// If/then/else
    If(IfAction),

    /// Repeat loop
    Repeat(RepeatAction),

    /// Sequence of actions (explicit)
    Sequence(SequenceAction),

    /// Parallel execution
    Parallel(ParallelAction),

    /// Mid-sequence condition check
    Condition(ConditionAction),

    /// Stop script execution
    Stop(StopAction),

    /// Fire an event
    Event(EventAction),

    /// Set a scene
    Scene(SceneAction),
}

/// Service call action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAction {
    /// Optional alias for this step
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Service to call (e.g., "light.turn_on")
    pub service: String,

    /// Target entities/devices/areas
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,

    /// Service data
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub data: HashMap<String, serde_json::Value>,

    /// Variable to store response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_variable: Option<String>,

    /// Whether this action is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Delay action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Delay duration (can be template)
    pub delay: DelaySpec,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Delay specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DelaySpec {
    /// Template string
    Template(String),
    /// Duration components
    Components {
        #[serde(default)]
        hours: u64,
        #[serde(default)]
        minutes: u64,
        #[serde(default)]
        seconds: u64,
        #[serde(default)]
        milliseconds: u64,
    },
}

impl DelaySpec {
    /// Convert to Duration if possible (non-template)
    pub fn to_duration(&self) -> Option<Duration> {
        match self {
            DelaySpec::Template(_) => None,
            DelaySpec::Components {
                hours,
                minutes,
                seconds,
                milliseconds,
            } => Some(Duration::from_millis(
                hours * 3600 * 1000 + minutes * 60 * 1000 + seconds * 1000 + milliseconds,
            )),
        }
    }
}

/// Wait for trigger action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitForTriggerAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Triggers to wait for
    pub wait_for_trigger: Vec<Trigger>,

    /// Timeout duration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,

    /// Continue if timeout occurs
    #[serde(default = "default_true")]
    pub continue_on_timeout: bool,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Wait for template action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitTemplateAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Template that must become true
    pub wait_template: String,

    /// Timeout duration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,

    /// Continue if timeout occurs
    #[serde(default = "default_true")]
    pub continue_on_timeout: bool,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Variables action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariablesAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Variables to set (name -> template string)
    pub variables: HashMap<String, serde_json::Value>,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Choose action (if/elseif/else)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChooseAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Choices (condition -> sequence pairs)
    pub choose: Vec<ChooseOption>,

    /// Default sequence if no conditions match
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default: Vec<serde_json::Value>,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// A single option in a choose action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChooseOption {
    /// Conditions that must be true (can also be a template string)
    #[serde(default)]
    pub conditions: ChooseConditions,

    /// Actions to execute if conditions match
    pub sequence: Vec<serde_json::Value>,
}

/// Conditions for a choose option
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChooseConditions {
    /// Template string shorthand
    Template(String),
    /// List of conditions
    List(Vec<Condition>),
}

impl Default for ChooseConditions {
    fn default() -> Self {
        ChooseConditions::List(Vec::new())
    }
}

/// If action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Condition(s) to evaluate
    pub r#if: ChooseConditions,

    /// Actions if condition is true
    pub then: Vec<serde_json::Value>,

    /// Actions if condition is false
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub r#else: Vec<serde_json::Value>,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Repeat action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepeatAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Repeat configuration
    pub repeat: RepeatConfig,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Repeat configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RepeatConfig {
    /// Count-based repeat
    Count {
        count: RepeatCount,
        sequence: Vec<serde_json::Value>,
    },
    /// For-each repeat
    ForEach {
        for_each: serde_json::Value,
        sequence: Vec<serde_json::Value>,
    },
    /// While repeat
    While {
        r#while: Vec<Condition>,
        sequence: Vec<serde_json::Value>,
    },
    /// Until repeat
    Until {
        until: Vec<Condition>,
        sequence: Vec<serde_json::Value>,
    },
}

/// Repeat count (can be number or template)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RepeatCount {
    Number(usize),
    Template(String),
}

/// Sequence action (explicit sequential execution)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Actions to execute
    pub sequence: Vec<serde_json::Value>,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Parallel action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Actions to execute in parallel
    pub parallel: Vec<serde_json::Value>,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Condition action (mid-sequence check)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Condition to check
    #[serde(flatten)]
    pub condition: Condition,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Stop action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Stop reason
    pub stop: String,

    /// Whether to set response_variable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_variable: Option<String>,

    /// Whether it was an error
    #[serde(default)]
    pub error: bool,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Event action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Event type to fire
    pub event: String,

    /// Event data
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub event_data: HashMap<String, serde_json::Value>,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Scene action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneAction {
    /// Optional alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Scene entity ID
    pub scene: String,

    /// Whether enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_action_deserialize() {
        let json = r#"{
            "service": "light.turn_on",
            "target": {"entity_id": ["light.living_room"]},
            "data": {"brightness": 255}
        }"#;

        let action: Action = serde_json::from_str(json).unwrap();
        assert!(matches!(action, Action::Service(_)));
    }

    #[test]
    fn test_delay_action() {
        let json = r#"{
            "delay": {"hours": 0, "minutes": 5, "seconds": 0, "milliseconds": 0}
        }"#;

        let action: Action = serde_json::from_str(json).unwrap();
        if let Action::Delay(d) = action {
            assert_eq!(d.delay.to_duration(), Some(Duration::from_secs(300)));
        } else {
            panic!("Expected Delay action");
        }
    }

    #[test]
    fn test_choose_action() {
        let json = r#"{
            "choose": [
                {
                    "conditions": "{{ is_state('light.test', 'on') }}",
                    "sequence": [{"service": "light.turn_off"}]
                }
            ],
            "default": [{"service": "light.turn_on"}]
        }"#;

        let action: Action = serde_json::from_str(json).unwrap();
        if let Action::Choose(c) = action {
            assert_eq!(c.choose.len(), 1);
            assert_eq!(c.default.len(), 1);
        } else {
            panic!("Expected Choose action");
        }
    }

    #[test]
    fn test_repeat_count() {
        let json = r#"{
            "repeat": {
                "count": 5,
                "sequence": [{"service": "light.toggle"}]
            }
        }"#;

        let action: Action = serde_json::from_str(json).unwrap();
        if let Action::Repeat(r) = action {
            assert!(matches!(
                r.repeat,
                RepeatConfig::Count {
                    count: RepeatCount::Number(5),
                    ..
                }
            ));
        } else {
            panic!("Expected Repeat action");
        }
    }

    #[test]
    fn test_target() {
        let target = Target {
            entity_id: vec!["light.test".to_string()],
            ..Default::default()
        };
        assert!(!target.is_empty());

        let empty = Target::default();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_variables_action() {
        let json = r#"{
            "variables": {
                "brightness": 255,
                "color": "{{ 'red' if is_state('sun.sun', 'below_horizon') else 'white' }}"
            }
        }"#;

        let action: Action = serde_json::from_str(json).unwrap();
        if let Action::Variables(v) = action {
            assert!(v.variables.contains_key("brightness"));
            assert!(v.variables.contains_key("color"));
        } else {
            panic!("Expected Variables action");
        }
    }

    #[test]
    fn test_parallel_action() {
        let json = r#"{
            "parallel": [
                {"service": "light.turn_on", "target": {"entity_id": ["light.one"]}},
                {"service": "light.turn_on", "target": {"entity_id": ["light.two"]}}
            ]
        }"#;

        let action: Action = serde_json::from_str(json).unwrap();
        if let Action::Parallel(p) = action {
            assert_eq!(p.parallel.len(), 2);
        } else {
            panic!("Expected Parallel action");
        }
    }
}
