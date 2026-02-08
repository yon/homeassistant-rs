//! Config entry handlers

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::error::{WebSocketError, WsResult};
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::{EventMessage, OutgoingMessage};

use super::{send_error, send_result};

/// Convert ConfigEntryState to HA-compatible string
fn config_entry_state_to_string(state: &ha_config_entries::ConfigEntryState) -> &'static str {
    use ha_config_entries::ConfigEntryState;
    match state {
        ConfigEntryState::FailedUnload => "failed_unload",
        ConfigEntryState::Loaded => "loaded",
        ConfigEntryState::MigrationError => "migration_error",
        ConfigEntryState::NotLoaded => "not_loaded",
        ConfigEntryState::SetupError => "setup_error",
        ConfigEntryState::SetupInProgress => "setup_in_progress",
        ConfigEntryState::SetupRetry => "setup_retry",
        ConfigEntryState::UnloadInProgress => "unload_in_progress",
    }
}

/// Convert a ConfigEntry to JSON format expected by frontend
fn config_entry_to_json(entry: &ha_config_entries::ConfigEntry) -> serde_json::Value {
    serde_json::json!({
        "entry_id": entry.entry_id,
        "domain": entry.domain,
        "title": entry.title,
        "source": format!("{:?}", entry.source).to_lowercase(),
        "state": config_entry_state_to_string(&entry.state),
        "supports_options": false,
        "supports_remove_device": false,
        "supports_unload": true,
        "supports_reconfigure": false,
        "pref_disable_new_entities": entry.pref_disable_new_entities,
        "pref_disable_polling": entry.pref_disable_polling,
        "disabled_by": entry.disabled_by.as_ref().map(|d| format!("{:?}", d).to_lowercase()),
        "reason": entry.reason,
        // Required by frontend - empty object for integrations without subentries
        "supported_subentry_types": {},
    })
}

/// Handle config_entries/get command
pub async fn handle_config_entries_get(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entry_id: Option<&str>,
    domain: Option<&str>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Extract data from lock, then release before awaiting channel send
    let result_json = {
        let config_entries = conn.state.config_entries.read().await;

        if let Some(entry_id) = entry_id {
            // Get single entry by ID
            if let Some(entry) = config_entries.get(entry_id) {
                config_entry_to_json(&entry)
            } else {
                // Return a stub entry if not found to prevent frontend errors
                serde_json::json!({
                    "entry_id": entry_id,
                    "domain": "unknown",
                    "title": "Unknown",
                    "source": "user",
                    "state": "not_loaded",
                    "supports_options": false,
                    "supports_remove_device": false,
                    "supports_unload": true,
                    "supports_reconfigure": false,
                    "pref_disable_new_entities": false,
                    "pref_disable_polling": false,
                    "disabled_by": null,
                    "reason": null,
                    "supported_subentry_types": {},
                })
            }
        } else if let Some(domain) = domain {
            // Filter by domain
            let entries: Vec<serde_json::Value> = config_entries
                .iter()
                .filter(|entry| entry.domain == domain)
                .map(|entry| config_entry_to_json(&entry))
                .collect();
            serde_json::Value::Array(entries)
        } else {
            // Return all entries when no filter specified
            let entries: Vec<serde_json::Value> = config_entries
                .iter()
                .map(|entry| config_entry_to_json(&entry))
                .collect();
            serde_json::Value::Array(entries)
        }
    }; // Lock released here

    send_result(id, result_json, tx).await
}

/// Handle config_entries/subscribe command
pub async fn handle_config_entries_subscribe(
    conn: &Arc<ActiveConnection>,
    id: u64,
    type_filter: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Filter entries by integration type if type_filter is provided
    // For now, we only have device integrations (like "demo"), not helpers
    // If type_filter is ["helper"], return empty since we have no helpers
    let is_helper_only_filter = type_filter
        .as_ref()
        .map(|f| f.len() == 1 && f[0] == "helper")
        .unwrap_or(false);

    // Extract data from lock, then release before awaiting channel sends
    let entries: Vec<serde_json::Value> = {
        let config_entries = conn.state.config_entries.read().await;

        // Format entries as {"type": null, "entry": {...}} per native HA
        if is_helper_only_filter {
            // No helper integrations currently
            vec![]
        } else {
            config_entries
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "type": serde_json::Value::Null,
                        "entry": config_entry_to_json(&entry)
                    })
                })
                .collect()
        }
    }; // Lock released here

    // Native HA sends result FIRST, then event
    send_result(id, serde_json::Value::Null, tx).await?;

    // Then send the event with all config entries
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!(entries),
    });
    tx.send(event)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))
}

/// Handle application_credentials/config command
/// Returns list of domains that support application credentials and their config
pub async fn handle_application_credentials_config(
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Get domains that support application credentials from Python
    #[cfg(feature = "python")]
    let (domains, integrations) = {
        use pyo3::prelude::*;

        Python::with_gil(|py| {
            // Get domains from homeassistant.loader.async_get_application_credentials
            // For now, return common OAuth2 integrations
            let domains: Vec<String> =
                match py.import_bound("homeassistant.generated.application_credentials") {
                    Ok(module) => match module.getattr("APPLICATION_CREDENTIALS") {
                        Ok(app_creds) => app_creds.extract::<Vec<String>>().unwrap_or_default(),
                        Err(_) => vec![],
                    },
                    Err(_) => {
                        // Fallback to common OAuth2 integrations
                        vec![
                            "google".to_string(),
                            "spotify".to_string(),
                            "nest".to_string(),
                        ]
                    }
                };

            // Build integrations config (description_placeholders for each domain)
            let mut integrations = serde_json::Map::new();
            for domain in &domains {
                integrations.insert(domain.clone(), serde_json::json!({}));
            }

            (domains, integrations)
        })
    };

    #[cfg(not(feature = "python"))]
    let (domains, integrations) = {
        let domains: Vec<String> = vec![];
        let integrations = serde_json::Map::new();
        (domains, integrations)
    };

    send_result(
        id,
        serde_json::json!({
            "domains": domains,
            "integrations": serde_json::Value::Object(integrations)
        }),
        tx,
    )
    .await
}

/// Handle application_credentials/config_entry command
/// Returns credentials associated with a config entry (usually null for most integrations)
pub async fn handle_application_credentials_config_entry(
    id: u64,
    _entry_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Most integrations don't use application credentials
    // Return null to indicate no credentials
    send_result(id, serde_json::Value::Null, tx).await
}

/// Handle application_credentials/list command
/// Returns list of stored application credentials
pub async fn handle_application_credentials_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Get all credentials from storage
    let credentials: Vec<serde_json::Value> = conn
        .state
        .application_credentials
        .iter()
        .map(|entry| {
            let cred = entry.value();
            let mut obj = serde_json::json!({
                "id": cred.id,
                "domain": cred.domain,
                "client_id": cred.client_id,
                "client_secret": cred.client_secret,
            });
            // Include optional fields if present
            if let Some(ref name) = cred.name {
                obj["name"] = serde_json::Value::String(name.clone());
            }
            if let Some(ref auth_domain) = cred.auth_domain {
                obj["auth_domain"] = serde_json::Value::String(auth_domain.clone());
            }
            obj
        })
        .collect();

    send_result(id, serde_json::json!(credentials), tx).await
}

/// Handle application_credentials/create command
/// Creates a new application credential (OAuth2 client credentials)
// Handler requires shared connection, message id, credential fields, and response channel
#[allow(clippy::too_many_arguments)]
pub async fn handle_application_credentials_create(
    conn: &Arc<ActiveConnection>,
    id: u64,
    domain: &str,
    client_id: &str,
    client_secret: &str,
    auth_domain: Option<&str>,
    name: Option<&str>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    use crate::ApplicationCredential;

    // Strip whitespace from credentials (HA does this)
    let client_id = client_id.trim();
    let client_secret = client_secret.trim();

    // Generate credential ID (matches HA format: domain_client_id with underscores)
    let credential_id = format!("{}_{}", domain, client_id.replace('-', "_"));

    info!(
        "Creating application credential for domain: {}, client_id: {}",
        domain, client_id
    );

    // Create credential object
    let credential = ApplicationCredential {
        id: credential_id.clone(),
        domain: domain.to_string(),
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
        auth_domain: auth_domain.map(|s| s.to_string()),
        name: name.map(|s| s.to_string()),
    };

    // Store the credential
    conn.state
        .application_credentials
        .insert(credential_id.clone(), credential);

    // Build response with optional fields
    let mut result_obj = serde_json::json!({
        "id": credential_id,
        "domain": domain,
        "client_id": client_id,
        "client_secret": client_secret,
    });
    if let Some(n) = name {
        result_obj["name"] = serde_json::Value::String(n.to_string());
    }
    if let Some(ad) = auth_domain {
        result_obj["auth_domain"] = serde_json::Value::String(ad.to_string());
    }

    send_result(id, result_obj, tx).await
}

/// Handle application_credentials/delete command
/// Deletes an application credential
pub async fn handle_application_credentials_delete(
    conn: &Arc<ActiveConnection>,
    id: u64,
    credential_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    info!("Deleting application credential: {}", credential_id);

    // Try to remove the credential
    if conn
        .state
        .application_credentials
        .remove(credential_id)
        .is_some()
    {
        send_result(id, serde_json::Value::Null, tx).await
    } else {
        send_error(
            id,
            "not_found",
            format!(
                "Unable to find application_credentials_id {}",
                credential_id
            ),
            tx,
        )
        .await
    }
}

/// Handle config_entries/delete command
pub async fn handle_config_entries_delete(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entry_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    info!("Deleting config entry: {}", entry_id);

    // Remove the config entry, then release lock before sending response
    let remove_result = {
        let config_entries = conn.state.config_entries.write().await;
        config_entries.remove(entry_id).await
    }; // Write lock released here

    match remove_result {
        Ok(_entry) => {
            info!("Config entry {} deleted successfully", entry_id);
            send_result(
                id,
                serde_json::json!({
                    "require_restart": false
                }),
                tx,
            )
            .await
        }
        Err(e) => {
            warn!("Failed to delete config entry {}: {}", entry_id, e);
            send_error(
                id,
                "not_found",
                format!("Config entry {} not found", entry_id),
                tx,
            )
            .await
        }
    }
}

/// Handle config_entries/subentries/list command
///
/// Returns list of subentries for a config entry. Most integrations don't have subentries,
/// so this returns an empty array.
pub async fn handle_config_entries_subentries_list(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _entry_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    // Most integrations don't have subentries, return empty array
    // Per HA format: [{"subentry_id": "...", "subentry_type": "...", "title": "...", "unique_id": "..."}]
    send_result(id, serde_json::json!([]), tx).await
}
