//! Test harness for running comparison tests

use crate::client::HaClient;
use crate::compare::{compare_responses, CompareOptions, ComparisonResult};
use crate::config::ComparisonConfig;
use serde_json::{json, Value};
use std::time::Duration;

/// Test harness that manages both HA instances and runs comparisons
pub struct TestHarness {
    pub config: ComparisonConfig,
    pub python_client: HaClient,
    pub rust_client: HaClient,
    pub results: Vec<ComparisonResult>,
}

impl TestHarness {
    /// Create a new test harness from config
    pub fn new(config: ComparisonConfig) -> Self {
        let python_client = HaClient::python_ha(&config.python_ha_url, &config.python_ha_token);
        let rust_client = HaClient::rust_ha(&config.rust_ha_url, config.rust_ha_token.as_deref());

        Self {
            config,
            python_client,
            rust_client,
            results: Vec::new(),
        }
    }

    /// Wait for both servers to be healthy
    pub async fn wait_for_servers(&self, timeout: Duration) -> Result<(), String> {
        println!("Waiting for Python HA at {}...", self.config.python_ha_url);
        if !self.python_client.wait_for_healthy(timeout).await {
            return Err(format!(
                "Python HA at {} did not become healthy within {:?}",
                self.config.python_ha_url, timeout
            ));
        }
        println!("✓ Python HA is ready");

        println!("Waiting for Rust HA at {}...", self.config.rust_ha_url);
        if !self.rust_client.wait_for_healthy(timeout).await {
            return Err(format!(
                "Rust HA at {} did not become healthy within {:?}",
                self.config.rust_ha_url, timeout
            ));
        }
        println!("✓ Rust HA is ready");

        Ok(())
    }

    /// Run a GET comparison test
    pub async fn compare_get(
        &mut self,
        endpoint: &str,
        options: Option<CompareOptions>,
    ) -> &ComparisonResult {
        let options = options.unwrap_or_default();

        let python_response = self
            .python_client
            .get(endpoint)
            .await
            .expect("Python HA request failed");

        let rust_response = self
            .rust_client
            .get(endpoint)
            .await
            .expect("Rust HA request failed");

        let result = compare_responses(endpoint, &python_response, &rust_response, &options);
        self.results.push(result);
        self.results.last().unwrap()
    }

    /// Run a POST comparison test
    pub async fn compare_post(
        &mut self,
        endpoint: &str,
        body: Option<Value>,
        options: Option<CompareOptions>,
    ) -> &ComparisonResult {
        let options = options.unwrap_or_default();

        let python_response = self
            .python_client
            .post(endpoint, body.clone())
            .await
            .expect("Python HA request failed");

        let rust_response = self
            .rust_client
            .post(endpoint, body)
            .await
            .expect("Rust HA request failed");

        let result = compare_responses(endpoint, &python_response, &rust_response, &options);
        self.results.push(result);
        self.results.last().unwrap()
    }

    /// Print summary of all results
    pub fn print_summary(&self) {
        println!("\n=== Comparison Test Summary ===");
        println!("HA Version: {}", self.config.ha_version);
        println!("Python HA: {}", self.config.python_ha_url);
        println!("Rust HA: {}", self.config.rust_ha_url);
        println!();

        let passed = self.results.iter().filter(|r| r.passed).count();
        let total = self.results.len();

        for result in &self.results {
            result.print_summary();
        }

        println!();
        println!("Results: {}/{} passed", passed, total);

        if passed == total {
            println!("✅ All tests passed!");
        } else {
            println!("❌ {} tests failed", total - passed);
        }
    }

    /// Check if all tests passed
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new(ComparisonConfig::default())
    }
}

/// Predefined test suites
pub struct TestSuites;

impl TestSuites {
    /// Run all API endpoint comparison tests
    pub async fn run_all(harness: &mut TestHarness) {
        Self::run_basic_endpoints(harness).await;
        Self::run_state_endpoints(harness).await;
        Self::run_service_endpoints(harness).await;
        Self::run_event_endpoints(harness).await;
    }

    /// Test basic API endpoints
    pub async fn run_basic_endpoints(harness: &mut TestHarness) {
        println!("\n--- Basic Endpoints ---");

        // GET /api/
        harness.compare_get("/api/", None).await.print_summary();

        // GET /api/config
        harness
            .compare_get(
                "/api/config",
                Some(CompareOptions::new().ignore_field("whitelist_external_dirs")),
            )
            .await
            .print_summary();
    }

    /// Test state endpoints
    pub async fn run_state_endpoints(harness: &mut TestHarness) {
        println!("\n--- State Endpoints ---");

        // GET /api/states
        let options = CompareOptions::new()
            .ignore_field("last_changed")
            .ignore_field("last_updated")
            .ignore_field("context");

        harness
            .compare_get("/api/states", Some(options.clone()))
            .await
            .print_summary();

        // GET /api/states/<entity_id> - test with a demo entity
        harness
            .compare_get("/api/states/sun.sun", Some(options.clone()))
            .await
            .print_summary();

        // POST /api/states/<entity_id> - create/update state
        harness
            .compare_post(
                "/api/states/sensor.test_comparison",
                Some(json!({
                    "state": "test_value",
                    "attributes": {
                        "unit_of_measurement": "test",
                        "friendly_name": "Test Comparison Sensor"
                    }
                })),
                Some(options),
            )
            .await
            .print_summary();
    }

    /// Test service endpoints
    pub async fn run_service_endpoints(harness: &mut TestHarness) {
        println!("\n--- Service Endpoints ---");

        // GET /api/services
        harness
            .compare_get("/api/services", None)
            .await
            .print_summary();

        // POST /api/services/<domain>/<service>
        // Note: This may have side effects, so we use a safe service
        harness
            .compare_post(
                "/api/services/homeassistant/check_config",
                Some(json!({})),
                None,
            )
            .await
            .print_summary();
    }

    /// Test event endpoints
    pub async fn run_event_endpoints(harness: &mut TestHarness) {
        println!("\n--- Event Endpoints ---");

        // GET /api/events
        harness
            .compare_get("/api/events", None)
            .await
            .print_summary();

        // POST /api/events/<event_type>
        harness
            .compare_post(
                "/api/events/test_comparison_event",
                Some(json!({"test": "data"})),
                None,
            )
            .await
            .print_summary();
    }
}
