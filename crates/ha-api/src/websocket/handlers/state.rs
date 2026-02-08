//! State handlers: get_states, get_config, get_services

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::error::{WebSocketError, WsResult};
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::{OutgoingMessage, ResultMessage};

/// Handle get_states command
pub async fn handle_get_states(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle get_config command
pub async fn handle_get_config(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle get_services command
pub async fn handle_get_services(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}
