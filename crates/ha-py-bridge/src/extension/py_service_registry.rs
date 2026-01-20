//! Python wrapper for ServiceRegistry

use ha_core::{ServiceCall, SupportsResponse};
use ha_service_registry::{ServiceDescription, ServiceRegistry};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tokio::runtime::Handle;

use super::py_types::{json_to_py, py_to_json, PyContext};

/// Python wrapper for ServiceRegistry
#[pyclass(name = "ServiceRegistry")]
pub struct PyServiceRegistry {
    inner: Arc<ServiceRegistry>,
}

#[pymethods]
impl PyServiceRegistry {
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(ServiceRegistry::new()),
        }
    }

    /// Check if a service exists
    ///
    /// Args:
    ///     domain: The service domain
    ///     service: The service name
    ///
    /// Returns:
    ///     True if the service exists
    fn has_service(&self, domain: &str, service: &str) -> bool {
        self.inner.has_service(domain, service)
    }

    /// Get service description
    ///
    /// Args:
    ///     domain: The service domain
    ///     service: The service name
    ///
    /// Returns:
    ///     Service description dict, or None if not found
    fn get_service(
        &self,
        py: Python<'_>,
        domain: &str,
        service: &str,
    ) -> PyResult<Option<PyObject>> {
        match self.inner.get_service(domain, service) {
            Some(desc) => {
                let dict = PyDict::new_bound(py);
                dict.set_item("domain", &desc.domain)?;
                dict.set_item("service", &desc.service)?;
                dict.set_item("name", &desc.name)?;
                dict.set_item("description", &desc.description)?;
                Ok(Some(dict.into_any().unbind()))
            }
            None => Ok(None),
        }
    }

    /// Get all services for a domain
    ///
    /// Args:
    ///     domain: The domain to filter by
    ///
    /// Returns:
    ///     List of service description dicts
    fn domain_services(&self, py: Python<'_>, domain: &str) -> PyResult<Vec<PyObject>> {
        let services = self.inner.domain_services(domain);
        services
            .into_iter()
            .map(|desc| {
                let dict = PyDict::new_bound(py);
                dict.set_item("domain", &desc.domain)?;
                dict.set_item("service", &desc.service)?;
                dict.set_item("name", &desc.name)?;
                dict.set_item("description", &desc.description)?;
                Ok(dict.into_any().unbind())
            })
            .collect()
    }

    /// Get all domains that have registered services
    ///
    /// Returns:
    ///     List of domain names
    fn domains(&self) -> Vec<String> {
        self.inner.domains()
    }

    /// Get all registered services grouped by domain
    ///
    /// Returns:
    ///     Dict mapping domain names to lists of service dicts
    fn all_services(&self, py: Python<'_>) -> PyResult<PyObject> {
        let all = self.inner.all_services();
        let result = PyDict::new_bound(py);

        for (domain, services) in all {
            let service_list: Vec<PyObject> = services
                .into_iter()
                .map(|desc| {
                    let dict = PyDict::new_bound(py);
                    dict.set_item("domain", &desc.domain)?;
                    dict.set_item("service", &desc.service)?;
                    dict.set_item("name", &desc.name)?;
                    dict.set_item("description", &desc.description)?;
                    Ok(dict.into_any().unbind())
                })
                .collect::<PyResult<_>>()?;
            result.set_item(domain, service_list)?;
        }

        Ok(result.into_any().unbind())
    }

    /// Unregister a service
    ///
    /// Args:
    ///     domain: The service domain
    ///     service: The service name
    ///
    /// Returns:
    ///     True if the service was removed
    fn unregister(&self, domain: &str, service: &str) -> bool {
        self.inner.unregister(domain, service)
    }

    /// Unregister all services for a domain
    ///
    /// Args:
    ///     domain: The domain to unregister
    ///
    /// Returns:
    ///     Number of services removed
    fn unregister_domain(&self, domain: &str) -> usize {
        self.inner.unregister_domain(domain)
    }

    /// Get total number of registered services
    fn service_count(&self) -> usize {
        self.inner.service_count()
    }

    fn __repr__(&self) -> String {
        format!("ServiceRegistry(services={})", self.inner.service_count())
    }

    fn __len__(&self) -> usize {
        self.inner.service_count()
    }

    /// Register a service with a Python handler
    ///
    /// Args:
    ///     domain: The service domain (e.g., "light")
    ///     service: The service name (e.g., "turn_on")
    ///     handler: A callable that takes a ServiceCall dict and returns optional response
    ///     schema: Optional JSON schema for service data validation
    ///     supports_response: Whether the service supports responses ("none", "only", "optional")
    ///
    /// Example (Python):
    /// ```python
    /// def my_handler(call):
    ///     print(f"Called with: {call}")
    ///     return None
    ///
    /// registry.register("test", "my_service", my_handler)
    /// ```
    #[pyo3(signature = (domain, service, handler, schema=None, supports_response="none"))]
    fn register(
        &self,
        py: Python<'_>,
        domain: &str,
        service: &str,
        handler: PyObject,
        schema: Option<&Bound<'_, PyDict>>,
        supports_response: &str,
    ) -> PyResult<()> {
        // Validate the handler is callable
        if !handler.bind(py).is_callable() {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "handler must be callable",
            ));
        }

        // Parse supports_response
        let supports_response_enum = match supports_response.to_lowercase().as_str() {
            "none" => SupportsResponse::None,
            "only" => SupportsResponse::Only,
            "optional" => SupportsResponse::Optional,
            _ => {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "supports_response must be 'none', 'only', or 'optional'",
                ))
            }
        };

        // Convert schema to JSON if provided
        let schema_json = if let Some(dict) = schema {
            Some(py_to_json(dict.as_any())?)
        } else {
            None
        };

        let domain_str = domain.to_string();
        let service_str = service.to_string();

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

        // Clone the handler with GIL - this is safe because PyObject is Send+Sync
        // and the actual Python object reference counting is handled correctly
        let py_handler = handler.clone_ref(py);

        self.inner
            .register_with_description(description, move |call: ServiceCall| {
                // Clone again for the async block - we need to acquire GIL to clone
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
                            let data = json_to_py(py, &call.service_data)?;
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

        Ok(())
    }

    /// Call a service (blocking version that works like async)
    ///
    /// This is a blocking implementation that can be wrapped with asyncio.to_thread()
    /// in Python for async usage:
    ///     result = await asyncio.to_thread(registry.async_call, "light", "turn_on", {...})
    ///
    /// Args:
    ///     domain: The service domain
    ///     service: The service name
    ///     service_data: Dictionary of service data
    ///     context: Optional context for the call
    ///     blocking: Whether to wait for the service to complete (default: True)
    ///     return_response: Whether to return the service response (default: False)
    ///
    /// Returns:
    ///     The service response if return_response is True and the service supports it
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (domain, service, service_data=None, context=None, _blocking=true, return_response=false))]
    fn async_call(
        &self,
        py: Python<'_>,
        domain: &str,
        service: &str,
        service_data: Option<&Bound<'_, PyDict>>,
        context: Option<PyContext>,
        _blocking: bool,
        return_response: bool,
    ) -> PyResult<PyObject> {
        let inner = self.inner.clone();

        // Convert service_data to JSON
        let data = match service_data {
            Some(dict) => py_to_json(dict.as_any())?,
            None => serde_json::Value::Object(Default::default()),
        };

        let ctx = context.map(|c| c.into_inner()).unwrap_or_default();

        let domain = domain.to_string();
        let service = service.to_string();

        // Get the current Tokio runtime handle and block on the async call
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        // Use block_in_place to call the async function
        let result = tokio::task::block_in_place(|| {
            handle.block_on(async {
                inner
                    .call(&domain, &service, data, ctx, return_response)
                    .await
            })
        });

        match result {
            Ok(response) => match response {
                Some(val) => json_to_py(py, &val),
                None => Ok(py.None()),
            },
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                e.to_string(),
            )),
        }
    }

    /// Call a service synchronously (blocking)
    ///
    /// Args:
    ///     domain: The service domain
    ///     service: The service name
    ///     service_data: Dictionary of service data
    ///     context: Optional context for the call
    ///
    /// Returns:
    ///     None (synchronous calls don't return responses)
    #[pyo3(signature = (domain, service, service_data=None, context=None))]
    fn call(
        &self,
        domain: &str,
        service: &str,
        service_data: Option<&Bound<'_, PyDict>>,
        context: Option<PyContext>,
    ) -> PyResult<()> {
        let inner = self.inner.clone();

        // Convert service_data to JSON
        let data = match service_data {
            Some(dict) => py_to_json(dict.as_any())?,
            None => serde_json::Value::Object(Default::default()),
        };

        let ctx = context.map(|c| c.into_inner()).unwrap_or_default();

        let domain = domain.to_string();
        let service = service.to_string();

        // Get the current Tokio runtime handle and block on the async call
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        // Use block_in_place to call the async function
        let result = tokio::task::block_in_place(|| {
            handle.block_on(async { inner.call(&domain, &service, data, ctx, false).await })
        });

        result.map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        Ok(())
    }
}

impl PyServiceRegistry {
    pub fn from_arc(inner: Arc<ServiceRegistry>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<ServiceRegistry> {
        &self.inner
    }
}
