//! Subscription handlers: subscribe_events, unsubscribe_events, subscribe_entities

use std::sync::Arc;

use tokio::sync::{broadcast, mpsc};
use tracing::debug;

use super::send_result;
use crate::error::{WebSocketError, WsResult};
use crate::websocket::connection::ActiveConnection;
use crate::websocket::types::{EventMessage, OutgoingMessage};

/// Handle subscribe_events command
pub async fn handle_subscribe_events(
    conn: &Arc<ActiveConnection>,
    id: u64,
    event_type: Option<String>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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
    send_result(id, serde_json::Value::Null, tx).await
}

/// Handle unsubscribe_events command
pub async fn handle_unsubscribe_events(
    conn: &Arc<ActiveConnection>,
    id: u64,
    subscription: u64,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
    let mut subs = conn.subscriptions.write().await;
    if let Some(cancel_tx) = subs.remove(&subscription) {
        let _ = cancel_tx.send(());
    }
    drop(subs);

    // Explicitly include "result": null to match Python HA
    send_result(id, serde_json::Value::Null, tx).await
}

/// Handle subscribe_entities command
pub async fn handle_subscribe_entities(
    conn: &Arc<ActiveConnection>,
    id: u64,
    entity_ids: Option<Vec<String>>,
    tx: &mpsc::Sender<OutgoingMessage>,
) -> WsResult<()> {
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

    // Build initial state response, enriching with entity registry data
    let mut additions = serde_json::Map::new();
    for state in filtered_states {
        // Start with the state's attributes
        let mut attrs = state.attributes.clone();

        // Enrich with entity registry data if available
        if let Some(entry) = conn
            .state
            .registries
            .entities
            .get(&state.entity_id.to_string())
        {
            entry.enrich_attributes(&mut attrs);
        }

        additions.insert(
            state.entity_id.to_string(),
            serde_json::json!({
                "s": state.state,
                "a": attrs,
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
    tx.send(initial_event)
        .await
        .map_err(|e| WebSocketError::ChannelSend(e.to_string()))?;

    // Subscribe to state changes
    let entity_ids_filter = entity_ids.clone();
    let tx_clone = tx.clone();
    let sub_id = id;
    let registries = conn.state.registries.clone();

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

                                // Build change event with enriched attributes
                                if let Some(new_state) = event.data.get("new_state") {
                                    // Get attributes and enrich with entity registry data
                                    let mut attrs = new_state
                                        .get("attributes")
                                        .and_then(|a| a.as_object())
                                        .cloned()
                                        .unwrap_or_default();

                                    // Enrich with entity registry data
                                    if let Some(entry) = registries.entities.get(entity_id) {
                                        entry.enrich_json_attributes(&mut attrs);
                                    }

                                    let mut changes = serde_json::Map::new();
                                    changes.insert(
                                        entity_id.to_string(),
                                        serde_json::json!({
                                            "+": {
                                                "s": new_state.get("state"),
                                                "a": attrs,
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
    send_result(id, serde_json::Value::Null, tx).await
}
