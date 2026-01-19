//! Input Helper Components
//!
//! Implements input_boolean and input_number components for user-controlled
//! state in automations.

use ha_core::{Context, EntityId, ServiceCall, SupportsResponse};
use ha_service_registry::{ServiceDescription, ServiceRegistry};
use ha_state_machine::StateMachine;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

// =============================================================================
// Input Boolean
// =============================================================================

/// Input boolean configuration from YAML
#[derive(Debug, Clone, Deserialize)]
pub struct InputBooleanConfig {
    /// Display name
    #[serde(default)]
    pub name: Option<String>,
    /// Icon (e.g., "mdi:bug")
    #[serde(default)]
    pub icon: Option<String>,
    /// Initial state (default: false/off)
    #[serde(default)]
    pub initial: Option<bool>,
}

/// Load input_boolean entities from config and register them in the state machine
pub fn load_input_booleans(
    config: &HashMap<String, Option<InputBooleanConfig>>,
    states: &StateMachine,
) -> usize {
    let mut count = 0;

    for (id, config) in config {
        let entity_id = match EntityId::new("input_boolean", id) {
            Ok(e) => e,
            Err(e) => {
                warn!("Invalid input_boolean id '{}': {}", id, e);
                continue;
            }
        };

        let config = config.clone().unwrap_or(InputBooleanConfig {
            name: None,
            icon: None,
            initial: None,
        });

        let state = if config.initial.unwrap_or(false) {
            "on"
        } else {
            "off"
        };

        let mut attributes = HashMap::new();
        if let Some(name) = &config.name {
            attributes.insert("friendly_name".to_string(), json!(name));
        }
        if let Some(icon) = &config.icon {
            attributes.insert("icon".to_string(), json!(icon));
        }
        attributes.insert("editable".to_string(), json!(false));

        states.set(entity_id.clone(), state, attributes, Context::new());
        debug!("Loaded input_boolean.{} = {}", id, state);
        count += 1;
    }

    if count > 0 {
        info!("Loaded {} input_boolean entities", count);
    }
    count
}

/// Register input_boolean services
pub fn register_input_boolean_services(services: &ServiceRegistry, states: Arc<StateMachine>) {
    const DOMAIN: &str = "input_boolean";

    // turn_on service
    let states_clone = states.clone();
    services.register_with_description(
        ServiceDescription {
            domain: DOMAIN.to_string(),
            service: "turn_on".to_string(),
            name: Some("Turn on".to_string()),
            description: Some("Turn on an input boolean".to_string()),
            schema: None,
            target: Some(json!({"entity": {"domain": "input_boolean"}})),
            supports_response: SupportsResponse::None,
        },
        move |call: ServiceCall| {
            let states = states_clone.clone();
            async move {
                for entity_id in get_target_entities(&call, "input_boolean") {
                    if let Some(current) = states.get(&entity_id.to_string()) {
                        let attrs = current.attributes.clone();
                        states.set(entity_id, "on", attrs, call.context.clone());
                    }
                }
                Ok(None)
            }
        },
    );

    // turn_off service
    let states_clone = states.clone();
    services.register_with_description(
        ServiceDescription {
            domain: DOMAIN.to_string(),
            service: "turn_off".to_string(),
            name: Some("Turn off".to_string()),
            description: Some("Turn off an input boolean".to_string()),
            schema: None,
            target: Some(json!({"entity": {"domain": "input_boolean"}})),
            supports_response: SupportsResponse::None,
        },
        move |call: ServiceCall| {
            let states = states_clone.clone();
            async move {
                for entity_id in get_target_entities(&call, "input_boolean") {
                    if let Some(current) = states.get(&entity_id.to_string()) {
                        let attrs = current.attributes.clone();
                        states.set(entity_id, "off", attrs, call.context.clone());
                    }
                }
                Ok(None)
            }
        },
    );

    // toggle service
    let states_clone = states.clone();
    services.register_with_description(
        ServiceDescription {
            domain: DOMAIN.to_string(),
            service: "toggle".to_string(),
            name: Some("Toggle".to_string()),
            description: Some("Toggle an input boolean".to_string()),
            schema: None,
            target: Some(json!({"entity": {"domain": "input_boolean"}})),
            supports_response: SupportsResponse::None,
        },
        move |call: ServiceCall| {
            let states = states_clone.clone();
            async move {
                for entity_id in get_target_entities(&call, "input_boolean") {
                    if let Some(current) = states.get(&entity_id.to_string()) {
                        let new_state = if current.state == "on" { "off" } else { "on" };
                        let attrs = current.attributes.clone();
                        states.set(entity_id, new_state, attrs, call.context.clone());
                    }
                }
                Ok(None)
            }
        },
    );

    info!("Input boolean services registered");
}

// =============================================================================
// Input Number
// =============================================================================

/// Input number configuration from YAML
#[derive(Debug, Clone, Deserialize)]
pub struct InputNumberConfig {
    /// Display name
    #[serde(default)]
    pub name: Option<String>,
    /// Icon
    #[serde(default)]
    pub icon: Option<String>,
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Step value (default: 1)
    #[serde(default = "default_step")]
    pub step: f64,
    /// Initial value
    #[serde(default)]
    pub initial: Option<f64>,
    /// Unit of measurement
    #[serde(default)]
    pub unit_of_measurement: Option<String>,
    /// Display mode (slider or box)
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_step() -> f64 {
    1.0
}

fn default_mode() -> String {
    "slider".to_string()
}

/// Load input_number entities from config and register them in the state machine
pub fn load_input_numbers(
    config: &HashMap<String, InputNumberConfig>,
    states: &StateMachine,
) -> usize {
    let mut count = 0;

    for (id, config) in config {
        let entity_id = match EntityId::new("input_number", id) {
            Ok(e) => e,
            Err(e) => {
                warn!("Invalid input_number id '{}': {}", id, e);
                continue;
            }
        };

        // Validate min/max
        if config.min >= config.max {
            warn!(
                "input_number.{}: min ({}) must be less than max ({})",
                id, config.min, config.max
            );
            continue;
        }

        // Determine initial value
        let initial = config.initial.unwrap_or(config.min);
        let value = initial.clamp(config.min, config.max);

        let mut attributes = HashMap::new();
        if let Some(name) = &config.name {
            attributes.insert("friendly_name".to_string(), json!(name));
        }
        if let Some(icon) = &config.icon {
            attributes.insert("icon".to_string(), json!(icon));
        }
        if let Some(unit) = &config.unit_of_measurement {
            attributes.insert("unit_of_measurement".to_string(), json!(unit));
        }
        attributes.insert("min".to_string(), json!(config.min));
        attributes.insert("max".to_string(), json!(config.max));
        attributes.insert("step".to_string(), json!(config.step));
        attributes.insert("mode".to_string(), json!(config.mode));
        attributes.insert("editable".to_string(), json!(false));
        if let Some(init) = config.initial {
            attributes.insert("initial".to_string(), json!(init));
        }

        // Store state as string representation of the number
        let state_str = format_number(value);
        states.set(entity_id.clone(), &state_str, attributes, Context::new());
        debug!("Loaded input_number.{} = {}", id, state_str);
        count += 1;
    }

    if count > 0 {
        info!("Loaded {} input_number entities", count);
    }
    count
}

/// Format a number for state display (remove trailing zeros)
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{:.0}", value)
    } else {
        format!("{}", value)
    }
}

/// Register input_number services
pub fn register_input_number_services(services: &ServiceRegistry, states: Arc<StateMachine>) {
    const DOMAIN: &str = "input_number";

    // set_value service
    let states_clone = states.clone();
    services.register_with_description(
        ServiceDescription {
            domain: DOMAIN.to_string(),
            service: "set_value".to_string(),
            name: Some("Set value".to_string()),
            description: Some("Set the value of an input number".to_string()),
            schema: Some(json!({
                "value": {"required": true, "selector": {"number": {}}}
            })),
            target: Some(json!({"entity": {"domain": "input_number"}})),
            supports_response: SupportsResponse::None,
        },
        move |call: ServiceCall| {
            let states = states_clone.clone();
            async move {
                let value = call
                    .service_data
                    .get("value")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                for entity_id in get_target_entities(&call, "input_number") {
                    if let Some(current) = states.get(&entity_id.to_string()) {
                        let min = current
                            .attributes
                            .get("min")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        let max = current
                            .attributes
                            .get("max")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(100.0);

                        let clamped = value.clamp(min, max);
                        let attrs = current.attributes.clone();
                        states.set(
                            entity_id,
                            format_number(clamped),
                            attrs,
                            call.context.clone(),
                        );
                    }
                }
                Ok(None)
            }
        },
    );

    // increment service
    let states_clone = states.clone();
    services.register_with_description(
        ServiceDescription {
            domain: DOMAIN.to_string(),
            service: "increment".to_string(),
            name: Some("Increment".to_string()),
            description: Some("Increment the value of an input number".to_string()),
            schema: None,
            target: Some(json!({"entity": {"domain": "input_number"}})),
            supports_response: SupportsResponse::None,
        },
        move |call: ServiceCall| {
            let states = states_clone.clone();
            async move {
                for entity_id in get_target_entities(&call, "input_number") {
                    if let Some(current) = states.get(&entity_id.to_string()) {
                        let value: f64 = current.state.parse().unwrap_or(0.0);
                        let step = current
                            .attributes
                            .get("step")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0);
                        let max = current
                            .attributes
                            .get("max")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(100.0);

                        let new_value = (value + step).min(max);
                        let attrs = current.attributes.clone();
                        states.set(
                            entity_id,
                            format_number(new_value),
                            attrs,
                            call.context.clone(),
                        );
                    }
                }
                Ok(None)
            }
        },
    );

    // decrement service
    let states_clone = states.clone();
    services.register_with_description(
        ServiceDescription {
            domain: DOMAIN.to_string(),
            service: "decrement".to_string(),
            name: Some("Decrement".to_string()),
            description: Some("Decrement the value of an input number".to_string()),
            schema: None,
            target: Some(json!({"entity": {"domain": "input_number"}})),
            supports_response: SupportsResponse::None,
        },
        move |call: ServiceCall| {
            let states = states_clone.clone();
            async move {
                for entity_id in get_target_entities(&call, "input_number") {
                    if let Some(current) = states.get(&entity_id.to_string()) {
                        let value: f64 = current.state.parse().unwrap_or(0.0);
                        let step = current
                            .attributes
                            .get("step")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0);
                        let min = current
                            .attributes
                            .get("min")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);

                        let new_value = (value - step).max(min);
                        let attrs = current.attributes.clone();
                        states.set(
                            entity_id,
                            format_number(new_value),
                            attrs,
                            call.context.clone(),
                        );
                    }
                }
                Ok(None)
            }
        },
    );

    info!("Input number services registered");
}

// =============================================================================
// Helpers
// =============================================================================

/// Extract target entity IDs from a service call
fn get_target_entities(call: &ServiceCall, domain: &str) -> Vec<EntityId> {
    let mut entities = Vec::new();

    // Check for entity_id in service_data
    if let Some(entity_id) = call.service_data.get("entity_id") {
        if let Some(id_str) = entity_id.as_str() {
            if let Ok(entity) = EntityId::try_from(id_str.to_string()) {
                entities.push(entity);
            }
        } else if let Some(ids) = entity_id.as_array() {
            for id in ids {
                if let Some(id_str) = id.as_str() {
                    if let Ok(entity) = EntityId::try_from(id_str.to_string()) {
                        entities.push(entity);
                    }
                }
            }
        }
    }

    // Filter to only include entities from the specified domain
    entities
        .into_iter()
        .filter(|e| e.domain() == domain)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number_integer() {
        assert_eq!(format_number(120.0), "120");
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(-5.0), "-5");
    }

    #[test]
    fn test_format_number_decimal() {
        assert_eq!(format_number(120.5), "120.5");
        assert_eq!(format_number(0.123), "0.123");
    }

    #[test]
    fn test_input_boolean_config_deserialize() {
        let yaml = r#"
            name: Debug Mode
            icon: mdi:bug
            initial: true
        "#;
        let config: InputBooleanConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, Some("Debug Mode".to_string()));
        assert_eq!(config.icon, Some("mdi:bug".to_string()));
        assert_eq!(config.initial, Some(true));
    }

    #[test]
    fn test_input_number_config_deserialize() {
        let yaml = r#"
            name: Target Temperature
            min: 100
            max: 140
            step: 1
            initial: 120
            icon: mdi:thermometer
        "#;
        let config: InputNumberConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, Some("Target Temperature".to_string()));
        assert_eq!(config.min, 100.0);
        assert_eq!(config.max, 140.0);
        assert_eq!(config.step, 1.0);
        assert_eq!(config.initial, Some(120.0));
    }
}
