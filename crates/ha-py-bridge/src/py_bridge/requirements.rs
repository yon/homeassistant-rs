//! Requirements Manager
//!
//! Manages installation of Python dependencies for integrations.
//! When an integration is loaded, this module ensures all required
//! packages from the integration's manifest are installed.

use pyo3::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use tracing::{debug, info, warn};

/// Manages Python package requirements for integrations
pub struct RequirementsManager {
    /// Cache of packages we've verified are installed (avoids repeated checks)
    installed_cache: Mutex<HashSet<String>>,
    /// Packages that failed to install (avoid retry loops)
    failed_packages: Mutex<HashSet<String>>,
    /// Path to Python executable
    python_path: PathBuf,
    /// Whether to skip pip installation (for environments where it's not allowed)
    skip_pip: bool,
}

impl RequirementsManager {
    /// Create a new RequirementsManager
    pub fn new(python_path: PathBuf) -> Self {
        let skip_pip = std::env::var("HA_SKIP_PIP")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        Self {
            installed_cache: Mutex::new(HashSet::new()),
            failed_packages: Mutex::new(HashSet::new()),
            python_path,
            skip_pip,
        }
    }

    /// Ensure all requirements for an integration are installed
    ///
    /// Returns Ok(()) if all requirements are satisfied, or an error describing
    /// which packages failed to install.
    pub fn ensure_requirements(&self, domain: &str, requirements: &[String]) -> Result<(), String> {
        if requirements.is_empty() {
            return Ok(());
        }

        debug!(
            "Checking {} requirements for integration '{}' using python: {:?}",
            requirements.len(),
            domain,
            self.python_path
        );

        let mut missing: Vec<String> = Vec::new();

        for req in requirements {
            // Check cache first
            {
                let cache = self.installed_cache.lock().unwrap();
                if cache.contains(req) {
                    continue;
                }
            }

            // Check failed cache
            {
                let failed = self.failed_packages.lock().unwrap();
                if failed.contains(req) {
                    return Err(format!(
                        "Package '{}' previously failed to install for integration '{}'",
                        req, domain
                    ));
                }
            }

            // Check if package is installed
            let package_name = parse_package_name(req);
            let installed = self.is_package_installed(&package_name);
            debug!(
                "Package '{}' (from requirement '{}') installed: {}",
                package_name, req, installed
            );
            if !installed {
                missing.push(req.clone());
            } else {
                // Add to cache
                let mut cache = self.installed_cache.lock().unwrap();
                cache.insert(req.clone());
            }
        }

        if missing.is_empty() {
            return Ok(());
        }

        if self.skip_pip {
            return Err(format!(
                "Missing packages for integration '{}': {:?} (pip installation disabled via HA_SKIP_PIP)",
                domain, missing
            ));
        }

        // Install missing packages
        info!(
            "Installing {} missing packages for integration '{}': {:?}",
            missing.len(),
            domain,
            missing
        );

        for req in &missing {
            match self.install_package(req) {
                Ok(()) => {
                    let mut cache = self.installed_cache.lock().unwrap();
                    cache.insert(req.clone());
                }
                Err(e) => {
                    warn!("Failed to install package '{}': {}", req, e);
                    let mut failed = self.failed_packages.lock().unwrap();
                    failed.insert(req.clone());
                    return Err(format!(
                        "Failed to install package '{}' for integration '{}': {}",
                        req, domain, e
                    ));
                }
            }
        }

        Ok(())
    }

    /// Check if a package is installed using pip
    ///
    /// We can't use importlib.util.find_spec because it might find
    /// similarly-named folders in PYTHONPATH (e.g., integration folders).
    fn is_package_installed(&self, package_name: &str) -> bool {
        // Use pip show to check if the package is actually installed via pip
        debug!(
            "Checking if package '{}' is installed using: {:?} -m pip show --quiet {}",
            package_name, self.python_path, package_name
        );
        let output = Command::new(&self.python_path)
            .args(["-m", "pip", "show", "--quiet", package_name])
            .output();

        match output {
            Ok(result) => {
                let installed = result.status.success();
                debug!(
                    "pip show '{}' returned: {} (exit code: {:?})",
                    package_name,
                    if installed {
                        "installed"
                    } else {
                        "not installed"
                    },
                    result.status.code()
                );
                installed
            }
            Err(e) => {
                warn!("Failed to run pip show for '{}': {}", package_name, e);
                false
            }
        }
    }

    /// Install a package using pip
    fn install_package(&self, requirement: &str) -> Result<(), String> {
        info!("Installing package: {}", requirement);

        // Try uv pip first (faster), fall back to pip
        let result = if self.has_uv() {
            self.install_with_uv(requirement)
        } else {
            self.install_with_pip(requirement)
        };

        // Invalidate Python's import caches so newly installed packages are found
        if result.is_ok() {
            self.invalidate_import_caches();
        }

        result
    }

    /// Invalidate Python's import caches after installing packages
    fn invalidate_import_caches(&self) {
        Python::with_gil(|py| {
            if let Ok(importlib) = py.import_bound("importlib") {
                let _ = importlib.call_method0("invalidate_caches");
                debug!("Invalidated Python import caches");
            }
        });
    }

    /// Check if uv is available
    fn has_uv(&self) -> bool {
        Command::new("uv")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Install using uv pip (faster)
    fn install_with_uv(&self, requirement: &str) -> Result<(), String> {
        debug!("Installing {} with uv", requirement);

        let output = Command::new("uv")
            .args(["pip", "install", "--quiet", requirement])
            .output()
            .map_err(|e| format!("Failed to run uv: {}", e))?;

        if output.status.success() {
            info!("Successfully installed {} with uv", requirement);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("uv pip install failed: {}", stderr))
        }
    }

    /// Install using pip
    fn install_with_pip(&self, requirement: &str) -> Result<(), String> {
        debug!("Installing {} with pip", requirement);

        let output = Command::new(&self.python_path)
            .args(["-m", "pip", "install", "--quiet", requirement])
            .output()
            .map_err(|e| format!("Failed to run pip: {}", e))?;

        if output.status.success() {
            info!("Successfully installed {} with pip", requirement);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("pip install failed: {}", stderr))
        }
    }
}

/// Parse the package name from a requirement string
///
/// Examples:
/// - "accuweather==5.0.0" -> "accuweather"
/// - "aiohue>=4.5.0,<5.0" -> "aiohue"
/// - "somepackage[extra]>=1.0" -> "somepackage"
fn parse_package_name(requirement: &str) -> String {
    // Find the first occurrence of version specifier or extras
    let name = requirement
        .split(|c| c == '=' || c == '>' || c == '<' || c == '[' || c == '!' || c == '~')
        .next()
        .unwrap_or(requirement);

    name.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_package_name() {
        assert_eq!(parse_package_name("accuweather==5.0.0"), "accuweather");
        assert_eq!(parse_package_name("aiohue>=4.5.0"), "aiohue");
        assert_eq!(parse_package_name("somepackage[extra]>=1.0"), "somepackage");
        assert_eq!(parse_package_name("package"), "package");
        assert_eq!(parse_package_name("my-package>=1.0,<2.0"), "my-package");
        assert_eq!(parse_package_name("pkg!=1.0"), "pkg");
        assert_eq!(parse_package_name("pkg~=1.0"), "pkg");
    }

    #[test]
    fn test_requirements_manager_creation() {
        let manager = RequirementsManager::new(PathBuf::from("/usr/bin/python3"));
        assert!(!manager.skip_pip);
    }

    #[test]
    fn test_empty_requirements() {
        let manager = RequirementsManager::new(PathBuf::from("/usr/bin/python3"));
        assert!(manager.ensure_requirements("test", &[]).is_ok());
    }
}
