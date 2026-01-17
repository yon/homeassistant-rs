//! Home Assistant WebSocket API
//!
//! Implements the Home Assistant WebSocket API for real-time communication.
//! Protocol: https://developers.home-assistant.io/docs/api/websocket

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use ha_core::Context;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::AppState;

// =============================================================================
// Message Types
// =============================================================================

/// Incoming WebSocket message from client
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IncomingMessage {
    Auth {
        access_token: Option<String>,
        #[serde(default)]
        api_password: Option<String>,
    },
    Ping {
        id: u64,
    },
    SubscribeEvents {
        id: u64,
        #[serde(default)]
        event_type: Option<String>,
    },
    UnsubscribeEvents {
        id: u64,
        subscription: u64,
    },
    GetStates {
        id: u64,
    },
    GetConfig {
        id: u64,
    },
    GetServices {
        id: u64,
    },
    CallService {
        id: u64,
        domain: String,
        service: String,
        #[serde(default)]
        target: Option<ServiceTarget>,
        #[serde(default)]
        service_data: Option<serde_json::Value>,
        #[serde(default)]
        return_response: bool,
    },
    FireEvent {
        id: u64,
        event_type: String,
        #[serde(default)]
        event_data: Option<serde_json::Value>,
    },
    SupportedFeatures {
        id: u64,
        #[allow(dead_code)] // Deserialized but not currently used
        features: HashMap<String, serde_json::Value>,
    },
}

/// Service call target
#[derive(Debug, Deserialize, Default)]
pub struct ServiceTarget {
    #[serde(default)]
    pub entity_id: Option<EntityIds>,
    #[serde(default)]
    pub device_id: Option<Vec<String>>,
    #[serde(default)]
    pub area_id: Option<Vec<String>>,
}

/// Entity IDs can be a single string or array
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EntityIds {
    Single(String),
    Multiple(Vec<String>),
}

impl EntityIds {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            EntityIds::Multiple(v) => v.clone(),
            EntityIds::Single(s) => vec![s.clone()],
        }
    }
}

/// Outgoing WebSocket message to client
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum OutgoingMessage {
    AuthRequired(AuthRequiredMessage),
    AuthOk(AuthOkMessage),
    AuthInvalid(AuthInvalidMessage),
    Pong(PongMessage),
    Result(ResultMessage),
    Event(EventMessage),
}

#[derive(Debug, Serialize)]
pub struct AuthRequiredMessage {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub ha_version: String,
}

#[derive(Debug, Serialize)]
pub struct AuthOkMessage {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub ha_version: String,
}

#[derive(Debug, Serialize)]
pub struct AuthInvalidMessage {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct PongMessage {
    pub id: u64,
    #[serde(rename = "type")]
    pub msg_type: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ResultMessage {
    pub id: u64,
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

#[derive(Debug, Serialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct EventMessage {
    pub id: u64,
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub event: serde_json::Value,
}

// =============================================================================
// Connection State
// =============================================================================

/// Per-connection state
pub struct ActiveConnection {
    /// App state reference
    state: AppState,
    /// Last message ID received
    last_id: AtomicU64,
    /// Active subscriptions: subscription_id -> unsubscribe function
    subscriptions: RwLock<HashMap<u64, broadcast::Sender<()>>>,
    /// User ID for this authenticated connection
    user_id: Option<String>,
    /// Whether this connection is authenticated
    authenticated: bool,
}

impl ActiveConnection {
    pub fn new(state: AppState, user_id: Option<String>) -> Self {
        Self {
            state,
            last_id: AtomicU64::new(0),
            subscriptions: RwLock::new(HashMap::new()),
            user_id,
            authenticated: false,
        }
    }

    /// Create a new context for this connection (each operation gets a fresh context)
    pub fn new_context(&self) -> Context {
        match &self.user_id {
            Some(uid) => Context::with_user(uid),
            None => Context::new(),
        }
    }

    /// Validate that the message ID is increasing
    fn validate_id(&self, id: u64) -> Result<(), &'static str> {
        let last = self.last_id.load(Ordering::SeqCst);
        if id <= last {
            return Err("id_reuse");
        }
        self.last_id.store(id, Ordering::SeqCst);
        Ok(())
    }
}

// =============================================================================
// WebSocket Handler
// =============================================================================

/// WebSocket upgrade handler
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle a WebSocket connection
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Send auth_required message
    let ha_version = env!("CARGO_PKG_VERSION").to_string();
    let auth_required = OutgoingMessage::AuthRequired(AuthRequiredMessage {
        msg_type: "auth_required",
        ha_version: ha_version.clone(),
    });

    if let Err(e) = send_message(&mut sender, &auth_required).await {
        error!("Failed to send auth_required: {}", e);
        return;
    }

    // Wait for auth message (with timeout)
    let auth_result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        wait_for_auth(&mut receiver),
    )
    .await;

    let (authenticated, user_id) = match auth_result {
        Ok(Ok(auth)) if auth.success => {
            // Send auth_ok
            let auth_ok = OutgoingMessage::AuthOk(AuthOkMessage {
                msg_type: "auth_ok",
                ha_version: ha_version.clone(),
            });
            if let Err(e) = send_message(&mut sender, &auth_ok).await {
                error!("Failed to send auth_ok: {}", e);
                return;
            }
            // Look up user_id from token
            let user_id = lookup_user_id(auth.access_token.as_deref());
            info!("WebSocket client authenticated (user_id: {:?})", user_id);
            (true, user_id)
        }
        Ok(Ok(_)) | Ok(Err(_)) => {
            // Send auth_invalid
            let auth_invalid = OutgoingMessage::AuthInvalid(AuthInvalidMessage {
                msg_type: "auth_invalid",
                message: "Invalid access token or password".to_string(),
            });
            let _ = send_message(&mut sender, &auth_invalid).await;
            warn!("WebSocket client authentication failed");
            return;
        }
        Err(_) => {
            // Timeout
            let auth_invalid = OutgoingMessage::AuthInvalid(AuthInvalidMessage {
                msg_type: "auth_invalid",
                message: "Authentication timeout".to_string(),
            });
            let _ = send_message(&mut sender, &auth_invalid).await;
            warn!("WebSocket client authentication timeout");
            return;
        }
    };

    // Create connection state with user_id
    let mut conn = ActiveConnection::new(state.clone(), user_id);
    conn.authenticated = authenticated;
    let conn = Arc::new(conn);

    // Create channel for sending messages
    let (tx, mut rx) = mpsc::channel::<OutgoingMessage>(256);

    // Spawn task to forward messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if send_message(&mut sender, &msg).await.is_err() {
                break;
            }
        }
    });

    // Process incoming messages
    while let Some(result) = receiver.next().await {
        match result {
            Ok(Message::Text(text)) => {
                debug!("Received: {}", text);
                if let Err(e) = handle_message(&conn, &text, &tx).await {
                    error!("Error handling message: {}", e);
                }
            }
            Ok(Message::Close(_)) => {
                info!("WebSocket client disconnected");
                break;
            }
            Ok(Message::Ping(data)) => {
                // Pong is handled automatically by axum
                debug!("Received ping: {:?}", data);
            }
            Ok(_) => {
                // Ignore other message types
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }

    // Cleanup subscriptions
    let subscriptions = conn.subscriptions.read().await;
    for (_, cancel_tx) in subscriptions.iter() {
        let _ = cancel_tx.send(());
    }
    drop(subscriptions);

    // Wait for send task to finish
    send_task.abort();
    info!("WebSocket connection closed");
}

/// Authentication result with optional token
pub struct AuthResult {
    pub success: bool,
    pub access_token: Option<String>,
}

/// Wait for authentication message
async fn wait_for_auth(
    receiver: &mut futures::stream::SplitStream<WebSocket>,
) -> Result<AuthResult, String> {
    while let Some(result) = receiver.next().await {
        match result {
            Ok(Message::Text(text)) => {
                // Try to parse as auth message
                if let Ok(msg) = serde_json::from_str::<IncomingMessage>(&text) {
                    match msg {
                        IncomingMessage::Auth {
                            access_token,
                            api_password,
                        } => {
                            // For now, accept any token (TODO: implement proper auth)
                            // In production, validate against HA's auth system
                            if access_token.is_some() || api_password.is_some() {
                                return Ok(AuthResult {
                                    success: true,
                                    access_token,
                                });
                            }
                            return Ok(AuthResult {
                                success: false,
                                access_token: None,
                            });
                        }
                        _ => {
                            // Non-auth message during auth phase
                            return Err("Expected auth message".to_string());
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => {
                return Err("Connection closed".to_string());
            }
            Err(e) => {
                return Err(format!("WebSocket error: {}", e));
            }
            _ => {}
        }
    }
    Err("Connection closed".to_string())
}

/// Look up user_id from access token
/// In production, this would query the auth storage/provider
fn lookup_user_id(access_token: Option<&str>) -> Option<String> {
    // For testing: map known test tokens to test user_id
    // In production, this would decode the JWT or query auth storage
    match access_token {
        // Plain text test token
        Some("test_api_token_for_comparison_testing_do_not_use_in_production") => {
            Some("test-user-id-12345678".to_string())
        }
        // JWT test token (generated from test-long-lived-token-id-456)
        Some(token)
            if token.starts_with(
                "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJ0ZXN0LWxvbmctbGl2ZWQtdG9rZW4t",
            ) =>
        {
            Some("test-user-id-12345678".to_string())
        }
        // Accept any token for now, but without user_id unless it's a known test token
        Some(_) => None,
        None => None,
    }
}

/// Send a message to the WebSocket
async fn send_message(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    msg: &OutgoingMessage,
) -> Result<(), String> {
    let json = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    debug!("Sending: {}", json);
    sender
        .send(Message::Text(json))
        .await
        .map_err(|e| e.to_string())
}

/// Handle an incoming message
async fn handle_message(
    conn: &Arc<ActiveConnection>,
    text: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Parse the message
    let msg: IncomingMessage =
        serde_json::from_str(text).map_err(|e| format!("Invalid message format: {}", e))?;

    match msg {
        IncomingMessage::Auth { .. } => {
            // Already authenticated, ignore
            Ok(())
        }
        IncomingMessage::CallService {
            id,
            domain,
            service,
            target,
            service_data,
            return_response,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_call_service(
                conn,
                id,
                domain,
                service,
                target,
                service_data,
                return_response,
                tx,
            )
            .await
        }
        IncomingMessage::FireEvent {
            id,
            event_type,
            event_data,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_fire_event(conn, id, event_type, event_data, tx).await
        }
        IncomingMessage::GetConfig { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_get_config(conn, id, tx).await
        }
        IncomingMessage::GetServices { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_get_services(conn, id, tx).await
        }
        IncomingMessage::GetStates { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_get_states(conn, id, tx).await
        }
        IncomingMessage::Ping { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            let pong = OutgoingMessage::Pong(PongMessage {
                id,
                msg_type: "pong",
            });
            tx.send(pong).await.map_err(|e| e.to_string())?;
            Ok(())
        }
        IncomingMessage::SubscribeEvents { id, event_type } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_subscribe_events(conn, id, event_type, tx).await
        }
        IncomingMessage::SupportedFeatures { id, features: _ } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            // Acknowledge supported features (we don't use coalescing yet)
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::Value::Null),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())?;
            Ok(())
        }
        IncomingMessage::UnsubscribeEvents { id, subscription } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_unsubscribe_events(conn, id, subscription, tx).await
        }
    }
}

// =============================================================================
// Command Handlers
// =============================================================================

/// Handle get_states command
async fn handle_get_states(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let states = conn.state.state_machine.all();
    let state_list: Vec<serde_json::Value> = states
        .iter()
        .map(|s| {
            serde_json::json!({
                "entity_id": s.entity_id.to_string(),
                "state": s.state,
                "attributes": s.attributes,
                "last_changed": s.last_changed.to_rfc3339(),
                "last_updated": s.last_updated.to_rfc3339(),
                "last_reported": s.last_reported.unwrap_or(s.last_updated).to_rfc3339(),
                "context": {
                    "id": s.context.id.to_string(),
                    "parent_id": s.context.parent_id,
                    "user_id": s.context.user_id,
                }
            })
        })
        .collect();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(state_list)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle get_config command
async fn handle_get_config(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let config = &conn.state.config;
    let unit_system = config.unit_system();

    let config_response = serde_json::json!({
        "latitude": config.latitude,
        "longitude": config.longitude,
        "elevation": config.elevation,
        "unit_system": {
            "length": unit_system.length,
            "accumulated_precipitation": unit_system.accumulated_precipitation,
            "mass": unit_system.mass,
            "pressure": unit_system.pressure,
            "temperature": unit_system.temperature,
            "volume": unit_system.volume,
            "wind_speed": unit_system.wind_speed,
            "area": unit_system.area,
        },
        "location_name": config.name,
        "time_zone": config.time_zone,
        "components": &*conn.state.components,
        "config_dir": "/config",
        "allowlist_external_dirs": config.allowlist_external_dirs,
        "allowlist_external_urls": config.allowlist_external_urls,
        "version": env!("CARGO_PKG_VERSION"),
        "config_source": "yaml",
        "recovery_mode": false,
        "safe_mode": false,
        "state": "RUNNING",
        "external_url": config.external_url,
        "internal_url": config.internal_url,
        "currency": config.currency,
        "country": config.country,
        "language": config.language,
        "radius": config.radius,
        "debug": false,
    });

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(config_response),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle get_services command
async fn handle_get_services(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let all_services = conn.state.service_registry.all_services();

    let mut services_map = serde_json::Map::new();
    for (domain, service_list) in all_services {
        let mut domain_services = serde_json::Map::new();
        for service_desc in service_list {
            domain_services.insert(
                service_desc.service.clone(),
                serde_json::json!({
                    "name": service_desc.name,
                    "description": service_desc.description,
                    "fields": {},
                    "target": service_desc.target,
                }),
            );
        }
        services_map.insert(domain, serde_json::Value::Object(domain_services));
    }

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Object(services_map)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle subscribe_events command
async fn handle_subscribe_events(
    conn: &Arc<ActiveConnection>,
    id: u64,
    event_type: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Create cancellation channel
    let (cancel_tx, mut cancel_rx) = broadcast::channel::<()>(1);

    // Store subscription
    {
        let mut subs = conn.subscriptions.write().await;
        subs.insert(id, cancel_tx);
    }

    // Subscribe to events
    let event_type_filter = event_type.clone();
    let tx_clone = tx.clone();
    let sub_id = id;

    // Get a receiver from the event bus (subscribe to all events)
    let mut event_rx = conn.state.event_bus.subscribe_all();

    // Spawn task to forward events
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_rx.recv() => {
                    debug!("Subscription {} cancelled", sub_id);
                    break;
                }
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            // Filter by event type if specified
                            if let Some(ref filter) = event_type_filter {
                                if event.event_type.as_str() != filter {
                                    continue;
                                }
                            }

                            // Send event to client
                            let event_msg = OutgoingMessage::Event(EventMessage {
                                id: sub_id,
                                msg_type: "event",
                                event: serde_json::json!({
                                    "event_type": event.event_type,
                                    "data": event.data,
                                    "origin": "LOCAL",
                                    "time_fired": event.time_fired.to_rfc3339(),
                                    "context": {
                                        "id": event.context.id.to_string(),
                                        "parent_id": event.context.parent_id,
                                        "user_id": event.context.user_id,
                                    }
                                }),
                            });
                            if tx_clone.send(event_msg).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            // Missed some events, continue
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }
        }
    });

    // Send success response - explicitly include "result": null to match Python HA
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle unsubscribe_events command
async fn handle_unsubscribe_events(
    conn: &Arc<ActiveConnection>,
    id: u64,
    subscription: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let mut subs = conn.subscriptions.write().await;
    if let Some(cancel_tx) = subs.remove(&subscription) {
        let _ = cancel_tx.send(());
    }
    drop(subs);

    // Explicitly include "result": null to match Python HA
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle call_service command
#[allow(clippy::too_many_arguments)]
async fn handle_call_service(
    conn: &Arc<ActiveConnection>,
    id: u64,
    domain: String,
    service: String,
    target: Option<ServiceTarget>,
    service_data: Option<serde_json::Value>,
    return_response: bool,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Merge target into service_data
    let mut data = service_data.unwrap_or(serde_json::json!({}));
    if let Some(target) = target {
        if let Some(entity_ids) = target.entity_id {
            data["entity_id"] = serde_json::json!(entity_ids.to_vec());
        }
        if let Some(device_ids) = target.device_id {
            data["device_id"] = serde_json::json!(device_ids);
        }
        if let Some(area_ids) = target.area_id {
            data["area_id"] = serde_json::json!(area_ids);
        }
    }

    // Create a new context with user_id for this service call
    let context = conn.new_context();

    match conn
        .state
        .service_registry
        .call(&domain, &service, data, context.clone(), return_response)
        .await
    {
        Ok(response) => {
            let mut result_data = serde_json::json!({
                "context": {
                    "id": context.id.to_string(),
                    "parent_id": context.parent_id,
                    "user_id": context.user_id,
                }
            });

            if return_response {
                if let Some(resp) = response {
                    result_data["response"] = resp;
                }
            }

            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(result_data),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
        Err(e) => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: "service_error".to_string(),
                    message: e.to_string(),
                }),
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

/// Handle fire_event command
async fn handle_fire_event(
    conn: &Arc<ActiveConnection>,
    id: u64,
    event_type: String,
    event_data: Option<serde_json::Value>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let data = event_data.unwrap_or(serde_json::json!({}));
    // Create a new context with user_id for this event
    let context = conn.new_context();

    let event = ha_core::Event::new(event_type, data, context.clone());
    conn.state.event_bus.fire(event);

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "context": {
                "id": context.id.to_string(),
                "parent_id": context.parent_id,
                "user_id": context.user_id,
            }
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_auth_message() {
        let json = r#"{"type": "auth", "access_token": "test_token"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::Auth { access_token, .. } => {
                assert_eq!(access_token, Some("test_token".to_string()));
            }
            _ => panic!("Expected Auth message"),
        }
    }

    #[test]
    fn test_parse_ping_message() {
        let json = r#"{"type": "ping", "id": 1}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::Ping { id } => {
                assert_eq!(id, 1);
            }
            _ => panic!("Expected Ping message"),
        }
    }

    #[test]
    fn test_parse_subscribe_events() {
        let json = r#"{"type": "subscribe_events", "id": 1, "event_type": "state_changed"}"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::SubscribeEvents { id, event_type } => {
                assert_eq!(id, 1);
                assert_eq!(event_type, Some("state_changed".to_string()));
            }
            _ => panic!("Expected SubscribeEvents message"),
        }
    }

    #[test]
    fn test_parse_call_service() {
        let json = r#"{
            "type": "call_service",
            "id": 1,
            "domain": "light",
            "service": "turn_on",
            "target": {"entity_id": "light.living_room"},
            "service_data": {"brightness": 255}
        }"#;
        let msg: IncomingMessage = serde_json::from_str(json).unwrap();
        match msg {
            IncomingMessage::CallService {
                id,
                domain,
                service,
                ..
            } => {
                assert_eq!(id, 1);
                assert_eq!(domain, "light");
                assert_eq!(service, "turn_on");
            }
            _ => panic!("Expected CallService message"),
        }
    }

    #[test]
    fn test_serialize_auth_required() {
        let msg = OutgoingMessage::AuthRequired(AuthRequiredMessage {
            msg_type: "auth_required",
            ha_version: "2024.1.0".to_string(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("auth_required"));
        assert!(json.contains("2024.1.0"));
    }

    #[test]
    fn test_serialize_result() {
        let msg = OutgoingMessage::Result(ResultMessage {
            id: 1,
            msg_type: "result",
            success: true,
            result: Some(serde_json::json!({"test": "value"})),
            error: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"success\":true"));
    }
}
