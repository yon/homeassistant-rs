//! States object for accessing entity states in templates
//!
//! Provides the `states` object that allows templates to access entity states.

use ha_core::State;
use ha_state_machine::StateMachine;
use minijinja::value::{Object, ObjectRepr, Value};
use minijinja::{Error, ErrorKind};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;

/// Helper to convert Value to f64
fn value_to_f64(value: &Value) -> Option<f64> {
    f64::try_from(value.clone())
        .ok()
        .or_else(|| value.as_i64().map(|i| i as f64))
}

/// Helper to convert Value to bool
fn value_to_bool(value: &Value) -> Option<bool> {
    bool::try_from(value.clone()).ok()
}

/// The states object exposed to templates
///
/// Allows access to entity states via:
/// - `states('entity_id')` - Get state value as string
/// - `states.entity_id` - Get full state object
/// - `states.domain` - Get domain proxy for `states.domain.entity`
#[derive(Clone)]
pub struct StatesObject {
    state_machine: Arc<StateMachine>,
}

impl std::fmt::Debug for StatesObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatesObject").finish_non_exhaustive()
    }
}

impl StatesObject {
    pub fn new(state_machine: Arc<StateMachine>) -> Self {
        Self { state_machine }
    }

    /// Get the state value as a string
    pub fn get_state(&self, entity_id: &str) -> Option<String> {
        self.state_machine.get_state(entity_id)
    }

    /// Get the full state object
    pub fn get_full_state(&self, entity_id: &str) -> Option<State> {
        self.state_machine.get(entity_id)
    }

    /// Check if entity is in a specific state
    pub fn is_state(&self, entity_id: &str, state: &str) -> bool {
        self.state_machine.is_state(entity_id, state)
    }

    /// Check if entity is in any of the specified states
    pub fn is_state_any(&self, entity_id: &str, states: &[&str]) -> bool {
        if let Some(current) = self.get_state(entity_id) {
            states.iter().any(|s| *s == current)
        } else {
            false
        }
    }

    /// Get an attribute value
    pub fn state_attr(&self, entity_id: &str, attribute: &str) -> Value {
        self.state_machine
            .get(entity_id)
            .and_then(|s| s.attributes.get(attribute).cloned())
            .map(json_to_value)
            .unwrap_or(Value::UNDEFINED)
    }

    /// Check if entity attribute matches value
    pub fn is_state_attr(&self, entity_id: &str, attribute: &str, value: Value) -> bool {
        let attr_value = self.state_attr(entity_id, attribute);
        values_equal(&attr_value, &value)
    }

    /// Check if entity has a meaningful value (not unknown/unavailable)
    pub fn has_value(&self, entity_id: &str) -> bool {
        if let Some(state) = self.state_machine.get(entity_id) {
            !state.is_unavailable() && !state.is_unknown()
        } else {
            false
        }
    }

    /// Get all entities for a domain
    pub fn domain_entities(&self, domain: &str) -> Vec<String> {
        self.state_machine.entity_ids(domain)
    }
}

impl Object for StatesObject {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Plain
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key = key.as_str()?;

        // Check if this is a full entity_id (domain.object_id)
        if key.contains('.') {
            return self.get_full_state(key).map(state_to_value);
        }

        // Otherwise, return a domain proxy
        Some(Value::from_object(DomainProxy {
            domain: key.to_string(),
            state_machine: self.state_machine.clone(),
        }))
    }

    fn call(self: &Arc<Self>, _state: &minijinja::State, args: &[Value]) -> Result<Value, Error> {
        // states('entity_id') -> returns state string
        let entity_id = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
            Error::new(ErrorKind::InvalidOperation, "states() requires entity_id")
        })?;

        Ok(self
            .get_state(entity_id)
            .map(Value::from)
            .unwrap_or(Value::UNDEFINED))
    }
}

/// Proxy for accessing entities by domain
///
/// Allows `states.light.living_room` syntax
#[derive(Clone)]
struct DomainProxy {
    domain: String,
    state_machine: Arc<StateMachine>,
}

impl std::fmt::Debug for DomainProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DomainProxy")
            .field("domain", &self.domain)
            .finish_non_exhaustive()
    }
}

impl Object for DomainProxy {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Plain
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let object_id = key.as_str()?;
        let entity_id = format!("{}.{}", self.domain, object_id);

        self.state_machine.get(&entity_id).map(state_to_value)
    }

    fn call(self: &Arc<Self>, _state: &minijinja::State, _args: &[Value]) -> Result<Value, Error> {
        // Return all entities in this domain as a list
        let entities: Vec<Value> = self
            .state_machine
            .domain_states(&self.domain)
            .into_iter()
            .map(state_to_value)
            .collect();

        Ok(Value::from(entities))
    }
}

/// Convert a State to a template Value
fn state_to_value(state: State) -> Value {
    Value::from_object(StateWrapper(state))
}

/// Wrapper for State to expose to templates
#[derive(Debug, Clone)]
pub struct StateWrapper(pub State);

impl std::fmt::Display for StateWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.state)
    }
}

impl Object for StateWrapper {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Plain
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key = key.as_str()?;
        match key {
            "state" => Some(Value::from(self.0.state.as_str())),
            "entity_id" => Some(Value::from(self.0.entity_id.to_string())),
            "domain" => Some(Value::from(self.0.entity_id.domain())),
            "object_id" => Some(Value::from(self.0.entity_id.object_id())),
            "name" => {
                // Use friendly_name attribute or fall back to object_id
                self.0
                    .attributes
                    .get("friendly_name")
                    .and_then(|v| v.as_str().map(|s| Value::from(s)))
                    .or_else(|| Some(Value::from(self.0.entity_id.object_id())))
            }
            "last_changed" => Some(Value::from(self.0.last_changed.to_rfc3339())),
            "last_updated" => Some(Value::from(self.0.last_updated.to_rfc3339())),
            "attributes" => {
                let attrs: HashMap<String, Value> = self
                    .0
                    .attributes
                    .iter()
                    .map(|(k, v)| (k.clone(), json_to_value(v.clone())))
                    .collect();
                Some(Value::from_object(attrs))
            }
            _ => {
                // Check if it's an attribute
                self.0.attributes.get(key).map(|v| json_to_value(v.clone()))
            }
        }
    }
}

/// Convert serde_json::Value to minijinja Value
fn json_to_value(json: serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::from(()),
        serde_json::Value::Bool(b) => Value::from(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(f) = n.as_f64() {
                Value::from(f)
            } else {
                Value::from(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::from(s),
        serde_json::Value::Array(arr) => {
            Value::from(arr.into_iter().map(json_to_value).collect::<Vec<_>>())
        }
        serde_json::Value::Object(obj) => {
            let map: std::collections::BTreeMap<String, Value> = obj
                .into_iter()
                .map(|(k, v)| (k, json_to_value(v)))
                .collect();
            Value::from_object(map)
        }
    }
}

/// Compare two Values for equality
fn values_equal(a: &Value, b: &Value) -> bool {
    if a.is_undefined() && b.is_undefined() {
        return true;
    }
    if a.is_none() && b.is_none() {
        return true;
    }

    // Try string comparison
    if let (Some(a_str), Some(b_str)) = (a.as_str(), b.as_str()) {
        return a_str == b_str;
    }

    // Try numeric comparison
    if let (Some(a_num), Some(b_num)) = (value_to_f64(a), value_to_f64(b)) {
        return (a_num - b_num).abs() < f64::EPSILON;
    }

    if let (Some(a_num), Some(b_num)) = (a.as_i64(), b.as_i64()) {
        return a_num == b_num;
    }

    // Try bool comparison
    if let (Some(a_bool), Some(b_bool)) = (value_to_bool(a), value_to_bool(b)) {
        return a_bool == b_bool;
    }

    false
}

/// Function wrapper for is_state
pub fn is_state_fn(states: Arc<StatesObject>, entity_id: &str, state: Value) -> bool {
    // Check for string first (strings are iterable in minijinja, so check this first)
    if let Some(s) = state.as_str() {
        states.is_state(entity_id, s)
    } else if let Ok(iter) = state.try_iter() {
        // Check against multiple states (list/array)
        let states_vec: Vec<String> = iter
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let refs: Vec<&str> = states_vec.iter().map(|s| s.as_str()).collect();
        states.is_state_any(entity_id, &refs)
    } else {
        false
    }
}

/// Function wrapper for state_attr
pub fn state_attr_fn(states: Arc<StatesObject>, entity_id: &str, attribute: &str) -> Value {
    states.state_attr(entity_id, attribute)
}

/// Function wrapper for is_state_attr
pub fn is_state_attr_fn(
    states: Arc<StatesObject>,
    entity_id: &str,
    attribute: &str,
    value: Value,
) -> bool {
    states.is_state_attr(entity_id, attribute, value)
}

/// Function wrapper for has_value
pub fn has_value_fn(states: Arc<StatesObject>, entity_id: &str) -> bool {
    states.has_value(entity_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ha_core::{Context, EntityId};
    use ha_event_bus::EventBus;
    use std::collections::HashMap;

    fn make_test_setup() -> (Arc<StateMachine>, StatesObject) {
        let event_bus = Arc::new(EventBus::new());
        let state_machine = Arc::new(StateMachine::new(event_bus));

        // Add some test states
        state_machine.set(
            EntityId::new("light", "living_room").unwrap(),
            "on",
            HashMap::from([
                ("brightness".to_string(), serde_json::json!(255)),
                (
                    "friendly_name".to_string(),
                    serde_json::json!("Living Room Light"),
                ),
            ]),
            Context::new(),
        );

        state_machine.set(
            EntityId::new("sensor", "temperature").unwrap(),
            "23.5",
            HashMap::from([("unit_of_measurement".to_string(), serde_json::json!("°C"))]),
            Context::new(),
        );

        state_machine.set(
            EntityId::new("switch", "unavailable_device").unwrap(),
            "unavailable",
            HashMap::new(),
            Context::new(),
        );

        let states = StatesObject::new(state_machine.clone());
        (state_machine, states)
    }

    #[test]
    fn test_get_state() {
        let (_, states) = make_test_setup();
        assert_eq!(
            states.get_state("light.living_room"),
            Some("on".to_string())
        );
        assert_eq!(
            states.get_state("sensor.temperature"),
            Some("23.5".to_string())
        );
        assert_eq!(states.get_state("nonexistent.entity"), None);
    }

    #[test]
    fn test_is_state() {
        let (_, states) = make_test_setup();
        assert!(states.is_state("light.living_room", "on"));
        assert!(!states.is_state("light.living_room", "off"));
    }

    #[test]
    fn test_is_state_any() {
        let (_, states) = make_test_setup();
        assert!(states.is_state_any("light.living_room", &["on", "off"]));
        assert!(!states.is_state_any("light.living_room", &["off", "unavailable"]));
    }

    #[test]
    fn test_state_attr() {
        let (_, states) = make_test_setup();
        let brightness = states.state_attr("light.living_room", "brightness");
        assert_eq!(brightness.as_i64(), Some(255));

        let name = states.state_attr("light.living_room", "friendly_name");
        assert_eq!(name.as_str(), Some("Living Room Light"));
    }

    #[test]
    fn test_has_value() {
        let (_, states) = make_test_setup();
        assert!(states.has_value("light.living_room"));
        assert!(!states.has_value("switch.unavailable_device"));
        assert!(!states.has_value("nonexistent.entity"));
    }

    #[test]
    fn test_state_wrapper_display() {
        let (sm, _) = make_test_setup();
        let state = sm.get("light.living_room").unwrap();
        let wrapper = StateWrapper(state);
        assert_eq!(format!("{}", wrapper), "on");
    }
}
