//! Python runtime management for embedded interpreter

use super::errors::{PyBridgeError, PyBridgeResult};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;
use std::sync::OnceLock;
use tracing::{debug, info, warn};

/// Global Python runtime instance
static PYTHON_RUNTIME: OnceLock<PythonRuntime> = OnceLock::new();

/// Manages the embedded Python interpreter
pub struct PythonRuntime {
    /// Path to Home Assistant Python installation
    #[allow(dead_code)]
    ha_path: Option<String>,
    /// Whether the runtime is initialized
    #[allow(dead_code)]
    initialized: bool,
}

impl PythonRuntime {
    /// Get or initialize the global Python runtime
    pub fn get() -> &'static PythonRuntime {
        PYTHON_RUNTIME.get_or_init(|| PythonRuntime {
            ha_path: None,
            initialized: false,
        })
    }

    /// Initialize the Python runtime with Home Assistant path
    pub fn initialize(ha_path: Option<&Path>) -> PyBridgeResult<()> {
        // pyo3 with auto-initialize handles Python initialization
        Python::with_gil(|py| {
            let sys = py.import_bound("sys")?;
            let sys_path = sys.getattr("path")?;

            // Add PYTHONPATH entries to sys.path (embedded Python doesn't auto-load these)
            if let Ok(pythonpath) = std::env::var("PYTHONPATH") {
                for path in pythonpath.split(':') {
                    if !path.is_empty() {
                        sys_path.call_method1("insert", (0, path))?;
                        debug!("Added PYTHONPATH entry to sys.path: {}", path);
                    }
                }
            }

            // Add Home Assistant path to sys.path if provided
            if let Some(path) = ha_path {
                sys_path.call_method1("insert", (0, path.to_string_lossy().as_ref()))?;
                info!("Added Home Assistant path to sys.path: {:?}", path);
            }

            // Verify we can import homeassistant
            match py.import_bound("homeassistant") {
                Ok(_) => {
                    info!("Home Assistant Python package found");
                    Ok(())
                }
                Err(e) => {
                    warn!("Home Assistant Python package not found: {}", e);
                    // Not a fatal error - we can still run without Python HA
                    Ok(())
                }
            }
        })
    }

    /// Execute Python code and return the result
    pub fn exec<F, T>(&self, f: F) -> PyBridgeResult<T>
    where
        F: FnOnce(Python<'_>) -> PyResult<T>,
    {
        Python::with_gil(|py| f(py).map_err(PyBridgeError::from))
    }

    /// Import a Python module
    pub fn import_module(&self, name: &str) -> PyBridgeResult<()> {
        Python::with_gil(|py| {
            py.import_bound(name)?;
            debug!("Imported Python module: {}", name);
            Ok(())
        })
    }

    /// Check if a Python module is available
    pub fn has_module(&self, name: &str) -> bool {
        Python::with_gil(|py| py.import_bound(name).is_ok())
    }

    /// Get Python version info
    pub fn python_version(&self) -> PyBridgeResult<String> {
        Python::with_gil(|py| {
            let sys = py.import_bound("sys")?;
            let version = sys.getattr("version")?;
            Ok(version.to_string())
        })
    }

    /// Get the path to the Python executable
    ///
    /// When Python is embedded, sys.executable returns the embedding binary,
    /// not the Python interpreter. We need to find the actual Python interpreter
    /// that was used to build PyO3.
    pub fn python_executable(&self) -> PyBridgeResult<std::path::PathBuf> {
        // First try PYO3_PYTHON environment variable (set at build time or runtime)
        if let Ok(path) = std::env::var("PYO3_PYTHON") {
            debug!("Using PYO3_PYTHON from environment: {}", path);
            return Ok(std::path::PathBuf::from(path));
        }

        // When Python is embedded via PyO3, sys.prefix points to the base Python
        // installation, not our venv. Look for venv paths in PYTHONPATH instead.
        if let Ok(pythonpath) = std::env::var("PYTHONPATH") {
            for path in pythonpath.split(':') {
                // Look for paths ending with site-packages (venv structure)
                if path.ends_with("site-packages") {
                    // site-packages is at .venv/lib/python3.x/site-packages
                    // We want .venv/bin/python3
                    if let Some(venv_path) = std::path::Path::new(path)
                        .parent() // lib/python3.x
                        .and_then(|p| p.parent()) // lib
                        .and_then(|p| p.parent())
                    // .venv
                    {
                        let candidates = [
                            venv_path.join("bin/python3"),
                            venv_path.join("bin/python"),
                            venv_path.join("Scripts/python.exe"), // Windows
                        ];

                        for candidate in &candidates {
                            if candidate.exists() {
                                debug!(
                                    "Found Python executable from PYTHONPATH at: {:?}",
                                    candidate
                                );
                                return Ok(candidate.to_path_buf());
                            }
                        }
                    }
                }
            }
        }

        // Fallback: try sys.prefix (works when not in embedded mode)
        Python::with_gil(|py| {
            let sys = py.import_bound("sys")?;

            // Get the prefix (e.g., /path/to/.venv)
            let prefix: String = sys.getattr("prefix")?.extract()?;
            let prefix_path = std::path::PathBuf::from(&prefix);

            // Try common Python executable locations
            let candidates = [
                prefix_path.join("bin/python3"),
                prefix_path.join("bin/python"),
                prefix_path.join("Scripts/python.exe"), // Windows
            ];

            for candidate in &candidates {
                if candidate.exists() {
                    debug!("Found Python executable at: {:?}", candidate);
                    return Ok(candidate.clone());
                }
            }

            // Fallback: use sys.executable (may not work for embedded Python)
            let executable: String = sys.getattr("executable")?.extract()?;
            warn!(
                "No Python found in prefix '{}', falling back to sys.executable: {}",
                prefix, executable
            );
            Ok(std::path::PathBuf::from(executable))
        })
    }

    /// Create a new Python dict
    pub fn new_dict<'py>(&self, py: Python<'py>) -> Bound<'py, PyDict> {
        PyDict::new_bound(py)
    }
}

/// RAII guard for Python GIL
#[allow(dead_code)]
pub struct GilGuard<'py> {
    py: Python<'py>,
}

#[allow(dead_code)]
impl<'py> GilGuard<'py> {
    /// Get Python interpreter reference
    pub fn python(&self) -> Python<'py> {
        self.py
    }
}

/// Helper to run code with the GIL held
pub fn with_gil<F, T>(f: F) -> PyBridgeResult<T>
where
    F: FnOnce(Python<'_>) -> PyResult<T>,
{
    Python::with_gil(|py| f(py).map_err(PyBridgeError::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_version() {
        let runtime = PythonRuntime::get();
        let version = runtime.python_version().unwrap();
        assert!(version.starts_with("3."));
    }

    #[test]
    fn test_has_module() {
        let runtime = PythonRuntime::get();
        assert!(runtime.has_module("sys"));
        assert!(runtime.has_module("os"));
        assert!(!runtime.has_module("nonexistent_module_12345"));
    }

    #[test]
    fn test_with_gil() {
        let result = with_gil(|py| {
            let sys = py.import_bound("sys")?;
            let platform = sys.getattr("platform")?;
            Ok(platform.to_string())
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_sys_path_can_be_modified() {
        // This test verifies that we can add entries to sys.path via Python,
        // which is the mechanism used by initialize() to add PYTHONPATH entries

        let test_path = "/test/unique/path/98765";

        let result = with_gil(|py| {
            let sys = py.import_bound("sys")?;
            let sys_path = sys.getattr("path")?;

            // Add the path
            sys_path.call_method1("insert", (0, test_path))?;

            // Verify it was added
            let path_str = sys_path.to_string();
            Ok(path_str.contains(test_path))
        });

        assert!(result.unwrap(), "Should be able to add entries to sys.path");
    }

    #[test]
    fn test_initialize_with_ha_path() {
        // Test that initialize() adds the ha_path to sys.path
        let test_path = std::path::Path::new("/test/ha/path/54321");

        // Call initialize with a specific path
        let result = PythonRuntime::initialize(Some(test_path));
        assert!(result.is_ok());

        // Verify the path is in sys.path
        let found = with_gil(|py| {
            let sys = py.import_bound("sys")?;
            let sys_path = sys.getattr("path")?;
            let path_str = sys_path.to_string();
            Ok(path_str.contains("/test/ha/path/54321"))
        });

        assert!(found.unwrap(), "ha_path should be added to sys.path");
    }
}
