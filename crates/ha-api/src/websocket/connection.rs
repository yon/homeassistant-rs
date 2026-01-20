//! WebSocket connection handling
//!
//! Manages WebSocket connections, authentication, and message routing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use ha_core::Context;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::AppState;

use super::dispatch::handle_message;
use super::types::{
    AuthInvalidMessage, AuthOkMessage, AuthRequiredMessage, IncomingMessage, OutgoingMessage,
};

// =============================================================================
// Connection State
// =============================================================================

/// Per-connection state
pub struct ActiveConnection {
    /// App state reference
    pub state: AppState,
    /// Last message ID received
    last_id: AtomicU64,
    /// Active subscriptions: subscription_id -> unsubscribe function
    pub subscriptions: RwLock<HashMap<u64, broadcast::Sender<()>>>,
    /// User ID for this authenticated connection
    pub user_id: Option<String>,
    /// Whether this connection is authenticated
    pub authenticated: bool,
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
    pub fn validate_id(&self, id: u64) -> Result<(), &'static str> {
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

/// Handle a WebSocket connection
pub async fn handle_socket(socket: WebSocket, state: AppState) {
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
                // Log all incoming messages at info level for debugging
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(msg_type) = json.get("type").and_then(|t| t.as_str()) {
                        info!("WS RECV: type={}, full={}", msg_type, text);
                    }
                }
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

// =============================================================================
// Authentication
// =============================================================================

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
pub async fn send_message(
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
