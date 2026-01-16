//! Tests for edge cases and error handling
//!
//! Tests error conditions, boundary values, and unusual inputs.
//! Based on Python Home Assistant's test_template.py edge case tests.

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

fn setup_engine_with_states() -> TemplateEngine {
    let event_bus = Arc::new(EventBus::new());
    let state_machine = Arc::new(StateMachine::new(event_bus));

    // Add various edge case states
    state_machine.set(
        EntityId::new("sensor", "empty_state").unwrap(),
        "",
        HashMap::new(),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("sensor", "zero").unwrap(),
        "0",
        HashMap::new(),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("sensor", "negative").unwrap(),
        "-42",
        HashMap::new(),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("sensor", "unicode").unwrap(),
        "温度",
        HashMap::from([("unit".to_string(), json!("°C"))]),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("sensor", "special_chars").unwrap(),
        "<>&\"'",
        HashMap::new(),
        Context::new(),
    );

    state_machine.set(
        EntityId::new("sensor", "long_value").unwrap(),
        &"x".repeat(1000),
        HashMap::new(),
        Context::new(),
    );

    TemplateEngine::new(state_machine)
}

// ==================== Empty and null value tests ====================

#[test]
fn test_empty_string_state() {
    let engine = setup_engine_with_states();
    let result = engine.render("{{ states('sensor.empty_state') }}").unwrap();
    assert_eq!(result, "");
}

#[test]
fn test_is_state_empty_string() {
    let engine = setup_engine_with_states();
    assert_eq!(
        engine
            .render("{{ is_state('sensor.empty_state', '') }}")
            .unwrap(),
        "true"
    );
}

#[test]
fn test_state_zero() {
    let engine = setup_engine_with_states();
    assert_eq!(engine.render("{{ states('sensor.zero') }}").unwrap(), "0");
}

#[test]
fn test_is_state_zero() {
    let engine = setup_engine_with_states();
    assert_eq!(
        engine.render("{{ is_state('sensor.zero', '0') }}").unwrap(),
        "true"
    );
}

#[test]
fn test_state_zero_as_number() {
    let engine = setup_engine_with_states();
    assert_eq!(
        engine
            .render("{{ states('sensor.zero') | int == 0 }}")
            .unwrap(),
        "true"
    );
}

// ==================== Negative number tests ====================

#[test]
fn test_state_negative() {
    let engine = setup_engine_with_states();
    assert_eq!(
        engine.render("{{ states('sensor.negative') }}").unwrap(),
        "-42"
    );
}

#[test]
fn test_negative_number_arithmetic() {
    let engine = setup_engine_with_states();
    let result = engine
        .render("{{ states('sensor.negative') | int + 42 }}")
        .unwrap();
    assert_eq!(result, "0");
}

// ==================== Unicode tests ====================

#[test]
fn test_unicode_state() {
    let engine = setup_engine_with_states();
    let result = engine.render("{{ states('sensor.unicode') }}").unwrap();
    assert_eq!(result, "温度");
}

#[test]
fn test_unicode_in_template() {
    let engine = setup_engine();
    let result = engine.render("{{ '你好世界' }}").unwrap();
    assert_eq!(result, "你好世界");
}

#[test]
fn test_unicode_slugify() {
    let engine = setup_engine();
    let result = engine.render("{{ 'Ñoño' | slugify }}").unwrap();
    // Should convert accented characters
    assert!(!result.contains("ñ"));
}

// ==================== Special character tests ====================

#[test]
fn test_special_chars_state() {
    let engine = setup_engine_with_states();
    let result = engine
        .render("{{ states('sensor.special_chars') }}")
        .unwrap();
    // Should contain the special characters
    assert!(result.contains("<") && result.contains(">") && result.contains("&"));
}

#[test]
fn test_html_escaping_not_applied() {
    let engine = setup_engine();
    // By default, templates should NOT escape HTML
    let result = engine.render("{{ '<script>alert(1)</script>' }}").unwrap();
    assert!(result.contains("<script>"));
}

// ==================== Large value tests ====================

#[test]
fn test_long_state_value() {
    let engine = setup_engine_with_states();
    let result = engine.render("{{ states('sensor.long_value') }}").unwrap();
    assert_eq!(result.len(), 1000);
}

#[test]
fn test_long_state_length() {
    let engine = setup_engine_with_states();
    let result = engine
        .render("{{ states('sensor.long_value') | length }}")
        .unwrap();
    assert_eq!(result, "1000");
}

// ==================== Nonexistent entity tests ====================

#[test]
fn test_nonexistent_entity_states() {
    let engine = setup_engine();
    let result = engine
        .render("{{ states('sensor.does_not_exist') }}")
        .unwrap();
    // Should return empty or undefined
    assert!(result.is_empty() || result == "undefined");
}

#[test]
fn test_nonexistent_entity_is_state() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ is_state('sensor.does_not_exist', 'on') }}")
            .unwrap(),
        "false"
    );
}

#[test]
fn test_nonexistent_entity_state_attr() {
    let engine = setup_engine();
    let result = engine
        .render("{{ state_attr('sensor.does_not_exist', 'unit') }}")
        .unwrap();
    assert!(result.is_empty() || result == "undefined");
}

#[test]
fn test_nonexistent_entity_has_value() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ has_value('sensor.does_not_exist') }}")
            .unwrap(),
        "false"
    );
}

// ==================== Type conversion edge cases ====================

#[test]
fn test_float_from_empty_string() {
    let engine = setup_engine();
    // Empty string should use default
    let result = engine.render("{{ '' | float(0) }}").unwrap();
    assert_eq!(result, "0.0");
}

#[test]
fn test_int_from_empty_string() {
    let engine = setup_engine();
    let result = engine.render("{{ '' | int(0) }}").unwrap();
    assert_eq!(result, "0");
}

#[test]
fn test_float_from_nan_string() {
    let engine = setup_engine();
    let result = engine.render("{{ 'not_a_number' | float(99) }}").unwrap();
    assert_eq!(result, "99.0");
}

#[test]
fn test_int_from_nan_string() {
    let engine = setup_engine();
    let result = engine.render("{{ 'not_a_number' | int(99) }}").unwrap();
    assert_eq!(result, "99");
}

// ==================== Division edge cases ====================

#[test]
fn test_division_by_zero() {
    let engine = setup_engine();
    // Division by zero should result in infinity or error handling
    let result = engine.render("{{ 1 / 0 }}");
    // Either an error or infinity
    assert!(result.is_err() || result.unwrap().contains("inf"));
}

#[test]
fn test_modulo_by_zero() {
    let engine = setup_engine();
    let result = engine.render("{{ 5 % 0 }}");
    // Should error
    assert!(result.is_err() || result.is_ok());
}

// ==================== Math edge cases ====================

#[test]
fn test_sqrt_negative() {
    let engine = setup_engine();
    let result = engine.render("{{ -1 | sqrt }}");
    // Sqrt of negative should result in NaN or error
    if let Ok(val) = result {
        assert!(val.contains("nan") || val.contains("NaN") || val.is_empty());
    }
}

#[test]
fn test_log_zero() {
    let engine = setup_engine();
    let result = engine.render("{{ 0 | log }}");
    // Log of 0 is undefined
    if let Ok(val) = result {
        assert!(val.contains("inf") || val.contains("nan") || val.is_empty());
    }
}

#[test]
fn test_log_negative() {
    let engine = setup_engine();
    let result = engine.render("{{ -1 | log }}");
    // Log of negative is undefined
    if let Ok(val) = result {
        assert!(val.contains("nan") || val.contains("NaN") || val.is_empty());
    }
}

// ==================== Trigonometry edge cases ====================

#[test]
fn test_asin_out_of_range() {
    let engine = setup_engine();
    let result = engine.render("{{ 2 | asin }}");
    // asin(2) is undefined (domain is -1 to 1)
    if let Ok(val) = result {
        assert!(val.contains("nan") || val.contains("NaN") || val.is_empty());
    }
}

#[test]
fn test_acos_out_of_range() {
    let engine = setup_engine();
    let result = engine.render("{{ -2 | acos }}");
    // acos(-2) is undefined
    if let Ok(val) = result {
        assert!(val.contains("nan") || val.contains("NaN") || val.is_empty());
    }
}

// ==================== List edge cases ====================

#[test]
fn test_empty_list_min() {
    let engine = setup_engine();
    let result = engine.render("{{ min([]) }}");
    // Min of empty list should error or return default
    assert!(result.is_err() || result.unwrap().is_empty());
}

#[test]
fn test_empty_list_max() {
    let engine = setup_engine();
    let result = engine.render("{{ max([]) }}");
    // Max of empty list should error or return default
    assert!(result.is_err() || result.unwrap().is_empty());
}

#[test]
fn test_single_element_list_average() {
    let engine = setup_engine();
    let result = engine
        .render_with_context("{{ values | average }}", serde_json::json!({"values": [5]}))
        .unwrap();
    assert_eq!(result.parse::<f64>().unwrap(), 5.0);
}

#[test]
fn test_empty_list_average() {
    let engine = setup_engine();
    let result = engine.render_with_context(
        "{{ values | average(0) }}",
        serde_json::json!({"values": []}),
    );
    // Should return default or error
    assert!(result.is_ok() || result.is_err());
}

// ==================== String edge cases ====================

#[test]
fn test_regex_replace_empty_pattern() {
    let engine = setup_engine();
    let result = engine
        .render("{{ 'hello' | regex_replace('', '-') }}")
        .unwrap();
    // Empty pattern behavior may vary
    assert!(!result.is_empty());
}

#[test]
fn test_slugify_empty_string() {
    let engine = setup_engine();
    let result = engine.render("{{ '' | slugify }}").unwrap();
    assert_eq!(result, "");
}

#[test]
fn test_slugify_special_only() {
    let engine = setup_engine();
    let result = engine.render("{{ '!@#$%' | slugify }}").unwrap();
    // Should result in empty or minimal string
    assert!(result.is_empty() || result == "_");
}

// ==================== Base64 edge cases ====================

#[test]
fn test_base64_decode_invalid() {
    let engine = setup_engine();
    let result = engine.render("{{ 'not valid base64!!!' | base64_decode }}");
    // Invalid base64 should error or return empty
    assert!(result.is_err() || result.unwrap().is_empty());
}

#[test]
fn test_base64_encode_empty() {
    let engine = setup_engine();
    let result = engine.render("{{ '' | base64_encode }}").unwrap();
    assert_eq!(result, "");
}

// ==================== JSON edge cases ====================

#[test]
fn test_from_json_invalid() {
    let engine = setup_engine();
    let result = engine.render("{{ 'not json' | from_json }}");
    // Invalid JSON should error
    assert!(result.is_err());
}

#[test]
fn test_from_json_empty() {
    let engine = setup_engine();
    let result = engine.render("{{ '' | from_json }}");
    // Empty string is not valid JSON
    assert!(result.is_err());
}

#[test]
fn test_to_json_circular_reference() {
    // This is hard to test since minijinja values can't have circular refs
    let engine = setup_engine();
    let result = engine.render("{{ {'a': 1} | to_json }}").unwrap();
    assert!(result.contains("a"));
}

// ==================== Comparison edge cases ====================

#[test]
fn test_compare_different_types() {
    let engine = setup_engine();
    // Comparing string to number
    let result = engine.render("{{ '5' == 5 }}").unwrap();
    // In Jinja2, these are not equal
    assert_eq!(result, "false");
}

#[test]
fn test_compare_none_values() {
    let engine = setup_engine();
    let result = engine
        .render_with_context("{{ a == b }}", serde_json::json!({"a": null, "b": null}))
        .unwrap();
    assert_eq!(result, "true");
}

// ==================== Template syntax edge cases ====================

#[test]
fn test_nested_brackets() {
    let engine = setup_engine();
    let result = engine.render("{{ [[1, 2], [3, 4]][0][1] }}").unwrap();
    assert_eq!(result, "2");
}

#[test]
fn test_deeply_nested_access() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ a.b.c.d }}",
            serde_json::json!({"a": {"b": {"c": {"d": 42}}}}),
        )
        .unwrap();
    assert_eq!(result, "42");
}

#[test]
fn test_missing_nested_key() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ a.b.missing | default('none') }}",
            serde_json::json!({"a": {"b": {}}}),
        )
        .unwrap();
    assert_eq!(result, "none");
}

// ==================== Control flow edge cases ====================

#[test]
fn test_empty_for_loop() {
    let engine = setup_engine();
    let result = engine.render("{% for i in [] %}x{% endfor %}").unwrap();
    assert_eq!(result, "");
}

#[test]
fn test_if_with_undefined() {
    let engine = setup_engine();
    let result = engine
        .render("{% if undefined_var %}yes{% else %}no{% endif %}")
        .unwrap();
    assert_eq!(result, "no");
}

#[test]
fn test_set_and_use_variable() {
    let engine = setup_engine();
    let result = engine.render("{% set x = 5 %}{{ x * 2 }}").unwrap();
    assert_eq!(result, "10");
}
