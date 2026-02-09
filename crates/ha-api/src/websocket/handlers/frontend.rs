//! Frontend handlers: themes, icons, translations, panels, lovelace

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::debug;

use super::send_result;
use crate::error::{WebSocketError, WsResult};
use crate::translations;
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::{EventMessage, OutgoingMessage};

/// Handle frontend/get_user_data command (one-shot read)
pub async fn handle_frontend_get_user_data(
    id: u64,
    _key: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Stub: no persistence yet, return null for all keys
    send_result(id, serde_json::json!({"value": null}), tx).await
}

/// Handle frontend/set_user_data command (one-shot write)
pub async fn handle_frontend_set_user_data(
    id: u64,
    _key: &str,
    _value: Option<serde_json::Value>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Stub: accept the write but don't persist
    send_result(id, serde_json::Value::Null, tx).await
}

/// Handle frontend/get_themes command
pub async fn handle_frontend_get_themes(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Return default themes structure
    send_result(
        id,
        serde_json::json!({
            "themes": {},
            "default_theme": "default",
            "default_dark_theme": null,
        }),
        tx,
    )
    .await
}

/// Handle frontend/get_icons command
/// Returns icons.json data from integration components
pub async fn handle_frontend_get_icons(
    conn: &Arc<ActiveConnection>,
    id: u64,
    category: &str,
    integration: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Get components path from state
    let components_path = match &conn.state.components_path {
        Some(path) => path.clone(),
        None => {
            // No components path configured, return empty result
            return send_result(id, serde_json::json!({}), tx).await;
        }
    };

    // Determine which integrations to load icons for
    let integrations: Vec<String> = integration.unwrap_or_else(|| {
        // Default to loaded components
        conn.state.components.iter().cloned().collect()
    });

    let mut icons_result: HashMap<String, serde_json::Value> = HashMap::new();

    for integration_name in &integrations {
        let icons_path = components_path.join(integration_name).join("icons.json");

        if icons_path.exists() {
            match tokio::fs::read_to_string(&icons_path).await {
                Ok(content) => {
                    if let Ok(icons_data) = serde_json::from_str::<serde_json::Value>(&content) {
                        // Extract the requested category from the icons data
                        if let Some(category_data) = icons_data.get(category) {
                            icons_result.insert(integration_name.clone(), category_data.clone());
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to read icons.json for {}: {}", integration_name, e);
                }
            }
        }
    }

    send_result(
        id,
        serde_json::to_value(icons_result).unwrap_or_default(),
        tx,
    )
    .await
}

/// Handle frontend/get_translations command
// Handler requires shared connection, message id, translation filter params, and response channel
#[allow(clippy::too_many_arguments)]
pub async fn handle_frontend_get_translations(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    language: Option<String>,
    category: Option<String>,
    integration: Option<Vec<String>>,
    config_flow: Option<bool>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    let lang = language.as_deref().unwrap_or("en");
    let cat = category.as_deref();
    let is_config_flow = config_flow.unwrap_or(false);

    let translations =
        translations::get_translations(cat, integration.as_deref(), is_config_flow, lang);

    send_result(id, translations, tx).await
}

/// Handle frontend/subscribe_user_data command
pub async fn handle_frontend_subscribe_user_data(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    key: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Send initial user data event
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({
            "key": key.unwrap_or_default(),
            "data": {}
        }),
    });
    tx.send(event)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))?;

    // Send success response
    send_result(id, serde_json::Value::Null, tx).await
}

/// Handle frontend/subscribe_system_data command
pub async fn handle_frontend_subscribe_system_data(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    key: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Send initial system data event
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({
            "key": key.unwrap_or_default(),
            "data": {}
        }),
    });
    tx.send(event)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))?;

    // Send success response
    send_result(id, serde_json::Value::Null, tx).await
}

/// Handle get_panels command
pub async fn handle_get_panels(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Return default panels structure.
    // The `title` field is a translation key used by the frontend as
    // `localize("panel.<title>")`. Panels with null title are hidden
    // from the sidebar (except the default panel).
    let panels = serde_json::json!({
        "lovelace": {
            "component_name": "lovelace",
            "icon": "mdi:view-dashboard",
            "title": "states",
            "default_visible": true,
            "config": {"mode": "storage"},
            "url_path": "lovelace",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "config": {
            "component_name": "config",
            "icon": "mdi:cog",
            "title": "config",
            "default_visible": true,
            "config": null,
            "url_path": "config",
            "require_admin": true,
            "config_panel_domain": null,
        },
        "developer-tools": {
            "component_name": "developer-tools",
            "icon": "mdi:hammer",
            "title": "developer_tools",
            "default_visible": true,
            "config": null,
            "url_path": "developer-tools",
            "require_admin": true,
            "config_panel_domain": null,
        },
        "energy": {
            "component_name": "energy",
            "icon": "mdi:lightning-bolt",
            "title": "energy",
            "default_visible": true,
            "config": null,
            "url_path": "energy",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "history": {
            "component_name": "history",
            "icon": "mdi:chart-box",
            "title": "history",
            "default_visible": true,
            "config": null,
            "url_path": "history",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "logbook": {
            "component_name": "logbook",
            "icon": "mdi:format-list-bulleted-type",
            "title": "logbook",
            "default_visible": true,
            "config": null,
            "url_path": "logbook",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "map": {
            "component_name": "map",
            "icon": "mdi:tooltip-account",
            "title": "map",
            "default_visible": true,
            "config": null,
            "url_path": "map",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "media-browser": {
            "component_name": "media-browser",
            "icon": "mdi:play-box-multiple",
            "title": "media_browser",
            "default_visible": true,
            "config": null,
            "url_path": "media-browser",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "todo": {
            "component_name": "todo",
            "icon": "mdi:clipboard-list",
            "title": "todo",
            "default_visible": true,
            "config": null,
            "url_path": "todo",
            "require_admin": false,
            "config_panel_domain": null,
        },
    });

    send_result(id, panels, tx).await
}

/// Handle lovelace/config command
pub async fn handle_lovelace_config(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _url_path: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Return a basic auto-generated lovelace config
    let config = serde_json::json!({
        "title": "Home",
        "views": [
            {
                "path": "default_view",
                "title": "Home",
                "cards": [],
            }
        ],
    });

    send_result(id, config, tx).await
}

/// Handle lovelace/resources command
pub async fn handle_lovelace_resources(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Return empty resources list
    send_result(id, serde_json::Value::Array(vec![]), tx).await
}
