//! Trigger evaluation and matching logic
//!
//! This module provides the runtime evaluation of triggers against incoming events.
//! When an event matches a trigger, it produces TriggerData that can be used
//! by conditions and actions.

use chrono::{DateTime, Local, NaiveTime, Timelike, Utc};
use ha_core::events::{StateChangedData, STATE_CHANGED};
use ha_core::{Event, State};
use ha_state_store::StateStore;
use ha_template::TemplateEngine;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, trace};

use crate::trigger::{
    EventTrigger, HassEvent, HomeassistantTrigger, NumericStateTrigger, NumericValue, StateTrigger,
    SunEvent, SunTrigger, TemplateTrigger, TimePatternTrigger, TimeSpec, TimeTrigger, Trigger,
    TriggerData, TriggerError, TriggerResult, ZoneEvent, ZoneTrigger,
};

/// Context for trigger evaluation
///
/// Contains runtime state needed to evaluate triggers, such as the previous
/// state of entities for state triggers with duration requirements.
#[derive(Debug, Clone, Default)]
pub struct TriggerEvalContext {
    /// Additional variables available in templates
    pub variables: HashMap<String, serde_json::Value>,
}

impl TriggerEvalContext {
    /// Create a new empty context
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a variable to the context
    pub fn with_var(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.variables.insert(key.into(), value);
        self
    }
}

/// Trigger evaluator
///
/// Evaluates whether incoming events match trigger configurations and produces
/// TriggerData when they do.
pub struct TriggerEvaluator {
    state_machine: Arc<StateStore>,
    template_engine: Arc<TemplateEngine>,
}

impl TriggerEvaluator {
    /// Create a new trigger evaluator
    pub fn new(state_machine: Arc<StateStore>, template_engine: Arc<TemplateEngine>) -> Self {
        Self {
            state_machine,
            template_engine,
        }
    }

    /// Evaluate a trigger against an event
    ///
    /// Returns Some(TriggerData) if the trigger matched, None otherwise.
    pub fn evaluate(
        &self,
        trigger: &Trigger,
        event: &Event<serde_json::Value>,
        ctx: &TriggerEvalContext,
    ) -> TriggerResult<Option<TriggerData>> {
        match trigger {
            Trigger::State(t) => self.eval_state_trigger(t, event),
            Trigger::Event(t) => self.eval_event_trigger(t, event),
            Trigger::NumericState(t) => self.eval_numeric_state_trigger(t, event),
            Trigger::Homeassistant(t) => self.eval_homeassistant_trigger(t, event),
            Trigger::Zone(t) => self.eval_zone_trigger(t, event),
            Trigger::Template(t) => self.eval_template_trigger(t, event, ctx),
            // Time-based triggers don't respond to events - they use scheduling
            Trigger::Time(_) | Trigger::TimePattern(_) | Trigger::Sun(_) => Ok(None),
            // Webhook triggers are handled by the HTTP layer
            Trigger::Webhook(_) => Ok(None),
        }
    }

    /// Check if a trigger should fire at a given time
    ///
    /// Used for time-based triggers (time, time_pattern, sun).
    pub fn should_fire_at_time(
        &self,
        trigger: &Trigger,
        time: DateTime<Local>,
    ) -> TriggerResult<bool> {
        match trigger {
            Trigger::Time(t) => self.check_time_trigger(t, time),
            Trigger::TimePattern(t) => self.check_time_pattern_trigger(t, time),
            Trigger::Sun(t) => self.check_sun_trigger(t, time),
            _ => Ok(false),
        }
    }

    /// Create TriggerData for a time-based trigger
    pub fn create_time_trigger_data(&self, trigger: &Trigger) -> TriggerData {
        let platform = trigger.platform();
        let mut data = TriggerData::new(platform);

        if let Some(id) = trigger.id() {
            data = data.with_id(id);
        }

        data
    }

    // --- State trigger evaluation ---

    fn eval_state_trigger(
        &self,
        trigger: &StateTrigger,
        event: &Event<serde_json::Value>,
    ) -> TriggerResult<Option<TriggerData>> {
        // Only match state_changed events
        if event.event_type.as_str() != STATE_CHANGED {
            return Ok(None);
        }

        // Parse state change data
        let state_data: StateChangedData =
            serde_json::from_value(event.data.clone()).map_err(|e| {
                TriggerError::InvalidConfig(format!("Invalid state change data: {}", e))
            })?;

        let entity_id_str = state_data.entity_id.to_string();

        // Check if this entity is being monitored
        let monitored_ids = trigger.entity_id.ids();
        if !monitored_ids.contains(&entity_id_str.as_str()) {
            return Ok(None);
        }

        debug!(
            entity_id = %entity_id_str,
            from = ?trigger.from,
            to = ?trigger.to,
            "Evaluating state trigger"
        );

        // Get old and new values
        let (old_value, new_value) = if let Some(attr) = &trigger.attribute {
            // Check attribute changes
            let old_val = state_data
                .old_state
                .as_ref()
                .and_then(|s| s.attributes.get(attr))
                .map(json_value_to_string);
            let new_val = state_data
                .new_state
                .as_ref()
                .and_then(|s| s.attributes.get(attr))
                .map(json_value_to_string);
            (old_val, new_val)
        } else {
            // Check state changes
            let old_val = state_data.old_state.as_ref().map(|s| s.state.clone());
            let new_val = state_data.new_state.as_ref().map(|s| s.state.clone());
            (old_val, new_val)
        };

        trace!(?old_value, ?new_value, "State values");

        // Check not_from filter
        if let Some(old) = &old_value {
            if trigger.not_from.contains(old) {
                trace!("Filtered by not_from");
                return Ok(None);
            }
        }

        // Check not_to filter
        if let Some(new) = &new_value {
            if trigger.not_to.contains(new) {
                trace!("Filtered by not_to");
                return Ok(None);
            }
        }

        // Check from constraint
        if let Some(from_match) = &trigger.from {
            match &old_value {
                Some(old) if from_match.matches(old) => {}
                _ => {
                    trace!("From state doesn't match");
                    return Ok(None);
                }
            }
        }

        // Check to constraint
        if let Some(to_match) = &trigger.to {
            match &new_value {
                Some(new) if to_match.matches(new) => {}
                _ => {
                    trace!("To state doesn't match");
                    return Ok(None);
                }
            }
        }

        // If neither from nor to is specified, trigger on any change
        if trigger.from.is_none() && trigger.to.is_none() {
            // Must have actually changed
            if old_value == new_value {
                trace!("No actual change");
                return Ok(None);
            }
        }

        // TODO: Handle 'for' duration constraint
        // This requires tracking when the state changed and waiting

        // Build trigger data
        let mut data = TriggerData::new("state")
            .with_var("entity_id", serde_json::json!(entity_id_str))
            .with_var(
                "from_state",
                serde_json::to_value(&state_data.old_state).unwrap_or_default(),
            )
            .with_var(
                "to_state",
                serde_json::to_value(&state_data.new_state).unwrap_or_default(),
            );

        if let Some(id) = &trigger.id {
            data = data.with_id(id);
        }

        if let Some(attr) = &trigger.attribute {
            data = data.with_var("attribute", serde_json::json!(attr));
        }

        debug!("State trigger matched");
        Ok(Some(data))
    }

    // --- Event trigger evaluation ---

    fn eval_event_trigger(
        &self,
        trigger: &EventTrigger,
        event: &Event<serde_json::Value>,
    ) -> TriggerResult<Option<TriggerData>> {
        // Check event type matches
        if event.event_type.as_str() != trigger.event_type {
            return Ok(None);
        }

        debug!(
            event_type = %trigger.event_type,
            "Evaluating event trigger"
        );

        // Check event data if specified
        if let Some(expected_data) = &trigger.event_data {
            if !json_matches(&event.data, expected_data) {
                trace!("Event data doesn't match");
                return Ok(None);
            }
        }

        // Check context filter if specified
        if let Some(ctx_filter) = &trigger.context {
            if let Some(expected_user) = &ctx_filter.user_id {
                if event.context.user_id.as_deref() != Some(expected_user.as_str()) {
                    trace!("Context user_id doesn't match");
                    return Ok(None);
                }
            }
        }

        // Build trigger data
        let mut data = TriggerData::new("event")
            .with_var("event_type", serde_json::json!(trigger.event_type))
            .with_var("event", event.data.clone());

        if let Some(id) = &trigger.id {
            data = data.with_id(id);
        }

        debug!("Event trigger matched");
        Ok(Some(data))
    }

    // --- Numeric state trigger evaluation ---

    fn eval_numeric_state_trigger(
        &self,
        trigger: &NumericStateTrigger,
        event: &Event<serde_json::Value>,
    ) -> TriggerResult<Option<TriggerData>> {
        // Only match state_changed events
        if event.event_type.as_str() != STATE_CHANGED {
            return Ok(None);
        }

        let state_data: StateChangedData =
            serde_json::from_value(event.data.clone()).map_err(|e| {
                TriggerError::InvalidConfig(format!("Invalid state change data: {}", e))
            })?;

        let entity_id_str = state_data.entity_id.to_string();

        // Check if this entity is being monitored
        let monitored_ids = trigger.entity_id.ids();
        if !monitored_ids.contains(&entity_id_str.as_str()) {
            return Ok(None);
        }

        debug!(
            entity_id = %entity_id_str,
            above = ?trigger.above,
            below = ?trigger.below,
            "Evaluating numeric state trigger"
        );

        // Get old and new values
        let get_numeric_value = |state: &Option<State>| -> Option<f64> {
            state.as_ref().and_then(|s| {
                if let Some(attr) = &trigger.attribute {
                    s.attributes.get(attr).and_then(json_to_f64)
                } else {
                    s.state.parse().ok()
                }
            })
        };

        let old_value = get_numeric_value(&state_data.old_state);
        let new_value = get_numeric_value(&state_data.new_state);

        trace!(?old_value, ?new_value, "Numeric values");

        let Some(new_val) = new_value else {
            trace!("New value is not numeric");
            return Ok(None);
        };

        // Resolve threshold values
        let above_threshold = trigger
            .above
            .as_ref()
            .map(|v| self.resolve_numeric_value(v))
            .transpose()?;

        let below_threshold = trigger
            .below
            .as_ref()
            .map(|v| self.resolve_numeric_value(v))
            .transpose()?;

        // Check if value crossed threshold
        let crossed = match (above_threshold, below_threshold) {
            (Some(above), Some(below)) => {
                // Must be in range and have crossed into it
                let in_range = new_val > above && new_val < below;
                let was_in_range = old_value
                    .map(|old| old > above && old < below)
                    .unwrap_or(false);
                in_range && !was_in_range
            }
            (Some(above), None) => {
                // Must have crossed above threshold
                let is_above = new_val > above;
                let was_above = old_value.map(|old| old > above).unwrap_or(false);
                is_above && !was_above
            }
            (None, Some(below)) => {
                // Must have crossed below threshold
                let is_below = new_val < below;
                let was_below = old_value.map(|old| old < below).unwrap_or(false);
                is_below && !was_below
            }
            (None, None) => {
                // No thresholds, trigger on any numeric change
                old_value != Some(new_val)
            }
        };

        if !crossed {
            trace!("Threshold not crossed");
            return Ok(None);
        }

        // TODO: Handle 'for' duration constraint

        // Build trigger data
        let mut data = TriggerData::new("numeric_state")
            .with_var("entity_id", serde_json::json!(entity_id_str))
            .with_var(
                "from_state",
                serde_json::to_value(&state_data.old_state).unwrap_or_default(),
            )
            .with_var(
                "to_state",
                serde_json::to_value(&state_data.new_state).unwrap_or_default(),
            )
            .with_var("below", serde_json::json!(below_threshold))
            .with_var("above", serde_json::json!(above_threshold));

        if let Some(id) = &trigger.id {
            data = data.with_id(id);
        }

        debug!("Numeric state trigger matched");
        Ok(Some(data))
    }

    // --- Homeassistant trigger evaluation ---

    fn eval_homeassistant_trigger(
        &self,
        trigger: &HomeassistantTrigger,
        event: &Event<serde_json::Value>,
    ) -> TriggerResult<Option<TriggerData>> {
        let expected_event_type = match trigger.event {
            HassEvent::Shutdown => "homeassistant_stop",
            HassEvent::Start => "homeassistant_start",
        };

        if event.event_type.as_str() != expected_event_type {
            return Ok(None);
        }

        let mut data = TriggerData::new("homeassistant");

        if let Some(id) = &trigger.id {
            data = data.with_id(id);
        }

        debug!(event = ?trigger.event, "Homeassistant trigger matched");
        Ok(Some(data))
    }

    // --- Zone trigger evaluation ---

    fn eval_zone_trigger(
        &self,
        trigger: &ZoneTrigger,
        event: &Event<serde_json::Value>,
    ) -> TriggerResult<Option<TriggerData>> {
        // Only match state_changed events
        if event.event_type.as_str() != STATE_CHANGED {
            return Ok(None);
        }

        let state_data: StateChangedData =
            serde_json::from_value(event.data.clone()).map_err(|e| {
                TriggerError::InvalidConfig(format!("Invalid state change data: {}", e))
            })?;

        let entity_id_str = state_data.entity_id.to_string();

        // Check if this entity is being monitored
        let monitored_ids = trigger.entity_id.ids();
        if !monitored_ids.contains(&entity_id_str.as_str()) {
            return Ok(None);
        }

        // Get zone name (strip "zone." prefix if present)
        let zone_name = trigger.zone.strip_prefix("zone.").unwrap_or(&trigger.zone);

        // Get old and new zone states
        let old_zone = state_data.old_state.as_ref().map(|s| s.state.as_str());
        let new_zone = state_data.new_state.as_ref().map(|s| s.state.as_str());

        let matched = match trigger.event {
            ZoneEvent::Enter => {
                // Was not in zone, now is in zone
                old_zone != Some(zone_name) && new_zone == Some(zone_name)
            }
            ZoneEvent::Leave => {
                // Was in zone, now is not in zone
                old_zone == Some(zone_name) && new_zone != Some(zone_name)
            }
        };

        if !matched {
            return Ok(None);
        }

        let mut data = TriggerData::new("zone")
            .with_var("entity_id", serde_json::json!(entity_id_str))
            .with_var("zone", serde_json::json!(trigger.zone))
            .with_var(
                "event",
                serde_json::json!(format!("{:?}", trigger.event).to_lowercase()),
            )
            .with_var(
                "from_state",
                serde_json::to_value(&state_data.old_state).unwrap_or_default(),
            )
            .with_var(
                "to_state",
                serde_json::to_value(&state_data.new_state).unwrap_or_default(),
            );

        if let Some(id) = &trigger.id {
            data = data.with_id(id);
        }

        debug!(zone = zone_name, event = ?trigger.event, "Zone trigger matched");
        Ok(Some(data))
    }

    // --- Template trigger evaluation ---

    fn eval_template_trigger(
        &self,
        trigger: &TemplateTrigger,
        event: &Event<serde_json::Value>,
        _ctx: &TriggerEvalContext,
    ) -> TriggerResult<Option<TriggerData>> {
        // Template triggers check on every state change
        if event.event_type.as_str() != STATE_CHANGED {
            return Ok(None);
        }

        // Evaluate the template
        let result = self
            .template_engine
            .render(&trigger.value_template)
            .map_err(|e| TriggerError::Template(e.to_string()))?;

        let is_true = is_truthy(&result);

        if !is_true {
            return Ok(None);
        }

        // TODO: Handle 'for' duration constraint
        // Template must remain true for the duration

        let mut data = TriggerData::new("template");

        if let Some(id) = &trigger.id {
            data = data.with_id(id);
        }

        debug!(template = %trigger.value_template, "Template trigger matched");
        Ok(Some(data))
    }

    // --- Time-based trigger checks ---

    fn check_time_trigger(
        &self,
        trigger: &TimeTrigger,
        time: DateTime<Local>,
    ) -> TriggerResult<bool> {
        let target_time = match &trigger.at {
            TimeSpec::Entity(entity_id) => {
                let state_str = self
                    .state_machine
                    .get_state(entity_id)
                    .ok_or_else(|| TriggerError::EntityNotFound(entity_id.clone()))?;

                parse_time(&state_str).ok_or_else(|| {
                    TriggerError::InvalidConfig(format!("'{}' is not a valid time", state_str))
                })?
            }
            TimeSpec::Fixed(t) => *t,
        };

        let current_time = time.time();

        // Check if times match (within a second)
        let matches = current_time.hour() == target_time.hour()
            && current_time.minute() == target_time.minute()
            && current_time.second() == target_time.second();

        Ok(matches)
    }

    fn check_time_pattern_trigger(
        &self,
        trigger: &TimePatternTrigger,
        time: DateTime<Local>,
    ) -> TriggerResult<bool> {
        let current_time = time.time();

        // Check hours pattern
        if let Some(hours) = &trigger.hours {
            if !matches_time_pattern(hours, current_time.hour())? {
                return Ok(false);
            }
        }

        // Check minutes pattern
        if let Some(minutes) = &trigger.minutes {
            if !matches_time_pattern(minutes, current_time.minute())? {
                return Ok(false);
            }
        }

        // Check seconds pattern
        if let Some(seconds) = &trigger.seconds {
            if !matches_time_pattern(seconds, current_time.second())? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn check_sun_trigger(
        &self,
        trigger: &SunTrigger,
        time: DateTime<Local>,
    ) -> TriggerResult<bool> {
        // Get sun entity for sunrise/sunset times
        let sun_state = self
            .state_machine
            .get("sun.sun")
            .ok_or_else(|| TriggerError::EntityNotFound("sun.sun".to_string()))?;

        let attr_name = match trigger.event {
            SunEvent::Sunrise => "next_rising",
            SunEvent::Sunset => "next_setting",
        };

        let time_str = sun_state
            .attributes
            .get(attr_name)
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                TriggerError::InvalidConfig(format!("sun.sun missing {} attribute", attr_name))
            })?;

        let sun_time: DateTime<Utc> = DateTime::parse_from_rfc3339(time_str)
            .map_err(|e| TriggerError::InvalidConfig(format!("Invalid sun time: {}", e)))?
            .into();

        let mut target_time = sun_time.with_timezone(&Local);

        // Apply offset
        if let Some(offset_secs) = trigger.offset {
            target_time += chrono::Duration::seconds(offset_secs);
        }

        // Check if times match (within a minute since sun times change daily)
        let matches = time.date_naive() == target_time.date_naive()
            && time.hour() == target_time.hour()
            && time.minute() == target_time.minute();

        Ok(matches)
    }

    // --- Helper methods ---

    fn resolve_numeric_value(&self, value: &NumericValue) -> TriggerResult<f64> {
        match value {
            NumericValue::Entity(entity_id) => {
                let state_str = self
                    .state_machine
                    .get_state(entity_id)
                    .ok_or_else(|| TriggerError::EntityNotFound(entity_id.clone()))?;

                state_str.parse().map_err(|_| {
                    TriggerError::InvalidConfig(format!("'{}' is not a number", state_str))
                })
            }
            NumericValue::Literal(n) => Ok(*n),
        }
    }
}

// --- Utility functions ---

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

/// Convert JSON value to f64
fn json_to_f64(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// Check if actual JSON matches expected pattern
///
/// The expected pattern can be a subset of the actual data.
fn json_matches(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    match (actual, expected) {
        (serde_json::Value::Object(actual_obj), serde_json::Value::Object(expected_obj)) => {
            // All keys in expected must match in actual
            expected_obj.iter().all(|(key, expected_val)| {
                actual_obj
                    .get(key)
                    .map(|actual_val| json_matches(actual_val, expected_val))
                    .unwrap_or(false)
            })
        }
        (serde_json::Value::Array(actual_arr), serde_json::Value::Array(expected_arr)) => {
            // Arrays must match exactly
            actual_arr.len() == expected_arr.len()
                && actual_arr
                    .iter()
                    .zip(expected_arr.iter())
                    .all(|(a, e)| json_matches(a, e))
        }
        _ => actual == expected,
    }
}

/// Check if a string value is truthy
fn is_truthy(value: &str) -> bool {
    let trimmed = value.trim().to_lowercase();

    if trimmed.is_empty() {
        return false;
    }

    !matches!(trimmed.as_str(), "false" | "no" | "off" | "0" | "none")
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

/// Check if a value matches a time pattern
///
/// Patterns can be:
/// - A specific number: "5" matches 5
/// - A divisor: "/5" matches 0, 5, 10, 15, etc.
/// - A wildcard: "*" matches any value (same as not specifying)
fn matches_time_pattern(pattern: &str, value: u32) -> TriggerResult<bool> {
    let pattern = pattern.trim();

    if pattern == "*" {
        return Ok(true);
    }

    if let Some(divisor_str) = pattern.strip_prefix('/') {
        let divisor: u32 = divisor_str.parse().map_err(|_| {
            TriggerError::InvalidConfig(format!("Invalid time pattern divisor: {}", divisor_str))
        })?;
        if divisor == 0 {
            return Err(TriggerError::InvalidConfig(
                "Time pattern divisor cannot be 0".to_string(),
            ));
        }
        return Ok(value % divisor == 0);
    }

    let target: u32 = pattern
        .parse()
        .map_err(|_| TriggerError::InvalidConfig(format!("Invalid time pattern: {}", pattern)))?;

    Ok(value == target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ha_core::{Context, EntityId};
    use ha_event_bus::EventBus;

    fn make_test_evaluator() -> (TriggerEvaluator, Arc<StateStore>, Arc<EventBus>) {
        let event_bus = Arc::new(EventBus::new());
        let state_machine = Arc::new(StateStore::new(event_bus.clone()));
        let template_engine = Arc::new(TemplateEngine::new(state_machine.clone()));
        let evaluator = TriggerEvaluator::new(state_machine.clone(), template_engine);
        (evaluator, state_machine, event_bus)
    }

    fn make_state_change_event(
        entity_id: &str,
        old_state: Option<&str>,
        new_state: Option<&str>,
    ) -> Event<serde_json::Value> {
        let (domain, object_id) = entity_id.split_once('.').unwrap();
        let eid = EntityId::new(domain, object_id).unwrap();

        let old = old_state.map(|s| State::new(eid.clone(), s, HashMap::new(), Context::new()));
        let new = new_state.map(|s| State::new(eid.clone(), s, HashMap::new(), Context::new()));

        let data = StateChangedData {
            entity_id: eid,
            old_state: old,
            new_state: new,
        };

        Event::new(
            STATE_CHANGED,
            serde_json::to_value(data).unwrap(),
            Context::new(),
        )
    }

    #[test]
    fn test_state_trigger_basic() {
        let (evaluator, _sm, _bus) = make_test_evaluator();

        let trigger = Trigger::State(StateTrigger {
            id: Some("test".to_string()),
            entity_id: crate::trigger::EntityIdSpec::Single("light.living_room".to_string()),
            from: None,
            to: Some(crate::trigger::StateMatch::Single("on".to_string())),
            attribute: None,
            r#for: None,
            not_from: vec![],
            not_to: vec![],
        });

        let event = make_state_change_event("light.living_room", Some("off"), Some("on"));
        let ctx = TriggerEvalContext::new();

        let result = evaluator.evaluate(&trigger, &event, &ctx).unwrap();
        assert!(result.is_some());

        let data = result.unwrap();
        assert_eq!(data.platform, "state");
        assert_eq!(data.id, Some("test".to_string()));
    }

    #[test]
    fn test_state_trigger_no_match() {
        let (evaluator, _sm, _bus) = make_test_evaluator();

        let trigger = Trigger::State(StateTrigger {
            id: None,
            entity_id: crate::trigger::EntityIdSpec::Single("light.living_room".to_string()),
            from: None,
            to: Some(crate::trigger::StateMatch::Single("on".to_string())),
            attribute: None,
            r#for: None,
            not_from: vec![],
            not_to: vec![],
        });

        // Different entity
        let event = make_state_change_event("light.bedroom", Some("off"), Some("on"));
        let ctx = TriggerEvalContext::new();
        let result = evaluator.evaluate(&trigger, &event, &ctx).unwrap();
        assert!(result.is_none());

        // Wrong state
        let event = make_state_change_event("light.living_room", Some("off"), Some("off"));
        let result = evaluator.evaluate(&trigger, &event, &ctx).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_state_trigger_from_to() {
        let (evaluator, _sm, _bus) = make_test_evaluator();

        let trigger = Trigger::State(StateTrigger {
            id: None,
            entity_id: crate::trigger::EntityIdSpec::Single("light.test".to_string()),
            from: Some(crate::trigger::StateMatch::Single("off".to_string())),
            to: Some(crate::trigger::StateMatch::Single("on".to_string())),
            attribute: None,
            r#for: None,
            not_from: vec![],
            not_to: vec![],
        });

        let ctx = TriggerEvalContext::new();

        // Correct transition
        let event = make_state_change_event("light.test", Some("off"), Some("on"));
        let result = evaluator.evaluate(&trigger, &event, &ctx).unwrap();
        assert!(result.is_some());

        // Wrong 'from' state
        let event = make_state_change_event("light.test", Some("unavailable"), Some("on"));
        let result = evaluator.evaluate(&trigger, &event, &ctx).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_event_trigger() {
        let (evaluator, _sm, _bus) = make_test_evaluator();

        let trigger = Trigger::Event(EventTrigger {
            id: Some("button_pressed".to_string()),
            event_type: "zha_event".to_string(),
            event_data: Some(serde_json::json!({"command": "on"})),
            context: None,
        });

        let ctx = TriggerEvalContext::new();

        // Matching event
        let event = Event::new(
            "zha_event",
            serde_json::json!({"command": "on", "device_id": "abc123"}),
            Context::new(),
        );
        let result = evaluator.evaluate(&trigger, &event, &ctx).unwrap();
        assert!(result.is_some());

        // Non-matching event data
        let event = Event::new(
            "zha_event",
            serde_json::json!({"command": "off"}),
            Context::new(),
        );
        let result = evaluator.evaluate(&trigger, &event, &ctx).unwrap();
        assert!(result.is_none());

        // Wrong event type
        let event = Event::new(
            "other_event",
            serde_json::json!({"command": "on"}),
            Context::new(),
        );
        let result = evaluator.evaluate(&trigger, &event, &ctx).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_json_matches() {
        // Exact match
        assert!(json_matches(
            &serde_json::json!({"a": 1}),
            &serde_json::json!({"a": 1})
        ));

        // Subset match
        assert!(json_matches(
            &serde_json::json!({"a": 1, "b": 2}),
            &serde_json::json!({"a": 1})
        ));

        // Missing key
        assert!(!json_matches(
            &serde_json::json!({"a": 1}),
            &serde_json::json!({"b": 1})
        ));

        // Wrong value
        assert!(!json_matches(
            &serde_json::json!({"a": 1}),
            &serde_json::json!({"a": 2})
        ));

        // Nested match
        assert!(json_matches(
            &serde_json::json!({"outer": {"inner": "value"}}),
            &serde_json::json!({"outer": {"inner": "value"}})
        ));
    }

    #[test]
    fn test_time_pattern_matching() {
        assert!(matches_time_pattern("*", 5).unwrap());
        assert!(matches_time_pattern("5", 5).unwrap());
        assert!(!matches_time_pattern("5", 6).unwrap());
        assert!(matches_time_pattern("/5", 0).unwrap());
        assert!(matches_time_pattern("/5", 5).unwrap());
        assert!(matches_time_pattern("/5", 10).unwrap());
        assert!(!matches_time_pattern("/5", 3).unwrap());
    }

    #[test]
    fn test_is_truthy() {
        assert!(is_truthy("true"));
        assert!(is_truthy("yes"));
        assert!(is_truthy("1"));
        assert!(is_truthy("hello"));

        assert!(!is_truthy("false"));
        assert!(!is_truthy("no"));
        assert!(!is_truthy("0"));
        assert!(!is_truthy(""));
    }
}
