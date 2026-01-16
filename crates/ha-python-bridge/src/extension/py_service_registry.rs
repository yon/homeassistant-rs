//! Python wrapper for ServiceRegistry

use ha_service_registry::ServiceRegistry;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;

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
}

impl PyServiceRegistry {
    pub fn from_arc(inner: Arc<ServiceRegistry>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<ServiceRegistry> {
        &self.inner
    }
}
