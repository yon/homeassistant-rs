//! WebSocket message dispatch
//!
//! Routes incoming messages to the appropriate handler.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::warn;

use super::connection::ActiveConnection;
use super::handlers;
use super::types::{IncomingMessage, OutgoingMessage, PongMessage, ResultMessage};

/// Handle an incoming message
pub async fn handle_message(
    conn: &Arc<ActiveConnection>,
    text: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
            return Err(format!("Invalid message format: {}", e));
        }
    };

    match msg {
        IncomingMessage::AreaRegistryList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_area_registry_list(conn, id, tx).await
        }
        IncomingMessage::Auth { .. } => {
            // Already authenticated, ignore
            Ok(())
        }
        IncomingMessage::AuthCurrentUser { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_auth_current_user(conn, id, tx).await
        }
        IncomingMessage::AutomationConfig { id, entity_id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_automation_config(conn, id, &entity_id, tx).await
        }
        IncomingMessage::BlueprintList { id, domain } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
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
            conn.validate_id(id).map_err(|e| e.to_string())?;
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
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_category_registry_list(conn, id, scope, tx).await
        }
        IncomingMessage::ConfigEntriesFlow {
            id,
            handler,
            show_advanced_options,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_config_entries_flow(conn, id, &handler, show_advanced_options, tx)
                .await
        }
        IncomingMessage::ConfigEntriesFlowProgress {
            id,
            flow_id,
            user_input,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            match flow_id {
                Some(ref fid) => {
                    handlers::handle_config_entries_flow_progress(conn, id, fid, user_input, tx)
                        .await
                }
                None => {
                    // List all flows in progress (non-user initiated)
                    handlers::handle_config_entries_flow_progress_list(id, tx).await
                }
            }
        }
        IncomingMessage::ConfigEntriesFlowSubscribe { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_config_entries_flow_subscribe(conn, id, tx).await
        }
        IncomingMessage::ConfigEntriesDelete { id, entry_id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_config_entries_delete(conn, id, &entry_id, tx).await
        }
        IncomingMessage::ApplicationCredentialsConfig { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_application_credentials_config(id, tx).await
        }
        IncomingMessage::ApplicationCredentialsConfigEntry {
            id,
            config_entry_id,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_application_credentials_config_entry(id, &config_entry_id, tx).await
        }
        IncomingMessage::ApplicationCredentialsList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_application_credentials_list(conn, id, tx).await
        }
        IncomingMessage::ApplicationCredentialsCreate {
            id,
            domain,
            client_id,
            client_secret,
            auth_domain,
            name,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
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
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_application_credentials_delete(
                conn,
                id,
                &application_credentials_id,
                tx,
            )
            .await
        }
        IncomingMessage::ConfigEntriesGet {
            id,
            entry_id,
            domain,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
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
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_config_entries_subentries_list(conn, id, &entry_id, tx).await
        }
        IncomingMessage::ConfigEntriesSubscribe { id, type_filter } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_config_entries_subscribe(conn, id, type_filter, tx).await
        }
        IncomingMessage::DeviceRegistryList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_device_registry_list(conn, id, tx).await
        }
        IncomingMessage::EntityRegistryGet { id, entity_id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_entity_registry_get(conn, id, &entity_id, tx).await
        }
        IncomingMessage::EntityRegistryList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_entity_registry_list(conn, id, tx).await
        }
        IncomingMessage::EntityRegistryListForDisplay { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_entity_registry_list_for_display(conn, id, tx).await
        }
        IncomingMessage::EntityRegistryRemove { id, entity_id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
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
            conn.validate_id(id).map_err(|e| e.to_string())?;
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
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_entity_source(conn, id, entity_id, tx).await
        }
        IncomingMessage::FireEvent {
            id,
            event_type,
            event_data,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_fire_event(conn, id, event_type, event_data, tx).await
        }
        IncomingMessage::FloorRegistryList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_floor_registry_list(conn, id, tx).await
        }
        IncomingMessage::FrontendGetThemes { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_frontend_get_themes(conn, id, tx).await
        }
        IncomingMessage::FrontendGetTranslations {
            id,
            language,
            category,
            integration,
            config_flow,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
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
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_frontend_subscribe_system_data(conn, id, key, tx).await
        }
        IncomingMessage::FrontendSubscribeUserData { id, key } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_frontend_subscribe_user_data(conn, id, key, tx).await
        }
        IncomingMessage::GetConfig { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_get_config(conn, id, tx).await
        }
        IncomingMessage::GetPanels { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_get_panels(conn, id, tx).await
        }
        IncomingMessage::GetServices { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_get_services(conn, id, tx).await
        }
        IncomingMessage::GetStates { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_get_states(conn, id, tx).await
        }
        IncomingMessage::LabelRegistryList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_label_registry_list(conn, id, tx).await
        }
        IncomingMessage::LabsSubscribe { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_labs_subscribe(conn, id, tx).await
        }
        IncomingMessage::IntegrationDescriptions { id, integrations } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_integration_descriptions(conn, id, integrations, tx).await
        }
        IncomingMessage::LoggerLogInfo { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_logger_log_info(conn, id, tx).await
        }
        IncomingMessage::LovelaceConfig { id, url_path } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_lovelace_config(conn, id, url_path, tx).await
        }
        IncomingMessage::LovelaceResources { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_lovelace_resources(conn, id, tx).await
        }
        IncomingMessage::ManifestGet { id, integration } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_manifest_get(conn, id, &integration, tx).await
        }
        IncomingMessage::ManifestList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_manifest_list(conn, id, tx).await
        }
        IncomingMessage::PersistentNotificationSubscribe { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_persistent_notification_subscribe(conn, id, tx).await
        }
        IncomingMessage::Ping { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            let pong = OutgoingMessage::Pong(PongMessage {
                id,
                msg_type: "pong",
            });
            tx.send(pong).await.map_err(|e| e.to_string())?;
            Ok(())
        }
        IncomingMessage::RecorderInfo { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_recorder_info(conn, id, tx).await
        }
        IncomingMessage::RenderTemplate {
            id,
            template,
            variables,
            timeout: _,
            report_errors: _,
        } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_render_template(conn, id, &template, variables, tx).await
        }
        IncomingMessage::RepairsListIssues { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_repairs_list_issues(conn, id, tx).await
        }
        IncomingMessage::ScriptConfig { id, entity_id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_script_config(conn, id, &entity_id, tx).await
        }
        IncomingMessage::SubscribeEntities { id, entity_ids } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_subscribe_entities(conn, id, entity_ids, tx).await
        }
        IncomingMessage::SubscribeEvents { id, event_type } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_subscribe_events(conn, id, event_type, tx).await
        }
        IncomingMessage::SupportedFeatures { id, features: _ } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            // Acknowledge supported features (we don't use coalescing yet)
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::Value::Null),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())?;
            Ok(())
        }
        IncomingMessage::SystemLogList { id } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_system_log_list(conn, id, tx).await
        }
        IncomingMessage::UnsubscribeEvents { id, subscription } => {
            conn.validate_id(id).map_err(|e| e.to_string())?;
            handlers::handle_unsubscribe_events(conn, id, subscription, tx).await
        }
    }
}
