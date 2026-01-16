//! Service registry with async handlers for Home Assistant
//!
//! This crate provides the ServiceRegistry, which manages all registered
//! services in Home Assistant. Services are the primary way to control
//! entities and trigger actions.

use dashmap::DashMap;
use ha_core::{Context, ServiceCall, SupportsResponse};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, instrument, warn};

/// Result type for service calls
pub type ServiceResult = Result<Option<serde_json::Value>, ServiceError>;

/// Future type for async service handlers
pub type ServiceFuture = Pin<Box<dyn Future<Output = ServiceResult> + Send>>;

/// Service handler function type
pub type ServiceHandler = Arc<dyn Fn(ServiceCall) -> ServiceFuture + Send + Sync>;

/// Errors that can occur when working with services
#[derive(Debug, Clone, Error)]
pub enum ServiceError {
    #[error("service not found: {domain}.{service}")]
    NotFound { domain: String, service: String },

    #[error("service call failed: {0}")]
    CallFailed(String),

    #[error("invalid service data: {0}")]
    InvalidData(String),

    #[error("service does not support responses")]
    ResponseNotSupported,
}

/// Information about a registered service
#[derive(Debug, Clone)]
pub struct ServiceDescription {
    /// Domain the service belongs to
    pub domain: String,
    /// Service name
    pub service: String,
    /// Human-readable name
    pub name: Option<String>,
    /// Description of what the service does
    pub description: Option<String>,
    /// JSON schema for service data (optional)
    pub schema: Option<serde_json::Value>,
    /// Whether this service supports returning a response
    pub supports_response: SupportsResponse,
}

/// Internal representation of a registered service
struct RegisteredService {
    handler: ServiceHandler,
    description: ServiceDescription,
}

/// The service registry manages all registered services
///
/// The ServiceRegistry is responsible for:
/// - Registering services with their handlers
/// - Calling services and routing to the appropriate handler
/// - Providing information about available services
pub struct ServiceRegistry {
    /// Services indexed by "domain.service" key
    services: DashMap<String, RegisteredService>,
}

impl ServiceRegistry {
    /// Create a new empty service registry
    pub fn new() -> Self {
        Self {
            services: DashMap::new(),
        }
    }

    /// Register a new service
    ///
    /// # Arguments
    /// * `domain` - The domain the service belongs to (e.g., "light")
    /// * `service` - The service name (e.g., "turn_on")
    /// * `handler` - Async function to handle service calls
    /// * `schema` - Optional JSON schema for validating service data
    /// * `supports_response` - Whether the service can return a response
    #[instrument(skip(self, domain, service, handler, schema))]
    pub fn register<F, Fut>(
        &self,
        domain: impl Into<String>,
        service: impl Into<String>,
        handler: F,
        schema: Option<serde_json::Value>,
        supports_response: SupportsResponse,
    ) where
        F: Fn(ServiceCall) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ServiceResult> + Send + 'static,
    {
        let domain = domain.into();
        let service = service.into();
        let key = format!("{}.{}", domain, service);

        debug!(domain = %domain, service = %service, "Registering service");

        let handler: ServiceHandler =
            Arc::new(move |call| Box::pin(handler(call)) as ServiceFuture);

        let description = ServiceDescription {
            domain: domain.clone(),
            service: service.clone(),
            name: None,
            description: None,
            schema,
            supports_response,
        };

        self.services.insert(
            key,
            RegisteredService {
                handler,
                description,
            },
        );
    }

    /// Register a service with full description
    #[instrument(skip(self, handler))]
    pub fn register_with_description<F, Fut>(&self, description: ServiceDescription, handler: F)
    where
        F: Fn(ServiceCall) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ServiceResult> + Send + 'static,
    {
        let key = format!("{}.{}", description.domain, description.service);

        debug!(
            domain = %description.domain,
            service = %description.service,
            "Registering service with description"
        );

        let handler: ServiceHandler =
            Arc::new(move |call| Box::pin(handler(call)) as ServiceFuture);

        self.services.insert(
            key,
            RegisteredService {
                handler,
                description,
            },
        );
    }

    /// Call a service
    ///
    /// # Arguments
    /// * `domain` - The domain of the service
    /// * `service` - The service name
    /// * `service_data` - Data to pass to the service
    /// * `context` - Context for tracking the call origin
    /// * `return_response` - Whether to return the service response
    #[instrument(skip(self, service_data, context))]
    pub async fn call(
        &self,
        domain: &str,
        service: &str,
        service_data: serde_json::Value,
        context: Context,
        return_response: bool,
    ) -> ServiceResult {
        let key = format!("{}.{}", domain, service);

        let registered = self.services.get(&key).ok_or_else(|| {
            warn!(domain = %domain, service = %service, "Service not found");
            ServiceError::NotFound {
                domain: domain.to_string(),
                service: service.to_string(),
            }
        })?;

        // Check response support
        if return_response && registered.description.supports_response == SupportsResponse::None {
            return Err(ServiceError::ResponseNotSupported);
        }

        let call = ServiceCall::new(domain, service, service_data, context);

        debug!(domain = %domain, service = %service, "Calling service");

        let handler = registered.handler.clone();
        drop(registered); // Release the lock before calling the handler

        let result = handler(call).await?;

        // Only return response if requested and supported
        if return_response {
            Ok(result)
        } else {
            Ok(None)
        }
    }

    /// Check if a service exists
    pub fn has_service(&self, domain: &str, service: &str) -> bool {
        let key = format!("{}.{}", domain, service);
        self.services.contains_key(&key)
    }

    /// Get service description
    pub fn get_service(&self, domain: &str, service: &str) -> Option<ServiceDescription> {
        let key = format!("{}.{}", domain, service);
        self.services.get(&key).map(|s| s.description.clone())
    }

    /// Get all services for a domain
    pub fn domain_services(&self, domain: &str) -> Vec<ServiceDescription> {
        self.services
            .iter()
            .filter(|s| s.description.domain == domain)
            .map(|s| s.description.clone())
            .collect()
    }

    /// Get all domains that have registered services
    pub fn domains(&self) -> Vec<String> {
        let mut domains: Vec<_> = self
            .services
            .iter()
            .map(|s| s.description.domain.clone())
            .collect();
        domains.sort();
        domains.dedup();
        domains
    }

    /// Get all registered services grouped by domain
    pub fn all_services(&self) -> HashMap<String, Vec<ServiceDescription>> {
        let mut result: HashMap<String, Vec<ServiceDescription>> = HashMap::new();

        for entry in self.services.iter() {
            result
                .entry(entry.description.domain.clone())
                .or_default()
                .push(entry.description.clone());
        }

        result
    }

    /// Unregister a service
    #[instrument(skip(self))]
    pub fn unregister(&self, domain: &str, service: &str) -> bool {
        let key = format!("{}.{}", domain, service);
        let removed = self.services.remove(&key).is_some();

        if removed {
            debug!(domain = %domain, service = %service, "Unregistered service");
        }

        removed
    }

    /// Unregister all services for a domain
    #[instrument(skip(self))]
    pub fn unregister_domain(&self, domain: &str) -> usize {
        let keys_to_remove: Vec<_> = self
            .services
            .iter()
            .filter(|s| s.description.domain == domain)
            .map(|s| format!("{}.{}", s.description.domain, s.description.service))
            .collect();

        let count = keys_to_remove.len();
        for key in keys_to_remove {
            self.services.remove(&key);
        }

        debug!(domain = %domain, count = count, "Unregistered domain services");
        count
    }

    /// Get total number of registered services
    pub fn service_count(&self) -> usize {
        self.services.len()
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe wrapper for ServiceRegistry
pub type SharedServiceRegistry = Arc<ServiceRegistry>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_register_and_call() {
        let registry = ServiceRegistry::new();

        registry.register(
            "test",
            "echo",
            |call: ServiceCall| async move { Ok(Some(call.service_data)) },
            None,
            SupportsResponse::Optional,
        );

        let result = registry
            .call(
                "test",
                "echo",
                json!({"msg": "hello"}),
                Context::new(),
                true,
            )
            .await
            .unwrap();

        assert_eq!(result, Some(json!({"msg": "hello"})));
    }

    #[tokio::test]
    async fn test_service_not_found() {
        let registry = ServiceRegistry::new();

        let result = registry
            .call("nonexistent", "service", json!({}), Context::new(), false)
            .await;

        assert!(matches!(result, Err(ServiceError::NotFound { .. })));
    }

    #[tokio::test]
    async fn test_service_without_response() {
        let registry = ServiceRegistry::new();

        registry.register(
            "light",
            "turn_on",
            |_call: ServiceCall| async move { Ok(None) },
            None,
            SupportsResponse::None,
        );

        // Calling without requesting response should work
        let result = registry
            .call("light", "turn_on", json!({}), Context::new(), false)
            .await;
        assert!(result.is_ok());

        // Calling with response should fail
        let result = registry
            .call("light", "turn_on", json!({}), Context::new(), true)
            .await;
        assert!(matches!(result, Err(ServiceError::ResponseNotSupported)));
    }

    #[test]
    fn test_has_service() {
        let registry = ServiceRegistry::new();

        registry.register(
            "light",
            "turn_on",
            |_: ServiceCall| async { Ok(None) },
            None,
            SupportsResponse::None,
        );

        assert!(registry.has_service("light", "turn_on"));
        assert!(!registry.has_service("light", "turn_off"));
        assert!(!registry.has_service("switch", "turn_on"));
    }

    #[test]
    fn test_domain_services() {
        let registry = ServiceRegistry::new();

        registry.register(
            "light",
            "turn_on",
            |_: ServiceCall| async { Ok(None) },
            None,
            SupportsResponse::None,
        );
        registry.register(
            "light",
            "turn_off",
            |_: ServiceCall| async { Ok(None) },
            None,
            SupportsResponse::None,
        );
        registry.register(
            "switch",
            "toggle",
            |_: ServiceCall| async { Ok(None) },
            None,
            SupportsResponse::None,
        );

        let light_services = registry.domain_services("light");
        assert_eq!(light_services.len(), 2);

        let switch_services = registry.domain_services("switch");
        assert_eq!(switch_services.len(), 1);
    }

    #[test]
    fn test_domains() {
        let registry = ServiceRegistry::new();

        registry.register(
            "light",
            "turn_on",
            |_: ServiceCall| async { Ok(None) },
            None,
            SupportsResponse::None,
        );
        registry.register(
            "switch",
            "toggle",
            |_: ServiceCall| async { Ok(None) },
            None,
            SupportsResponse::None,
        );
        registry.register(
            "automation",
            "trigger",
            |_: ServiceCall| async { Ok(None) },
            None,
            SupportsResponse::None,
        );

        let domains = registry.domains();
        assert_eq!(domains.len(), 3);
        assert!(domains.contains(&"light".to_string()));
        assert!(domains.contains(&"switch".to_string()));
        assert!(domains.contains(&"automation".to_string()));
    }

    #[test]
    fn test_unregister() {
        let registry = ServiceRegistry::new();

        registry.register(
            "light",
            "turn_on",
            |_: ServiceCall| async { Ok(None) },
            None,
            SupportsResponse::None,
        );

        assert!(registry.has_service("light", "turn_on"));
        assert!(registry.unregister("light", "turn_on"));
        assert!(!registry.has_service("light", "turn_on"));
        assert!(!registry.unregister("light", "turn_on")); // Already removed
    }

    #[test]
    fn test_unregister_domain() {
        let registry = ServiceRegistry::new();

        registry.register(
            "light",
            "turn_on",
            |_: ServiceCall| async { Ok(None) },
            None,
            SupportsResponse::None,
        );
        registry.register(
            "light",
            "turn_off",
            |_: ServiceCall| async { Ok(None) },
            None,
            SupportsResponse::None,
        );
        registry.register(
            "switch",
            "toggle",
            |_: ServiceCall| async { Ok(None) },
            None,
            SupportsResponse::None,
        );

        let count = registry.unregister_domain("light");
        assert_eq!(count, 2);
        assert!(!registry.has_service("light", "turn_on"));
        assert!(!registry.has_service("light", "turn_off"));
        assert!(registry.has_service("switch", "toggle"));
    }

    #[tokio::test]
    async fn test_service_error() {
        let registry = ServiceRegistry::new();

        registry.register(
            "test",
            "fail",
            |_: ServiceCall| async move {
                Err(ServiceError::CallFailed("intentional failure".to_string()))
            },
            None,
            SupportsResponse::None,
        );

        let result = registry
            .call("test", "fail", json!({}), Context::new(), false)
            .await;

        assert!(matches!(result, Err(ServiceError::CallFailed(_))));
    }
}
