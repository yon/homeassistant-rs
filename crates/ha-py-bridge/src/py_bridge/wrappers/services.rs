//! ServicesWrapper - wraps Rust ServiceRegistry for Python access

use super::util::py_to_json;
use dashmap::DashMap;
use ha_core::{Context, ServiceCall, SupportsResponse};
use ha_service_registry::{ServiceDescription, ServiceRegistry};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tokio::runtime::Handle;

/// Python wrapper for the Rust ServiceRegistry
///
/// This provides bidirectional service bridge:
/// - Python integrations can register services that Rust can call
/// - Python code can call services registered in Rust
#[pyclass(name = "ServicesWrapper")]
pub struct ServicesWrapper {
    services: Arc<ServiceRegistry>,
    /// Store Python handlers for services registered via Python
    /// Key: "domain.service", Value: PyObject (the Python callable)
    python_handlers: Arc<DashMap<String, PyObject>>,
}

impl ServicesWrapper {
    pub fn new(services: Arc<ServiceRegistry>) -> Self {
        Self {
            services,
            python_handlers: Arc::new(DashMap::new()),
        }
    }
}

#[pymethods]
impl ServicesWrapper {
    /// Call a service
    ///
    /// This method bridges Python service calls to either:
    /// - Python-registered handlers (via python_handlers map)
    /// - Rust-registered handlers (via ServiceRegistry)
    #[pyo3(signature = (domain, service, service_data=None, blocking=None, context=None, target=None, return_response=None))]
    fn async_call<'py>(
        &self,
        py: Python<'py>,
        domain: &str,
        service: &str,
        service_data: Option<&Bound<'py, PyDict>>,
        blocking: Option<bool>,
        context: Option<PyObject>,
        target: Option<PyObject>,
        return_response: Option<bool>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = (blocking, target); // Suppress unused warnings

        let data: serde_json::Value = match service_data {
            Some(dict) => py_to_json(dict.as_any()),
            None => serde_json::Value::Object(serde_json::Map::new()),
        };

        let ctx = match context {
            Some(_) => Context::new(), // TODO: Extract context from PyObject
            None => Context::new(),
        };

        let return_response = return_response.unwrap_or(false);

        tracing::debug!(domain = %domain, service = %service, "Service call via Rust bridge");

        // Check if we have a Tokio runtime available
        if let Ok(handle) = Handle::try_current() {
            let services = self.services.clone();
            let domain = domain.to_string();
            let service = service.to_string();

            // Use block_in_place to call the async service
            let result = tokio::task::block_in_place(|| {
                handle.block_on(async {
                    services
                        .call(&domain, &service, data, ctx, return_response)
                        .await
                })
            });

            // Return result as completed future
            let asyncio = py.import_bound("asyncio")?;
            let future = asyncio.call_method0("Future")?;

            match result {
                Ok(response) => {
                    if let Some(val) = response {
                        let py_val = super::util::json_to_py(py, &val)?;
                        future.call_method1("set_result", (py_val,))?;
                    } else {
                        future.call_method1("set_result", (py.None(),))?;
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Service call failed");
                    future.call_method1("set_result", (py.None(),))?;
                }
            }

            Ok(future)
        } else {
            // No Tokio runtime - just check if service exists and return completed future
            let _ = self.services.has_service(domain, service);

            let asyncio = py.import_bound("asyncio")?;
            let future = asyncio.call_method0("Future")?;
            future.call_method1("set_result", (py.None(),))?;
            Ok(future)
        }
    }

    /// Register a service with a Python handler
    ///
    /// The handler will be stored and called when the service is invoked.
    /// This bridges Python integration services to the Rust ServiceRegistry.
    #[pyo3(signature = (domain, service, service_func, schema=None, supports_response=None))]
    fn async_register<'py>(
        &self,
        py: Python<'py>,
        domain: &str,
        service: &str,
        service_func: PyObject,
        schema: Option<PyObject>,
        supports_response: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Validate service_func is callable
        if !service_func.bind(py).is_callable() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "service_func must be callable",
            ));
        }

        // Parse supports_response
        let supports_response_enum =
            match supports_response.unwrap_or("none").to_lowercase().as_str() {
                "none" => SupportsResponse::None,
                "only" => SupportsResponse::Only,
                "optional" => SupportsResponse::Optional,
                _ => SupportsResponse::None,
            };

        // Convert schema to JSON if provided
        let schema_json = if let Some(ref schema_obj) = schema {
            if let Ok(dict) = schema_obj.downcast_bound::<PyDict>(py) {
                Some(py_to_json(dict.as_any()))
            } else {
                None
            }
        } else {
            None
        };

        let domain_str = domain.to_string();
        let service_str = service.to_string();
        let key = format!("{}.{}", domain, service);

        // Store the Python handler
        let py_handler = service_func.clone_ref(py);
        self.python_handlers
            .insert(key.clone(), py_handler.clone_ref(py));

        tracing::info!(domain = %domain, service = %service, "Registering Python service handler");

        // Create the service description
        let description = ServiceDescription {
            domain: domain_str.clone(),
            service: service_str.clone(),
            name: None,
            description: None,
            schema: schema_json,
            target: None,
            supports_response: supports_response_enum,
        };

        // Register with the Rust ServiceRegistry using an async handler that calls the Python function
        self.services
            .register_with_description(description, move |call: ServiceCall| {
                // Clone handler for the async block - acquire GIL to clone
                let py_handler_clone = Python::with_gil(|py| py_handler.clone_ref(py));

                async move {
                    // Run the Python callback in a blocking task to avoid holding the GIL
                    // across await points
                    let spawn_result = tokio::task::spawn_blocking(move || {
                        Python::with_gil(|py| {
                            // Convert ServiceCall to Python dict
                            let call_dict = PyDict::new_bound(py);
                            call_dict.set_item("domain", &call.domain)?;
                            call_dict.set_item("service", &call.service)?;

                            // Convert service_data to Python
                            let data = super::util::json_to_py(py, &call.service_data)?;
                            call_dict.set_item("data", data)?;

                            // Add context
                            let ctx_dict = PyDict::new_bound(py);
                            ctx_dict.set_item("id", &call.context.id)?;
                            ctx_dict.set_item("user_id", &call.context.user_id)?;
                            ctx_dict.set_item("parent_id", &call.context.parent_id)?;
                            call_dict.set_item("context", ctx_dict)?;

                            // Call the Python handler
                            let result = py_handler_clone.call1(py, (call_dict,))?;

                            // Convert result back to JSON if not None
                            if result.is_none(py) {
                                Ok(None)
                            } else {
                                let json_val = py_to_json(result.bind(py))?;
                                Ok(Some(json_val))
                            }
                        })
                        .map_err(|e: PyErr| e.to_string())
                    })
                    .await;

                    match spawn_result {
                        Ok(Ok(val)) => Ok(val),
                        Ok(Err(e)) => Err(ha_service_registry::ServiceError::CallFailed(e)),
                        Err(e) => Err(ha_service_registry::ServiceError::CallFailed(e.to_string())),
                    }
                }
            });

        // Return completed future
        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (py.None(),))?;
        Ok(future)
    }

    /// Check if a service exists
    fn has_service(&self, domain: &str, service: &str) -> bool {
        self.services.has_service(domain, service)
    }

    /// Get all services for a domain
    fn services(&self, py: Python<'_>, domain: &str) -> PyResult<PyObject> {
        let services = self.services.domain_services(domain);
        let dict = PyDict::new_bound(py);
        for desc in services {
            let service_dict = PyDict::new_bound(py);
            service_dict.set_item("name", &desc.name)?;
            service_dict.set_item("description", &desc.description)?;
            dict.set_item(&desc.service, service_dict)?;
        }
        Ok(dict.into())
    }

    /// Get all registered services grouped by domain
    fn async_services<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let all = self.services.all_services();
        let result = PyDict::new_bound(py);

        for (domain, services) in all {
            let domain_dict = PyDict::new_bound(py);
            for desc in services {
                let service_dict = PyDict::new_bound(py);
                service_dict.set_item("name", &desc.name)?;
                service_dict.set_item("description", &desc.description)?;
                domain_dict.set_item(&desc.service, service_dict)?;
            }
            result.set_item(domain, domain_dict)?;
        }

        // Return as completed future
        let asyncio = py.import_bound("asyncio")?;
        let future = asyncio.call_method0("Future")?;
        future.call_method1("set_result", (result,))?;
        Ok(future)
    }
}
