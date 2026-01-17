//! Condition types
//!
//! Conditions are state-based tests evaluated at trigger time.
//! All conditions must evaluate to true for actions to execute.

use chrono::{NaiveTime, Weekday};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

use crate::trigger::{EntityIdSpec, NumericValue, StateMatch};

/// Condition errors
#[derive(Debug, Error)]
pub enum ConditionError {
    #[error("Invalid condition configuration: {0}")]
    InvalidConfig(String),

    #[error("Template error: {0}")]
    Template(String),

    #[error("Entity not found: {0}")]
    EntityNotFound(String),

    #[error("Invalid state value: {0}")]
    InvalidState(String),
}

/// Result type for condition operations
pub type ConditionResult<T> = Result<T, ConditionError>;

/// Condition definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "condition", rename_all = "snake_case")]
pub enum Condition {
    /// Check entity state
    State(StateCondition),

    /// Check numeric value thresholds
    NumericState(NumericStateCondition),

    /// Check current time
    Time(TimeCondition),

    /// Check sun position
    Sun(SunCondition),

    /// Check zone membership
    Zone(ZoneCondition),

    /// Evaluate a template
    Template(TemplateCondition),

    /// Check which trigger fired
    Trigger(TriggerCondition),

    /// All conditions must be true (AND)
    And(AndCondition),

    /// Any condition must be true (OR)
    Or(OrCondition),

    /// Condition must be false (NOT)
    Not(NotCondition),

    /// Check device condition (integration-specific)
    Device(DeviceCondition),
}

impl Condition {
    /// Create an AND condition
    pub fn and(conditions: Vec<Condition>) -> Self {
        Condition::And(AndCondition { conditions })
    }

    /// Create an OR condition
    pub fn or(conditions: Vec<Condition>) -> Self {
        Condition::Or(OrCondition { conditions })
    }

    /// Create a NOT condition
    pub fn not(condition: Condition) -> Self {
        Condition::Not(NotCondition {
            condition: Box::new(condition),
        })
    }
}

/// State condition - check entity state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateCondition {
    /// Entity IDs to check
    pub entity_id: EntityIdSpec,

    /// State to match (can be single or list)
    pub state: StateMatch,

    /// Attribute to check instead of state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,

    /// Duration the state must have been held
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        with = "crate::trigger::option_duration_serde"
    )]
    pub r#for: Option<Duration>,

    /// Match using regex pattern
    #[serde(default)]
    pub match_regex: bool,
}

/// Numeric state condition - check numeric thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericStateCondition {
    /// Entity IDs to check
    pub entity_id: EntityIdSpec,

    /// Attribute to check (uses state if not set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,

    /// Value must be above this
    #[serde(skip_serializing_if = "Option::is_none")]
    pub above: Option<NumericValue>,

    /// Value must be below this
    #[serde(skip_serializing_if = "Option::is_none")]
    pub below: Option<NumericValue>,

    /// Template to extract value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_template: Option<String>,
}

/// Time condition - check current time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeCondition {
    /// Must be after this time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<TimeSpec>,

    /// Must be before this time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<TimeSpec>,

    /// Only on these weekdays
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weekday: Vec<WeekdaySpec>,
}

/// Sun condition - check sun position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SunCondition {
    /// Must be after sunrise/sunset
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<SunPosition>,

    /// Offset after the after position
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        with = "crate::trigger::option_duration_serde"
    )]
    pub after_offset: Option<Duration>,

    /// Must be before sunrise/sunset
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<SunPosition>,

    /// Offset before the before position
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        with = "crate::trigger::option_duration_serde"
    )]
    pub before_offset: Option<Duration>,
}

/// Zone condition - check entity location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneCondition {
    /// Entity to check (person or device_tracker)
    pub entity_id: EntityIdSpec,

    /// Zone entity
    pub zone: String,
}

/// Template condition - evaluate template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateCondition {
    /// Template that must evaluate to true
    pub value_template: String,
}

/// Trigger condition - check which trigger fired
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerCondition {
    /// Trigger ID to match
    pub id: String,
}

/// AND condition - all must be true
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndCondition {
    /// Conditions to evaluate
    pub conditions: Vec<Condition>,
}

/// OR condition - any must be true
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrCondition {
    /// Conditions to evaluate
    pub conditions: Vec<Condition>,
}

/// NOT condition - must be false
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotCondition {
    /// Condition to negate
    pub condition: Box<Condition>,
}

/// Device condition - integration-specific
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCondition {
    /// Device ID
    pub device_id: String,

    /// Integration domain
    pub domain: String,

    /// Condition type (integration-specific)
    pub r#type: String,

    /// Additional data
    #[serde(flatten)]
    pub data: serde_json::Value,
}

// --- Supporting types ---

/// Time specification for conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TimeSpec {
    /// Fixed time (HH:MM:SS)
    Fixed(NaiveTime),
    /// Entity ID (input_datetime)
    Entity(String),
}

/// Weekday specification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WeekdaySpec {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl From<WeekdaySpec> for Weekday {
    fn from(w: WeekdaySpec) -> Self {
        match w {
            WeekdaySpec::Mon => Weekday::Mon,
            WeekdaySpec::Tue => Weekday::Tue,
            WeekdaySpec::Wed => Weekday::Wed,
            WeekdaySpec::Thu => Weekday::Thu,
            WeekdaySpec::Fri => Weekday::Fri,
            WeekdaySpec::Sat => Weekday::Sat,
            WeekdaySpec::Sun => Weekday::Sun,
        }
    }
}

/// Sun position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SunPosition {
    Sunrise,
    Sunset,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_condition_deserialize() {
        let json = r#"{
            "condition": "state",
            "entity_id": "light.living_room",
            "state": "on"
        }"#;

        let condition: Condition = serde_json::from_str(json).unwrap();
        assert!(matches!(condition, Condition::State(_)));
    }

    #[test]
    fn test_numeric_state_condition() {
        let json = r#"{
            "condition": "numeric_state",
            "entity_id": "sensor.temperature",
            "above": 70,
            "below": 80
        }"#;

        let condition: Condition = serde_json::from_str(json).unwrap();
        if let Condition::NumericState(c) = condition {
            assert!(matches!(c.above, Some(NumericValue::Literal(70.0))));
            assert!(matches!(c.below, Some(NumericValue::Literal(80.0))));
        } else {
            panic!("Expected NumericState condition");
        }
    }

    #[test]
    fn test_time_condition() {
        let json = r#"{
            "condition": "time",
            "after": "08:00:00",
            "before": "20:00:00",
            "weekday": ["mon", "tue", "wed"]
        }"#;

        let condition: Condition = serde_json::from_str(json).unwrap();
        if let Condition::Time(c) = condition {
            assert_eq!(c.weekday.len(), 3);
        } else {
            panic!("Expected Time condition");
        }
    }

    #[test]
    fn test_and_condition() {
        let json = r#"{
            "condition": "and",
            "conditions": [
                {"condition": "state", "entity_id": "light.one", "state": "on"},
                {"condition": "state", "entity_id": "light.two", "state": "on"}
            ]
        }"#;

        let condition: Condition = serde_json::from_str(json).unwrap();
        if let Condition::And(c) = condition {
            assert_eq!(c.conditions.len(), 2);
        } else {
            panic!("Expected And condition");
        }
    }

    #[test]
    fn test_template_condition() {
        let json = r#"{
            "condition": "template",
            "value_template": "{{ is_state('light.test', 'on') }}"
        }"#;

        let condition: Condition = serde_json::from_str(json).unwrap();
        if let Condition::Template(c) = condition {
            assert!(c.value_template.contains("is_state"));
        } else {
            panic!("Expected Template condition");
        }
    }

    #[test]
    fn test_trigger_condition() {
        let json = r#"{
            "condition": "trigger",
            "id": "motion_detected"
        }"#;

        let condition: Condition = serde_json::from_str(json).unwrap();
        if let Condition::Trigger(c) = condition {
            assert_eq!(c.id, "motion_detected");
        } else {
            panic!("Expected Trigger condition");
        }
    }

    #[test]
    fn test_condition_helpers() {
        let c1 = Condition::State(StateCondition {
            entity_id: EntityIdSpec::Single("light.test".to_string()),
            state: StateMatch::Single("on".to_string()),
            attribute: None,
            r#for: None,
            match_regex: false,
        });

        let c2 = Condition::State(StateCondition {
            entity_id: EntityIdSpec::Single("light.test2".to_string()),
            state: StateMatch::Single("on".to_string()),
            attribute: None,
            r#for: None,
            match_regex: false,
        });

        // Test AND helper
        let and = Condition::and(vec![c1.clone(), c2.clone()]);
        assert!(matches!(and, Condition::And(_)));

        // Test OR helper
        let or = Condition::or(vec![c1.clone(), c2.clone()]);
        assert!(matches!(or, Condition::Or(_)));

        // Test NOT helper
        let not = Condition::not(c1);
        assert!(matches!(not, Condition::Not(_)));
    }
}
