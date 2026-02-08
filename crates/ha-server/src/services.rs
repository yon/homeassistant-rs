//! Service registration helpers
//!
//! Provides helper functions that eliminate boilerplate when registering
//! common service patterns (stubs, state-setters, toggles).

use std::collections::HashMap;
use std::sync::Arc;

use ha_core::{Context, EntityId, ServiceCall, SupportsResponse, STATE_OFF, STATE_ON};
use ha_service_registry::{ServiceDescription, ServiceRegistry};
use ha_state_store::StateStore;
use serde_json::json;
use tracing::info;

/// Extract entity_id from service call data, optionally filtering by domain.
///
/// Returns `None` if entity_id is missing, not a string, malformed, or
/// belongs to a different domain than `domain_filter`.
pub(crate) fn extract_entity_id(
    call: &ServiceCall,
    domain_filter: Option<&str>,
) -> Option<EntityId> {
    let id_str = call.service_data.get("entity_id")?.as_str()?;
    let entity_id: EntityId = id_str.parse().ok()?;
    if let Some(domain) = domain_filter {
        if entity_id.domain() != domain {
            return None;
        }
    }
    Some(entity_id)
}

/// Register a stub service that optionally logs and returns `Ok(None)`.
///
/// Used for services that are recognized by the API but not yet fully
/// implemented (reload, restart, stop, etc.).
pub(crate) fn register_stub_service(
    services: &Arc<ServiceRegistry>,
    domain: &str,
    service: &str,
    target: Option<serde_json::Value>,
    log_message: Option<&str>,
) {
    let log_msg = log_message.map(String::from);
    services.register_with_description(
        ServiceDescription {
            domain: domain.to_string(),
            description: None,
            name: None,
            schema: None,
            service: service.to_string(),
            supports_response: SupportsResponse::None,
            target,
        },
        move |_call: ServiceCall| {
            let msg = log_msg.clone();
            async move {
                if let Some(m) = msg {
                    info!("{}", m);
                }
                Ok(None)
            }
        },
    );
}

/// Register a service that sets an entity to a specific state.
///
/// If `domain_filter` is `Some`, only entities in that domain are affected.
/// If `preserve_attrs` is true, existing attributes are preserved; otherwise
/// attributes are cleared to an empty map.
pub(crate) fn register_state_service(
    services: &Arc<ServiceRegistry>,
    states: &Arc<StateStore>,
    domain: &str,
    service: &str,
    target_state: &'static str,
    domain_filter: Option<&str>,
    preserve_attrs: bool,
) {
    let states = states.clone();
    let domain_filter_owned = domain_filter.map(String::from);
    let target = match &domain_filter_owned {
        Some(d) => Some(json!({"entity": {"domain": d}})),
        None => Some(json!({})),
    };

    services.register_with_description(
        ServiceDescription {
            domain: domain.to_string(),
            description: None,
            name: None,
            schema: None,
            service: service.to_string(),
            supports_response: SupportsResponse::None,
            target,
        },
        move |call: ServiceCall| {
            let states = states.clone();
            let domain_filter = domain_filter_owned.clone();
            async move {
                if let Some(entity_id) = extract_entity_id(&call, domain_filter.as_deref()) {
                    let attrs = if preserve_attrs {
                        states
                            .get(&entity_id.to_string())
                            .map(|s| s.attributes.clone())
                            .unwrap_or_default()
                    } else {
                        HashMap::new()
                    };
                    states.set(entity_id, target_state, attrs, Context::new());
                }
                Ok(None)
            }
        },
    );
}

/// Register a toggle service that flips entity state between ON and OFF.
///
/// If `domain_filter` is `Some`, only entities in that domain are affected.
pub(crate) fn register_toggle_service(
    services: &Arc<ServiceRegistry>,
    states: &Arc<StateStore>,
    domain: &str,
    domain_filter: Option<&str>,
) {
    let states = states.clone();
    let domain_filter_owned = domain_filter.map(String::from);
    let target = match &domain_filter_owned {
        Some(d) => Some(json!({"entity": {"domain": d}})),
        None => Some(json!({})),
    };

    services.register_with_description(
        ServiceDescription {
            domain: domain.to_string(),
            description: None,
            name: None,
            schema: None,
            service: "toggle".to_string(),
            supports_response: SupportsResponse::None,
            target,
        },
        move |call: ServiceCall| {
            let states = states.clone();
            let domain_filter = domain_filter_owned.clone();
            async move {
                if let Some(entity_id) = extract_entity_id(&call, domain_filter.as_deref()) {
                    if let Some(state) = states.get(&entity_id.to_string()) {
                        let new_state = if state.state == STATE_ON {
                            STATE_OFF
                        } else {
                            STATE_ON
                        };
                        states.set(
                            entity_id,
                            new_state,
                            state.attributes.clone(),
                            Context::new(),
                        );
                    }
                }
                Ok(None)
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_call(service_data: serde_json::Value) -> ServiceCall {
        ServiceCall {
            domain: "test".to_string(),
            service: "test".to_string(),
            service_data,
            context: Context::new(),
        }
    }

    #[test]
    fn extract_entity_id_returns_valid_entity() {
        let call = make_call(json!({"entity_id": "light.living_room"}));
        let result = extract_entity_id(&call, None);
        assert!(result.is_some());
        let entity = result.unwrap();
        assert_eq!(entity.domain(), "light");
        assert_eq!(entity.object_id(), "living_room");
    }

    #[test]
    fn extract_entity_id_returns_none_for_missing() {
        let call = make_call(json!({}));
        assert!(extract_entity_id(&call, None).is_none());
    }

    #[test]
    fn extract_entity_id_returns_none_for_non_string() {
        let call = make_call(json!({"entity_id": 42}));
        assert!(extract_entity_id(&call, None).is_none());
    }

    #[test]
    fn extract_entity_id_returns_none_for_malformed() {
        let call = make_call(json!({"entity_id": "no_dot_here"}));
        assert!(extract_entity_id(&call, None).is_none());
    }

    #[test]
    fn extract_entity_id_filters_by_domain() {
        let call = make_call(json!({"entity_id": "light.living_room"}));
        assert!(extract_entity_id(&call, Some("light")).is_some());
        assert!(extract_entity_id(&call, Some("switch")).is_none());
    }

    #[test]
    fn extract_entity_id_none_filter_accepts_any_domain() {
        let call = make_call(json!({"entity_id": "sensor.temperature"}));
        let result = extract_entity_id(&call, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().domain(), "sensor");
    }
}
