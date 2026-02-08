//! Service handlers: call_service, fire_event

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::error::{WebSocketError, WsResult};
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::{ErrorInfo, OutgoingMessage, ResultMessage, ServiceTarget};

/// Handle call_service command
// Handler requires shared connection, message id, service params, and response channel
#[allow(clippy::too_many_arguments)]
pub async fn handle_call_service(
    conn: &Arc<ActiveConnection>,
    id: u64,
    domain: String,
    service: String,
    target: Option<ServiceTarget>,
    service_data: Option<serde_json::Value>,
    return_response: bool,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
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
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
        }
    }
}

/// Handle fire_event command
pub async fn handle_fire_event(
    conn: &Arc<ActiveConnection>,
    id: u64,
    event_type: String,
    event_data: Option<serde_json::Value>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}
