//! WebSocket client for API comparison tests

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// WebSocket client that can talk to either Python HA or Rust HA
pub struct WsClient {
    base_url: String,
    token: String,
}

/// A single WebSocket message exchange (request -> response)
#[derive(Debug, Clone)]
pub struct WsExchange {
    pub request: Value,
    pub response: Value,
}

/// Result of a WebSocket test sequence
#[derive(Debug, Clone)]
pub struct WsTestResult {
    pub name: String,
    pub exchanges: Vec<WsExchange>,
    pub error: Option<String>,
}

impl WsTestResult {
    pub fn success(name: &str, exchanges: Vec<WsExchange>) -> Self {
        Self {
            name: name.to_string(),
            exchanges,
            error: None,
        }
    }

    pub fn failure(name: &str, error: String) -> Self {
        Self {
            name: name.to_string(),
            exchanges: Vec::new(),
            error: Some(error),
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

impl WsClient {
    /// Create a new WebSocket client for Python HA
    pub fn python_ha(base_url: &str, token: &str) -> Self {
        Self::new(base_url, token)
    }

    /// Create a new WebSocket client for Rust HA
    pub fn rust_ha(base_url: &str, token: &str) -> Self {
        Self::new(base_url, token)
    }

    fn new(base_url: &str, token: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    /// Get WebSocket URL from HTTP URL
    fn ws_url(&self) -> String {
        let url = self
            .base_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        format!("{}/api/websocket", url)
    }

    /// Run the auth flow test - returns auth_ok response
    pub async fn test_auth_flow(&self) -> WsTestResult {
        let ws_url = self.ws_url();

        let connect_result = timeout(Duration::from_secs(10), connect_async(&ws_url)).await;

        let (ws_stream, _) = match connect_result {
            Ok(Ok((stream, response))) => (stream, response),
            Ok(Err(e)) => {
                return WsTestResult::failure("auth_flow", format!("Connect failed: {}", e))
            }
            Err(_) => return WsTestResult::failure("auth_flow", "Connect timeout".to_string()),
        };

        let (mut write, mut read) = ws_stream.split();
        let mut exchanges = Vec::new();

        // 1. Receive auth_required
        let auth_required = match Self::recv_message(&mut read).await {
            Ok(msg) => msg,
            Err(e) => {
                return WsTestResult::failure("auth_flow", format!("No auth_required: {}", e))
            }
        };

        // 2. Send auth
        let auth_msg = json!({
            "type": "auth",
            "access_token": self.token
        });
        if let Err(e) = write.send(Message::Text(auth_msg.to_string())).await {
            return WsTestResult::failure("auth_flow", format!("Send auth failed: {}", e));
        }

        // 3. Receive auth_ok or auth_invalid
        let auth_response = match Self::recv_message(&mut read).await {
            Ok(msg) => msg,
            Err(e) => {
                return WsTestResult::failure("auth_flow", format!("No auth response: {}", e))
            }
        };

        exchanges.push(WsExchange {
            request: json!({"type": "auth_required"}),
            response: auth_required,
        });
        exchanges.push(WsExchange {
            request: auth_msg,
            response: auth_response.clone(),
        });

        // Check if auth succeeded
        if auth_response.get("type").and_then(|t| t.as_str()) != Some("auth_ok") {
            return WsTestResult::failure("auth_flow", format!("Auth failed: {:?}", auth_response));
        }

        WsTestResult::success("auth_flow", exchanges)
    }

    /// Run get_states test
    pub async fn test_get_states(&self) -> WsTestResult {
        match self
            .run_command("get_states", json!({"type": "get_states", "id": 1}))
            .await
        {
            Ok((request, response)) => {
                WsTestResult::success("get_states", vec![WsExchange { request, response }])
            }
            Err(e) => WsTestResult::failure("get_states", e),
        }
    }

    /// Run get_config test
    pub async fn test_get_config(&self) -> WsTestResult {
        match self
            .run_command("get_config", json!({"type": "get_config", "id": 1}))
            .await
        {
            Ok((request, response)) => {
                WsTestResult::success("get_config", vec![WsExchange { request, response }])
            }
            Err(e) => WsTestResult::failure("get_config", e),
        }
    }

    /// Run get_services test
    pub async fn test_get_services(&self) -> WsTestResult {
        match self
            .run_command("get_services", json!({"type": "get_services", "id": 1}))
            .await
        {
            Ok((request, response)) => {
                WsTestResult::success("get_services", vec![WsExchange { request, response }])
            }
            Err(e) => WsTestResult::failure("get_services", e),
        }
    }

    /// Run ping/pong test
    pub async fn test_ping_pong(&self) -> WsTestResult {
        match self
            .run_command("ping", json!({"type": "ping", "id": 1}))
            .await
        {
            Ok((request, response)) => {
                WsTestResult::success("ping_pong", vec![WsExchange { request, response }])
            }
            Err(e) => WsTestResult::failure("ping_pong", e),
        }
    }

    /// Run subscribe_events test
    pub async fn test_subscribe_events(&self) -> WsTestResult {
        match self
            .run_command(
                "subscribe_events",
                json!({"type": "subscribe_events", "id": 1, "event_type": "state_changed"}),
            )
            .await
        {
            Ok((request, response)) => {
                WsTestResult::success("subscribe_events", vec![WsExchange { request, response }])
            }
            Err(e) => WsTestResult::failure("subscribe_events", e),
        }
    }

    /// Run call_service test
    pub async fn test_call_service(&self) -> WsTestResult {
        match self
            .run_command(
                "call_service",
                json!({
                    "type": "call_service",
                    "id": 1,
                    "domain": "homeassistant",
                    "service": "check_config",
                    "service_data": {}
                }),
            )
            .await
        {
            Ok((request, response)) => {
                WsTestResult::success("call_service", vec![WsExchange { request, response }])
            }
            Err(e) => WsTestResult::failure("call_service", e),
        }
    }

    /// Run config/device_registry/list test
    pub async fn test_device_registry_list(&self) -> WsTestResult {
        match self
            .run_command(
                "device_registry_list",
                json!({"type": "config/device_registry/list", "id": 1}),
            )
            .await
        {
            Ok((request, response)) => WsTestResult::success(
                "device_registry_list",
                vec![WsExchange { request, response }],
            ),
            Err(e) => WsTestResult::failure("device_registry_list", e),
        }
    }

    /// Run config_entries/subentries/list test
    pub async fn test_config_entries_subentries_list(&self) -> WsTestResult {
        // We need an entry_id - test with empty string which should return empty array
        match self
            .run_command(
                "config_entries_subentries_list",
                json!({"type": "config_entries/subentries/list", "id": 1, "entry_id": "test_entry"}),
            )
            .await
        {
            Ok((request, response)) => WsTestResult::success(
                "config_entries_subentries_list",
                vec![WsExchange { request, response }],
            ),
            Err(e) => WsTestResult::failure("config_entries_subentries_list", e),
        }
    }

    /// Run config_entries/get test
    pub async fn test_config_entries_get(&self) -> WsTestResult {
        match self
            .run_command(
                "config_entries_get",
                json!({"type": "config_entries/get", "id": 1}),
            )
            .await
        {
            Ok((request, response)) => {
                WsTestResult::success("config_entries_get", vec![WsExchange { request, response }])
            }
            Err(e) => WsTestResult::failure("config_entries_get", e),
        }
    }

    /// Run config_entries/subscribe test
    pub async fn test_config_entries_subscribe(&self) -> WsTestResult {
        match self
            .run_command(
                "config_entries_subscribe",
                json!({"type": "config_entries/subscribe", "id": 1}),
            )
            .await
        {
            Ok((request, response)) => WsTestResult::success(
                "config_entries_subscribe",
                vec![WsExchange { request, response }],
            ),
            Err(e) => WsTestResult::failure("config_entries_subscribe", e),
        }
    }

    /// Run config/entity_registry/list test
    pub async fn test_entity_registry_list(&self) -> WsTestResult {
        match self
            .run_command(
                "entity_registry_list",
                json!({"type": "config/entity_registry/list", "id": 1}),
            )
            .await
        {
            Ok((request, response)) => WsTestResult::success(
                "entity_registry_list",
                vec![WsExchange { request, response }],
            ),
            Err(e) => WsTestResult::failure("entity_registry_list", e),
        }
    }

    /// Run config/area_registry/list test
    pub async fn test_area_registry_list(&self) -> WsTestResult {
        match self
            .run_command(
                "area_registry_list",
                json!({"type": "config/area_registry/list", "id": 1}),
            )
            .await
        {
            Ok((request, response)) => {
                WsTestResult::success("area_registry_list", vec![WsExchange { request, response }])
            }
            Err(e) => WsTestResult::failure("area_registry_list", e),
        }
    }

    /// Run config/floor_registry/list test
    pub async fn test_floor_registry_list(&self) -> WsTestResult {
        match self
            .run_command(
                "floor_registry_list",
                json!({"type": "config/floor_registry/list", "id": 1}),
            )
            .await
        {
            Ok((request, response)) => WsTestResult::success(
                "floor_registry_list",
                vec![WsExchange { request, response }],
            ),
            Err(e) => WsTestResult::failure("floor_registry_list", e),
        }
    }

    /// Run config/label_registry/list test
    pub async fn test_label_registry_list(&self) -> WsTestResult {
        match self
            .run_command(
                "label_registry_list",
                json!({"type": "config/label_registry/list", "id": 1}),
            )
            .await
        {
            Ok((request, response)) => WsTestResult::success(
                "label_registry_list",
                vec![WsExchange { request, response }],
            ),
            Err(e) => WsTestResult::failure("label_registry_list", e),
        }
    }

    /// Connect, authenticate, and run a single command
    async fn run_command(&self, name: &str, command: Value) -> Result<(Value, Value), String> {
        let ws_url = self.ws_url();

        let connect_result = timeout(Duration::from_secs(10), connect_async(&ws_url)).await;

        let (ws_stream, _) = match connect_result {
            Ok(Ok((stream, _))) => (stream, ()),
            Ok(Err(e)) => return Err(format!("Connect failed: {}", e)),
            Err(_) => return Err("Connect timeout".to_string()),
        };

        let (mut write, mut read) = ws_stream.split();

        // Auth flow
        let _ = Self::recv_message(&mut read).await?; // auth_required

        let auth_msg = json!({
            "type": "auth",
            "access_token": self.token
        });
        write
            .send(Message::Text(auth_msg.to_string()))
            .await
            .map_err(|e| format!("Send auth failed: {}", e))?;

        let auth_response = Self::recv_message(&mut read).await?;
        if auth_response.get("type").and_then(|t| t.as_str()) != Some("auth_ok") {
            return Err(format!("Auth failed: {:?}", auth_response));
        }

        // Send command
        write
            .send(Message::Text(command.to_string()))
            .await
            .map_err(|e| format!("Send {} failed: {}", name, e))?;

        // Receive response
        let response = Self::recv_message(&mut read).await?;

        Ok((command, response))
    }

    /// Receive a JSON message with timeout
    async fn recv_message(
        read: &mut futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
    ) -> Result<Value, String> {
        let msg = timeout(Duration::from_secs(10), read.next())
            .await
            .map_err(|_| "Receive timeout")?
            .ok_or("Connection closed")?
            .map_err(|e| format!("Receive error: {}", e))?;

        match msg {
            Message::Text(text) => {
                serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))
            }
            other => Err(format!("Unexpected message type: {:?}", other)),
        }
    }
}
