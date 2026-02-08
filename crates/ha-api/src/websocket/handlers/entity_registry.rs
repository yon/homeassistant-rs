//! Entity registry handlers

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::warn;

use crate::error::{WebSocketError, WsResult};
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::{ErrorInfo, OutgoingMessage, ResultMessage};

/// Handle config/entity_registry/get command
pub async fn handle_entity_registry_get(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    match conn.state.registries.entities.get(entity_id) {
        Some(entry) => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(entity_entry_to_json(&entry)),
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
                    message: format!("Entity not found: {}", entity_id),
                }),
            });
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
        }
    }
}

/// Handle config/entity_registry/list command
pub async fn handle_entity_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    let entries: Vec<serde_json::Value> = conn
        .state
        .registries
        .entities
        .iter()
        .into_iter()
        .map(|entry| entity_entry_to_json(&entry))
        .collect();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(entries)),
        error: None,
    });
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle config/entity_registry/remove command
pub async fn handle_entity_registry_remove(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
                    message: format!("Entity not found: {}", entity_id),
                }),
            });
            tx.send(result)
                .await
                .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
        }
    }
}

/// Handle config/entity_registry/update command
// Mirrors Python HA entity registry update which accepts many optional fields
#[allow(clippy::too_many_arguments)]
pub async fn handle_entity_registry_update(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    name: Option<String>,
    icon: Option<String>,
    area_id: Option<String>,
    disabled_by: Option<String>,
    hidden_by: Option<String>,
    new_entity_id: Option<String>,
    aliases: Option<HashSet<String>>,
    labels: Option<HashSet<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
        return tx
            .send(result)
            .await
            .map_err(|e| WebSocketError::ChannelSend(e.to_string()));
    }

    // Update the entity entry
    let updated_entry = conn
        .state
        .registries
        .entities
        .update(entity_id, |entry| {
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
                    "config_entry" => Some(ha_registries::DisabledBy::ConfigEntry),
                    "device" => Some(ha_registries::DisabledBy::Device),
                    "hass" => Some(ha_registries::DisabledBy::Hass),
                    "integration" => Some(ha_registries::DisabledBy::Integration),
                    "user" => Some(ha_registries::DisabledBy::User),
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
        })
        .expect("Entity should exist after presence check");

    // Handle entity_id rename if requested
    if let Some(new_id) = new_entity_id {
        if new_id != entity_id {
            // TODO(plan:T-03): Implement entity_id rename - requires updating the entity_id field
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle config/entity_registry/list_for_display command
pub async fn handle_entity_registry_list_for_display(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Return a simplified entity list for display purposes
    // Uses short keys matching HA's entity_registry.py display format
    let entries: Vec<serde_json::Value> = conn
        .state
        .registries
        .entities
        .iter()
        .into_iter()
        .map(|entry| {
            // device_class: use override if set, otherwise original_device_class
            let device_class = entry
                .device_class
                .clone()
                .or_else(|| entry.original_device_class.clone());
            let mut obj = serde_json::json!({
                "ei": entry.entity_id,
                "di": entry.device_id,
                "pl": entry.platform,
                "tk": entry.translation_key,
                // "en" should be name OR original_name (matching HA's logic)
                "en": entry.name.clone().or_else(|| entry.original_name.clone()),
                "ic": entry.icon,
                "ai": entry.area_id,
                "dc": device_class,
                "odc": entry.original_device_class,
                "ec": entry.entity_category.map(|c| match c {
                    ha_registries::EntityCategory::Config => 1,
                    ha_registries::EntityCategory::Diagnostic => 2,
                }),
                "hb": entry.hidden_by.map(|h| match h {
                    ha_registries::HiddenBy::Integration => "integration",
                    ha_registries::HiddenBy::User => "user",
                }),
                "lb": entry.labels,
            });
            // Add "hn" (has_entity_name) if true - frontend needs this
            if entry.has_entity_name == Some(true) {
                obj.as_object_mut()
                    .unwrap()
                    .insert("hn".to_string(), serde_json::json!(true));
            }
            obj
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
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
            ha_registries::DisabledBy::ConfigEntry => "config_entry",
            ha_registries::DisabledBy::Device => "device",
            ha_registries::DisabledBy::Hass => "hass",
            ha_registries::DisabledBy::Integration => "integration",
            ha_registries::DisabledBy::User => "user",
        }),
        "hidden_by": entry.hidden_by.map(|h| match h {
            ha_registries::HiddenBy::Integration => "integration",
            ha_registries::HiddenBy::User => "user",
        }),
        "entity_category": entry.entity_category.map(|c| match c {
            ha_registries::EntityCategory::Config => "config",
            ha_registries::EntityCategory::Diagnostic => "diagnostic",
        }),
        "has_entity_name": entry.has_entity_name.unwrap_or(false),
        "aliases": entry.aliases,
        "labels": entry.labels,
        "categories": entry.categories.clone().unwrap_or_else(|| serde_json::json!({})),
        "capabilities": entry.capabilities,
        "device_class": entry.device_class,
        "original_device_class": entry.original_device_class,
        "translation_key": entry.translation_key,
        "options": entry.options.clone().unwrap_or_else(|| serde_json::json!({})),
        "created_at": entry.created_at.timestamp_millis() as f64 / 1000.0,
        "modified_at": entry.modified_at.timestamp_millis() as f64 / 1000.0,
    })
}
