//! Tests for utility template functions
//!
//! Tests iif, distance, and other utility functions.
//! Based on Python Home Assistant's test_template.py utility tests.

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
    TemplateEngine::new(state_machine)
}

fn setup_engine_with_trackers() -> TemplateEngine {
    let event_bus = Arc::new(EventBus::new());
    let state_machine = Arc::new(StateMachine::new(event_bus));

    // Add device trackers with GPS coordinates
    state_machine.set(
        EntityId::new("device_tracker", "phone").unwrap(),
        "home",
        HashMap::from([
            ("latitude".to_string(), json!(40.7128)),
            ("longitude".to_string(), json!(-74.0060)),
            ("friendly_name".to_string(), json!("Phone")),
        ]),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("device_tracker", "car").unwrap(),
        "not_home",
        HashMap::from([
            ("latitude".to_string(), json!(34.0522)),
            ("longitude".to_string(), json!(-118.2437)),
            ("friendly_name".to_string(), json!("Car")),
        ]),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("zone", "home").unwrap(),
        "zoning",
        HashMap::from([
            ("latitude".to_string(), json!(40.7128)),
            ("longitude".to_string(), json!(-74.0060)),
            ("radius".to_string(), json!(100)),
        ]),
        Context::new(),
    );

    TemplateEngine::new(state_machine)
}

// ==================== iif() function tests ====================

#[test]
fn test_iif_true_condition() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ iif(true, 'yes', 'no') }}").unwrap(),
        "yes"
    );
}

#[test]
fn test_iif_false_condition() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ iif(false, 'yes', 'no') }}").unwrap(),
        "no"
    );
}

#[test]
fn test_iif_with_expression() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ iif(5 > 3, 'bigger', 'smaller') }}")
            .unwrap(),
        "bigger"
    );
}

#[test]
fn test_iif_with_state_check() {
    let event_bus = Arc::new(EventBus::new());
    let state_machine = Arc::new(StateMachine::new(event_bus));
    state_machine.set(
        EntityId::new("light", "test").unwrap(),
        "on",
        HashMap::new(),
        Context::new(),
    );
    let engine = TemplateEngine::new(state_machine);

    assert_eq!(
        engine
            .render("{{ iif(is_state('light.test', 'on'), 'Light is on', 'Light is off') }}")
            .unwrap(),
        "Light is on"
    );
}

#[test]
fn test_iif_numeric_values() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ iif(true, 100, 0) }}").unwrap(), "100");
}

#[test]
fn test_iif_with_none_condition() {
    let engine = setup_engine();
    // When condition is None/null, should use if_none parameter if provided
    let result = engine
        .render_with_context(
            "{{ iif(value, 'has value', 'no value') }}",
            serde_json::json!({"value": null}),
        )
        .unwrap();
    assert_eq!(result, "no value");
}

#[test]
fn test_iif_truthy_string() {
    let engine = setup_engine();
    // Non-empty string is truthy
    assert_eq!(
        engine
            .render("{{ iif('hello', 'truthy', 'falsy') }}")
            .unwrap(),
        "truthy"
    );
}

#[test]
fn test_iif_falsy_empty_string() {
    let engine = setup_engine();
    // Empty string is falsy
    assert_eq!(
        engine.render("{{ iif('', 'truthy', 'falsy') }}").unwrap(),
        "falsy"
    );
}

#[test]
fn test_iif_truthy_nonzero() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ iif(42, 'truthy', 'falsy') }}").unwrap(),
        "truthy"
    );
}

#[test]
fn test_iif_falsy_zero() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ iif(0, 'truthy', 'falsy') }}").unwrap(),
        "falsy"
    );
}

// ==================== distance() function tests ====================

#[test]
fn test_distance_between_coordinates() {
    let engine = setup_engine();
    // Distance from NYC (40.7128, -74.0060) to LA (34.0522, -118.2437)
    // Should be approximately 3935-3944 km
    let result = engine
        .render("{{ distance(40.7128, -74.0060, 34.0522, -118.2437) }}")
        .unwrap();
    let dist: f64 = result.parse().expect("Should be a number");
    assert!(dist > 3900.0 && dist < 4000.0);
}

#[test]
fn test_distance_same_point() {
    let engine = setup_engine();
    let result = engine
        .render("{{ distance(40.7128, -74.0060, 40.7128, -74.0060) }}")
        .unwrap();
    let dist: f64 = result.parse().expect("Should be a number");
    assert!(dist < 0.1); // Should be essentially 0
}

#[test]
fn test_distance_london_to_paris() {
    let engine = setup_engine();
    // London (51.5074, -0.1278) to Paris (48.8566, 2.3522)
    // Should be approximately 343 km
    let result = engine
        .render("{{ distance(51.5074, -0.1278, 48.8566, 2.3522) }}")
        .unwrap();
    let dist: f64 = result.parse().expect("Should be a number");
    assert!(dist > 330.0 && dist < 360.0);
}

#[test]
fn test_distance_southern_hemisphere() {
    let engine = setup_engine();
    // Sydney to Auckland
    // Sydney (-33.8688, 151.2093) to Auckland (-36.8485, 174.7633)
    // Should be approximately 2155 km
    let result = engine
        .render("{{ distance(-33.8688, 151.2093, -36.8485, 174.7633) }}")
        .unwrap();
    let dist: f64 = result.parse().expect("Should be a number");
    assert!(dist > 2100.0 && dist < 2200.0);
}

// ==================== closest() function tests ====================

// Note: closest() requires zone entities which we may not have fully implemented
// These tests are placeholders for when that functionality is added

// ==================== Conditional expression tests ====================

#[test]
fn test_ternary_operator() {
    let engine = setup_engine();
    // Minijinja supports Python-style conditional expressions
    assert_eq!(
        engine.render("{{ 'yes' if true else 'no' }}").unwrap(),
        "yes"
    );
}

#[test]
fn test_ternary_with_comparison() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ 'big' if 10 > 5 else 'small' }}").unwrap(),
        "big"
    );
}

#[test]
fn test_nested_iif() {
    let engine = setup_engine();
    let result = engine
        .render("{{ iif(5 > 10, 'big', iif(5 > 3, 'medium', 'small')) }}")
        .unwrap();
    assert_eq!(result, "medium");
}

// ==================== Boolean logic tests ====================

#[test]
fn test_and_operator() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ true and true }}").unwrap(), "true");
    assert_eq!(engine.render("{{ true and false }}").unwrap(), "false");
    assert_eq!(engine.render("{{ false and true }}").unwrap(), "false");
    assert_eq!(engine.render("{{ false and false }}").unwrap(), "false");
}

#[test]
fn test_or_operator() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ true or true }}").unwrap(), "true");
    assert_eq!(engine.render("{{ true or false }}").unwrap(), "true");
    assert_eq!(engine.render("{{ false or true }}").unwrap(), "true");
    assert_eq!(engine.render("{{ false or false }}").unwrap(), "false");
}

#[test]
fn test_not_operator() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ not true }}").unwrap(), "false");
    assert_eq!(engine.render("{{ not false }}").unwrap(), "true");
}

#[test]
fn test_complex_boolean_expression() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ (true and false) or (not false) }}")
            .unwrap(),
        "true"
    );
}

// ==================== Comparison operators tests ====================

#[test]
fn test_equality() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 5 == 5 }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 5 == 3 }}").unwrap(), "false");
}

#[test]
fn test_inequality() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 5 != 3 }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 5 != 5 }}").unwrap(), "false");
}

#[test]
fn test_less_than() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 3 < 5 }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 5 < 3 }}").unwrap(), "false");
}

#[test]
fn test_less_than_or_equal() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 3 <= 5 }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 5 <= 5 }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 6 <= 5 }}").unwrap(), "false");
}

#[test]
fn test_greater_than() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 5 > 3 }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 3 > 5 }}").unwrap(), "false");
}

#[test]
fn test_greater_than_or_equal() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 5 >= 3 }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 5 >= 5 }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 4 >= 5 }}").unwrap(), "false");
}

#[test]
fn test_string_comparison() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'abc' == 'abc' }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 'abc' == 'def' }}").unwrap(), "false");
}

// ==================== in operator tests ====================

#[test]
fn test_in_list() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 2 in [1, 2, 3] }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 5 in [1, 2, 3] }}").unwrap(), "false");
}

#[test]
fn test_in_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'ell' in 'hello' }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 'xyz' in 'hello' }}").unwrap(), "false");
}

// ==================== List operations ====================

#[test]
fn test_list_length() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ [1, 2, 3] | length }}").unwrap(), "3");
}

#[test]
fn test_list_first() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ [1, 2, 3] | first }}").unwrap(), "1");
}

#[test]
fn test_list_last() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ [1, 2, 3] | last }}").unwrap(), "3");
}

#[test]
fn test_list_join() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ [1, 2, 3] | join(', ') }}").unwrap(),
        "1, 2, 3"
    );
}

#[test]
fn test_list_reverse() {
    let engine = setup_engine();
    let result = engine.render("{{ [1, 2, 3] | reverse | list }}").unwrap();
    // Should be [3, 2, 1]
    assert!(result.starts_with("[3"));
}

#[test]
fn test_list_sort() {
    let engine = setup_engine();
    let result = engine.render("{{ [3, 1, 2] | sort | list }}").unwrap();
    // Should be [1, 2, 3]
    assert!(result.starts_with("[1"));
}
