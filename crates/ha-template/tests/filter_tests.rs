//! Tests for template filters
//!
//! Tests slugify, regex_replace, to_json, from_json, base64, urlencode, etc.
//! Based on Python Home Assistant's test_template.py filter tests.

use ha_event_bus::EventBus;
use ha_state_machine::StateMachine;
use ha_template::TemplateEngine;
use std::sync::Arc;

fn setup_engine() -> TemplateEngine {
    let event_bus = Arc::new(EventBus::new());
    let state_machine = Arc::new(StateMachine::new(event_bus));
    TemplateEngine::new(state_machine)
}

// ==================== slugify filter tests ====================

#[test]
fn test_slugify_basic() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ 'Hello World' | slugify }}").unwrap(),
        "hello_world"
    );
}

#[test]
fn test_slugify_special_chars() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ 'Hello, World!' | slugify }}").unwrap(),
        "hello_world"
    );
}

#[test]
fn test_slugify_numbers() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ 'Test 123 Value' | slugify }}").unwrap(),
        "test_123_value"
    );
}

#[test]
fn test_slugify_accents() {
    let engine = setup_engine();
    // Accented characters should be converted
    assert_eq!(
        engine.render("{{ 'Café Résumé' | slugify }}").unwrap(),
        "cafe_resume"
    );
}

#[test]
fn test_slugify_uppercase() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ 'ALL CAPS' | slugify }}").unwrap(),
        "all_caps"
    );
}

#[test]
fn test_slugify_with_separator() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'Hello World' | slugify(separator='-') }}")
            .unwrap(),
        "hello-world"
    );
}

// ==================== regex_replace filter tests ====================

#[test]
fn test_regex_replace_basic() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'hello world' | regex_replace('\\\\s+', '-') }}")
            .unwrap(),
        "hello-world"
    );
}

#[test]
fn test_regex_replace_digits() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'test123abc456' | regex_replace('[0-9]+', 'NUM') }}")
            .unwrap(),
        "testNUMabcNUM"
    );
}

#[test]
fn test_regex_replace_no_match() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'hello' | regex_replace('[0-9]', 'X') }}")
            .unwrap(),
        "hello"
    );
}

#[test]
fn test_regex_replace_empty_replacement() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'test123' | regex_replace('[0-9]+', '') }}")
            .unwrap(),
        "test"
    );
}

// ==================== regex_findall filter tests ====================

#[test]
fn test_regex_findall_digits() {
    let engine = setup_engine();
    let result = engine
        .render("{{ 'test123abc456' | regex_findall('[0-9]+') }}")
        .unwrap();
    // Should contain 123 and 456
    assert!(result.contains("123") && result.contains("456"));
}

#[test]
fn test_regex_findall_no_match() {
    let engine = setup_engine();
    let result = engine
        .render("{{ 'hello' | regex_findall('[0-9]+') }}")
        .unwrap();
    assert!(result == "[]" || result.is_empty());
}

// ==================== to_json filter tests ====================

#[test]
fn test_to_json_dict() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ data | to_json }}",
            serde_json::json!({"data": {"key": "value"}}),
        )
        .unwrap();
    assert!(result.contains("key") && result.contains("value"));
}

#[test]
fn test_to_json_list() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ items | to_json }}",
            serde_json::json!({"items": [1, 2, 3]}),
        )
        .unwrap();
    assert!(result.contains("[") && result.contains("]"));
    assert!(result.contains("1") && result.contains("2") && result.contains("3"));
}

#[test]
fn test_to_json_string() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ text | to_json }}",
            serde_json::json!({"text": "hello world"}),
        )
        .unwrap();
    assert!(result.contains("hello world"));
}

#[test]
fn test_to_json_number() {
    let engine = setup_engine();
    let result = engine
        .render_with_context("{{ num | to_json }}", serde_json::json!({"num": 42}))
        .unwrap();
    assert_eq!(result, "42");
}

// ==================== from_json filter tests ====================

#[test]
fn test_from_json_dict() {
    let engine = setup_engine();
    let result = engine
        .render("{{ '{\"key\": \"value\"}' | from_json }}")
        .unwrap();
    assert!(result.contains("key") && result.contains("value"));
}

#[test]
fn test_from_json_access_key() {
    let engine = setup_engine();
    // Parse JSON and access a key
    let result = engine
        .render("{% set data = '{\"name\": \"test\", \"value\": 42}' | from_json %}{{ data.name }}")
        .unwrap();
    assert_eq!(result, "test");
}

#[test]
fn test_from_json_list() {
    let engine = setup_engine();
    let result = engine.render("{{ '[1, 2, 3]' | from_json }}").unwrap();
    assert!(result.contains("1") && result.contains("2") && result.contains("3"));
}

// ==================== base64_encode filter tests ====================

#[test]
fn test_base64_encode() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'homeassistant' | base64_encode }}")
            .unwrap(),
        "aG9tZWFzc2lzdGFudA=="
    );
}

#[test]
fn test_base64_encode_hello() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'Hello, World!' | base64_encode }}")
            .unwrap(),
        "SGVsbG8sIFdvcmxkIQ=="
    );
}

#[test]
fn test_base64_encode_empty() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ '' | base64_encode }}").unwrap(), "");
}

// ==================== base64_decode filter tests ====================

#[test]
fn test_base64_decode() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'aG9tZWFzc2lzdGFudA==' | base64_decode }}")
            .unwrap(),
        "homeassistant"
    );
}

#[test]
fn test_base64_decode_hello() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'SGVsbG8sIFdvcmxkIQ==' | base64_decode }}")
            .unwrap(),
        "Hello, World!"
    );
}

#[test]
fn test_base64_roundtrip() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'test string' | base64_encode | base64_decode }}")
            .unwrap(),
        "test string"
    );
}

// ==================== urlencode filter tests ====================

#[test]
fn test_urlencode_spaces() {
    let engine = setup_engine();
    let result = engine.render("{{ 'hello world' | urlencode }}").unwrap();
    // Spaces should be encoded as %20 or +
    assert!(result.contains("%20") || result.contains("+"));
}

#[test]
fn test_urlencode_special_chars() {
    let engine = setup_engine();
    let result = engine
        .render("{{ 'key=value&foo=bar' | urlencode }}")
        .unwrap();
    assert!(result.contains("%3D") || result.contains("="));
}

#[test]
fn test_urlencode_unicode() {
    let engine = setup_engine();
    let result = engine.render("{{ 'café' | urlencode }}").unwrap();
    // The é should be encoded
    assert!(result.contains("%"));
}

// ==================== ordinal filter tests ====================

#[test]
fn test_ordinal_1() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 1 | ordinal }}").unwrap(), "1st");
}

#[test]
fn test_ordinal_2() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 2 | ordinal }}").unwrap(), "2nd");
}

#[test]
fn test_ordinal_3() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 3 | ordinal }}").unwrap(), "3rd");
}

#[test]
fn test_ordinal_4() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 4 | ordinal }}").unwrap(), "4th");
}

#[test]
fn test_ordinal_11() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 11 | ordinal }}").unwrap(), "11th");
}

#[test]
fn test_ordinal_12() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 12 | ordinal }}").unwrap(), "12th");
}

#[test]
fn test_ordinal_13() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 13 | ordinal }}").unwrap(), "13th");
}

#[test]
fn test_ordinal_21() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 21 | ordinal }}").unwrap(), "21st");
}

#[test]
fn test_ordinal_22() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 22 | ordinal }}").unwrap(), "22nd");
}

#[test]
fn test_ordinal_23() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 23 | ordinal }}").unwrap(), "23rd");
}

#[test]
fn test_ordinal_100() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 100 | ordinal }}").unwrap(), "100th");
}

// ==================== flatten filter tests ====================

#[test]
fn test_flatten_nested() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ data | flatten }}",
            serde_json::json!({"data": [[1, 2], [3, 4]]}),
        )
        .unwrap();
    // Should flatten to [1, 2, 3, 4]
    assert!(
        result.contains("1")
            && result.contains("2")
            && result.contains("3")
            && result.contains("4")
    );
}

#[test]
fn test_flatten_already_flat() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ data | flatten }}",
            serde_json::json!({"data": [1, 2, 3]}),
        )
        .unwrap();
    assert!(result.contains("1") && result.contains("2") && result.contains("3"));
}

// ==================== contains filter tests ====================

#[test]
fn test_contains_string_true() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'hello world' | contains('world') }}")
            .unwrap(),
        "true"
    );
}

#[test]
fn test_contains_string_false() {
    let engine = setup_engine();
    assert_eq!(
        engine
            .render("{{ 'hello world' | contains('foo') }}")
            .unwrap(),
        "false"
    );
}

#[test]
fn test_contains_list_true() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ items | contains(2) }}",
            serde_json::json!({"items": [1, 2, 3]}),
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn test_contains_list_false() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ items | contains(5) }}",
            serde_json::json!({"items": [1, 2, 3]}),
        )
        .unwrap();
    assert_eq!(result, "false");
}
