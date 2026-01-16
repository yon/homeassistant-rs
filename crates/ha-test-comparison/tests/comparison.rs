//! Integration tests comparing Rust HA API against Python HA
//!
//! These tests require both a Python Home Assistant instance and
//! the Rust HA server to be running.
//!
//! # Running locally
//!
//! ```bash
//! # Start HA test instance
//! make ha-start
//!
//! # Start Rust HA server (in another terminal)
//! HA_PORT=18124 make run
//!
//! # Run comparison tests
//! make test-compare
//! ```
//!
//! # Environment Variables
//!
//! - `PYTHON_HA_URL`: URL of Python HA (default: http://localhost:18123)
//! - `RUST_HA_URL`: URL of Rust HA (default: http://localhost:18124)
//! - `PYTHON_HA_TOKEN`: Bearer token for Python HA
//! - `HA_VERSION`: Version being tested (default: 2026.1.1)

use ha_test_comparison::config::ComparisonConfig;
use ha_test_comparison::harness::{TestHarness, TestSuites};
use std::time::Duration;

/// Main comparison test - runs all endpoint comparisons
#[tokio::test]
#[ignore] // Run with: cargo test -p ha-test-comparison -- --ignored --nocapture
async fn test_api_comparison() {
    let config = ComparisonConfig::from_env();
    let mut harness = TestHarness::new(config);

    // Wait for both servers
    harness
        .wait_for_servers(Duration::from_secs(120))
        .await
        .expect("Servers did not become healthy");

    // Run all test suites
    TestSuites::run_all(&mut harness).await;

    // Print summary
    harness.print_summary();

    // Assert all passed
    assert!(harness.all_passed(), "Some comparison tests failed");
}

/// Test just the basic endpoints (quick sanity check)
#[tokio::test]
#[ignore]
async fn test_basic_endpoints_comparison() {
    let config = ComparisonConfig::from_env();
    let mut harness = TestHarness::new(config);

    harness
        .wait_for_servers(Duration::from_secs(60))
        .await
        .expect("Servers did not become healthy");

    TestSuites::run_basic_endpoints(&mut harness).await;

    harness.print_summary();
    assert!(harness.all_passed());
}

/// Test state endpoints
#[tokio::test]
#[ignore]
async fn test_state_endpoints_comparison() {
    let config = ComparisonConfig::from_env();
    let mut harness = TestHarness::new(config);

    harness
        .wait_for_servers(Duration::from_secs(60))
        .await
        .expect("Servers did not become healthy");

    TestSuites::run_state_endpoints(&mut harness).await;

    harness.print_summary();
    assert!(harness.all_passed());
}

/// Test service endpoints
#[tokio::test]
#[ignore]
async fn test_service_endpoints_comparison() {
    let config = ComparisonConfig::from_env();
    let mut harness = TestHarness::new(config);

    harness
        .wait_for_servers(Duration::from_secs(60))
        .await
        .expect("Servers did not become healthy");

    TestSuites::run_service_endpoints(&mut harness).await;

    harness.print_summary();
    assert!(harness.all_passed());
}

/// Test event endpoints
#[tokio::test]
#[ignore]
async fn test_event_endpoints_comparison() {
    let config = ComparisonConfig::from_env();
    let mut harness = TestHarness::new(config);

    harness
        .wait_for_servers(Duration::from_secs(60))
        .await
        .expect("Servers did not become healthy");

    TestSuites::run_event_endpoints(&mut harness).await;

    harness.print_summary();
    assert!(harness.all_passed());
}
