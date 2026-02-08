//! Miscellaneous handlers: system_log, render_template, auth, recorder, etc.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::error::{WebSocketError, WsResult};
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::{EventMessage, OutgoingMessage, PongMessage, ResultMessage};

/// Handle system_log/list command
pub async fn handle_system_log_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    let entries = conn.state.system_log.list();
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!(entries)),
        error: None,
    });
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle render_template command
pub async fn handle_render_template(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    template: &str,
    variables: Option<HashMap<String, serde_json::Value>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle auth/current_user command - returns current user info
pub async fn handle_auth_current_user(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle recorder/info command
pub async fn handle_recorder_info(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle repairs/list_issues command
pub async fn handle_repairs_list_issues(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle ping command
pub async fn handle_ping(id: u64, tx: &mpsc::Sender<OutgoingMessage>) -> WsResult<()> {
    tx.send(OutgoingMessage::Pong(PongMessage {
        id,
        msg_type: "pong",
    }))
    .await
    .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle persistent_notification/subscribe command
pub async fn handle_persistent_notification_subscribe(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Get current notifications
    let notifications = conn.state.notifications.get_all_map();

    // Convert to JSON-serializable format
    let notifications_json: serde_json::Map<String, serde_json::Value> = notifications
        .into_iter()
        .map(|(k, v)| (k, serde_json::to_value(v).unwrap_or_default()))
        .collect();

    // Send success response first (matching Python HA behavior)
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))?;

    // Send initial notifications event with "current" type
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({
            "type": "current",
            "notifications": notifications_json
        }),
    });
    tx.send(event)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle labs/subscribe command
pub async fn handle_labs_subscribe(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Send initial labs state event (empty)
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({}),
    });
    tx.send(event)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))?;

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle logger/log_info command
pub async fn handle_logger_log_info(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Return empty array of logger info
    // Format: [{"domain": "integration_name", "level": 20}, ...]
    // Level values: DEBUG=10, INFO=20, WARNING=30, ERROR=40, CRITICAL=50
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!([])),
        error: None,
    });
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle entity/source command
pub async fn handle_entity_source(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_ids: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle supported_features command
pub async fn handle_supported_features(
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Acknowledge supported features (we don't use coalescing yet)
    super::send_result(id, serde_json::Value::Null, tx).await
}

/// Handle blueprint/list command
pub async fn handle_blueprint_list(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _domain: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Return empty blueprints
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({})),
        error: None,
    });
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}
