//! Integration and manifest handlers

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::error::{WebSocketError, WsResult};
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::{OutgoingMessage, ResultMessage};

/// Handle integration/descriptions command
///
/// Returns descriptions of integrations for the "Add Integration" dialog.
/// This provides the list of available integrations the user can configure.
pub async fn handle_integration_descriptions(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _integrations: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Load integration descriptions from manifest files
    let integrations = crate::manifest::build_integration_descriptions();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(integrations),
        error: None,
    });
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle manifest/list command
pub async fn handle_manifest_list(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    let manifests = crate::manifest::build_manifest_list();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(manifests),
        error: None,
    });
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle manifest/get command
pub async fn handle_manifest_get(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    integration: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    let manifest = crate::manifest::build_manifest_response(integration).unwrap_or_else(|| {
        // Fallback for unknown integrations
        serde_json::json!({
            "domain": integration,
            "name": capitalize_first(integration),
            "config_flow": true,
            "documentation": format!("https://www.home-assistant.io/integrations/{}/", integration),
            "codeowners": [],
            "requirements": [],
            "dependencies": [],
            "iot_class": "calculated",
            "integration_type": "service",
            "is_built_in": false,
        })
    });

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(manifest),
        error: None,
    });
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle sensor/numeric_device_classes command
///
/// Returns the list of numeric sensor device classes. The frontend uses this
/// to determine which sensors should display numeric values vs other UI elements.
pub async fn handle_sensor_numeric_device_classes(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // All sensor device classes except date, enum, and timestamp are numeric
    // This list matches homeassistant/components/sensor/const.py
    let numeric_device_classes = vec![
        "absolute_humidity",
        "apparent_power",
        "aqi",
        "area",
        "atmospheric_pressure",
        "battery",
        "blood_glucose_concentration",
        "carbon_dioxide",
        "carbon_monoxide",
        "conductivity",
        "current",
        "data_rate",
        "data_size",
        "distance",
        "duration",
        "energy",
        "energy_storage",
        "frequency",
        "gas",
        "humidity",
        "illuminance",
        "irradiance",
        "moisture",
        "monetary",
        "nitrogen_dioxide",
        "nitrogen_monoxide",
        "nitrous_oxide",
        "ozone",
        "ph",
        "pm1",
        "pm10",
        "pm25",
        "power",
        "power_factor",
        "precipitation",
        "precipitation_intensity",
        "pressure",
        "reactive_power",
        "signal_strength",
        "sound_pressure",
        "speed",
        "sulphur_dioxide",
        "temperature",
        "volatile_organic_compounds",
        "volatile_organic_compounds_parts",
        "voltage",
        "volume",
        "volume_flow_rate",
        "volume_storage",
        "water",
        "weight",
        "wind_speed",
    ];

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "numeric_device_classes": numeric_device_classes
        })),
        error: None,
    });
    tx.send(result)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Capitalize first letter of a string
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
