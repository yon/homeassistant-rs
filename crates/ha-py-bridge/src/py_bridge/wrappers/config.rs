//! ConfigWrapper - configuration data for Python

use super::unit_system::UnitSystemWrapper;
use pyo3::prelude::*;
use pyo3::types::{PySet, PyTuple};

/// Python wrapper for Home Assistant configuration
#[pyclass(name = "ConfigWrapper")]
pub struct ConfigWrapper {
    #[pyo3(get)]
    pub config_dir: String,
    #[pyo3(get)]
    pub latitude: f64,
    #[pyo3(get)]
    pub longitude: f64,
    #[pyo3(get)]
    pub elevation: i32,
    #[pyo3(get)]
    pub time_zone: String,
    #[pyo3(get)]
    pub location_name: String,
    #[pyo3(get)]
    pub internal_url: Option<String>,
    #[pyo3(get)]
    pub external_url: Option<String>,
    #[pyo3(get)]
    pub recovery_mode: bool,
    #[pyo3(get)]
    pub safe_mode: bool,
    #[pyo3(get)]
    pub language: String,
    #[pyo3(get)]
    pub country: Option<String>,
    #[pyo3(get)]
    pub currency: String,
    #[pyo3(get)]
    pub skip_pip: bool,
    #[pyo3(get)]
    pub skip_pip_packages: Vec<String>,
    components: Py<PySet>,
    units: Py<UnitSystemWrapper>,
}

impl ConfigWrapper {
    pub fn new(py: Python<'_>) -> PyResult<Self> {
        let units = Py::new(py, UnitSystemWrapper::metric())?;
        Ok(Self {
            config_dir: "/config".to_string(),
            latitude: 32.87336,
            longitude: -117.22743,
            elevation: 0,
            time_zone: "UTC".to_string(),
            location_name: "Home".to_string(),
            internal_url: None,
            external_url: None,
            recovery_mode: false,
            safe_mode: false,
            language: "en".to_string(),
            country: None,
            currency: "EUR".to_string(),
            skip_pip: true, // Skip pip installs since we assume deps are pre-installed
            skip_pip_packages: Vec::new(),
            components: PySet::empty_bound(py)?.unbind(),
            units,
        })
    }
}

#[pymethods]
impl ConfigWrapper {
    #[getter]
    fn components(&self, py: Python<'_>) -> PyResult<Py<PySet>> {
        Ok(self.components.clone_ref(py))
    }

    #[getter]
    fn units(&self, py: Python<'_>) -> PyResult<Py<UnitSystemWrapper>> {
        Ok(self.units.clone_ref(py))
    }

    // Alias for backwards compatibility
    #[getter]
    fn unit_system(&self, py: Python<'_>) -> PyResult<Py<UnitSystemWrapper>> {
        Ok(self.units.clone_ref(py))
    }

    /// Return path to the config directory or a path within it
    ///
    /// If called with no arguments, returns the config directory.
    /// If called with a relative path, returns the joined path.
    #[pyo3(signature = (*args))]
    fn path(&self, args: &Bound<'_, PyTuple>) -> PyResult<String> {
        if args.is_empty() {
            Ok(self.config_dir.clone())
        } else {
            // Join all path segments
            let mut path = std::path::PathBuf::from(&self.config_dir);
            for arg in args.iter() {
                let segment: String = arg.extract()?;
                path.push(segment);
            }
            Ok(path.to_string_lossy().to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_wrapper() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let config = ConfigWrapper::new(py).unwrap();
            assert_eq!(config.latitude, 32.87336);
            assert_eq!(config.time_zone, "UTC");
        });
    }
}
