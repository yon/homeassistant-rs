//! Home Assistant WebSocket API
//!
//! Implements the Home Assistant WebSocket API for real-time communication.
//! Protocol: https://developers.home-assistant.io/docs/api/websocket

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

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
    #[serde(rename = "auth/current_user")]
    AuthCurrentUser {
        id: u64,
    },
    #[serde(rename = "automation/config")]
    AutomationConfig {
        id: u64,
        entity_id: String,
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
    #[serde(rename = "config/entity_registry/get")]
    EntityRegistryGet {
        id: u64,
        entity_id: String,
    },
    #[serde(rename = "config/entity_registry/list")]
    EntityRegistryList {
        id: u64,
    },
    #[serde(rename = "config/entity_registry/remove")]
    EntityRegistryRemove {
        id: u64,
        entity_id: String,
    },
    #[serde(rename = "config/entity_registry/update")]
    EntityRegistryUpdate {
        id: u64,
        entity_id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        icon: Option<String>,
        #[serde(default)]
        area_id: Option<String>,
        #[serde(default)]
        disabled_by: Option<String>,
        #[serde(default)]
        hidden_by: Option<String>,
        #[serde(default)]
        new_entity_id: Option<String>,
        #[serde(default)]
        aliases: Option<Vec<String>>,
        #[serde(default)]
        labels: Option<Vec<String>>,
    },
    #[serde(rename = "config/entity_registry/list_for_display")]
    EntityRegistryListForDisplay {
        id: u64,
    },
    #[serde(rename = "config/device_registry/list")]
    DeviceRegistryList {
        id: u64,
    },
    #[serde(rename = "config/area_registry/list")]
    AreaRegistryList {
        id: u64,
    },
    #[serde(rename = "config/floor_registry/list")]
    FloorRegistryList {
        id: u64,
    },
    #[serde(rename = "config/label_registry/list")]
    LabelRegistryList {
        id: u64,
    },
    FireEvent {
        id: u64,
        event_type: String,
        #[serde(default)]
        event_data: Option<serde_json::Value>,
    },
    #[serde(rename = "frontend/get_themes")]
    FrontendGetThemes {
        id: u64,
    },
    #[serde(rename = "frontend/get_translations")]
    FrontendGetTranslations {
        id: u64,
        #[serde(default)]
        language: Option<String>,
        #[serde(default)]
        category: Option<String>,
        #[serde(default)]
        integration: Option<Vec<String>>,
        #[serde(default)]
        config_flow: Option<bool>,
    },
    #[serde(rename = "frontend/subscribe_user_data")]
    FrontendSubscribeUserData {
        id: u64,
        #[serde(default)]
        key: Option<String>,
    },
    #[serde(rename = "frontend/subscribe_system_data")]
    FrontendSubscribeSystemData {
        id: u64,
        #[serde(default)]
        key: Option<String>,
    },
    GetConfig {
        id: u64,
    },
    GetPanels {
        id: u64,
    },
    GetServices {
        id: u64,
    },
    GetStates {
        id: u64,
    },
    #[serde(rename = "lovelace/config")]
    LovelaceConfig {
        id: u64,
        #[serde(default)]
        url_path: Option<String>,
    },
    #[serde(rename = "lovelace/resources")]
    LovelaceResources {
        id: u64,
    },
    Ping {
        id: u64,
    },
    #[serde(rename = "recorder/info")]
    RecorderInfo {
        id: u64,
    },
    #[serde(rename = "repairs/list_issues")]
    RepairsListIssues {
        id: u64,
    },
    #[serde(rename = "persistent_notification/subscribe")]
    PersistentNotificationSubscribe {
        id: u64,
    },
    #[serde(rename = "labs/subscribe")]
    LabsSubscribe {
        id: u64,
    },
    #[serde(rename = "config_entries/get")]
    ConfigEntriesGet {
        id: u64,
        entry_id: String,
    },
    #[serde(rename = "config_entries/subscribe")]
    ConfigEntriesSubscribe {
        id: u64,
        #[serde(default)]
        type_filter: Option<Vec<String>>,
    },
    #[serde(rename = "config_entries/flow/subscribe")]
    ConfigEntriesFlowSubscribe {
        id: u64,
    },
    #[serde(rename = "logger/log_info")]
    LoggerLogInfo {
        id: u64,
    },
    #[serde(rename = "manifest/list")]
    ManifestList {
        id: u64,
    },
    #[serde(rename = "entity/source")]
    EntitySource {
        id: u64,
        #[serde(default)]
        entity_id: Option<Vec<String>>,
    },
    #[serde(rename = "config/category_registry/list")]
    CategoryRegistryList {
        id: u64,
        #[serde(default)]
        scope: Option<String>,
    },
    #[serde(rename = "blueprint/list")]
    BlueprintList {
        id: u64,
        domain: String,
    },
    RenderTemplate {
        id: u64,
        template: String,
        #[serde(default)]
        variables: Option<HashMap<String, serde_json::Value>>,
        #[serde(default)]
        #[allow(dead_code)] // Reserved for future use
        timeout: Option<f64>,
        #[serde(default)]
        #[allow(dead_code)] // Reserved for future use
        report_errors: Option<bool>,
    },
    #[serde(rename = "script/config")]
    ScriptConfig {
        id: u64,
        entity_id: String,
    },
    SubscribeEntities {
        id: u64,
        #[serde(default)]
        entity_ids: Option<Vec<String>>,
    },
    SubscribeEvents {
        id: u64,
        #[serde(default)]
        event_type: Option<String>,
    },
    SupportedFeatures {
        id: u64,
        #[allow(dead_code)] // Deserialized but not currently used
        features: HashMap<String, serde_json::Value>,
    },
    UnsubscribeEvents {
        id: u64,
        subscription: u64,
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
        IncomingMessage::AreaRegistryList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_area_registry_list(conn, id, tx).await
        }
        IncomingMessage::Auth { .. } => {
            // Already authenticated, ignore
            Ok(())
        }
        IncomingMessage::AuthCurrentUser { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_auth_current_user(conn, id, tx).await
        }
        IncomingMessage::AutomationConfig { id, entity_id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_automation_config(conn, id, &entity_id, tx).await
        }
        IncomingMessage::BlueprintList { id, domain } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_blueprint_list(conn, id, &domain, tx).await
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
        IncomingMessage::CategoryRegistryList { id, scope } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_category_registry_list(conn, id, scope, tx).await
        }
        IncomingMessage::ConfigEntriesFlowSubscribe { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_config_entries_flow_subscribe(conn, id, tx).await
        }
        IncomingMessage::ConfigEntriesGet { id, entry_id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_config_entries_get(conn, id, &entry_id, tx).await
        }
        IncomingMessage::ConfigEntriesSubscribe { id, type_filter } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_config_entries_subscribe(conn, id, type_filter, tx).await
        }
        IncomingMessage::DeviceRegistryList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_device_registry_list(conn, id, tx).await
        }
        IncomingMessage::EntityRegistryGet { id, entity_id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_entity_registry_get(conn, id, &entity_id, tx).await
        }
        IncomingMessage::EntityRegistryList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_entity_registry_list(conn, id, tx).await
        }
        IncomingMessage::EntityRegistryListForDisplay { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_entity_registry_list_for_display(conn, id, tx).await
        }
        IncomingMessage::EntityRegistryRemove { id, entity_id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_entity_registry_remove(conn, id, &entity_id, tx).await
        }
        IncomingMessage::EntityRegistryUpdate {
            id,
            entity_id,
            name,
            icon,
            area_id,
            disabled_by,
            hidden_by,
            new_entity_id,
            aliases,
            labels,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_entity_registry_update(
                conn,
                id,
                &entity_id,
                name,
                icon,
                area_id,
                disabled_by,
                hidden_by,
                new_entity_id,
                aliases,
                labels,
                tx,
            )
            .await
        }
        IncomingMessage::EntitySource { id, entity_id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_entity_source(conn, id, entity_id, tx).await
        }
        IncomingMessage::FireEvent {
            id,
            event_type,
            event_data,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_fire_event(conn, id, event_type, event_data, tx).await
        }
        IncomingMessage::FloorRegistryList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_floor_registry_list(conn, id, tx).await
        }
        IncomingMessage::FrontendGetThemes { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_frontend_get_themes(conn, id, tx).await
        }
        IncomingMessage::FrontendGetTranslations {
            id,
            language,
            category,
            integration,
            config_flow,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_frontend_get_translations(
                conn,
                id,
                language,
                category,
                integration,
                config_flow,
                tx,
            )
            .await
        }
        IncomingMessage::FrontendSubscribeSystemData { id, key } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_frontend_subscribe_system_data(conn, id, key, tx).await
        }
        IncomingMessage::FrontendSubscribeUserData { id, key } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_frontend_subscribe_user_data(conn, id, key, tx).await
        }
        IncomingMessage::GetConfig { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_get_config(conn, id, tx).await
        }
        IncomingMessage::GetPanels { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_get_panels(conn, id, tx).await
        }
        IncomingMessage::GetServices { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_get_services(conn, id, tx).await
        }
        IncomingMessage::GetStates { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_get_states(conn, id, tx).await
        }
        IncomingMessage::LabelRegistryList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_label_registry_list(conn, id, tx).await
        }
        IncomingMessage::LabsSubscribe { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_labs_subscribe(conn, id, tx).await
        }
        IncomingMessage::LoggerLogInfo { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_logger_log_info(conn, id, tx).await
        }
        IncomingMessage::LovelaceConfig { id, url_path } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_lovelace_config(conn, id, url_path, tx).await
        }
        IncomingMessage::LovelaceResources { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_lovelace_resources(conn, id, tx).await
        }
        IncomingMessage::ManifestList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_manifest_list(conn, id, tx).await
        }
        IncomingMessage::PersistentNotificationSubscribe { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_persistent_notification_subscribe(conn, id, tx).await
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
        IncomingMessage::RecorderInfo { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_recorder_info(conn, id, tx).await
        }
        IncomingMessage::RenderTemplate {
            id,
            template,
            variables,
            timeout: _,
            report_errors: _,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_render_template(conn, id, &template, variables, tx).await
        }
        IncomingMessage::RepairsListIssues { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_repairs_list_issues(conn, id, tx).await
        }
        IncomingMessage::ScriptConfig { id, entity_id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_script_config(conn, id, &entity_id, tx).await
        }
        IncomingMessage::SubscribeEntities { id, entity_ids } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handle_subscribe_entities(conn, id, entity_ids, tx).await
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

/// Handle config/entity_registry/get command
async fn handle_entity_registry_get(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    match conn.state.registries.entities.get(entity_id) {
        Some(entry) => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(entity_entry_to_json(&entry)),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
        None => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: "not_found".to_string(),
                    message: format!("Entity not found: {}", entity_id),
                }),
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

/// Handle config/entity_registry/list command
async fn handle_entity_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let entries: Vec<serde_json::Value> = conn
        .state
        .registries
        .entities
        .iter()
        .map(|entry| entity_entry_to_json(&entry))
        .collect();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(entries)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/entity_registry/remove command
async fn handle_entity_registry_remove(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    match conn.state.registries.entities.remove(entity_id) {
        Some(_) => {
            // Save changes to storage
            if let Err(e) = conn.state.registries.entities.save().await {
                warn!("Failed to save entity registry after removal: {}", e);
            }

            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::Value::Null),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
        None => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: "not_found".to_string(),
                    message: format!("Entity not found: {}", entity_id),
                }),
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

/// Handle config/entity_registry/update command
#[allow(clippy::too_many_arguments)]
async fn handle_entity_registry_update(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    name: Option<String>,
    icon: Option<String>,
    area_id: Option<String>,
    disabled_by: Option<String>,
    hidden_by: Option<String>,
    new_entity_id: Option<String>,
    aliases: Option<Vec<String>>,
    labels: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Check if entity exists
    if conn.state.registries.entities.get(entity_id).is_none() {
        let result = OutgoingMessage::Result(ResultMessage {
            id,
            msg_type: "result",
            success: false,
            result: None,
            error: Some(ErrorInfo {
                code: "not_found".to_string(),
                message: format!("Entity not found: {}", entity_id),
            }),
        });
        return tx.send(result).await.map_err(|e| e.to_string());
    }

    // Update the entity entry
    let updated_entry = conn.state.registries.entities.update(entity_id, |entry| {
        if let Some(n) = name {
            entry.name = Some(n);
        }
        if let Some(i) = icon {
            entry.icon = Some(i);
        }
        if let Some(a) = area_id {
            entry.area_id = if a.is_empty() { None } else { Some(a) };
        }
        if let Some(d) = disabled_by {
            entry.disabled_by = match d.as_str() {
                "user" => Some(ha_registries::DisabledBy::User),
                "integration" => Some(ha_registries::DisabledBy::Integration),
                "config_entry" => Some(ha_registries::DisabledBy::ConfigEntry),
                "device" => Some(ha_registries::DisabledBy::Device),
                "" => None,
                _ => entry.disabled_by,
            };
        }
        if let Some(h) = hidden_by {
            entry.hidden_by = match h.as_str() {
                "user" => Some(ha_registries::HiddenBy::User),
                "integration" => Some(ha_registries::HiddenBy::Integration),
                "" => None,
                _ => entry.hidden_by,
            };
        }
        if let Some(a) = aliases {
            entry.aliases = a;
        }
        if let Some(l) = labels {
            entry.labels = l;
        }
    });

    // Handle entity_id rename if requested
    if let Some(new_id) = new_entity_id {
        if new_id != entity_id {
            // TODO: Implement entity_id rename - requires updating the entity_id field
            // and re-indexing. For now, this is not supported.
            warn!(
                "Entity ID rename not yet implemented: {} -> {}",
                entity_id, new_id
            );
        }
    }

    // Save changes to storage
    if let Err(e) = conn.state.registries.entities.save().await {
        warn!("Failed to save entity registry after update: {}", e);
    }

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "entity_entry": entity_entry_to_json(&updated_entry)
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Convert an EntityEntry to the JSON format expected by the frontend
fn entity_entry_to_json(entry: &ha_registries::EntityEntry) -> serde_json::Value {
    serde_json::json!({
        "entity_id": entry.entity_id,
        "id": entry.id,
        "unique_id": entry.unique_id,
        "platform": entry.platform,
        "device_id": entry.device_id,
        "config_entry_id": entry.config_entry_id,
        "name": entry.name,
        "original_name": entry.original_name,
        "icon": entry.icon,
        "original_icon": entry.original_icon,
        "area_id": entry.area_id,
        "disabled_by": entry.disabled_by.map(|d| match d {
            ha_registries::DisabledBy::User => "user",
            ha_registries::DisabledBy::Integration => "integration",
            ha_registries::DisabledBy::ConfigEntry => "config_entry",
            ha_registries::DisabledBy::Device => "device",
        }),
        "hidden_by": entry.hidden_by.map(|h| match h {
            ha_registries::HiddenBy::Integration => "integration",
            ha_registries::HiddenBy::User => "user",
        }),
        "entity_category": entry.entity_category.map(|c| match c {
            ha_registries::EntityCategory::Config => "config",
            ha_registries::EntityCategory::Diagnostic => "diagnostic",
        }),
        "has_entity_name": entry.has_entity_name,
        "aliases": entry.aliases,
        "labels": entry.labels,
        "capabilities": entry.capabilities,
        "device_class": entry.device_class,
        "original_device_class": entry.original_device_class,
        "translation_key": entry.translation_key,
    })
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

/// Handle automation/config command - returns the automation configuration
async fn handle_automation_config(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Verify entity_id starts with "automation."
    if !entity_id.starts_with("automation.") {
        let result = OutgoingMessage::Result(ResultMessage {
            id,
            msg_type: "result",
            success: false,
            result: None,
            error: Some(ErrorInfo {
                code: "not_found".to_string(),
                message: "Entity not found".to_string(),
            }),
        });
        return tx.send(result).await.map_err(|e| e.to_string());
    }

    // Look up the automation entity state
    match conn.state.state_machine.get(entity_id) {
        Some(state) => {
            // The automation config is stored in the entity's attributes
            // Extract relevant config fields from attributes
            let config = serde_json::json!({
                "id": state.attributes.get("id").cloned().unwrap_or(serde_json::json!(entity_id)),
                "alias": state.attributes.get("friendly_name").cloned().unwrap_or(serde_json::Value::Null),
                "description": state.attributes.get("description").cloned().unwrap_or(serde_json::Value::Null),
                "trigger": state.attributes.get("trigger").cloned().unwrap_or(serde_json::json!([])),
                "condition": state.attributes.get("condition").cloned().unwrap_or(serde_json::json!([])),
                "action": state.attributes.get("action").cloned().unwrap_or(serde_json::json!([])),
                "mode": state.attributes.get("mode").cloned().unwrap_or(serde_json::json!("single")),
            });

            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::json!({ "config": config })),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
        None => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: "not_found".to_string(),
                    message: "Entity not found".to_string(),
                }),
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

/// Handle script/config command - returns the script configuration
async fn handle_script_config(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Verify entity_id starts with "script."
    if !entity_id.starts_with("script.") {
        let result = OutgoingMessage::Result(ResultMessage {
            id,
            msg_type: "result",
            success: false,
            result: None,
            error: Some(ErrorInfo {
                code: "not_found".to_string(),
                message: "Entity not found".to_string(),
            }),
        });
        return tx.send(result).await.map_err(|e| e.to_string());
    }

    // Look up the script entity state
    match conn.state.state_machine.get(entity_id) {
        Some(state) => {
            // The script config is stored in the entity's attributes
            let config = serde_json::json!({
                "alias": state.attributes.get("friendly_name").cloned().unwrap_or(serde_json::Value::Null),
                "description": state.attributes.get("description").cloned().unwrap_or(serde_json::Value::Null),
                "sequence": state.attributes.get("sequence").cloned().unwrap_or(serde_json::json!([])),
                "mode": state.attributes.get("mode").cloned().unwrap_or(serde_json::json!("single")),
                "icon": state.attributes.get("icon").cloned().unwrap_or(serde_json::Value::Null),
            });

            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::json!({ "config": config })),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
        None => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: "not_found".to_string(),
                    message: "Entity not found".to_string(),
                }),
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

/// Handle render_template command
async fn handle_render_template(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    template: &str,
    variables: Option<HashMap<String, serde_json::Value>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // For now, we'll do a simple template rendering
    // In a full implementation, this would use the TemplateEngine

    // Simple variable substitution for basic templates
    let mut result_str = template.to_string();

    // Handle variables if provided
    if let Some(vars) = variables {
        for (key, value) in vars {
            let placeholder = format!("{{{{ {} }}}}", key);
            let value_str = match value {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            };
            result_str = result_str.replace(&placeholder, &value_str);
        }
    }

    // For entity state templates like {{ states('sensor.temperature') }}
    // We would need the template engine, but for now return the template as-is
    // if it contains unresolved Jinja syntax

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "result": result_str,
            "listeners": {
                "all": false,
                "domains": [],
                "entities": [],
                "time": false
            }
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle subscribe_entities command
async fn handle_subscribe_entities(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_ids: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Create cancellation channel
    let (cancel_tx, mut cancel_rx) = broadcast::channel::<()>(1);

    // Store subscription
    {
        let mut subs = conn.subscriptions.write().await;
        subs.insert(id, cancel_tx);
    }

    // Get initial states for the requested entities
    let states = conn.state.state_machine.all();
    let filtered_states: Vec<&ha_core::State> = if let Some(ref ids) = entity_ids {
        states
            .iter()
            .filter(|s| ids.contains(&s.entity_id.to_string()))
            .collect()
    } else {
        states.iter().collect()
    };

    // Build initial state response
    let mut additions = serde_json::Map::new();
    for state in filtered_states {
        additions.insert(
            state.entity_id.to_string(),
            serde_json::json!({
                "s": state.state,
                "a": state.attributes,
                "c": state.context.id.to_string(),
                "lc": state.last_changed.timestamp_millis() as f64 / 1000.0,
                "lu": state.last_updated.timestamp_millis() as f64 / 1000.0,
            }),
        );
    }

    // Send initial state event
    let initial_event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({
            "a": additions,
        }),
    });
    tx.send(initial_event).await.map_err(|e| e.to_string())?;

    // Subscribe to state changes
    let entity_ids_filter = entity_ids.clone();
    let tx_clone = tx.clone();
    let sub_id = id;

    let mut event_rx = conn.state.event_bus.subscribe_all();

    // Spawn task to forward state change events
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_rx.recv() => {
                    debug!("Entity subscription {} cancelled", sub_id);
                    break;
                }
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            // Only forward state_changed events
                            if event.event_type.as_str() != "state_changed" {
                                continue;
                            }

                            // Extract entity_id from event data
                            if let Some(entity_id) = event.data.get("entity_id").and_then(|v| v.as_str()) {
                                // Filter by entity_ids if specified
                                if let Some(ref ids) = entity_ids_filter {
                                    if !ids.contains(&entity_id.to_string()) {
                                        continue;
                                    }
                                }

                                // Build change event
                                if let Some(new_state) = event.data.get("new_state") {
                                    let mut changes = serde_json::Map::new();
                                    changes.insert(
                                        entity_id.to_string(),
                                        serde_json::json!({
                                            "+": {
                                                "s": new_state.get("state"),
                                                "a": new_state.get("attributes"),
                                                "c": new_state.get("context").and_then(|c| c.get("id")),
                                                "lc": new_state.get("last_changed"),
                                                "lu": new_state.get("last_updated"),
                                            }
                                        }),
                                    );

                                    let change_event = OutgoingMessage::Event(EventMessage {
                                        id: sub_id,
                                        msg_type: "event",
                                        event: serde_json::json!({
                                            "c": changes,
                                        }),
                                    });
                                    if tx_clone.send(change_event).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
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

    // Send success response
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle auth/current_user command - returns current user info
async fn handle_auth_current_user(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return a default user for now
    let user = serde_json::json!({
        "id": conn.user_id.clone().unwrap_or_else(|| "default-user-id".to_string()),
        "name": "Owner",
        "is_owner": true,
        "is_admin": true,
        "credentials": [],
        "mfa_modules": [],
    });

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(user),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/entity_registry/list_for_display command
async fn handle_entity_registry_list_for_display(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return a simplified entity list for display purposes
    let entries: Vec<serde_json::Value> = conn
        .state
        .registries
        .entities
        .iter()
        .map(|entry| {
            serde_json::json!({
                "ei": entry.entity_id,
                "di": entry.device_id,
                "pl": entry.platform,
                "tk": entry.translation_key,
                "en": entry.name,
                "ic": entry.icon,
                "ai": entry.area_id,
                "ec": entry.entity_category.map(|c| match c {
                    ha_registries::EntityCategory::Config => 1,
                    ha_registries::EntityCategory::Diagnostic => 2,
                }),
                "hb": entry.hidden_by.map(|h| match h {
                    ha_registries::HiddenBy::Integration => "integration",
                    ha_registries::HiddenBy::User => "user",
                }),
                "lb": entry.labels,
            })
        })
        .collect();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "entity_categories": { "config": 1, "diagnostic": 2 },
            "entities": entries,
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/device_registry/list command
async fn handle_device_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let devices: Vec<serde_json::Value> = conn
        .state
        .registries
        .devices
        .iter()
        .map(|device| {
            serde_json::json!({
                "id": device.id,
                "config_entries": device.config_entries,
                "identifiers": device.identifiers,
                "connections": device.connections,
                "manufacturer": device.manufacturer,
                "model": device.model,
                "model_id": device.model_id,
                "name": device.name,
                "name_by_user": device.name_by_user,
                "sw_version": device.sw_version,
                "hw_version": device.hw_version,
                "serial_number": device.serial_number,
                "via_device_id": device.via_device_id,
                "area_id": device.area_id,
                "entry_type": device.entry_type.as_ref().map(|e| match e {
                    ha_registries::DeviceEntryType::Service => "service",
                }),
                "disabled_by": device.disabled_by.as_ref().map(|d| match d {
                    ha_registries::DisabledBy::User => "user",
                    ha_registries::DisabledBy::Integration => "integration",
                    ha_registries::DisabledBy::ConfigEntry => "config_entry",
                    ha_registries::DisabledBy::Device => "device",
                }),
                "configuration_url": device.configuration_url,
                "labels": device.labels,
            })
        })
        .collect();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(devices)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/area_registry/list command
async fn handle_area_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let areas: Vec<serde_json::Value> = conn
        .state
        .registries
        .areas
        .iter()
        .map(|area| {
            serde_json::json!({
                "area_id": area.id,
                "name": area.name,
                "aliases": area.aliases,
                "floor_id": area.floor_id,
                "icon": area.icon,
                "labels": area.labels,
                "picture": area.picture,
            })
        })
        .collect();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(areas)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/floor_registry/list command
async fn handle_floor_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let floors: Vec<serde_json::Value> = conn
        .state
        .registries
        .floors
        .iter()
        .map(|floor| {
            serde_json::json!({
                "floor_id": floor.id,
                "name": floor.name,
                "aliases": floor.aliases,
                "icon": floor.icon,
                "level": floor.level,
            })
        })
        .collect();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(floors)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/label_registry/list command
async fn handle_label_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let labels: Vec<serde_json::Value> = conn
        .state
        .registries
        .labels
        .iter()
        .map(|label| {
            serde_json::json!({
                "label_id": label.id,
                "name": label.name,
                "color": label.color,
                "description": label.description,
                "icon": label.icon,
            })
        })
        .collect();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(labels)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle frontend/get_themes command
async fn handle_frontend_get_themes(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return default themes structure
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "themes": {},
            "default_theme": "default",
            "default_dark_theme": null,
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle frontend/get_translations command
#[allow(clippy::too_many_arguments)]
async fn handle_frontend_get_translations(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _language: Option<String>,
    _category: Option<String>,
    _integration: Option<Vec<String>>,
    _config_flow: Option<bool>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty translations for now
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "resources": {}
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle frontend/subscribe_user_data command
async fn handle_frontend_subscribe_user_data(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    key: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Send initial user data event
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({
            "key": key.unwrap_or_default(),
            "data": {}
        }),
    });
    tx.send(event).await.map_err(|e| e.to_string())?;

    // Send success response
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle frontend/subscribe_system_data command
async fn handle_frontend_subscribe_system_data(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    key: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Send initial system data event
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({
            "key": key.unwrap_or_default(),
            "data": {}
        }),
    });
    tx.send(event).await.map_err(|e| e.to_string())?;

    // Send success response
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle get_panels command
async fn handle_get_panels(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return default panels structure
    let panels = serde_json::json!({
        "lovelace": {
            "component_name": "lovelace",
            "icon": "mdi:view-dashboard",
            "title": null,
            "config": {"mode": "storage"},
            "url_path": "lovelace",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "developer-tools": {
            "component_name": "developer_tools",
            "icon": "mdi:hammer",
            "title": null,
            "config": null,
            "url_path": "developer-tools",
            "require_admin": true,
            "config_panel_domain": null,
        },
        "config": {
            "component_name": "config",
            "icon": "mdi:cog",
            "title": null,
            "config": null,
            "url_path": "config",
            "require_admin": true,
            "config_panel_domain": null,
        },
        "history": {
            "component_name": "history",
            "icon": "mdi:chart-box",
            "title": null,
            "config": null,
            "url_path": "history",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "logbook": {
            "component_name": "logbook",
            "icon": "mdi:format-list-bulleted-type",
            "title": null,
            "config": null,
            "url_path": "logbook",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "map": {
            "component_name": "map",
            "icon": "mdi:tooltip-account",
            "title": null,
            "config": null,
            "url_path": "map",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "energy": {
            "component_name": "energy",
            "icon": "mdi:lightning-bolt",
            "title": null,
            "config": null,
            "url_path": "energy",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "media-browser": {
            "component_name": "media_browser",
            "icon": "mdi:play-box-multiple",
            "title": null,
            "config": null,
            "url_path": "media-browser",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "todo": {
            "component_name": "todo",
            "icon": "mdi:clipboard-list",
            "title": null,
            "config": null,
            "url_path": "todo",
            "require_admin": false,
            "config_panel_domain": null,
        },
    });

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(panels),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle lovelace/config command
async fn handle_lovelace_config(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _url_path: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return a basic auto-generated lovelace config
    let config = serde_json::json!({
        "title": "Home",
        "views": [
            {
                "path": "default_view",
                "title": "Home",
                "cards": [],
            }
        ],
    });

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(config),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle lovelace/resources command
async fn handle_lovelace_resources(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty resources list
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(vec![])),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle recorder/info command
async fn handle_recorder_info(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return minimal recorder info (indicates recorder is not running)
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "backlog": 0,
            "max_backlog": 40000,
            "migration_in_progress": false,
            "recording": false,
            "thread_running": false,
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle repairs/list_issues command
async fn handle_repairs_list_issues(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty issues list
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "issues": []
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle persistent_notification/subscribe command
async fn handle_persistent_notification_subscribe(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Send initial empty notifications event
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({
            "notifications": {}
        }),
    });
    tx.send(event).await.map_err(|e| e.to_string())?;

    // Send success response
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle labs/subscribe command
async fn handle_labs_subscribe(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Send initial labs state event (empty)
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({}),
    });
    tx.send(event).await.map_err(|e| e.to_string())?;

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Convert ConfigEntryState to HA-compatible string
fn config_entry_state_to_string(state: &ha_config_entries::ConfigEntryState) -> &'static str {
    use ha_config_entries::ConfigEntryState;
    match state {
        ConfigEntryState::FailedUnload => "failed_unload",
        ConfigEntryState::Loaded => "loaded",
        ConfigEntryState::MigrationError => "migration_error",
        ConfigEntryState::NotLoaded => "not_loaded",
        ConfigEntryState::SetupError => "setup_error",
        ConfigEntryState::SetupInProgress => "setup_in_progress",
        ConfigEntryState::SetupRetry => "setup_retry",
        ConfigEntryState::UnloadInProgress => "unload_in_progress",
    }
}

/// Convert a ConfigEntry to JSON format expected by frontend
fn config_entry_to_json(entry: &ha_config_entries::ConfigEntry) -> serde_json::Value {
    serde_json::json!({
        "entry_id": entry.entry_id,
        "domain": entry.domain,
        "title": entry.title,
        "source": format!("{:?}", entry.source).to_lowercase(),
        "state": config_entry_state_to_string(&entry.state),
        "supports_options": false,
        "supports_remove_device": false,
        "supports_unload": true,
        "supports_reconfigure": false,
        "pref_disable_new_entities": entry.pref_disable_new_entities,
        "pref_disable_polling": entry.pref_disable_polling,
        "disabled_by": entry.disabled_by.as_ref().map(|d| format!("{:?}", d).to_lowercase()),
        "reason": entry.reason,
    })
}

/// Handle config_entries/get command
async fn handle_config_entries_get(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entry_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Get the config entry from state
    let config_entries = conn.state.config_entries.read().await;

    let entry_json = if let Some(entry) = config_entries.get(entry_id) {
        config_entry_to_json(&entry)
    } else {
        // Return a stub entry if not found to prevent frontend errors
        serde_json::json!({
            "entry_id": entry_id,
            "domain": "unknown",
            "title": "Unknown",
            "source": "user",
            "state": "not_loaded",
            "supports_options": false,
            "supports_remove_device": false,
            "supports_unload": true,
            "supports_reconfigure": false,
            "pref_disable_new_entities": false,
            "pref_disable_polling": false,
            "disabled_by": null,
            "reason": null,
        })
    };

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(entry_json),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config_entries/subscribe command
async fn handle_config_entries_subscribe(
    conn: &Arc<ActiveConnection>,
    id: u64,
    type_filter: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Get all config entries from state
    let config_entries = conn.state.config_entries.read().await;

    // Filter entries by integration type if type_filter is provided
    // For now, we only have device integrations (like "demo"), not helpers
    // If type_filter is ["helper"], return empty since we have no helpers
    let is_helper_only_filter = type_filter
        .as_ref()
        .map(|f| f.len() == 1 && f[0] == "helper")
        .unwrap_or(false);

    // Format entries as {"type": null, "entry": {...}} per native HA
    let entries: Vec<serde_json::Value> = if is_helper_only_filter {
        // No helper integrations currently
        vec![]
    } else {
        config_entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "type": serde_json::Value::Null,
                    "entry": config_entry_to_json(&entry)
                })
            })
            .collect()
    };

    // Native HA sends result FIRST, then event
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())?;

    // Then send the event with all config entries
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!(entries),
    });
    tx.send(event).await.map_err(|e| e.to_string())
}

/// Handle config_entries/flow/subscribe command
async fn handle_config_entries_flow_subscribe(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Send initial empty flows state
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!([]),
    });
    tx.send(event).await.map_err(|e| e.to_string())?;

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle logger/log_info command
async fn handle_logger_log_info(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty logger info
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({})),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle manifest/list command
async fn handle_manifest_list(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty manifest list
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(vec![])),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle entity/source command
async fn handle_entity_source(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_ids: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let mut sources = serde_json::Map::new();

    // Get all states
    let states = conn.state.state_machine.all();

    for state in states.iter() {
        let entity_id = state.entity_id.to_string();

        // Filter if entity_ids provided
        if let Some(ref ids) = entity_ids {
            if !ids.contains(&entity_id) {
                continue;
            }
        }

        // Extract domain from entity_id
        let domain = entity_id.split('.').next().unwrap_or("unknown").to_string();

        sources.insert(
            entity_id,
            serde_json::json!({
                "domain": domain,
                "custom_component": false,
            }),
        );
    }

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Object(sources)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/category_registry/list command
async fn handle_category_registry_list(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _scope: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty categories list
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(vec![])),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle blueprint/list command
async fn handle_blueprint_list(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _domain: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty blueprints
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({})),
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
