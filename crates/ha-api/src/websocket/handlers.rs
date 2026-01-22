//! WebSocket command handlers
//!
//! Individual handlers for each WebSocket command type.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use crate::translations;
use crate::AppState;

use super::connection::ActiveConnection;
use super::types::{ErrorInfo, EventMessage, OutgoingMessage, ResultMessage, ServiceTarget};

// =============================================================================
// State Handlers
// =============================================================================

/// Handle get_states command
pub async fn handle_get_states(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle get_config command
pub async fn handle_get_config(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle get_services command
pub async fn handle_get_services(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
    tx.send(result).await.map_err(|e| e.to_string())
}

// =============================================================================
// Event Subscription Handlers
// =============================================================================

/// Handle subscribe_events command
pub async fn handle_subscribe_events(
    conn: &Arc<ActiveConnection>,
    id: u64,
    event_type: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Create cancellation channel
    let (cancel_tx, mut cancel_rx) = broadcast::channel::<()>(1);

    // Store subscription
    {
        let mut subs = conn.subscriptions.write().await;
        subs.insert(id, cancel_tx);
    }

    // Subscribe to events
    let event_type_filter = event_type.clone();
    let tx_clone = tx.clone();
    let sub_id = id;

    // Get a receiver from the event bus (subscribe to all events)
    let mut event_rx = conn.state.event_bus.subscribe_all();

    // Spawn task to forward events
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_rx.recv() => {
                    debug!("Subscription {} cancelled", sub_id);
                    break;
                }
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            // Filter by event type if specified
                            if let Some(ref filter) = event_type_filter {
                                if event.event_type.as_str() != filter {
                                    continue;
                                }
                            }

                            // Send event to client
                            let event_msg = OutgoingMessage::Event(EventMessage {
                                id: sub_id,
                                msg_type: "event",
                                event: serde_json::json!({
                                    "event_type": event.event_type,
                                    "data": event.data,
                                    "origin": "LOCAL",
                                    "time_fired": event.time_fired.to_rfc3339(),
                                    "context": {
                                        "id": event.context.id.to_string(),
                                        "parent_id": event.context.parent_id,
                                        "user_id": event.context.user_id,
                                    }
                                }),
                            });
                            if tx_clone.send(event_msg).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            // Missed some events, continue
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }
        }
    });

    // Send success response - explicitly include "result": null to match Python HA
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle unsubscribe_events command
pub async fn handle_unsubscribe_events(
    conn: &Arc<ActiveConnection>,
    id: u64,
    subscription: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let mut subs = conn.subscriptions.write().await;
    if let Some(cancel_tx) = subs.remove(&subscription) {
        let _ = cancel_tx.send(());
    }
    drop(subs);

    // Explicitly include "result": null to match Python HA
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle subscribe_entities command
pub async fn handle_subscribe_entities(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_ids: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Create cancellation channel
    let (cancel_tx, mut cancel_rx) = broadcast::channel::<()>(1);

    // Store subscription
    {
        let mut subs = conn.subscriptions.write().await;
        subs.insert(id, cancel_tx);
    }

    // Get initial states for the requested entities
    let states = conn.state.state_machine.all();
    let filtered_states: Vec<&ha_core::State> = if let Some(ref ids) = entity_ids {
        states
            .iter()
            .filter(|s| ids.contains(&s.entity_id.to_string()))
            .collect()
    } else {
        states.iter().collect()
    };

    // Build initial state response
    let mut additions = serde_json::Map::new();
    for state in filtered_states {
        additions.insert(
            state.entity_id.to_string(),
            serde_json::json!({
                "s": state.state,
                "a": state.attributes,
                "c": state.context.id.to_string(),
                "lc": state.last_changed.timestamp_millis() as f64 / 1000.0,
                "lu": state.last_updated.timestamp_millis() as f64 / 1000.0,
            }),
        );
    }

    // Send initial state event
    let initial_event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({
            "a": additions,
        }),
    });
    tx.send(initial_event).await.map_err(|e| e.to_string())?;

    // Subscribe to state changes
    let entity_ids_filter = entity_ids.clone();
    let tx_clone = tx.clone();
    let sub_id = id;

    let mut event_rx = conn.state.event_bus.subscribe_all();

    // Spawn task to forward state change events
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_rx.recv() => {
                    debug!("Entity subscription {} cancelled", sub_id);
                    break;
                }
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            // Only forward state_changed events
                            if event.event_type.as_str() != "state_changed" {
                                continue;
                            }

                            // Extract entity_id from event data
                            if let Some(entity_id) = event.data.get("entity_id").and_then(|v| v.as_str()) {
                                // Filter by entity_ids if specified
                                if let Some(ref ids) = entity_ids_filter {
                                    if !ids.contains(&entity_id.to_string()) {
                                        continue;
                                    }
                                }

                                // Build change event
                                if let Some(new_state) = event.data.get("new_state") {
                                    let mut changes = serde_json::Map::new();
                                    changes.insert(
                                        entity_id.to_string(),
                                        serde_json::json!({
                                            "+": {
                                                "s": new_state.get("state"),
                                                "a": new_state.get("attributes"),
                                                "c": new_state.get("context").and_then(|c| c.get("id")),
                                                "lc": new_state.get("last_changed"),
                                                "lu": new_state.get("last_updated"),
                                            }
                                        }),
                                    );

                                    let change_event = OutgoingMessage::Event(EventMessage {
                                        id: sub_id,
                                        msg_type: "event",
                                        event: serde_json::json!({
                                            "c": changes,
                                        }),
                                    });
                                    if tx_clone.send(change_event).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }
        }
    });

    // Send success response
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

// =============================================================================
// Service Handlers
// =============================================================================

/// Handle call_service command
#[allow(clippy::too_many_arguments)]
pub async fn handle_call_service(
    conn: &Arc<ActiveConnection>,
    id: u64,
    domain: String,
    service: String,
    target: Option<ServiceTarget>,
    service_data: Option<serde_json::Value>,
    return_response: bool,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Merge target into service_data
    let mut data = service_data.unwrap_or(serde_json::json!({}));
    if let Some(target) = target {
        if let Some(entity_ids) = target.entity_id {
            data["entity_id"] = serde_json::json!(entity_ids.to_vec());
        }
        if let Some(device_ids) = target.device_id {
            data["device_id"] = serde_json::json!(device_ids);
        }
        if let Some(area_ids) = target.area_id {
            data["area_id"] = serde_json::json!(area_ids);
        }
    }

    // Create a new context with user_id for this service call
    let context = conn.new_context();

    match conn
        .state
        .service_registry
        .call(&domain, &service, data, context.clone(), return_response)
        .await
    {
        Ok(response) => {
            let mut result_data = serde_json::json!({
                "context": {
                    "id": context.id.to_string(),
                    "parent_id": context.parent_id,
                    "user_id": context.user_id,
                }
            });

            if return_response {
                if let Some(resp) = response {
                    result_data["response"] = resp;
                }
            }

            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(result_data),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
        Err(e) => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: "service_error".to_string(),
                    message: e.to_string(),
                }),
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

/// Handle fire_event command
pub async fn handle_fire_event(
    conn: &Arc<ActiveConnection>,
    id: u64,
    event_type: String,
    event_data: Option<serde_json::Value>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let data = event_data.unwrap_or(serde_json::json!({}));
    // Create a new context with user_id for this event
    let context = conn.new_context();

    let event = ha_core::Event::new(event_type, data, context.clone());
    conn.state.event_bus.fire(event);

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "context": {
                "id": context.id.to_string(),
                "parent_id": context.parent_id,
                "user_id": context.user_id,
            }
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

// =============================================================================
// Entity Registry Handlers
// =============================================================================

/// Handle config/entity_registry/get command
pub async fn handle_entity_registry_get(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    match conn.state.registries.entities.get(entity_id) {
        Some(entry) => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(entity_entry_to_json(&entry)),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
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
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

/// Handle config/entity_registry/list command
pub async fn handle_entity_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let entries: Vec<serde_json::Value> = conn
        .state
        .registries
        .entities
        .iter()
        .map(|entry| entity_entry_to_json(&entry))
        .collect();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(entries)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/entity_registry/remove command
pub async fn handle_entity_registry_remove(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
            tx.send(result).await.map_err(|e| e.to_string())
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
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

/// Handle config/entity_registry/update command
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
    aliases: Option<Vec<String>>,
    labels: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
        return tx.send(result).await.map_err(|e| e.to_string());
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
                    "user" => Some(ha_registries::DisabledBy::User),
                    "integration" => Some(ha_registries::DisabledBy::Integration),
                    "config_entry" => Some(ha_registries::DisabledBy::ConfigEntry),
                    "device" => Some(ha_registries::DisabledBy::Device),
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
            // TODO: Implement entity_id rename - requires updating the entity_id field
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
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/entity_registry/list_for_display command
pub async fn handle_entity_registry_list_for_display(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return a simplified entity list for display purposes
    // Uses short keys matching HA's entity_registry.py display format
    let entries: Vec<serde_json::Value> = conn
        .state
        .registries
        .entities
        .iter()
        .map(|entry| {
            let mut obj = serde_json::json!({
                "ei": entry.entity_id,
                "di": entry.device_id,
                "pl": entry.platform,
                "tk": entry.translation_key,
                // "en" should be name OR original_name (matching HA's logic)
                "en": entry.name.clone().or_else(|| entry.original_name.clone()),
                "ic": entry.icon,
                "ai": entry.area_id,
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
            if entry.has_entity_name {
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
    tx.send(result).await.map_err(|e| e.to_string())
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
            ha_registries::DisabledBy::User => "user",
            ha_registries::DisabledBy::Integration => "integration",
            ha_registries::DisabledBy::ConfigEntry => "config_entry",
            ha_registries::DisabledBy::Device => "device",
        }),
        "hidden_by": entry.hidden_by.map(|h| match h {
            ha_registries::HiddenBy::Integration => "integration",
            ha_registries::HiddenBy::User => "user",
        }),
        "entity_category": entry.entity_category.map(|c| match c {
            ha_registries::EntityCategory::Config => "config",
            ha_registries::EntityCategory::Diagnostic => "diagnostic",
        }),
        "has_entity_name": entry.has_entity_name,
        "aliases": entry.aliases,
        "labels": entry.labels,
        "categories": entry.categories,
        "capabilities": entry.capabilities,
        "device_class": entry.device_class,
        "original_device_class": entry.original_device_class,
        "translation_key": entry.translation_key,
    })
}

// =============================================================================
// Device Registry Handlers
// =============================================================================

/// Handle config/device_registry/list command
pub async fn handle_device_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
                    ha_registries::DisabledBy::User => "user",
                    ha_registries::DisabledBy::Integration => "integration",
                    ha_registries::DisabledBy::ConfigEntry => "config_entry",
                    ha_registries::DisabledBy::Device => "device",
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
    tx.send(result).await.map_err(|e| e.to_string())
}

// =============================================================================
// Area/Floor/Label Registry Handlers
// =============================================================================

/// Handle config/area_registry/list command
pub async fn handle_area_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/floor_registry/list command
pub async fn handle_floor_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/label_registry/list command
pub async fn handle_label_registry_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config/category_registry/list command
pub async fn handle_category_registry_list(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _scope: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty categories list
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(vec![])),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

// =============================================================================
// Frontend Handlers
// =============================================================================

/// Handle frontend/get_themes command
pub async fn handle_frontend_get_themes(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return default themes structure
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "themes": {},
            "default_theme": "default",
            "default_dark_theme": null,
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle frontend/get_translations command
#[allow(clippy::too_many_arguments)]
pub async fn handle_frontend_get_translations(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    language: Option<String>,
    category: Option<String>,
    integration: Option<Vec<String>>,
    config_flow: Option<bool>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let lang = language.as_deref().unwrap_or("en");
    let cat = category.as_deref();
    let is_config_flow = config_flow.unwrap_or(false);

    let translations =
        translations::get_translations(cat, integration.as_deref(), is_config_flow, lang);

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(translations),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle frontend/subscribe_user_data command
pub async fn handle_frontend_subscribe_user_data(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    key: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Send initial user data event
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({
            "key": key.unwrap_or_default(),
            "data": {}
        }),
    });
    tx.send(event).await.map_err(|e| e.to_string())?;

    // Send success response
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle frontend/subscribe_system_data command
pub async fn handle_frontend_subscribe_system_data(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    key: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Send initial system data event
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({
            "key": key.unwrap_or_default(),
            "data": {}
        }),
    });
    tx.send(event).await.map_err(|e| e.to_string())?;

    // Send success response
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle get_panels command
pub async fn handle_get_panels(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return default panels structure
    let panels = serde_json::json!({
        "lovelace": {
            "component_name": "lovelace",
            "icon": "mdi:view-dashboard",
            "title": null,
            "config": {"mode": "storage"},
            "url_path": "lovelace",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "developer-tools": {
            "component_name": "developer_tools",
            "icon": "mdi:hammer",
            "title": null,
            "config": null,
            "url_path": "developer-tools",
            "require_admin": true,
            "config_panel_domain": null,
        },
        "config": {
            "component_name": "config",
            "icon": "mdi:cog",
            "title": null,
            "config": null,
            "url_path": "config",
            "require_admin": true,
            "config_panel_domain": null,
        },
        "history": {
            "component_name": "history",
            "icon": "mdi:chart-box",
            "title": null,
            "config": null,
            "url_path": "history",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "logbook": {
            "component_name": "logbook",
            "icon": "mdi:format-list-bulleted-type",
            "title": null,
            "config": null,
            "url_path": "logbook",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "map": {
            "component_name": "map",
            "icon": "mdi:tooltip-account",
            "title": null,
            "config": null,
            "url_path": "map",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "energy": {
            "component_name": "energy",
            "icon": "mdi:lightning-bolt",
            "title": null,
            "config": null,
            "url_path": "energy",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "media-browser": {
            "component_name": "media_browser",
            "icon": "mdi:play-box-multiple",
            "title": null,
            "config": null,
            "url_path": "media-browser",
            "require_admin": false,
            "config_panel_domain": null,
        },
        "todo": {
            "component_name": "todo",
            "icon": "mdi:clipboard-list",
            "title": null,
            "config": null,
            "url_path": "todo",
            "require_admin": false,
            "config_panel_domain": null,
        },
    });

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(panels),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle lovelace/config command
pub async fn handle_lovelace_config(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _url_path: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(config),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle lovelace/resources command
pub async fn handle_lovelace_resources(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty resources list
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(vec![])),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

// =============================================================================
// Automation/Script Handlers
// =============================================================================

/// Handle automation/config command - returns the automation configuration
pub async fn handle_automation_config(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Verify entity_id starts with "automation."
    if !entity_id.starts_with("automation.") {
        let result = OutgoingMessage::Result(ResultMessage {
            id,
            msg_type: "result",
            success: false,
            result: None,
            error: Some(ErrorInfo {
                code: "not_found".to_string(),
                message: "Entity not found".to_string(),
            }),
        });
        return tx.send(result).await.map_err(|e| e.to_string());
    }

    // Look up the automation entity state
    match conn.state.state_machine.get(entity_id) {
        Some(state) => {
            // The automation config is stored in the entity's attributes
            // Extract relevant config fields from attributes
            let config = serde_json::json!({
                "id": state.attributes.get("id").cloned().unwrap_or(serde_json::json!(entity_id)),
                "alias": state.attributes.get("friendly_name").cloned().unwrap_or(serde_json::Value::Null),
                "description": state.attributes.get("description").cloned().unwrap_or(serde_json::Value::Null),
                "trigger": state.attributes.get("trigger").cloned().unwrap_or(serde_json::json!([])),
                "condition": state.attributes.get("condition").cloned().unwrap_or(serde_json::json!([])),
                "action": state.attributes.get("action").cloned().unwrap_or(serde_json::json!([])),
                "mode": state.attributes.get("mode").cloned().unwrap_or(serde_json::json!("single")),
            });

            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::json!({ "config": config })),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
        None => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: "not_found".to_string(),
                    message: "Entity not found".to_string(),
                }),
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

/// Handle script/config command - returns the script configuration
pub async fn handle_script_config(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Verify entity_id starts with "script."
    if !entity_id.starts_with("script.") {
        let result = OutgoingMessage::Result(ResultMessage {
            id,
            msg_type: "result",
            success: false,
            result: None,
            error: Some(ErrorInfo {
                code: "not_found".to_string(),
                message: "Entity not found".to_string(),
            }),
        });
        return tx.send(result).await.map_err(|e| e.to_string());
    }

    // Look up the script entity state
    match conn.state.state_machine.get(entity_id) {
        Some(state) => {
            // The script config is stored in the entity's attributes
            let config = serde_json::json!({
                "alias": state.attributes.get("friendly_name").cloned().unwrap_or(serde_json::Value::Null),
                "description": state.attributes.get("description").cloned().unwrap_or(serde_json::Value::Null),
                "sequence": state.attributes.get("sequence").cloned().unwrap_or(serde_json::json!([])),
                "mode": state.attributes.get("mode").cloned().unwrap_or(serde_json::json!("single")),
                "icon": state.attributes.get("icon").cloned().unwrap_or(serde_json::Value::Null),
            });

            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::json!({ "config": config })),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
        None => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: "not_found".to_string(),
                    message: "Entity not found".to_string(),
                }),
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

// =============================================================================
// Misc Handlers
// =============================================================================

/// Handle system_log/list command
pub async fn handle_system_log_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let entries = conn.state.system_log.list();
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!(entries)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle render_template command
pub async fn handle_render_template(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    template: &str,
    variables: Option<HashMap<String, serde_json::Value>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // For now, we'll do a simple template rendering
    // In a full implementation, this would use the TemplateEngine

    // Simple variable substitution for basic templates
    let mut result_str = template.to_string();

    // Handle variables if provided
    if let Some(vars) = variables {
        for (key, value) in vars {
            let placeholder = format!("{{{{ {} }}}}", key);
            let value_str = match value {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            };
            result_str = result_str.replace(&placeholder, &value_str);
        }
    }

    // For entity state templates like {{ states('sensor.temperature') }}
    // We would need the template engine, but for now return the template as-is
    // if it contains unresolved Jinja syntax

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "result": result_str,
            "listeners": {
                "all": false,
                "domains": [],
                "entities": [],
                "time": false
            }
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle auth/current_user command - returns current user info
pub async fn handle_auth_current_user(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return a default user for now
    let user = serde_json::json!({
        "id": conn.user_id.clone().unwrap_or_else(|| "default-user-id".to_string()),
        "name": "Owner",
        "is_owner": true,
        "is_admin": true,
        "credentials": [],
        "mfa_modules": [],
    });

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(user),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle recorder/info command
pub async fn handle_recorder_info(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return minimal recorder info (indicates recorder is not running)
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "backlog": 0,
            "max_backlog": 40000,
            "migration_in_progress": false,
            "recording": false,
            "thread_running": false,
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle repairs/list_issues command
pub async fn handle_repairs_list_issues(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty issues list
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "issues": []
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle persistent_notification/subscribe command
pub async fn handle_persistent_notification_subscribe(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Get current notifications
    let notifications = conn.state.notifications.get_all_map();

    // Convert to JSON-serializable format
    let notifications_json: serde_json::Map<String, serde_json::Value> = notifications
        .into_iter()
        .map(|(k, v)| (k, serde_json::to_value(v).unwrap_or_default()))
        .collect();

    // Send success response first (matching Python HA behavior)
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())?;

    // Send initial notifications event with "current" type
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({
            "type": "current",
            "notifications": notifications_json
        }),
    });
    tx.send(event).await.map_err(|e| e.to_string())
}

/// Handle labs/subscribe command
pub async fn handle_labs_subscribe(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Send initial labs state event (empty)
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!({}),
    });
    tx.send(event).await.map_err(|e| e.to_string())?;

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle logger/log_info command
pub async fn handle_logger_log_info(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty array of logger info
    // Format: [{"domain": "integration_name", "level": 20}, ...]
    // Level values: DEBUG=10, INFO=20, WARNING=30, ERROR=40, CRITICAL=50
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!([])),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle entity/source command
pub async fn handle_entity_source(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_ids: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let mut sources = serde_json::Map::new();

    // Get all states
    let states = conn.state.state_machine.all();

    for state in states.iter() {
        let entity_id = state.entity_id.to_string();

        // Filter if entity_ids provided
        if let Some(ref ids) = entity_ids {
            if !ids.contains(&entity_id) {
                continue;
            }
        }

        // Extract domain from entity_id
        let domain = entity_id.split('.').next().unwrap_or("unknown").to_string();

        sources.insert(
            entity_id,
            serde_json::json!({
                "domain": domain,
                "custom_component": false,
            }),
        );
    }

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Object(sources)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle blueprint/list command
pub async fn handle_blueprint_list(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _domain: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty blueprints
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({})),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

// =============================================================================
// Config Entry Handlers
// =============================================================================

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
) -> Result<(), String> {
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

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(result_json),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config_entries/subscribe command
pub async fn handle_config_entries_subscribe(
    conn: &Arc<ActiveConnection>,
    id: u64,
    type_filter: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())?;

    // Then send the event with all config entries
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!(entries),
    });
    tx.send(event).await.map_err(|e| e.to_string())
}

/// Handle application_credentials/config command
/// Returns list of domains that support application credentials and their config
pub async fn handle_application_credentials_config(
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!({
            "domains": domains,
            "integrations": serde_json::Value::Object(integrations)
        })),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle application_credentials/config_entry command
/// Returns credentials associated with a config entry (usually null for most integrations)
pub async fn handle_application_credentials_config_entry(
    id: u64,
    _entry_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Most integrations don't use application credentials
    // Return null to indicate no credentials
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle application_credentials/list command
/// Returns list of stored application credentials
pub async fn handle_application_credentials_list(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!(credentials)),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle application_credentials/create command
/// Creates a new application credential (OAuth2 client credentials)
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
) -> Result<(), String> {
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

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(result_obj),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle application_credentials/delete command
/// Deletes an application credential
pub async fn handle_application_credentials_delete(
    conn: &Arc<ActiveConnection>,
    id: u64,
    credential_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    info!("Deleting application credential: {}", credential_id);

    // Try to remove the credential
    if conn
        .state
        .application_credentials
        .remove(credential_id)
        .is_some()
    {
        let result = OutgoingMessage::Result(ResultMessage {
            id,
            msg_type: "result",
            success: true,
            result: Some(serde_json::Value::Null),
            error: None,
        });
        tx.send(result).await.map_err(|e| e.to_string())
    } else {
        let result = OutgoingMessage::Result(ResultMessage {
            id,
            msg_type: "result",
            success: false,
            result: None,
            error: Some(ErrorInfo {
                code: "not_found".to_string(),
                message: format!(
                    "Unable to find application_credentials_id {}",
                    credential_id
                ),
            }),
        });
        tx.send(result).await.map_err(|e| e.to_string())
    }
}

/// Handle config_entries/delete command
pub async fn handle_config_entries_delete(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entry_id: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    info!("Deleting config entry: {}", entry_id);

    // Remove the config entry, then release lock before sending response
    let remove_result = {
        let config_entries = conn.state.config_entries.write().await;
        config_entries.remove(entry_id).await
    }; // Write lock released here

    match remove_result {
        Ok(_entry) => {
            info!("Config entry {} deleted successfully", entry_id);
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::json!({
                    "require_restart": false
                })),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
        Err(e) => {
            warn!("Failed to delete config entry {}: {}", entry_id, e);
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: "not_found".to_string(),
                    message: format!("Config entry {} not found", entry_id),
                }),
            });
            tx.send(result).await.map_err(|e| e.to_string())
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
) -> Result<(), String> {
    // Most integrations don't have subentries, return empty array
    // Per HA format: [{"subentry_id": "...", "subentry_type": "...", "title": "...", "unique_id": "..."}]
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::json!([])),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

// =============================================================================
// Config Flow Handlers
// =============================================================================

/// Handle config_entries/flow/progress without flow_id - lists flows in progress
///
/// Returns flows that are in progress but not started by a user (e.g., discovered devices).
pub async fn handle_config_entries_flow_progress_list(
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Return empty list since we don't have any auto-discovered flows yet
    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Array(vec![])),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config_entries/flow/subscribe command
pub async fn handle_config_entries_flow_subscribe(
    conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Get list of active flows if config flow manager is available
    let flows = if let Some(cfm) = &conn.state.config_flow_handler {
        cfm.list_flows().await
    } else {
        vec![]
    };

    // Send initial flows state
    let event = OutgoingMessage::Event(EventMessage {
        id,
        msg_type: "event",
        event: serde_json::json!(flows),
    });
    tx.send(event).await.map_err(|e| e.to_string())?;

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(serde_json::Value::Null),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle config_entries/flow command - start a new config flow
pub async fn handle_config_entries_flow(
    conn: &Arc<ActiveConnection>,
    id: u64,
    handler: &str,
    show_advanced_options: bool,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    info!("Starting config flow for handler: {}", handler);

    let config_flow_handler = conn
        .state
        .config_flow_handler
        .as_ref()
        .ok_or_else(|| "Config flow manager not available".to_string())?;

    match config_flow_handler
        .start_flow(handler, show_advanced_options)
        .await
    {
        Ok(flow_result) => {
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::to_value(&flow_result).unwrap_or_default()),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
        Err(e) => {
            error!("Failed to start config flow: {}", e);
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: "flow_error".to_string(),
                    message: e,
                }),
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

/// Handle config_entries/flow/progress command - continue a config flow
pub async fn handle_config_entries_flow_progress(
    conn: &Arc<ActiveConnection>,
    id: u64,
    flow_id: &str,
    user_input: Option<serde_json::Value>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    info!("Progressing config flow: {}", flow_id);

    let config_flow_handler = conn
        .state
        .config_flow_handler
        .as_ref()
        .ok_or_else(|| "Config flow manager not available".to_string())?;

    match config_flow_handler.progress_flow(flow_id, user_input).await {
        Ok(flow_result) => {
            // If the flow created an entry, we need to save it
            if flow_result.result_type == "create_entry" {
                if let Some(ref result_data) = flow_result.result {
                    // Create and save the config entry
                    if let Err(e) = save_config_entry_from_flow(
                        &conn.state,
                        &flow_result.handler,
                        flow_result.title.as_deref().unwrap_or(&flow_result.handler),
                        result_data,
                    )
                    .await
                    {
                        warn!("Failed to save config entry: {}", e);
                    }
                }
            }

            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: true,
                result: Some(serde_json::to_value(&flow_result).unwrap_or_default()),
                error: None,
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
        Err(e) => {
            error!("Failed to progress config flow: {}", e);
            let result = OutgoingMessage::Result(ResultMessage {
                id,
                msg_type: "result",
                success: false,
                result: None,
                error: Some(ErrorInfo {
                    code: "flow_error".to_string(),
                    message: e,
                }),
            });
            tx.send(result).await.map_err(|e| e.to_string())
        }
    }
}

/// Save a config entry created by a flow
async fn save_config_entry_from_flow(
    state: &AppState,
    domain: &str,
    title: &str,
    data: &serde_json::Value,
) -> Result<(), String> {
    use ha_config_entries::ConfigEntry;

    // Create entry using the constructor which handles all defaults
    let mut entry = ConfigEntry::new(domain, title);

    // Set the data from the flow
    if let Some(obj) = data.as_object() {
        for (k, v) in obj {
            entry.data.insert(k.clone(), v.clone());
        }
    }

    let entry_id = entry.entry_id.clone();

    // Add to config entries
    {
        let config_entries = state.config_entries.write().await;
        let _ = config_entries.add(entry).await;
    }

    // Save to disk
    {
        let config_entries = state.config_entries.read().await;
        config_entries
            .save()
            .await
            .map_err(|e| format!("Failed to save config entries: {}", e))?;
    }

    info!("Created config entry {} for {}", entry_id, domain);
    Ok(())
}

// =============================================================================
// Integration/Manifest Handlers
// =============================================================================

/// Handle integration/descriptions command
///
/// Returns descriptions of integrations for the "Add Integration" dialog.
/// This provides the list of available integrations the user can configure.
pub async fn handle_integration_descriptions(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    _integrations: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    // Load integration descriptions from manifest files
    let integrations = crate::manifest::build_integration_descriptions();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(integrations),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle manifest/list command
pub async fn handle_manifest_list(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
    let manifests = crate::manifest::build_manifest_list();

    let result = OutgoingMessage::Result(ResultMessage {
        id,
        msg_type: "result",
        success: true,
        result: Some(manifests),
        error: None,
    });
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Handle manifest/get command
pub async fn handle_manifest_get(
    _conn: &Arc<ActiveConnection>,
    id: u64,
    integration: &str,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> Result<(), String> {
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
    tx.send(result).await.map_err(|e| e.to_string())
}

/// Capitalize first letter of a string
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
