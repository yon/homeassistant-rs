//! Test harness for running comparison tests

use crate::client::HaClient;
use crate::compare::{compare_responses, compare_ws_results, CompareOptions, ComparisonResult, WsComparisonResult};
use crate::config::ComparisonConfig;
use crate::ws_client::WsClient;
use serde_json::{json, Value};
use std::time::Duration;

/// Test harness that manages both HA instances and runs comparisons
pub struct TestHarness {
    pub config: ComparisonConfig,
    pub python_client: HaClient,
    pub rust_client: HaClient,
    pub python_ws: WsClient,
    pub rust_ws: WsClient,
    pub results: Vec<ComparisonResult>,
    pub ws_results: Vec<WsComparisonResult>,
}

impl TestHarness {
    /// Create a new test harness from config
    pub fn new(config: ComparisonConfig) -> Self {
        let python_client = HaClient::python_ha(&config.python_ha_url, &config.python_ha_token);
        let rust_client = HaClient::rust_ha(&config.rust_ha_url, config.rust_ha_token.as_deref());

        // WebSocket clients use the same token
        let python_ws = WsClient::python_ha(&config.python_ha_url, &config.python_ha_token);
        let rust_ws = WsClient::rust_ha(
            &config.rust_ha_url,
            config.rust_ha_token.as_deref().unwrap_or(&config.python_ha_token),
        );

        Self {
            config,
            python_client,
            rust_client,
            python_ws,
            rust_ws,
            results: Vec::new(),
            ws_results: Vec::new(),
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

    /// Run a WebSocket comparison test
    pub async fn compare_ws_auth(&mut self) -> &WsComparisonResult {
        let options = CompareOptions::new()
            .ignore_field("ha_version"); // Versions may differ

        let python_result = self.python_ws.test_auth_flow().await;
        let rust_result = self.rust_ws.test_auth_flow().await;

        let result = compare_ws_results("auth_flow", &python_result, &rust_result, &options);
        self.ws_results.push(result);
        self.ws_results.last().unwrap()
    }

    /// Run WebSocket get_states comparison
    pub async fn compare_ws_get_states(&mut self) -> &WsComparisonResult {
        let options = CompareOptions::new()
            .ignore_field("last_changed")
            .ignore_field("last_updated")
            .ignore_field("last_reported")
            .ignore_field("context")
            // Ignore dynamic fields that change at runtime
            .ignore_field("access_token")
            .ignore_field("entity_picture")
            .ignore_field("state")  // Demo sensors change values over time
            .sort_arrays_by("entity_id");

        let python_result = self.python_ws.test_get_states().await;
        let rust_result = self.rust_ws.test_get_states().await;

        let result = compare_ws_results("get_states", &python_result, &rust_result, &options);
        self.ws_results.push(result);
        self.ws_results.last().unwrap()
    }

    /// Run WebSocket get_config comparison
    pub async fn compare_ws_get_config(&mut self) -> &WsComparisonResult {
        let options = CompareOptions::new()
            .ignore_field("allowlist_external_dirs")
            .ignore_field("allowlist_external_urls")
            .ignore_field("whitelist_external_dirs")
            .ignore_field("components");

        let python_result = self.python_ws.test_get_config().await;
        let rust_result = self.rust_ws.test_get_config().await;

        let result = compare_ws_results("get_config", &python_result, &rust_result, &options);
        self.ws_results.push(result);
        self.ws_results.last().unwrap()
    }

    /// Run WebSocket ping/pong comparison
    pub async fn compare_ws_ping(&mut self) -> &WsComparisonResult {
        let options = CompareOptions::new();

        let python_result = self.python_ws.test_ping_pong().await;
        let rust_result = self.rust_ws.test_ping_pong().await;

        let result = compare_ws_results("ping_pong", &python_result, &rust_result, &options);
        self.ws_results.push(result);
        self.ws_results.last().unwrap()
    }

    /// Run WebSocket subscribe_events comparison
    pub async fn compare_ws_subscribe(&mut self) -> &WsComparisonResult {
        let options = CompareOptions::new();

        let python_result = self.python_ws.test_subscribe_events().await;
        let rust_result = self.rust_ws.test_subscribe_events().await;

        let result = compare_ws_results("subscribe_events", &python_result, &rust_result, &options);
        self.ws_results.push(result);
        self.ws_results.last().unwrap()
    }

    /// Run WebSocket call_service comparison
    pub async fn compare_ws_call_service(&mut self) -> &WsComparisonResult {
        // context.id is already ignored by default in CompareOptions::new()
        let options = CompareOptions::new();

        let python_result = self.python_ws.test_call_service().await;
        let rust_result = self.rust_ws.test_call_service().await;

        let result = compare_ws_results("call_service", &python_result, &rust_result, &options);
        self.ws_results.push(result);
        self.ws_results.last().unwrap()
    }

    /// Print summary of all results
    pub fn print_summary(&self) {
        println!("\n=== Comparison Test Summary ===");
        println!("HA Version: {}", self.config.ha_version);
        println!("Python HA: {}", self.config.python_ha_url);
        println!("Rust HA: {}", self.config.rust_ha_url);
        println!();

        let rest_passed = self.results.iter().filter(|r| r.passed).count();
        let rest_total = self.results.len();

        if rest_total > 0 {
            println!("--- REST API ---");
            for result in &self.results {
                result.print_summary();
            }
            println!();
        }

        let ws_passed = self.ws_results.iter().filter(|r| r.passed).count();
        let ws_total = self.ws_results.len();

        if ws_total > 0 {
            println!("--- WebSocket API ---");
            for result in &self.ws_results {
                result.print_summary();
            }
            println!();
        }

        let total_passed = rest_passed + ws_passed;
        let total = rest_total + ws_total;

        println!("Results: {}/{} passed", total_passed, total);

        if total_passed == total {
            println!("✅ All tests passed!");
        } else {
            println!("❌ {} tests failed", total - total_passed);
        }
    }

    /// Check if all tests passed
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed) && self.ws_results.iter().all(|r| r.passed)
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
        Self::run_websocket_endpoints(harness).await;
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
                Some(CompareOptions::new()
                    .ignore_field("whitelist_external_dirs")
                    .ignore_field("allowlist_external_dirs")),
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
            .ignore_field("last_reported")
            .ignore_field("context")
            // Ignore dynamic fields that change at runtime
            .ignore_field("access_token")
            .ignore_field("entity_picture")
            .ignore_field("state")  // Demo sensors change values over time
            .sort_arrays_by("entity_id");

        harness
            .compare_get("/api/states", Some(options.clone()))
            .await
            .print_summary();

        // GET /api/states/<entity_id> - test with a demo entity
        let single_options = CompareOptions::new()
            .ignore_field("last_changed")
            .ignore_field("last_updated")
            .ignore_field("last_reported")
            .ignore_field("context");
        harness
            .compare_get("/api/states/sun.sun", Some(single_options.clone()))
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
                Some(single_options),
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

    /// Test WebSocket endpoints
    pub async fn run_websocket_endpoints(harness: &mut TestHarness) {
        println!("\n--- WebSocket API ---");

        // Auth flow
        harness.compare_ws_auth().await.print_summary();

        // Ping/pong
        harness.compare_ws_ping().await.print_summary();

        // Get states
        harness.compare_ws_get_states().await.print_summary();

        // Get config
        harness.compare_ws_get_config().await.print_summary();

        // Subscribe events
        harness.compare_ws_subscribe().await.print_summary();

        // Call service
        harness.compare_ws_call_service().await.print_summary();
    }
}
