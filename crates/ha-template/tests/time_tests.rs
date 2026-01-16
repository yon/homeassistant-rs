//! Tests for time-related template functions
//!
//! Tests now(), utcnow(), as_timestamp(), relative_time(), timedelta(), etc.
//! Based on Python Home Assistant's test_template.py time tests.

use ha_event_bus::EventBus;
use ha_state_machine::StateMachine;
use ha_template::TemplateEngine;
use std::sync::Arc;

fn setup_engine() -> TemplateEngine {
    let event_bus = Arc::new(EventBus::new());
    let state_machine = Arc::new(StateMachine::new(event_bus));
    TemplateEngine::new(state_machine)
}

// ==================== now() function tests ====================

#[test]
fn test_now_returns_datetime() {
    let engine = setup_engine();
    // now() should return something that can be used as a datetime
    let result = engine.render("{{ now() }}").unwrap();
    // Should contain a date pattern like YYYY-MM-DD
    assert!(result.contains("-"));
}

#[test]
fn test_now_year() {
    let engine = setup_engine();
    let result = engine.render("{{ now().year }}").unwrap();
    // Year should be a 4-digit number >= 2024
    let year: i32 = result.parse().expect("Year should be numeric");
    assert!(year >= 2024);
}

#[test]
fn test_now_month() {
    let engine = setup_engine();
    let result = engine.render("{{ now().month }}").unwrap();
    let month: i32 = result.parse().expect("Month should be numeric");
    assert!(month >= 1 && month <= 12);
}

#[test]
fn test_now_day() {
    let engine = setup_engine();
    let result = engine.render("{{ now().day }}").unwrap();
    let day: i32 = result.parse().expect("Day should be numeric");
    assert!(day >= 1 && day <= 31);
}

#[test]
fn test_now_hour() {
    let engine = setup_engine();
    let result = engine.render("{{ now().hour }}").unwrap();
    let hour: i32 = result.parse().expect("Hour should be numeric");
    assert!(hour >= 0 && hour <= 23);
}

#[test]
fn test_now_minute() {
    let engine = setup_engine();
    let result = engine.render("{{ now().minute }}").unwrap();
    let minute: i32 = result.parse().expect("Minute should be numeric");
    assert!(minute >= 0 && minute <= 59);
}

#[test]
fn test_now_second() {
    let engine = setup_engine();
    let result = engine.render("{{ now().second }}").unwrap();
    let second: i32 = result.parse().expect("Second should be numeric");
    assert!(second >= 0 && second <= 59);
}

#[test]
fn test_now_weekday() {
    let engine = setup_engine();
    let result = engine.render("{{ now().weekday() }}").unwrap();
    let weekday: i32 = result.parse().expect("Weekday should be numeric");
    assert!(weekday >= 0 && weekday <= 6);
}

// ==================== utcnow() function tests ====================

#[test]
fn test_utcnow_returns_datetime() {
    let engine = setup_engine();
    let result = engine.render("{{ utcnow() }}").unwrap();
    assert!(result.contains("-"));
}

#[test]
fn test_utcnow_year() {
    let engine = setup_engine();
    let result = engine.render("{{ utcnow().year }}").unwrap();
    let year: i32 = result.parse().expect("Year should be numeric");
    assert!(year >= 2024);
}

#[test]
fn test_utcnow_hour() {
    let engine = setup_engine();
    let result = engine.render("{{ utcnow().hour }}").unwrap();
    let hour: i32 = result.parse().expect("Hour should be numeric");
    assert!(hour >= 0 && hour <= 23);
}

// ==================== as_timestamp() function tests ====================

#[test]
fn test_as_timestamp_from_now() {
    let engine = setup_engine();
    let result = engine.render("{{ as_timestamp(now()) }}").unwrap();
    let timestamp: f64 = result.parse().expect("Should be a float");
    // Timestamp should be a large number (seconds since 1970)
    assert!(timestamp > 1700000000.0);
}

#[test]
fn test_as_timestamp_from_utcnow() {
    let engine = setup_engine();
    let result = engine.render("{{ as_timestamp(utcnow()) }}").unwrap();
    let timestamp: f64 = result.parse().expect("Should be a float");
    assert!(timestamp > 1700000000.0);
}

#[test]
fn test_as_timestamp_from_string() {
    let engine = setup_engine();
    let result = engine
        .render("{{ as_timestamp('2024-01-01T00:00:00Z') }}")
        .unwrap();
    let timestamp: f64 = result.parse().expect("Should be a float");
    // 2024-01-01 00:00:00 UTC is 1704067200
    assert!((timestamp - 1704067200.0).abs() < 1.0);
}

// ==================== relative_time() function tests ====================

#[test]
fn test_relative_time_past() {
    let engine = setup_engine();
    // Test relative_time with a datetime 2 hours ago using method syntax
    let result = engine
        .render("{{ relative_time(now().sub(timedelta(hours=2))) }}")
        .unwrap();
    assert_eq!(result, "2 hours");
}

#[test]
fn test_relative_time_minutes() {
    let engine = setup_engine();
    let result = engine
        .render("{{ relative_time(now().sub(timedelta(minutes=30))) }}")
        .unwrap();
    assert_eq!(result, "30 minutes");
}

#[test]
fn test_relative_time_seconds() {
    let engine = setup_engine();
    let result = engine
        .render("{{ relative_time(now().sub(timedelta(seconds=45))) }}")
        .unwrap();
    assert_eq!(result, "45 seconds");
}

#[test]
fn test_relative_time_days() {
    let engine = setup_engine();
    let result = engine
        .render("{{ relative_time(now().sub(timedelta(days=3))) }}")
        .unwrap();
    assert_eq!(result, "3 days");
}

#[test]
fn test_relative_time_one_unit() {
    let engine = setup_engine();
    let result = engine
        .render("{{ relative_time(now().sub(timedelta(hours=1))) }}")
        .unwrap();
    assert_eq!(result, "1 hour");
}

// ==================== timedelta() function tests ====================

#[test]
fn test_timedelta_hours() {
    let engine = setup_engine();
    let result = engine.render("{{ timedelta(hours=2) }}").unwrap();
    // Should represent 2 hours
    assert!(result.contains("2") || result.contains("7200"));
}

#[test]
fn test_timedelta_days() {
    let engine = setup_engine();
    let result = engine.render("{{ timedelta(days=1) }}").unwrap();
    assert!(result.contains("1") || result.contains("86400"));
}

#[test]
fn test_timedelta_combined() {
    let engine = setup_engine();
    let result = engine
        .render("{{ timedelta(days=1, hours=2, minutes=30) }}")
        .unwrap();
    // Should be 1 day 2:30:00 or equivalent
    assert!(!result.is_empty());
}

#[test]
fn test_now_minus_timedelta() {
    let engine = setup_engine();
    // Use method syntax for datetime arithmetic
    let result = engine
        .render("{{ now().sub(timedelta(hours=1)) }}")
        .unwrap();
    assert!(result.contains("-")); // Should contain date separator
}

#[test]
fn test_now_plus_timedelta() {
    let engine = setup_engine();
    let result = engine.render("{{ now().add(timedelta(days=1)) }}").unwrap();
    assert!(result.contains("-"));
}

// ==================== today_at() function tests ====================

#[test]
fn test_today_at() {
    let engine = setup_engine();
    let result = engine.render("{{ today_at('14:30') }}").unwrap();
    // today_at returns UTC time, so check for today's date and that minutes are :30
    // The hour will vary based on timezone
    assert!(
        result.contains(":30:00"),
        "Expected ':30:00' in result: '{}'",
        result
    );
}

#[test]
fn test_today_at_with_seconds() {
    let engine = setup_engine();
    let result = engine.render("{{ today_at('14:30:45') }}").unwrap();
    // Check for :30:45 (minutes and seconds) since hour depends on timezone
    assert!(
        result.contains(":30:45"),
        "Expected ':30:45' in result: '{}'",
        result
    );
}

// ==================== time_since() and time_until() tests ====================

#[test]
fn test_time_since_past() {
    let engine = setup_engine();
    let result = engine
        .render("{{ time_since(now().sub(timedelta(hours=3))) }}")
        .unwrap();
    // Should show approximately 3 hours
    assert!(result.contains("3") && result.contains("hour"));
}

#[test]
fn test_time_until_future() {
    let engine = setup_engine();
    let result = engine
        .render("{{ time_until(now().add(timedelta(hours=3))) }}")
        .unwrap();
    // Should show approximately 2-3 hours (timing variations between now() calls)
    assert!(
        result.contains("hour"),
        "Expected 'hour' in result: '{}'",
        result
    );
    // Should be either "2 hours" or "3 hours" depending on timing
    assert!(
        result.contains("2") || result.contains("3"),
        "Expected '2' or '3' in result: '{}'",
        result
    );
}

// ==================== DateTime arithmetic tests ====================

#[test]
fn test_datetime_comparison() {
    let engine = setup_engine();
    // Use gt() method for comparison since operators aren't supported
    let result = engine
        .render("{{ now().gt(now().sub(timedelta(hours=1))) }}")
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn test_datetime_isoformat() {
    let engine = setup_engine();
    let result = engine.render("{{ now().isoformat() }}").unwrap();
    // Should be ISO format: YYYY-MM-DDTHH:MM:SS...
    assert!(result.contains("T"));
}

#[test]
fn test_datetime_strftime() {
    let engine = setup_engine();
    let result = engine.render("{{ now().strftime('%Y-%m-%d') }}").unwrap();
    // Should be formatted date
    assert!(result.len() == 10); // YYYY-MM-DD
    assert!(result.contains("-"));
}

// ==================== Timestamp operations ====================

#[test]
fn test_timestamp_arithmetic() {
    let engine = setup_engine();
    let result = engine
        .render("{{ as_timestamp(now()) - as_timestamp(now().sub(timedelta(hours=1))) }}")
        .unwrap();
    let diff: f64 = result.parse().expect("Should be a float");
    // Should be approximately 3600 seconds (1 hour)
    assert!((diff - 3600.0).abs() < 60.0); // Within 1 minute tolerance
}
