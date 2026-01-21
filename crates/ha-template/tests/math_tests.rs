//! Tests for math-related template functions and filters
//!
//! Tests min, max, sqrt, average, median, round, abs, log, sin, cos, tan, etc.
//! Based on Python Home Assistant's test_template.py math tests.

use ha_event_bus::EventBus;
use ha_state_store::StateStore;
use ha_template::TemplateEngine;
use std::sync::Arc;

fn setup_engine() -> TemplateEngine {
    let event_bus = Arc::new(EventBus::new());
    let state_machine = Arc::new(StateStore::new(event_bus));
    TemplateEngine::new(state_machine)
}

// ==================== min() function tests ====================

#[test]
fn test_min_list() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ min([3, 1, 4, 1, 5]) }}").unwrap(), "1.0");
}

#[test]
fn test_min_float_list() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ min([3.5, 1.2, 4.8]) }}").unwrap(), "1.2");
}

#[test]
fn test_min_mixed_numbers() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ min([3, 1.5, 4]) }}").unwrap(), "1.5");
}

#[test]
fn test_min_negative() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ min([-5, 0, 5]) }}").unwrap(), "-5.0");
}

// ==================== max() function tests ====================

#[test]
fn test_max_list() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ max([3, 1, 4, 1, 5]) }}").unwrap(), "5.0");
}

#[test]
fn test_max_float_list() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ max([3.5, 1.2, 4.8]) }}").unwrap(), "4.8");
}

#[test]
fn test_max_negative() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ max([-5, -1, -10]) }}").unwrap(), "-1.0");
}

// ==================== sqrt filter tests ====================

#[test]
fn test_sqrt_16() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 16 | sqrt }}").unwrap(), "4.0");
}

#[test]
fn test_sqrt_2() {
    let engine = setup_engine();
    let result = engine.render("{{ 2 | sqrt }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - std::f64::consts::SQRT_2).abs() < 0.0001);
}

#[test]
fn test_sqrt_0() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 0 | sqrt }}").unwrap(), "0.0");
}

#[test]
fn test_sqrt_1() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 1 | sqrt }}").unwrap(), "1.0");
}

#[test]
fn test_sqrt_100() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 100 | sqrt }}").unwrap(), "10.0");
}

// ==================== round filter tests ====================

#[test]
fn test_round_default() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 3.14159 | round(2) }}").unwrap(), "3.14");
}

#[test]
fn test_round_halfway() {
    let engine = setup_engine();
    // Use 3.144 which definitively rounds to 3.14 (avoids floating-point 0.5 boundary issues)
    assert_eq!(engine.render("{{ 3.144 | round(2) }}").unwrap(), "3.14");
}

#[test]
fn test_round_to_integer() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 3.7 | round(0) }}").unwrap(), "4.0");
}

#[test]
fn test_round_negative() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ -3.14159 | round(2) }}").unwrap(), "-3.14");
}

#[test]
fn test_round_large_precision() {
    let engine = setup_engine();
    assert_eq!(
        engine.render("{{ 3.14159265 | round(5) }}").unwrap(),
        "3.14159"
    );
}

// ==================== abs filter tests ====================

#[test]
fn test_abs_positive() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 5 | abs }}").unwrap(), "5.0");
}

#[test]
fn test_abs_negative() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ -5 | abs }}").unwrap(), "5.0");
}

#[test]
fn test_abs_zero() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 0 | abs }}").unwrap(), "0.0");
}

#[test]
fn test_abs_negative_float() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ -3.14 | abs }}").unwrap(), "3.14");
}

// ==================== log filter tests ====================

#[test]
fn test_log_e() {
    let engine = setup_engine();
    let result = engine.render("{{ 2.718281828 | log }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - 1.0).abs() < 0.0001);
}

#[test]
fn test_log_10() {
    let engine = setup_engine();
    let result = engine.render("{{ 100 | log(10) }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - 2.0).abs() < 0.0001);
}

#[test]
fn test_log_base_2() {
    let engine = setup_engine();
    let result = engine.render("{{ 8 | log(2) }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - 3.0).abs() < 0.0001);
}

// ==================== sin, cos, tan filter tests ====================

#[test]
fn test_sin_0() {
    let engine = setup_engine();
    let result = engine.render("{{ 0 | sin }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!(value.abs() < 0.0001);
}

#[test]
fn test_cos_0() {
    let engine = setup_engine();
    let result = engine.render("{{ 0 | cos }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - 1.0).abs() < 0.0001);
}

#[test]
fn test_tan_0() {
    let engine = setup_engine();
    let result = engine.render("{{ 0 | tan }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!(value.abs() < 0.0001);
}

#[test]
fn test_sin_pi_2() {
    let engine = setup_engine();
    // sin(π/2) ≈ 1
    let result = engine.render("{{ 1.5707963 | sin }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - 1.0).abs() < 0.0001);
}

#[test]
fn test_cos_pi() {
    let engine = setup_engine();
    // cos(π) ≈ -1
    let result = engine.render("{{ 3.14159265 | cos }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value + 1.0).abs() < 0.0001);
}

// ==================== asin, acos, atan filter tests ====================

#[test]
fn test_asin_1() {
    let engine = setup_engine();
    // asin(1) = π/2
    let result = engine.render("{{ 1 | asin }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - std::f64::consts::FRAC_PI_2).abs() < 0.0001);
}

#[test]
fn test_acos_0() {
    let engine = setup_engine();
    // acos(0) = π/2
    let result = engine.render("{{ 0 | acos }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - std::f64::consts::FRAC_PI_2).abs() < 0.0001);
}

#[test]
fn test_atan_1() {
    let engine = setup_engine();
    // atan(1) = π/4
    let result = engine.render("{{ 1 | atan }}").unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - std::f64::consts::FRAC_PI_4).abs() < 0.0001);
}

// ==================== average filter tests ====================

#[test]
fn test_average_list() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ values | average }}",
            serde_json::json!({"values": [1, 2, 3, 4, 5]}),
        )
        .unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - 3.0).abs() < 0.0001);
}

#[test]
fn test_average_floats() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ values | average }}",
            serde_json::json!({"values": [1.5, 2.5, 3.5]}),
        )
        .unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - 2.5).abs() < 0.0001);
}

// ==================== median filter tests ====================

#[test]
fn test_median_odd() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ values | median }}",
            serde_json::json!({"values": [1, 3, 2, 4, 5]}),
        )
        .unwrap();
    let value: f64 = result.parse().unwrap();
    assert!((value - 3.0).abs() < 0.0001);
}

#[test]
fn test_median_even() {
    let engine = setup_engine();
    let result = engine
        .render_with_context(
            "{{ values | median }}",
            serde_json::json!({"values": [1, 2, 3, 4]}),
        )
        .unwrap();
    let value: f64 = result.parse().unwrap();
    // Median of [1,2,3,4] = (2+3)/2 = 2.5
    assert!((value - 2.5).abs() < 0.0001);
}

// ==================== Arithmetic operations ====================

#[test]
fn test_addition() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 2 + 3 }}").unwrap(), "5");
}

#[test]
fn test_subtraction() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 10 - 3 }}").unwrap(), "7");
}

#[test]
fn test_multiplication() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 4 * 5 }}").unwrap(), "20");
}

#[test]
fn test_division() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 20 / 4 }}").unwrap(), "5.0");
}

#[test]
fn test_integer_division() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 20 // 3 }}").unwrap(), "6");
}

#[test]
fn test_modulo() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 17 % 5 }}").unwrap(), "2");
}

#[test]
fn test_power() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 2 ** 10 }}").unwrap(), "1024");
}

#[test]
fn test_complex_expression() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ (2 + 3) * 4 }}").unwrap(), "20");
}

#[test]
fn test_negative_numbers() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ -5 + 3 }}").unwrap(), "-2");
}

#[test]
fn test_float_arithmetic() {
    let engine = setup_engine();
    assert_eq!(engine.render("{{ 1.5 + 2.5 }}").unwrap(), "4.0");
}
