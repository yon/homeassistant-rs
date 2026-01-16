//! Tests for state-related template functions
//!
//! Tests states(), is_state(), state_attr(), has_value(), and related functions.
//! Based on Python Home Assistant's test_template.py state tests.

use ha_core::{Context, EntityId};
use ha_event_bus::EventBus;
use ha_state_machine::StateMachine;
use ha_template::TemplateEngine;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

fn setup_engine() -> TemplateEngine {
    let event_bus = Arc::new(EventBus::new());
    let state_machine = Arc::new(StateMachine::new(event_bus));

    // Add test entities
    state_machine.set(
        EntityId::new("light", "living_room").unwrap(),
        "on",
        HashMap::from([
            ("brightness".to_string(), json!(255)),
            ("friendly_name".to_string(), json!("Living Room Light")),
            ("color_temp".to_string(), json!(400)),
        ]),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("light", "bedroom").unwrap(),
        "off",
        HashMap::from([
            ("friendly_name".to_string(), json!("Bedroom Light")),
        ]),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("sensor", "temperature").unwrap(),
        "23.5",
        HashMap::from([
            ("unit_of_measurement".to_string(), json!("°C")),
            ("friendly_name".to_string(), json!("Temperature")),
        ]),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("sensor", "humidity").unwrap(),
        "65",
        HashMap::from([
            ("unit_of_measurement".to_string(), json!("%")),
        ]),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("switch", "unavailable_device").unwrap(),
        "unavailable",
        HashMap::new(),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("switch", "unknown_device").unwrap(),
        "unknown",
        HashMap::new(),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("device_tracker", "paulus").unwrap(),
        "home",
        HashMap::from([
            ("battery".to_string(), json!(85)),
            ("latitude".to_string(), json!(52.3731)),
            ("longitude".to_string(), json!(4.8922)),
        ]),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("binary_sensor", "motion").unwrap(),
        "on",
        HashMap::from([
            ("device_class".to_string(), json!("motion")),
        ]),
        Context::new(),
    );

    TemplateEngine::new(state_machine)
}

// ==================== states() function tests ====================

#[test]
fn test_states_function_returns_state_value() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ states('light.living_room') }}").unwrap(),
        "on"
    );
}

#[test]
fn test_states_function_returns_numeric_state() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ states('sensor.temperature') }}").unwrap(),
        "23.5"
    );
}

#[test]
fn test_states_function_nonexistent_entity_returns_undefined() {
    let engine = setup_engine();
    let result = engine.render("{{ states('sensor.nonexistent') }}").unwrap();
    // Undefined renders as empty string in minijinja
    assert!(result.is_empty() || result == "undefined");
}

#[test]
fn test_states_object_access() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ states.light.living_room.state }}").unwrap(),
        "on"
    );
}

#[test]
fn test_states_object_domain_access() {
    let engine = setup_engine();
    // Access state via domain.object_id
    assert_eq!(
        engine.render("{{ states.sensor.temperature.state }}").unwrap(),
        "23.5"
    );
}

#[test]
fn test_states_object_entity_id_attribute() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ states.light.living_room.entity_id }}").unwrap(),
        "light.living_room"
    );
}

#[test]
fn test_states_object_domain_attribute() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ states.light.living_room.domain }}").unwrap(),
        "light"
    );
}

#[test]
fn test_states_object_name_attribute() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ states.light.living_room.name }}").unwrap(),
        "Living Room Light"
    );
}

// ==================== is_state() function tests ====================

#[test]
fn test_is_state_true() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ is_state('light.living_room', 'on') }}").unwrap(),
        "true"
    );
}

#[test]
fn test_is_state_false() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ is_state('light.living_room', 'off') }}").unwrap(),
        "false"
    );
}

#[test]
fn test_is_state_with_list_matching() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ is_state('light.living_room', ['on', 'off']) }}").unwrap(),
        "true"
    );
}

#[test]
fn test_is_state_with_list_not_matching() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ is_state('light.living_room', ['off', 'unavailable']) }}").unwrap(),
        "false"
    );
}

#[test]
fn test_is_state_nonexistent_entity() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ is_state('light.nonexistent', 'on') }}").unwrap(),
        "false"
    );
}

#[test]
fn test_is_state_unavailable() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ is_state('switch.unavailable_device', 'unavailable') }}").unwrap(),
        "true"
    );
}

#[test]
fn test_is_state_unknown() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ is_state('switch.unknown_device', 'unknown') }}").unwrap(),
        "true"
    );
}

// ==================== state_attr() function tests ====================

#[test]
fn test_state_attr_returns_value() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ state_attr('light.living_room', 'brightness') }}").unwrap(),
        "255"
    );
}

#[test]
fn test_state_attr_returns_string() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ state_attr('light.living_room', 'friendly_name') }}").unwrap(),
        "Living Room Light"
    );
}

#[test]
fn test_state_attr_nonexistent_attribute() {
    let engine = setup_engine();
    let result = engine.render("{{ state_attr('light.living_room', 'nonexistent') }}").unwrap();
    assert!(result.is_empty() || result == "undefined");
}

#[test]
fn test_state_attr_nonexistent_entity() {
    let engine = setup_engine();
    let result = engine.render("{{ state_attr('light.nonexistent', 'brightness') }}").unwrap();
    assert!(result.is_empty() || result == "undefined");
}

#[test]
fn test_state_attr_numeric_comparison() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ state_attr('device_tracker.paulus', 'battery') > 50 }}").unwrap(),
        "true"
    );
}

#[test]
fn test_is_state_attr_matching() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ is_state_attr('light.living_room', 'brightness', 255) }}").unwrap(),
        "true"
    );
}

#[test]
fn test_is_state_attr_not_matching() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ is_state_attr('light.living_room', 'brightness', 100) }}").unwrap(),
        "false"
    );
}

// ==================== has_value() function tests ====================

#[test]
fn test_has_value_true() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ has_value('light.living_room') }}").unwrap(),
        "true"
    );
}

#[test]
fn test_has_value_false_unavailable() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ has_value('switch.unavailable_device') }}").unwrap(),
        "false"
    );
}

#[test]
fn test_has_value_false_unknown() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ has_value('switch.unknown_device') }}").unwrap(),
        "false"
    );
}

#[test]
fn test_has_value_nonexistent_entity() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ has_value('switch.nonexistent') }}").unwrap(),
        "false"
    );
}

// ==================== Complex state access tests ====================

#[test]
fn test_state_access_via_attributes() {
    let engine = setup_engine();
    // Access attributes directly from state object
    let result = engine.render("{{ states.light.living_room.attributes.brightness }}").unwrap();
    assert_eq!(result, "255");
}

#[test]
fn test_conditional_based_on_state() {
    let engine = setup_engine();
    let template = r#"
{%- if is_state('light.living_room', 'on') -%}
Light is on at {{ state_attr('light.living_room', 'brightness') }}%
{%- else -%}
Light is off
{%- endif -%}
"#;
    let result = engine.render(template).unwrap();
    assert_eq!(result.trim(), "Light is on at 255%");
}

#[test]
fn test_conditional_else_branch() {
    let engine = setup_engine();
    let template = r#"
{%- if is_state('light.bedroom', 'on') -%}
Light is on
{%- else -%}
Light is off
{%- endif -%}
"#;
    let result = engine.render(template).unwrap();
    assert_eq!(result.trim(), "Light is off");
}

#[test]
fn test_sensor_state_as_number() {
    let engine = setup_engine();
    // Test that sensor state can be used in calculations
    let result = engine.render("{{ states('sensor.temperature') | float * 2 }}").unwrap();
    assert_eq!(result, "47.0");
}

#[test]
fn test_multiple_state_checks() {
    let engine = setup_engine();
    let template = r#"{{ is_state('light.living_room', 'on') and is_state('binary_sensor.motion', 'on') }}"#;
    let result = engine.render(template).unwrap();
    assert_eq!(result, "true");
}

#[test]
fn test_state_in_arithmetic() {
    let engine = setup_engine();
    let template = r#"{{ states('sensor.temperature') | float + states('sensor.humidity') | float }}"#;
    let result = engine.render(template).unwrap();
    assert_eq!(result, "88.5");
}

#[test]
fn test_direct_attribute_access_shorthand() {
    let engine = setup_engine();
    // Access attribute directly on state object (not via .attributes)
    let result = engine.render("{{ states.light.living_room.brightness }}").unwrap();
    assert_eq!(result, "255");
}
