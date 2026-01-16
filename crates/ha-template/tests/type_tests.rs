//! Tests for type conversion and type checking template functions
//!
//! Tests float, int, bool, is_number, is_string, is_list, is_defined, etc.
//! Based on Python Home Assistant's test_template.py type tests.

use ha_event_bus::EventBus;
use ha_state_machine::StateMachine;
use ha_template::TemplateEngine;
use std::sync::Arc;

fn setup_engine() -> TemplateEngine {
    let event_bus = Arc::new(EventBus::new());
    let state_machine = Arc::new(StateMachine::new(event_bus));
    TemplateEngine::new(state_machine)
}

// ==================== float filter tests ====================

#[test]
fn test_float_from_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '3.14' | float }}").unwrap(), "3.14");
}

#[test]
fn test_float_from_integer_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '42' | float }}").unwrap(), "42.0");
}

#[test]
fn test_float_from_integer() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 42 | float }}").unwrap(), "42.0");
}

#[test]
fn test_float_from_negative() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '-3.14' | float }}").unwrap(), "-3.14");
}

#[test]
fn test_float_invalid_with_default() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ 'not_a_number' | float(0) }}").unwrap(),
        "0.0"
    );
}

#[test]
fn test_float_scientific_notation() {
    let engine = setup_engine();
    let result = engine.render("{{ '1.5e2' | float }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - 150.0).abs() < 0.0001);
}

// ==================== int filter tests ====================

#[test]
fn test_int_from_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '42' | int }}").unwrap(), "42");
}

#[test]
fn test_int_from_float_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '3.9' | int }}").unwrap(), "3");
}

#[test]
fn test_int_from_float() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 3.9 | int }}").unwrap(), "3");
}

#[test]
fn test_int_from_negative() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '-42' | int }}").unwrap(), "-42");
}

#[test]
fn test_int_invalid_with_default() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'not_a_number' | int(0) }}").unwrap(), "0");
}

#[test]
fn test_int_truncates_decimal() {
    let engine = setup_engine();
    // Should truncate, not round
    assert_eq!(engine.render("{{ 3.99 | int }}").unwrap(), "3");
}

// ==================== bool filter tests ====================

#[test]
fn test_bool_true_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'true' | bool }}").unwrap(), "true");
}

#[test]
fn test_bool_false_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'false' | bool }}").unwrap(), "false");
}

#[test]
fn test_bool_yes_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'yes' | bool }}").unwrap(), "true");
}

#[test]
fn test_bool_no_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'no' | bool }}").unwrap(), "false");
}

#[test]
fn test_bool_on_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'on' | bool }}").unwrap(), "true");
}

#[test]
fn test_bool_off_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'off' | bool }}").unwrap(), "false");
}

#[test]
fn test_bool_1_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '1' | bool }}").unwrap(), "true");
}

#[test]
fn test_bool_0_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '0' | bool }}").unwrap(), "false");
}

#[test]
fn test_bool_enable_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'enable' | bool }}").unwrap(), "true");
}

#[test]
fn test_bool_disable_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'disable' | bool }}").unwrap(), "false");
}

#[test]
fn test_bool_from_integer_nonzero() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 1 | bool }}").unwrap(), "true");
}

#[test]
fn test_bool_from_integer_zero() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 0 | bool }}").unwrap(), "false");
}

#[test]
fn test_bool_case_insensitive() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'TRUE' | bool }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 'FALSE' | bool }}").unwrap(), "false");
    assert_eq!(engine.render("{{ 'Yes' | bool }}").unwrap(), "true");
    assert_eq!(engine.render("{{ 'No' | bool }}").unwrap(), "false");
}

// ==================== is_number filter tests ====================

#[test]
fn test_is_number_integer() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 42 | is_number }}").unwrap(), "true");
}

#[test]
fn test_is_number_float() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 3.14 | is_number }}").unwrap(), "true");
}

#[test]
fn test_is_number_string_numeric() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '42' | is_number }}").unwrap(), "true");
}

#[test]
fn test_is_number_string_nonnumeric() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'hello' | is_number }}").unwrap(), "false");
}

#[test]
fn test_is_number_negative() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ -5 | is_number }}").unwrap(), "true");
}

#[test]
fn test_is_number_negative_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '-5' | is_number }}").unwrap(), "true");
}

// ==================== is_string filter tests ====================

#[test]
fn test_is_string_true() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'hello' | is_string }}").unwrap(), "true");
}

#[test]
fn test_is_string_number_false() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 42 | is_string }}").unwrap(), "false");
}

#[test]
fn test_is_string_empty() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '' | is_string }}").unwrap(), "true");
}

// ==================== is_list filter tests ====================

#[test]
fn test_is_list_true() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ items | is_list }}",
            serde_json::json!({"items": [1, 2, 3]}),
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn test_is_list_false_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 'hello' | is_list }}").unwrap(), "false");
}

#[test]
fn test_is_list_false_number() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 42 | is_list }}").unwrap(), "false");
}

#[test]
fn test_is_list_empty() {
    let engine = setup_engine();
    let result = engine
        .render_with_context("{{ items | is_list }}", serde_json::json!({"items": []}))
        .unwrap();
    assert_eq!(result, "true");
}

// ==================== is_defined test tests ====================

#[test]
fn test_is_defined_true() {
    let engine = setup_engine();
    let result = engine
        .render_with_context("{{ value is defined }}", serde_json::json!({"value": 42}))
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn test_is_defined_false() {
    let engine = setup_engine();
    let result = engine
        .render_with_context("{{ missing is defined }}", serde_json::json!({"other": 42}))
        .unwrap();
    assert_eq!(result, "false");
}

// ==================== typeof function tests ====================

#[test]
fn test_typeof_integer() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ typeof(42) }}").unwrap(), "integer");
}

#[test]
fn test_typeof_float() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ typeof(3.14) }}").unwrap(), "float");
}

#[test]
fn test_typeof_string() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ typeof('hello') }}").unwrap(), "string");
}

#[test]
fn test_typeof_boolean() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ typeof(true) }}").unwrap(), "boolean");
}

#[test]
fn test_typeof_list() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ typeof(items) }}",
            serde_json::json!({"items": [1, 2, 3]}),
        )
        .unwrap();
    assert_eq!(result, "list");
}

#[test]
fn test_typeof_dict() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ typeof(obj) }}",
            serde_json::json!({"obj": {"key": "value"}}),
        )
        .unwrap();
    // Could be "object" or "mapping" depending on implementation
    assert!(result == "object" || result == "mapping" || result == "dict");
}

// ==================== range function tests ====================

#[test]
fn test_range_basic() {
    let engine = setup_engine();
    let result = engine.render("{{ range(5) | list }}").unwrap();
    assert!(result.contains("0") && result.contains("4"));
}

#[test]
fn test_range_start_end() {
    let engine = setup_engine();
    let result = engine.render("{{ range(2, 5) | list }}").unwrap();
    assert!(result.contains("2") && result.contains("3") && result.contains("4"));
    assert!(!result.contains("5")); // End is exclusive
}

#[test]
fn test_range_with_step() {
    let engine = setup_engine();
    let result = engine.render("{{ range(0, 10, 2) | list }}").unwrap();
    // Should be 0, 2, 4, 6, 8
    assert!(result.contains("0") && result.contains("2") && result.contains("4"));
}

// ==================== Default value tests ====================

#[test]
fn test_default_filter_used() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ missing | default('fallback') }}",
            serde_json::json!({"other": 42}),
        )
        .unwrap();
    assert_eq!(result, "fallback");
}

#[test]
fn test_default_filter_not_used() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ value | default('fallback') }}",
            serde_json::json!({"value": "actual"}),
        )
        .unwrap();
    assert_eq!(result, "actual");
}

// ==================== Type coercion in expressions ====================

#[test]
fn test_string_to_number_comparison() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '42' | int > 10 }}").unwrap(), "true");
}

#[test]
fn test_float_to_int_in_expression() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ (3.7 | int) + 1 }}").unwrap(), "4");
}
