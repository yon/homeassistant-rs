//! Automation and script config handlers

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::error::{WebSocketError, WsResult};
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::{ErrorInfo, OutgoingMessage, ResultMessage};

/// Handle automation/config command - returns the automation configuration
pub async fn handle_automation_config(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
        return tx
            .send(result)
            .await
            .map_err(|e| WebSocketError::ChannelSend(e.to_string()));
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
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
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
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
        }
    }
}

/// Handle script/config command - returns the script configuration
pub async fn handle_script_config(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
        return tx
            .send(result)
            .await
            .map_err(|e| WebSocketError::ChannelSend(e.to_string()));
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
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
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
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
        }
    }
}
