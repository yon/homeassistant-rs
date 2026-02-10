//! WebSocket message dispatch
//!
//! Routes incoming messages to the appropriate handler.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::warn;

use crate::error::WsResult;

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
            // Extract id from raw JSON so we can send an error response.
            // Python HA sends "unknown_command" for unrecognized message types.
            let id = serde_json::from_str::<serde_json::Value>(text)
                .ok()
                .and_then(|json| {
                    if let Some(msg_type) = json.get("type").and_then(|t| t.as_str()) {
                        warn!("Unhandled WebSocket message type: {}", msg_type);
                    }
                    json.get("id").and_then(|v| v.as_u64())
                })
                .unwrap_or(0);
            return handlers::send_error(
                id,
                "unknown_command",
                format!("Unknown command: {e}"),
                tx,
            )
            .await;
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
        IncomingMessage::FrontendGetUserData { id, key } => {
            handlers::handle_frontend_get_user_data(id, key, tx).await
        }
        IncomingMessage::FrontendSetUserData { id, key, value } => {
            handlers::handle_frontend_set_user_data(id, &key, value, tx).await
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

#[cfg(test)]
mod tests {
    use super::*;

    use ha_components::system_log::SystemLog;
    use ha_config::CoreConfig;
    use ha_config_entries::ConfigEntries;
    use ha_event_bus::EventBus;
    use ha_registries::{Registries, Storage};
    use ha_service_registry::ServiceRegistry;
    use ha_state_store::StateStore;
    use tokio::sync::RwLock;

    use crate::websocket::types::ResultMessage;
    use crate::{auth, new_application_credentials_store, persistent_notification, AppState};

    fn make_test_conn() -> Arc<ActiveConnection> {
        let event_bus = Arc::new(EventBus::new());
        let state_machine = Arc::new(StateStore::new(event_bus.clone()));
        let service_registry = Arc::new(ServiceRegistry::new());
        let temp_dir = std::env::temp_dir().join("ha-api-dispatch-test");
        let registries = Arc::new(Registries::new(&temp_dir));
        let storage = Arc::new(Storage::new(&temp_dir));
        let config_entries = Arc::new(RwLock::new(ConfigEntries::new(storage)));
        let notifications = persistent_notification::create_manager();
        let system_log = Arc::new(SystemLog::with_defaults());
        let state = AppState {
            application_credentials: new_application_credentials_store(),
            auth_state: auth::AuthState::new_onboarded(),
            components: Arc::new(vec![]),
            components_path: None,
            config: Arc::new(CoreConfig::default()),
            config_entries,
            config_flow_handler: None,
            event_bus,
            events_cache: None,
            frontend_config: None,
            notifications,
            registries,
            service_registry,
            services_cache: None,
            state_machine,
            system_log,
        };
        Arc::new(ActiveConnection::new(state, None))
    }

    /// Extract ResultMessage from OutgoingMessage, panicking if it's a different variant.
    fn expect_result(msg: OutgoingMessage) -> ResultMessage {
        if let OutgoingMessage::Result(rm) = msg {
            rm
        } else {
            panic!("Expected Result message, got {:?}", msg)
        }
    }

    #[tokio::test]
    async fn unknown_message_type_returns_error_response() {
        let conn = make_test_conn();
        let (tx, mut rx) = mpsc::channel(16);

        let text = r#"{"id": 99, "type": "totally/unknown"}"#;
        let result = handle_message(&conn, text, &tx).await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);

        let rm = expect_result(rx.try_recv().expect("Should have received a response"));
        assert_eq!(rm.id, 99);
        assert!(!rm.success);
        assert_eq!(
            rm.error.expect("Should have error info").code,
            "unknown_command"
        );
    }

    #[tokio::test]
    async fn unknown_message_preserves_id() {
        let conn = make_test_conn();
        let (tx, mut rx) = mpsc::channel(16);

        let text = r#"{"id": 42, "type": "nonexistent/command"}"#;
        let result = handle_message(&conn, text, &tx).await;
        assert!(result.is_ok());

        let rm = expect_result(rx.try_recv().expect("Should have received a response"));
        assert_eq!(rm.id, 42, "Response ID should match request ID");
    }

    #[tokio::test]
    async fn frontend_get_user_data_returns_result() {
        let conn = make_test_conn();
        let (tx, mut rx) = mpsc::channel(16);

        let text = r#"{"id": 10, "type": "frontend/get_user_data"}"#;
        let result = handle_message(&conn, text, &tx).await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);

        let rm = expect_result(rx.try_recv().expect("Should have received a response"));
        assert_eq!(rm.id, 10);
        assert!(rm.success, "Should be a success response");
        let value = rm.result.expect("Should have result data");
        assert!(
            value.get("value").is_some(),
            "Result should contain 'value' key, got: {value}"
        );
    }

    #[tokio::test]
    async fn frontend_get_user_data_with_key_returns_null_value() {
        let conn = make_test_conn();
        let (tx, mut rx) = mpsc::channel(16);

        let text = r#"{"id": 11, "type": "frontend/get_user_data", "key": "sidebar"}"#;
        let result = handle_message(&conn, text, &tx).await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);

        let rm = expect_result(rx.try_recv().expect("Should have received a response"));
        assert_eq!(rm.id, 11);
        assert!(rm.success);
        let value = rm.result.expect("Should have result data");
        assert_eq!(
            value.get("value"),
            Some(&serde_json::Value::Null),
            "Should return null for unknown key"
        );
    }

    #[tokio::test]
    async fn frontend_set_user_data_returns_success() {
        let conn = make_test_conn();
        let (tx, mut rx) = mpsc::channel(16);

        let text = r#"{"id": 12, "type": "frontend/set_user_data", "key": "sidebar", "value": {"order": ["config"]}}"#;
        let result = handle_message(&conn, text, &tx).await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);

        let rm = expect_result(rx.try_recv().expect("Should have received a response"));
        assert_eq!(rm.id, 12);
        assert!(rm.success, "Should be a success response");
    }

    #[tokio::test]
    async fn config_entries_subscribe_filters_by_type() {
        let conn = make_test_conn();
        let (tx, mut rx) = mpsc::channel(16);

        // Add a config entry with domain "demo" (integration_type defaults to "device")
        {
            let entries = conn.state.config_entries.write().await;
            let mut entry = ha_config_entries::ConfigEntry::new("demo", "Demo Integration");
            entry.entry_id = "test_entry_1".to_string();
            entry.state = ha_config_entries::ConfigEntryState::Loaded;
            entries.add(entry).await.unwrap();
        }

        // Subscribe with helper filter — should return 0 entries (demo is "device", not "helper")
        let text = r#"{"id": 20, "type": "config_entries/subscribe", "type_filter": ["helper"]}"#;
        let result = handle_message(&conn, text, &tx).await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);

        // First message: result with null
        let rm = expect_result(rx.recv().await.expect("Should have received result"));
        assert_eq!(rm.id, 20);
        assert!(rm.success);

        // Second message: event with filtered entries
        let event_msg = rx.recv().await.expect("Should have received event");
        if let OutgoingMessage::Event(event) = event_msg {
            let entries = event.event.as_array().expect("Event should be an array");
            assert!(
                entries.is_empty(),
                "Helper filter should return 0 entries for device integration, got {}",
                entries.len()
            );
        } else {
            panic!("Expected Event message, got {:?}", event_msg);
        }
    }

    #[tokio::test]
    async fn malformed_json_returns_error_response() {
        let conn = make_test_conn();
        let (tx, mut rx) = mpsc::channel(16);

        let text = "not json at all";
        let result = handle_message(&conn, text, &tx).await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);

        let rm = expect_result(rx.try_recv().expect("Should have received a response"));
        assert_eq!(rm.id, 0, "Malformed JSON should use id 0");
        assert!(!rm.success);
        assert_eq!(
            rm.error.expect("Should have error info").code,
            "unknown_command"
        );
    }
}
