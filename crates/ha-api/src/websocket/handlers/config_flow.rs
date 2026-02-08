//! Config flow handlers

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::error::{WebSocketError, WsResult};
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::{ErrorInfo, EventMessage, OutgoingMessage, ResultMessage};
use crate::AppState;

/// Handle config_entries/flow/progress without flow_id - lists flows in progress
///
/// Returns flows that are in progress but not started by a user (e.g., discovered devices).
pub async fn handle_config_entries_flow_progress_list(
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Return empty list since we don't have any auto-discovered flows yet
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(vec![])),
        error: None,
    });
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle config_entries/flow/subscribe command
pub async fn handle_config_entries_flow_subscribe(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Get list of active flows if config flow manager is available
    let flows = if let Some(cfm) = &conn.state.config_flow_handler {
        cfm.list_flows().await
    } else {
        vec![]
    };

    // Send initial flows state
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!(flows),
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

/// Handle config_entries/flow command - start a new config flow
pub async fn handle_config_entries_flow(
    conn: &Arc<ActiveConnection>,
    id: u64,
    handler: &str,
    show_advanced_options: bool,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    info!("Starting config flow for handler: {}", handler);

    let config_flow_handler = conn
        .state
        .config_flow_handler
        .as_ref()
        .ok_or(WebSocketError::ConfigFlowUnavailable)?;

    match config_flow_handler
        .start_flow(handler, show_advanced_options)
        .await
    {
        Ok(flow_result) => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::to_value(&flow_result).unwrap_or_default()),
                error: None,
            });
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
        }
        Err(e) => {
            error!("Failed to start config flow: {}", e);
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: e.error_code().to_string(),
                    message: e.to_string(),
                }),
            });
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
        }
    }
}

/// Handle config_entries/flow/progress command - continue a config flow
pub async fn handle_config_entries_flow_progress(
    conn: &Arc<ActiveConnection>,
    id: u64,
    flow_id: &str,
    user_input: Option<serde_json::Value>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    info!("Progressing config flow: {}", flow_id);

    let config_flow_handler = conn
        .state
        .config_flow_handler
        .as_ref()
        .ok_or(WebSocketError::ConfigFlowUnavailable)?;

    match config_flow_handler.progress_flow(flow_id, user_input).await {
        Ok(flow_result) => {
            // If the flow created an entry, we need to save it
            if flow_result.result_type == "create_entry" {
                if let Some(ref result_data) = flow_result.result {
                    // Create and save the config entry
                    if let Err(e) = save_config_entry_from_flow(
                        &conn.state,
                        &flow_result.handler,
                        flow_result.title.as_deref().unwrap_or(&flow_result.handler),
                        result_data,
                    )
                    .await
                    {
                        warn!("Failed to save config entry: {}", e);
                    }
                }
            }

            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::to_value(&flow_result).unwrap_or_default()),
                error: None,
            });
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
        }
        Err(e) => {
            error!("Failed to progress config flow: {}", e);
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: e.error_code().to_string(),
                    message: e.to_string(),
                }),
            });
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
        }
    }
}

/// Save a config entry created by a flow
async fn save_config_entry_from_flow(
    state: &AppState,
    domain: &str,
    title: &str,
    data: &serde_json::Value,
) -> WsResult<()> {
    use ha_config_entries::ConfigEntry;

    // Create entry using the constructor which handles all defaults
    let mut entry = ConfigEntry::new(domain, title);

    // Set the data from the flow
    if let Some(obj) = data.as_object() {
        for (k, v) in obj {
            entry.data.insert(k.clone(), v.clone());
        }
    }

    let entry_id = entry.entry_id.clone();

    // Add to config entries
    {
        let config_entries = state.config_entries.write().await;
        let _ = config_entries.add(entry).await;
    }

    // Save to disk
    {
        let config_entries = state.config_entries.read().await;
        config_entries.save().await.map_err(|e| {
            WebSocketError::ChannelSend(format!("Failed to save config entries: {}", e))
        })?;
    }

    info!("Created config entry {} for {}", entry_id, domain);
    Ok(())
}
