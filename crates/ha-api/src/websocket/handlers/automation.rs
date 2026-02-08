//! Automation and script config handlers

use std::sync::Arc;

use tokio::sync::mpsc;

use super::{send_error, send_result};
use crate::error::WsResult;
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::OutgoingMessage;

/// Handle automation/config command - returns the automation configuration
pub async fn handle_automation_config(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Verify entity_id starts with "automation."
    if !entity_id.starts_with("automation.") {
        return send_error(id, "not_found", "Entity not found".to_string(), tx).await;
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

            send_result(id, serde_json::json!({ "config": config }), tx).await
        }
        None => send_error(id, "not_found", "Entity not found".to_string(), tx).await,
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
        return send_error(id, "not_found", "Entity not found".to_string(), tx).await;
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

            send_result(id, serde_json::json!({ "config": config }), tx).await
        }
        None => send_error(id, "not_found", "Entity not found".to_string(), tx).await,
    }
}
