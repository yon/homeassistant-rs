//! Registry handlers: device, area, floor, label, category

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::error::{WebSocketError, WsResult};
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::{OutgoingMessage, ResultMessage};

/// Handle config/device_registry/list command
pub async fn handle_device_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
                    ha_registries::DisabledBy::ConfigEntry => "config_entry",
                    ha_registries::DisabledBy::Device => "device",
                    ha_registries::DisabledBy::Hass => "hass",
                    ha_registries::DisabledBy::Integration => "integration",
                    ha_registries::DisabledBy::User => "user",
                }),
                "configuration_url": device.configuration_url,
                "labels": device.labels,
                "config_entries_subentries": device.config_entries_subentries,
                "primary_config_entry": device.primary_config_entry,
                "created_at": device.created_at.timestamp() as f64,
                "modified_at": device.modified_at.timestamp() as f64,
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle config/area_registry/list command
pub async fn handle_area_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle config/floor_registry/list command
pub async fn handle_floor_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle config/label_registry/list command
pub async fn handle_label_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle config/category_registry/list command
pub async fn handle_category_registry_list(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _scope: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Return empty categories list
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
