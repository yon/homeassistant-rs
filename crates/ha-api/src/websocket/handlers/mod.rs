//! WebSocket command handlers
//!
//! Individual handlers for each WebSocket command type, organized by domain.

mod automation;
mod config_entries;
mod config_flow;
mod entity_registry;
mod frontend;
mod integration;
mod misc;
mod registry;
mod service;
mod state;
mod subscription;

use tokio::sync::mpsc;

use crate::error::{WebSocketError, WsResult};

use super::types::{ErrorInfo, OutgoingMessage, ResultMessage};

/// Send a success result response.
pub(crate) async fn send_result(
    id: u64,
    data: serde_json::Value,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    tx.send(OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(data),
        error: None,
    }))
    .await
    .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Send an error result response.
pub(crate) async fn send_error(
    id: u64,
    code: &str,
    message: String,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    tx.send(OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: false,
        result: None,
        error: Some(ErrorInfo {
            code: code.to_string(),
            message,
        }),
    }))
    .await
    .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

pub use automation::{handle_automation_config, handle_script_config};
pub use config_entries::{
    handle_application_credentials_config, handle_application_credentials_config_entry,
    handle_application_credentials_create, handle_application_credentials_delete,
    handle_application_credentials_list, handle_config_entries_delete, handle_config_entries_get,
    handle_config_entries_subentries_list, handle_config_entries_subscribe,
};
pub use config_flow::{
    handle_config_entries_flow, handle_config_entries_flow_progress,
    handle_config_entries_flow_progress_list, handle_config_entries_flow_subscribe,
};
pub use entity_registry::{
    handle_entity_registry_get, handle_entity_registry_list,
    handle_entity_registry_list_for_display, handle_entity_registry_remove,
    handle_entity_registry_update,
};
pub use frontend::{
    handle_frontend_get_icons, handle_frontend_get_themes, handle_frontend_get_translations,
    handle_frontend_subscribe_system_data, handle_frontend_subscribe_user_data, handle_get_panels,
    handle_lovelace_config, handle_lovelace_resources,
};
pub use integration::{
    handle_integration_descriptions, handle_manifest_get, handle_manifest_list,
    handle_sensor_numeric_device_classes,
};
pub use misc::{
    handle_auth_current_user, handle_blueprint_list, handle_entity_source, handle_labs_subscribe,
    handle_logger_log_info, handle_persistent_notification_subscribe, handle_ping,
    handle_recorder_info, handle_render_template, handle_repairs_list_issues,
    handle_supported_features, handle_system_log_list,
};
pub use registry::{
    handle_area_registry_list, handle_category_registry_list, handle_device_registry_list,
    handle_floor_registry_list, handle_label_registry_list,
};
pub use service::{handle_call_service, handle_fire_event};
pub use state::{handle_get_config, handle_get_services, handle_get_states};
pub use subscription::{
    handle_subscribe_entities, handle_subscribe_events, handle_unsubscribe_events,
};
