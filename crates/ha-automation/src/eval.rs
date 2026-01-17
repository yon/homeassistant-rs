//! Condition evaluation logic
//!
//! This module provides the runtime evaluation of conditions against the current
//! state of the system. Conditions are evaluated at trigger time to determine
//! whether automation actions should execute.

use chrono::{Datelike, Local, NaiveTime};
use ha_state_machine::StateMachine;
use ha_template::TemplateEngine;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, trace};

use crate::condition::{
    AndCondition, Condition, ConditionError, ConditionResult, DeviceCondition, NotCondition,
    NumericStateCondition, OrCondition, StateCondition, SunCondition, SunPosition,
    TemplateCondition, TimeCondition, TimeSpec, TriggerCondition, ZoneCondition,
};
use crate::trigger::{NumericValue, StateMatch, TriggerData};

/// Context for condition evaluation
///
/// Contains all the runtime data needed to evaluate conditions, including
/// trigger information, variables, and time override for testing.
#[derive(Debug, Clone, Default)]
pub struct EvalContext {
    /// The trigger that fired (if any)
    pub trigger: Option<TriggerData>,

    /// Additional variables available in templates
    pub variables: HashMap<String, serde_json::Value>,

    /// Override for current time (for testing)
    pub time_override: Option<chrono::DateTime<Local>>,
}

impl EvalContext {
    /// Create a new empty evaluation context
    pub fn new() -> Self {
        Self::default()
    }

    /// Create context with trigger data
    pub fn with_trigger(trigger: TriggerData) -> Self {
        Self {
            trigger: Some(trigger),
            ..Default::default()
        }
    }

    /// Add a variable to the context
    pub fn with_var(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.variables.insert(key.into(), value);
        self
    }

    /// Set time override for testing
    pub fn with_time(mut self, time: chrono::DateTime<Local>) -> Self {
        self.time_override = Some(time);
        self
    }

    /// Get current time (or override if set)
    pub fn now(&self) -> chrono::DateTime<Local> {
        self.time_override.unwrap_or_else(Local::now)
    }

    /// Convert context to template variables
    pub fn to_template_context(&self) -> serde_json::Value {
        let mut ctx = serde_json::Map::new();

        // Add trigger data if present
        if let Some(trigger) = &self.trigger {
            ctx.insert(
                "trigger".to_string(),
                serde_json::to_value(trigger).unwrap_or(serde_json::Value::Null),
            );
        }

        // Add all variables
        for (k, v) in &self.variables {
            ctx.insert(k.clone(), v.clone());
        }

        serde_json::Value::Object(ctx)
    }
}

/// Condition evaluator
///
/// Evaluates conditions against the current system state using the state machine
/// and template engine for state access and template rendering.
pub struct ConditionEvaluator {
    state_machine: Arc<StateMachine>,
    template_engine: Arc<TemplateEngine>,
}

impl ConditionEvaluator {
    /// Create a new condition evaluator
    pub fn new(state_machine: Arc<StateMachine>, template_engine: Arc<TemplateEngine>) -> Self {
        Self {
            state_machine,
            template_engine,
        }
    }

    /// Evaluate a condition
    ///
    /// Returns `true` if the condition is satisfied, `false` otherwise.
    pub fn evaluate(&self, condition: &Condition, ctx: &EvalContext) -> ConditionResult<bool> {
        match condition {
            Condition::And(c) => self.eval_and(c, ctx),
            Condition::Device(c) => self.eval_device(c, ctx),
            Condition::Not(c) => self.eval_not(c, ctx),
            Condition::NumericState(c) => self.eval_numeric_state(c, ctx),
            Condition::Or(c) => self.eval_or(c, ctx),
            Condition::State(c) => self.eval_state(c, ctx),
            Condition::Sun(c) => self.eval_sun(c, ctx),
            Condition::Template(c) => self.eval_template(c, ctx),
            Condition::Time(c) => self.eval_time(c, ctx),
            Condition::Trigger(c) => self.eval_trigger(c, ctx),
            Condition::Zone(c) => self.eval_zone(c, ctx),
        }
    }

    /// Evaluate multiple conditions (all must pass)
    pub fn evaluate_all(
        &self,
        conditions: &[Condition],
        ctx: &EvalContext,
    ) -> ConditionResult<bool> {
        for condition in conditions {
            if !self.evaluate(condition, ctx)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Evaluate multiple conditions (any must pass)
    pub fn evaluate_any(
        &self,
        conditions: &[Condition],
        ctx: &EvalContext,
    ) -> ConditionResult<bool> {
        for condition in conditions {
            if self.evaluate(condition, ctx)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    // --- Individual condition evaluators ---

    fn eval_state(&self, condition: &StateCondition, _ctx: &EvalContext) -> ConditionResult<bool> {
        let entity_ids = condition.entity_id.ids();
        debug!(
            ?entity_ids,
            state = ?condition.state,
            attribute = ?condition.attribute,
            "Evaluating state condition"
        );

        for entity_id in entity_ids {
            let value = if let Some(attr) = &condition.attribute {
                // Check attribute value
                self.get_attribute_value(entity_id, attr)?
            } else {
                // Check state value
                self.state_machine
                    .get_state(entity_id)
                    .ok_or_else(|| ConditionError::EntityNotFound(entity_id.to_string()))?
            };

            let matches = if condition.match_regex {
                self.matches_regex(&value, &condition.state)?
            } else {
                condition.state.matches(&value)
            };

            trace!(entity_id, value, matches, "State check result");

            if !matches {
                return Ok(false);
            }

            // TODO: Handle 'for' duration check
            // This would require tracking when the entity entered this state
        }

        Ok(true)
    }

    fn eval_numeric_state(
        &self,
        condition: &NumericStateCondition,
        ctx: &EvalContext,
    ) -> ConditionResult<bool> {
        let entity_ids = condition.entity_id.ids();
        debug!(
            ?entity_ids,
            above = ?condition.above,
            below = ?condition.below,
            "Evaluating numeric state condition"
        );

        for entity_id in entity_ids {
            // Get the value to check
            let value_str = if let Some(attr) = &condition.attribute {
                self.get_attribute_value(entity_id, attr)?
            } else if let Some(value_template) = &condition.value_template {
                // Evaluate template to get value
                let template_ctx = ctx.to_template_context();
                self.template_engine
                    .render_with_context(value_template, &template_ctx)
                    .map_err(|e| ConditionError::Template(e.to_string()))?
            } else {
                self.state_machine
                    .get_state(entity_id)
                    .ok_or_else(|| ConditionError::EntityNotFound(entity_id.to_string()))?
            };

            // Parse as number
            let value: f64 = value_str.parse().map_err(|_| {
                ConditionError::InvalidState(format!("'{}' is not a number", value_str))
            })?;

            // Check above threshold
            if let Some(above) = &condition.above {
                let threshold = self.resolve_numeric_value(above)?;
                if value <= threshold {
                    trace!(entity_id, value, threshold, "Failed above check");
                    return Ok(false);
                }
            }

            // Check below threshold
            if let Some(below) = &condition.below {
                let threshold = self.resolve_numeric_value(below)?;
                if value >= threshold {
                    trace!(entity_id, value, threshold, "Failed below check");
                    return Ok(false);
                }
            }

            trace!(entity_id, value, "Numeric state check passed");
        }

        Ok(true)
    }

    fn eval_time(&self, condition: &TimeCondition, ctx: &EvalContext) -> ConditionResult<bool> {
        let now = ctx.now();
        let current_time = now.time();
        let current_weekday = now.weekday();

        debug!(
            ?current_time,
            ?current_weekday,
            after = ?condition.after,
            before = ?condition.before,
            weekday = ?condition.weekday,
            "Evaluating time condition"
        );

        // Check weekday filter
        if !condition.weekday.is_empty() {
            let weekday_matches = condition.weekday.iter().any(|w| {
                let expected: chrono::Weekday = (*w).into();
                expected == current_weekday
            });
            if !weekday_matches {
                trace!("Weekday doesn't match");
                return Ok(false);
            }
        }

        // Check after time
        if let Some(after) = &condition.after {
            let after_time = self.resolve_time_spec(after)?;
            if current_time < after_time {
                trace!(?current_time, ?after_time, "Current time is before 'after'");
                return Ok(false);
            }
        }

        // Check before time
        if let Some(before) = &condition.before {
            let before_time = self.resolve_time_spec(before)?;
            if current_time >= before_time {
                trace!(
                    ?current_time,
                    ?before_time,
                    "Current time is at or after 'before'"
                );
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn eval_sun(&self, condition: &SunCondition, ctx: &EvalContext) -> ConditionResult<bool> {
        // For sun conditions, we need sun.sun entity state
        // In HA, this is provided by the sun integration
        debug!(
            after = ?condition.after,
            before = ?condition.before,
            "Evaluating sun condition"
        );

        let sun_state = self.state_machine.get("sun.sun");

        let Some(sun) = sun_state else {
            // If no sun entity exists, we can't evaluate
            return Err(ConditionError::EntityNotFound("sun.sun".to_string()));
        };

        let now = ctx.now();

        // Get next sunrise/sunset times from sun entity attributes
        let get_sun_time = |event: &SunPosition| -> ConditionResult<chrono::DateTime<Local>> {
            let attr_name = match event {
                SunPosition::Sunrise => "next_rising",
                SunPosition::Sunset => "next_setting",
            };

            let time_str = sun
                .attributes
                .get(attr_name)
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ConditionError::InvalidState(format!("sun.sun missing {} attribute", attr_name))
                })?;

            // Parse ISO timestamp
            chrono::DateTime::parse_from_rfc3339(time_str)
                .map(|dt| dt.with_timezone(&Local))
                .map_err(|e| ConditionError::InvalidState(format!("Invalid sun time: {}", e)))
        };

        // Check after condition
        if let Some(after_pos) = &condition.after {
            let mut after_time = get_sun_time(after_pos)?;
            if let Some(offset_secs) = condition.after_offset {
                after_time += chrono::Duration::seconds(offset_secs);
            }
            if now < after_time {
                return Ok(false);
            }
        }

        // Check before condition
        if let Some(before_pos) = &condition.before {
            let mut before_time = get_sun_time(before_pos)?;
            if let Some(offset_secs) = condition.before_offset {
                before_time += chrono::Duration::seconds(offset_secs);
            }
            if now >= before_time {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn eval_zone(&self, condition: &ZoneCondition, _ctx: &EvalContext) -> ConditionResult<bool> {
        let entity_ids = condition.entity_id.ids();
        debug!(
            ?entity_ids,
            zone = %condition.zone,
            "Evaluating zone condition"
        );

        for entity_id in entity_ids {
            let state = self
                .state_machine
                .get(entity_id)
                .ok_or_else(|| ConditionError::EntityNotFound(entity_id.to_string()))?;

            // The state of a device_tracker/person is the zone name
            // e.g., "home", "work", or "not_home"
            let zone_name = if condition.zone.starts_with("zone.") {
                // Extract zone name from entity_id
                condition
                    .zone
                    .strip_prefix("zone.")
                    .unwrap_or(&condition.zone)
            } else {
                &condition.zone
            };

            if state.state != zone_name {
                trace!(
                    entity_id,
                    current_zone = %state.state,
                    expected_zone = zone_name,
                    "Zone doesn't match"
                );
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn eval_template(
        &self,
        condition: &TemplateCondition,
        ctx: &EvalContext,
    ) -> ConditionResult<bool> {
        debug!(template = %condition.value_template, "Evaluating template condition");

        let template_ctx = ctx.to_template_context();
        let result = self
            .template_engine
            .render_with_context(&condition.value_template, &template_ctx)
            .map_err(|e| ConditionError::Template(e.to_string()))?;

        // Template is true if it renders to a truthy value
        let is_true = is_truthy(&result);
        trace!(result, is_true, "Template evaluation result");

        Ok(is_true)
    }

    fn eval_trigger(
        &self,
        condition: &TriggerCondition,
        ctx: &EvalContext,
    ) -> ConditionResult<bool> {
        debug!(expected_id = %condition.id, "Evaluating trigger condition");

        let matches = ctx
            .trigger
            .as_ref()
            .and_then(|t| t.id.as_ref())
            .map(|id| id == &condition.id)
            .unwrap_or(false);

        trace!(matches, "Trigger ID check result");
        Ok(matches)
    }

    fn eval_and(&self, condition: &AndCondition, ctx: &EvalContext) -> ConditionResult<bool> {
        debug!(
            count = condition.conditions.len(),
            "Evaluating AND condition"
        );
        self.evaluate_all(&condition.conditions, ctx)
    }

    fn eval_or(&self, condition: &OrCondition, ctx: &EvalContext) -> ConditionResult<bool> {
        debug!(
            count = condition.conditions.len(),
            "Evaluating OR condition"
        );
        self.evaluate_any(&condition.conditions, ctx)
    }

    fn eval_not(&self, condition: &NotCondition, ctx: &EvalContext) -> ConditionResult<bool> {
        debug!("Evaluating NOT condition");
        let inner_result = self.evaluate(&condition.condition, ctx)?;
        Ok(!inner_result)
    }

    fn eval_device(
        &self,
        condition: &DeviceCondition,
        _ctx: &EvalContext,
    ) -> ConditionResult<bool> {
        // Device conditions are integration-specific and require the integration
        // to provide an evaluator. For now, log a warning and return true.
        debug!(
            device_id = %condition.device_id,
            domain = %condition.domain,
            r#type = %condition.r#type,
            "Device conditions not yet implemented, returning true"
        );
        Ok(true)
    }

    // --- Helper methods ---

    fn get_attribute_value(&self, entity_id: &str, attribute: &str) -> ConditionResult<String> {
        let state = self
            .state_machine
            .get(entity_id)
            .ok_or_else(|| ConditionError::EntityNotFound(entity_id.to_string()))?;

        let value = state
            .attributes
            .get(attribute)
            .ok_or_else(|| {
                ConditionError::InvalidState(format!(
                    "Entity {} missing attribute {}",
                    entity_id, attribute
                ))
            })?
            .clone();

        // Convert JSON value to string
        Ok(json_value_to_string(&value))
    }

    fn resolve_numeric_value(&self, value: &NumericValue) -> ConditionResult<f64> {
        match value {
            NumericValue::Entity(entity_id) => {
                let state_str = self
                    .state_machine
                    .get_state(entity_id)
                    .ok_or_else(|| ConditionError::EntityNotFound(entity_id.clone()))?;

                state_str.parse().map_err(|_| {
                    ConditionError::InvalidState(format!("'{}' is not a number", state_str))
                })
            }
            NumericValue::Literal(n) => Ok(*n),
        }
    }

    fn resolve_time_spec(&self, spec: &TimeSpec) -> ConditionResult<NaiveTime> {
        match spec {
            TimeSpec::Entity(entity_id) => {
                let state_str = self
                    .state_machine
                    .get_state(entity_id)
                    .ok_or_else(|| ConditionError::EntityNotFound(entity_id.clone()))?;

                // Parse time from entity state (format: HH:MM:SS or HH:MM)
                parse_time(&state_str).ok_or_else(|| {
                    ConditionError::InvalidState(format!("'{}' is not a valid time", state_str))
                })
            }
            TimeSpec::Fixed(time) => Ok(*time),
        }
    }

    fn matches_regex(&self, value: &str, pattern: &StateMatch) -> ConditionResult<bool> {
        let patterns = match pattern {
            StateMatch::Single(p) => vec![p.as_str()],
            StateMatch::List(ps) => ps.iter().map(|s| s.as_str()).collect(),
        };

        for pattern in patterns {
            let re = Regex::new(pattern)
                .map_err(|e| ConditionError::InvalidConfig(format!("Invalid regex: {}", e)))?;

            if re.is_match(value) {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

// --- Utility functions ---

/// Check if a string value is truthy (like Python's bool())
fn is_truthy(value: &str) -> bool {
    let trimmed = value.trim().to_lowercase();

    // Empty string is false
    if trimmed.is_empty() {
        return false;
    }

    // Common false values
    matches!(trimmed.as_str(), "false" | "no" | "off" | "0" | "none")
        .then_some(false)
        .unwrap_or(true)
}

/// Convert JSON value to string for comparison
fn json_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => value.to_string(),
    }
}

/// Parse time string in various formats
fn parse_time(s: &str) -> Option<NaiveTime> {
    // Try HH:MM:SS
    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M:%S") {
        return Some(t);
    }

    // Try HH:MM
    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M") {
        return Some(t);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::condition::WeekdaySpec;
    use crate::trigger::EntityIdSpec;
    use chrono::TimeZone;
    use ha_core::{Context, EntityId};
    use ha_event_bus::EventBus;

    fn make_test_evaluator() -> (ConditionEvaluator, Arc<StateMachine>) {
        let event_bus = Arc::new(EventBus::new());
        let state_machine = Arc::new(StateMachine::new(event_bus));
        let template_engine = Arc::new(TemplateEngine::new(state_machine.clone()));
        let evaluator = ConditionEvaluator::new(state_machine.clone(), template_engine);
        (evaluator, state_machine)
    }

    fn set_state(
        sm: &StateMachine,
        entity_id: &str,
        state: &str,
        attrs: HashMap<String, serde_json::Value>,
    ) {
        let (domain, object_id) = entity_id.split_once('.').unwrap();
        let eid = EntityId::new(domain, object_id).unwrap();
        sm.set(eid, state, attrs, Context::new());
    }

    #[test]
    fn test_state_condition_simple() {
        let (evaluator, sm) = make_test_evaluator();
        set_state(&sm, "light.living_room", "on", HashMap::new());

        let condition = Condition::State(StateCondition {
            entity_id: EntityIdSpec::Single("light.living_room".to_string()),
            state: StateMatch::Single("on".to_string()),
            attribute: None,
            r#for: None,
            match_regex: false,
        });

        let ctx = EvalContext::new();
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_state_condition_multiple_states() {
        let (evaluator, sm) = make_test_evaluator();
        set_state(&sm, "person.john", "home", HashMap::new());

        let condition = Condition::State(StateCondition {
            entity_id: EntityIdSpec::Single("person.john".to_string()),
            state: StateMatch::List(vec!["home".to_string(), "work".to_string()]),
            attribute: None,
            r#for: None,
            match_regex: false,
        });

        let ctx = EvalContext::new();
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_state_condition_attribute() {
        let (evaluator, sm) = make_test_evaluator();
        set_state(
            &sm,
            "light.living_room",
            "on",
            HashMap::from([("brightness".to_string(), serde_json::json!(255))]),
        );

        let condition = Condition::State(StateCondition {
            entity_id: EntityIdSpec::Single("light.living_room".to_string()),
            state: StateMatch::Single("255".to_string()),
            attribute: Some("brightness".to_string()),
            r#for: None,
            match_regex: false,
        });

        let ctx = EvalContext::new();
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_state_condition_regex() {
        let (evaluator, sm) = make_test_evaluator();
        set_state(&sm, "sensor.status", "running_fast", HashMap::new());

        let condition = Condition::State(StateCondition {
            entity_id: EntityIdSpec::Single("sensor.status".to_string()),
            state: StateMatch::Single("running.*".to_string()),
            attribute: None,
            r#for: None,
            match_regex: true,
        });

        let ctx = EvalContext::new();
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_numeric_state_condition() {
        let (evaluator, sm) = make_test_evaluator();
        set_state(&sm, "sensor.temperature", "75", HashMap::new());

        let condition = Condition::NumericState(NumericStateCondition {
            entity_id: EntityIdSpec::Single("sensor.temperature".to_string()),
            attribute: None,
            above: Some(NumericValue::Literal(70.0)),
            below: Some(NumericValue::Literal(80.0)),
            value_template: None,
        });

        let ctx = EvalContext::new();
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());

        // Value outside range
        set_state(&sm, "sensor.temperature", "85", HashMap::new());
        assert!(!evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_time_condition() {
        let (evaluator, _sm) = make_test_evaluator();

        let condition = Condition::Time(TimeCondition {
            after: Some(TimeSpec::Fixed(NaiveTime::from_hms_opt(0, 0, 0).unwrap())),
            before: Some(TimeSpec::Fixed(
                NaiveTime::from_hms_opt(23, 59, 59).unwrap(),
            )),
            weekday: vec![],
        });

        let ctx = EvalContext::new();
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_time_condition_weekday() {
        let (evaluator, _sm) = make_test_evaluator();

        // Create a condition that only allows Monday
        let condition = Condition::Time(TimeCondition {
            after: None,
            before: None,
            weekday: vec![WeekdaySpec::Mon],
        });

        // Test with a known Monday
        let monday = chrono::Local
            .with_ymd_and_hms(2024, 1, 1, 12, 0, 0)
            .unwrap();
        let ctx = EvalContext::new().with_time(monday);
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());

        // Test with a known Tuesday
        let tuesday = chrono::Local
            .with_ymd_and_hms(2024, 1, 2, 12, 0, 0)
            .unwrap();
        let ctx = EvalContext::new().with_time(tuesday);
        assert!(!evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_template_condition() {
        let (evaluator, sm) = make_test_evaluator();
        set_state(&sm, "light.test", "on", HashMap::new());

        let condition = Condition::Template(TemplateCondition {
            value_template: "{{ is_state('light.test', 'on') }}".to_string(),
        });

        let ctx = EvalContext::new();
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_trigger_condition() {
        let (evaluator, _sm) = make_test_evaluator();

        let condition = Condition::Trigger(TriggerCondition {
            id: "motion_detected".to_string(),
        });

        // Without trigger data
        let ctx = EvalContext::new();
        assert!(!evaluator.evaluate(&condition, &ctx).unwrap());

        // With matching trigger
        let trigger = TriggerData::new("state").with_id("motion_detected");
        let ctx = EvalContext::with_trigger(trigger);
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());

        // With non-matching trigger
        let trigger = TriggerData::new("state").with_id("other_trigger");
        let ctx = EvalContext::with_trigger(trigger);
        assert!(!evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_and_condition() {
        let (evaluator, sm) = make_test_evaluator();
        set_state(&sm, "light.one", "on", HashMap::new());
        set_state(&sm, "light.two", "on", HashMap::new());

        let condition = Condition::and(vec![
            Condition::State(StateCondition {
                entity_id: EntityIdSpec::Single("light.one".to_string()),
                state: StateMatch::Single("on".to_string()),
                attribute: None,
                r#for: None,
                match_regex: false,
            }),
            Condition::State(StateCondition {
                entity_id: EntityIdSpec::Single("light.two".to_string()),
                state: StateMatch::Single("on".to_string()),
                attribute: None,
                r#for: None,
                match_regex: false,
            }),
        ]);

        let ctx = EvalContext::new();
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());

        // Turn one light off
        set_state(&sm, "light.two", "off", HashMap::new());
        assert!(!evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_or_condition() {
        let (evaluator, sm) = make_test_evaluator();
        set_state(&sm, "light.one", "off", HashMap::new());
        set_state(&sm, "light.two", "on", HashMap::new());

        let condition = Condition::or(vec![
            Condition::State(StateCondition {
                entity_id: EntityIdSpec::Single("light.one".to_string()),
                state: StateMatch::Single("on".to_string()),
                attribute: None,
                r#for: None,
                match_regex: false,
            }),
            Condition::State(StateCondition {
                entity_id: EntityIdSpec::Single("light.two".to_string()),
                state: StateMatch::Single("on".to_string()),
                attribute: None,
                r#for: None,
                match_regex: false,
            }),
        ]);

        let ctx = EvalContext::new();
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());

        // Turn both lights off
        set_state(&sm, "light.two", "off", HashMap::new());
        assert!(!evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_not_condition() {
        let (evaluator, sm) = make_test_evaluator();
        set_state(&sm, "light.test", "off", HashMap::new());

        let condition = Condition::not(Condition::State(StateCondition {
            entity_id: EntityIdSpec::Single("light.test".to_string()),
            state: StateMatch::Single("on".to_string()),
            attribute: None,
            r#for: None,
            match_regex: false,
        }));

        let ctx = EvalContext::new();
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());

        // Turn light on
        set_state(&sm, "light.test", "on", HashMap::new());
        assert!(!evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_zone_condition() {
        let (evaluator, sm) = make_test_evaluator();
        set_state(&sm, "person.john", "home", HashMap::new());

        let condition = Condition::Zone(ZoneCondition {
            entity_id: EntityIdSpec::Single("person.john".to_string()),
            zone: "home".to_string(),
        });

        let ctx = EvalContext::new();
        assert!(evaluator.evaluate(&condition, &ctx).unwrap());

        // Change location
        set_state(&sm, "person.john", "work", HashMap::new());
        assert!(!evaluator.evaluate(&condition, &ctx).unwrap());
    }

    #[test]
    fn test_is_truthy() {
        assert!(is_truthy("true"));
        assert!(is_truthy("True"));
        assert!(is_truthy("yes"));
        assert!(is_truthy("on"));
        assert!(is_truthy("1"));
        assert!(is_truthy("hello"));

        assert!(!is_truthy("false"));
        assert!(!is_truthy("False"));
        assert!(!is_truthy("no"));
        assert!(!is_truthy("off"));
        assert!(!is_truthy("0"));
        assert!(!is_truthy("none"));
        assert!(!is_truthy(""));
        assert!(!is_truthy("   "));
    }

    #[test]
    fn test_eval_context() {
        let trigger = TriggerData::new("state")
            .with_id("test_trigger")
            .with_var("entity_id", serde_json::json!("light.test"));

        let ctx = EvalContext::with_trigger(trigger).with_var("custom_var", serde_json::json!(42));

        let template_ctx = ctx.to_template_context();
        assert!(template_ctx.get("trigger").is_some());
        assert!(template_ctx.get("custom_var").is_some());
    }
}
