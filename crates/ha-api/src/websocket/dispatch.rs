//! WebSocket message dispatch
//!
//! Routes incoming messages to the appropriate handler.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::warn;

use crate::error::{WebSocketError, WsResult};

use super::connection::ActiveConnection;
use super::handlers;
use super::types::{IncomingMessage, OutgoingMessage};

/// Handle an incoming message
pub async fn handle_message(
    conn: &Arc<ActiveConnection>,
    text: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Parse the message
    let msg: IncomingMessage = match serde_json::from_str(text) {
        Ok(msg) => msg,
        Err(e) => {
            // Log unhandled message types for debugging
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
                if let Some(msg_type) = json.get("type").and_then(|t| t.as_str()) {
                    warn!("Unhandled WebSocket message type: {}", msg_type);
                }
            }
            return Err(WebSocketError::InvalidMessage(e.to_string()));
        }
    };

    // Validate message ID once (all messages except Auth carry an ID)
    if let Some(id) = msg.id() {
        conn.validate_id(id)?;
    }

    match msg {
        IncomingMessage::ApplicationCredentialsConfig { id } => {
            handlers::handle_application_credentials_config(id, tx).await
        }
        IncomingMessage::ApplicationCredentialsConfigEntry {
            id,
            config_entry_id,
        } => handlers::handle_application_credentials_config_entry(id, &config_entry_id, tx).await,
        IncomingMessage::ApplicationCredentialsCreate {
            id,
            domain,
            client_id,
            client_secret,
            auth_domain,
            name,
        } => {
            handlers::handle_application_credentials_create(
                conn,
                id,
                &domain,
                &client_id,
                &client_secret,
                auth_domain.as_deref(),
                name.as_deref(),
                tx,
            )
            .await
        }
        IncomingMessage::ApplicationCredentialsDelete {
            id,
            application_credentials_id,
        } => {
            handlers::handle_application_credentials_delete(
                conn,
                id,
                &application_credentials_id,
                tx,
            )
            .await
        }
        IncomingMessage::ApplicationCredentialsList { id } => {
            handlers::handle_application_credentials_list(conn, id, tx).await
        }
        IncomingMessage::AreaRegistryList { id } => {
            handlers::handle_area_registry_list(conn, id, tx).await
        }
        IncomingMessage::Auth { .. } => Ok(()),
        IncomingMessage::AuthCurrentUser { id } => {
            handlers::handle_auth_current_user(conn, id, tx).await
        }
        IncomingMessage::AutomationConfig { id, entity_id } => {
            handlers::handle_automation_config(conn, id, &entity_id, tx).await
        }
        IncomingMessage::BlueprintList { id, domain } => {
            handlers::handle_blueprint_list(conn, id, &domain, tx).await
        }
        IncomingMessage::CallService {
            id,
            domain,
            service,
            target,
            service_data,
            return_response,
        } => {
            handlers::handle_call_service(
                conn,
                id,
                domain,
                service,
                target,
                service_data,
                return_response,
                tx,
            )
            .await
        }
        IncomingMessage::CategoryRegistryList { id, scope } => {
            handlers::handle_category_registry_list(conn, id, scope, tx).await
        }
        IncomingMessage::ConfigEntriesDelete { id, entry_id } => {
            handlers::handle_config_entries_delete(conn, id, &entry_id, tx).await
        }
        IncomingMessage::ConfigEntriesFlow {
            id,
            handler,
            show_advanced_options,
        } => {
            handlers::handle_config_entries_flow(conn, id, &handler, show_advanced_options, tx)
                .await
        }
        IncomingMessage::ConfigEntriesFlowProgress {
            id,
            flow_id,
            user_input,
        } => match flow_id {
            Some(ref fid) => {
                handlers::handle_config_entries_flow_progress(conn, id, fid, user_input, tx).await
            }
            None => handlers::handle_config_entries_flow_progress_list(id, tx).await,
        },
        IncomingMessage::ConfigEntriesFlowSubscribe { id } => {
            handlers::handle_config_entries_flow_subscribe(conn, id, tx).await
        }
        IncomingMessage::ConfigEntriesGet {
            id,
            entry_id,
            domain,
        } => {
            handlers::handle_config_entries_get(
                conn,
                id,
                entry_id.as_deref(),
                domain.as_deref(),
                tx,
            )
            .await
        }
        IncomingMessage::ConfigEntriesSubentriesList { id, entry_id } => {
            handlers::handle_config_entries_subentries_list(conn, id, &entry_id, tx).await
        }
        IncomingMessage::ConfigEntriesSubscribe { id, type_filter } => {
            handlers::handle_config_entries_subscribe(conn, id, type_filter, tx).await
        }
        IncomingMessage::DeviceRegistryList { id } => {
            handlers::handle_device_registry_list(conn, id, tx).await
        }
        IncomingMessage::EntityRegistryGet { id, entity_id } => {
            handlers::handle_entity_registry_get(conn, id, &entity_id, tx).await
        }
        IncomingMessage::EntityRegistryList { id } => {
            handlers::handle_entity_registry_list(conn, id, tx).await
        }
        IncomingMessage::EntityRegistryListForDisplay { id } => {
            handlers::handle_entity_registry_list_for_display(conn, id, tx).await
        }
        IncomingMessage::EntityRegistryRemove { id, entity_id } => {
            handlers::handle_entity_registry_remove(conn, id, &entity_id, tx).await
        }
        IncomingMessage::EntityRegistryUpdate {
            id,
            entity_id,
            name,
            icon,
            area_id,
            disabled_by,
            hidden_by,
            new_entity_id,
            aliases,
            labels,
        } => {
            handlers::handle_entity_registry_update(
                conn,
                id,
                &entity_id,
                name,
                icon,
                area_id,
                disabled_by,
                hidden_by,
                new_entity_id,
                aliases,
                labels,
                tx,
            )
            .await
        }
        IncomingMessage::EntitySource { id, entity_id } => {
            handlers::handle_entity_source(conn, id, entity_id, tx).await
        }
        IncomingMessage::FireEvent {
            id,
            event_type,
            event_data,
        } => handlers::handle_fire_event(conn, id, event_type, event_data, tx).await,
        IncomingMessage::FloorRegistryList { id } => {
            handlers::handle_floor_registry_list(conn, id, tx).await
        }
        IncomingMessage::FrontendGetIcons {
            id,
            category,
            integration,
        } => handlers::handle_frontend_get_icons(conn, id, &category, integration, tx).await,
        IncomingMessage::FrontendGetThemes { id } => {
            handlers::handle_frontend_get_themes(conn, id, tx).await
        }
        IncomingMessage::FrontendGetTranslations {
            id,
            language,
            category,
            integration,
            config_flow,
        } => {
            handlers::handle_frontend_get_translations(
                conn,
                id,
                language,
                category,
                integration,
                config_flow,
                tx,
            )
            .await
        }
        IncomingMessage::FrontendSubscribeSystemData { id, key } => {
            handlers::handle_frontend_subscribe_system_data(conn, id, key, tx).await
        }
        IncomingMessage::FrontendSubscribeUserData { id, key } => {
            handlers::handle_frontend_subscribe_user_data(conn, id, key, tx).await
        }
        IncomingMessage::GetConfig { id } => handlers::handle_get_config(conn, id, tx).await,
        IncomingMessage::GetPanels { id } => handlers::handle_get_panels(conn, id, tx).await,
        IncomingMessage::GetServices { id } => handlers::handle_get_services(conn, id, tx).await,
        IncomingMessage::GetStates { id } => handlers::handle_get_states(conn, id, tx).await,
        IncomingMessage::IntegrationDescriptions { id, integrations } => {
            handlers::handle_integration_descriptions(conn, id, integrations, tx).await
        }
        IncomingMessage::LabelRegistryList { id } => {
            handlers::handle_label_registry_list(conn, id, tx).await
        }
        IncomingMessage::LabsSubscribe { id } => {
            handlers::handle_labs_subscribe(conn, id, tx).await
        }
        IncomingMessage::LoggerLogInfo { id } => {
            handlers::handle_logger_log_info(conn, id, tx).await
        }
        IncomingMessage::LovelaceConfig { id, url_path } => {
            handlers::handle_lovelace_config(conn, id, url_path, tx).await
        }
        IncomingMessage::LovelaceResources { id } => {
            handlers::handle_lovelace_resources(conn, id, tx).await
        }
        IncomingMessage::ManifestGet { id, integration } => {
            handlers::handle_manifest_get(conn, id, &integration, tx).await
        }
        IncomingMessage::ManifestList { id } => handlers::handle_manifest_list(conn, id, tx).await,
        IncomingMessage::PersistentNotificationSubscribe { id } => {
            handlers::handle_persistent_notification_subscribe(conn, id, tx).await
        }
        IncomingMessage::Ping { id } => handlers::handle_ping(id, tx).await,
        IncomingMessage::RecorderInfo { id } => handlers::handle_recorder_info(conn, id, tx).await,
        IncomingMessage::RenderTemplate {
            id,
            template,
            variables,
            ..
        } => handlers::handle_render_template(conn, id, &template, variables, tx).await,
        IncomingMessage::RepairsListIssues { id } => {
            handlers::handle_repairs_list_issues(conn, id, tx).await
        }
        IncomingMessage::ScriptConfig { id, entity_id } => {
            handlers::handle_script_config(conn, id, &entity_id, tx).await
        }
        IncomingMessage::SensorNumericDeviceClasses { id } => {
            handlers::handle_sensor_numeric_device_classes(conn, id, tx).await
        }
        IncomingMessage::SubscribeEntities { id, entity_ids } => {
            handlers::handle_subscribe_entities(conn, id, entity_ids, tx).await
        }
        IncomingMessage::SubscribeEvents { id, event_type } => {
            handlers::handle_subscribe_events(conn, id, event_type, tx).await
        }
        IncomingMessage::SupportedFeatures { id, .. } => {
            handlers::handle_supported_features(id, tx).await
        }
        IncomingMessage::SystemLogList { id } => {
            handlers::handle_system_log_list(conn, id, tx).await
        }
        IncomingMessage::UnsubscribeEvents { id, subscription } => {
            handlers::handle_unsubscribe_events(conn, id, subscription, tx).await
        }
    }
}
