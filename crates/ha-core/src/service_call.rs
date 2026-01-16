//! Service call type for invoking Home Assistant services

use crate::Context;
use serde::{Deserialize, Serialize};

/// Represents a call to a Home Assistant service
///
/// Services are the primary way to control entities and trigger actions
/// in Home Assistant. Each service belongs to a domain and has associated
/// service data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCall {
    /// The domain the service belongs to (e.g., "light", "switch", "automation")
    pub domain: String,

    /// The service name (e.g., "turn_on", "turn_off", "toggle")
    pub service: String,

    /// Data passed to the service (e.g., entity_id, brightness, color)
    pub service_data: serde_json::Value,

    /// Context tracking who initiated this call
    pub context: Context,
}

impl ServiceCall {
    /// Create a new service call
    pub fn new(
        domain: impl Into<String>,
        service: impl Into<String>,
        service_data: serde_json::Value,
        context: Context,
    ) -> Self {
        Self {
            domain: domain.into(),
            service: service.into(),
            service_data,
            context,
        }
    }

    /// Create a service call with empty service data
    pub fn simple(domain: impl Into<String>, service: impl Into<String>, context: Context) -> Self {
        Self::new(
            domain,
            service,
            serde_json::Value::Object(Default::default()),
            context,
        )
    }

    /// Get the full service identifier (domain.service)
    pub fn service_id(&self) -> String {
        format!("{}.{}", self.domain, self.service)
    }

    /// Get a value from service_data
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.service_data
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get entity_id(s) from service data
    ///
    /// Returns a vector of entity IDs, handling both single string and array formats.
    pub fn entity_ids(&self) -> Vec<String> {
        match self.service_data.get("entity_id") {
            Some(serde_json::Value::String(s)) => vec![s.clone()],
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => vec![],
        }
    }
}

/// Whether a service supports returning a response
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SupportsResponse {
    /// Service never returns a response
    #[default]
    None,
    /// Service may optionally return a response
    Optional,
    /// Service always returns a response
    Only,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_service_call_creation() {
        let ctx = Context::new();
        let call = ServiceCall::new(
            "light",
            "turn_on",
            json!({"entity_id": "light.living_room", "brightness": 255}),
            ctx.clone(),
        );

        assert_eq!(call.domain, "light");
        assert_eq!(call.service, "turn_on");
        assert_eq!(call.service_id(), "light.turn_on");
        assert_eq!(call.context.id, ctx.id);
    }

    #[test]
    fn test_simple_service_call() {
        let ctx = Context::new();
        let call = ServiceCall::simple("homeassistant", "restart", ctx);

        assert_eq!(call.domain, "homeassistant");
        assert_eq!(call.service, "restart");
        assert!(call.service_data.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_get_service_data() {
        let ctx = Context::new();
        let call = ServiceCall::new(
            "light",
            "turn_on",
            json!({"brightness": 200, "transition": 2.5}),
            ctx,
        );

        assert_eq!(call.get::<i32>("brightness"), Some(200));
        assert_eq!(call.get::<f64>("transition"), Some(2.5));
        assert_eq!(call.get::<String>("missing"), None);
    }

    #[test]
    fn test_entity_ids_single() {
        let ctx = Context::new();
        let call = ServiceCall::new(
            "light",
            "turn_on",
            json!({"entity_id": "light.living_room"}),
            ctx,
        );

        assert_eq!(call.entity_ids(), vec!["light.living_room"]);
    }

    #[test]
    fn test_entity_ids_multiple() {
        let ctx = Context::new();
        let call = ServiceCall::new(
            "light",
            "turn_on",
            json!({"entity_id": ["light.living_room", "light.bedroom"]}),
            ctx,
        );

        assert_eq!(
            call.entity_ids(),
            vec!["light.living_room", "light.bedroom"]
        );
    }

    #[test]
    fn test_entity_ids_none() {
        let ctx = Context::new();
        let call = ServiceCall::new("homeassistant", "restart", json!({}), ctx);

        assert!(call.entity_ids().is_empty());
    }

    #[test]
    fn test_supports_response() {
        assert_eq!(SupportsResponse::default(), SupportsResponse::None);
    }

    #[test]
    fn test_serde_roundtrip() {
        let ctx = Context::new();
        let call = ServiceCall::new("light", "turn_on", json!({"entity_id": "light.test"}), ctx);

        let json = serde_json::to_string(&call).unwrap();
        let parsed: ServiceCall = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.domain, call.domain);
        assert_eq!(parsed.service, call.service);
        assert_eq!(parsed.service_data, call.service_data);
    }
}
