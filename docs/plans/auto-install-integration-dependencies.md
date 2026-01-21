# Plan: Auto-Install Integration Dependencies

## Overview

Implement automatic installation of Python dependencies when loading integrations, matching Home Assistant's native behavior. When a user enables an integration (e.g., `google`), automatically install its required packages (e.g., `gcal-sync`) if not already present.

## Background

Home Assistant declares dependencies in each integration's `manifest.json`:
```json
{
  "domain": "google",
  "name": "Google Calendar",
  "requirements": ["gcal-sync==9.0.0"]
}
```

Native HA installs these on-demand via `homeassistant/requirements.py` and `homeassistant/util/package.py`.

## Implementation

### Phase 1: Read Integration Requirements

**File**: `crates/ha-py-bridge/src/py_bridge/integration.rs`

1. Add function to read `manifest.json` for an integration:
   ```rust
   pub fn get_integration_requirements(domain: &str) -> Result<Vec<String>, Error> {
       // Path: homeassistant/components/{domain}/manifest.json
       // Parse JSON and extract "requirements" array
   }
   ```

2. Call this when loading an integration (in `IntegrationLoader`)

### Phase 2: Check If Package Is Installed

**File**: `crates/ha-py-bridge/src/py_bridge/requirements.rs` (new)

1. Use Python's `importlib` to check if a package is installed:
   ```rust
   fn is_package_installed(py: Python, package: &str) -> bool {
       // Parse package name from requirement string (e.g., "gcal-sync==9.0.0" -> "gcal_sync")
       // Try: importlib.util.find_spec(package_name)
       // Returns true if spec is not None
   }
   ```

2. Handle package name normalization:
   - `gcal-sync` → `gcal_sync` (hyphens to underscores)
   - Strip version specifiers (`==`, `>=`, etc.)

### Phase 3: Install Missing Packages

**File**: `crates/ha-py-bridge/src/py_bridge/requirements.rs`

1. Add package installation function:
   ```rust
   fn install_package(package: &str, constraints_file: Option<&Path>) -> Result<(), Error> {
       // Run: python -m pip install --quiet {package}
       // Or: python -m uv pip install {package} (if uv available)
       // With optional --constraint flag
   }
   ```

2. Run pip as subprocess (not via PyO3) to avoid GIL issues:
   ```rust
   use std::process::Command;

   let output = Command::new(&python_path)
       .args(["-m", "pip", "install", "--quiet", package])
       .output()?;
   ```

3. Handle timeouts (default 60s per package)

### Phase 4: RequirementsManager

**File**: `crates/ha-py-bridge/src/py_bridge/requirements.rs`

Create a manager to orchestrate installation with caching:

```rust
pub struct RequirementsManager {
    /// Cache of installed packages (avoid repeated checks)
    installed_cache: HashSet<String>,
    /// Packages that failed to install (avoid retry loops)
    failed_packages: HashSet<String>,
    /// Path to constraints file (optional)
    constraints_file: Option<PathBuf>,
    /// Python executable path
    python_path: PathBuf,
}

impl RequirementsManager {
    /// Ensure all requirements for an integration are installed
    pub async fn ensure_requirements(&mut self, domain: &str) -> Result<(), Error> {
        let requirements = get_integration_requirements(domain)?;

        for req in requirements {
            if self.installed_cache.contains(&req) {
                continue;
            }
            if self.failed_packages.contains(&req) {
                return Err(Error::RequirementsFailed(domain, vec![req]));
            }

            if !self.is_installed(&req)? {
                self.install(&req)?;
            }

            self.installed_cache.insert(req);
        }
        Ok(())
    }
}
```

### Phase 5: Integration with Config Flow

**File**: `crates/ha-py-bridge/src/py_bridge/config_flow.rs`

1. Before importing a config flow module, ensure requirements are installed:
   ```rust
   // In create_flow_instance() or start_flow()
   self.requirements_manager.ensure_requirements(handler).await?;

   // Then proceed with module import
   let module = py.import_bound(module_path)?;
   ```

2. Return user-friendly error if installation fails:
   ```json
   {
     "message": "Failed to install required packages for google: gcal-sync",
     "error_code": "requirements_failed"
   }
   ```

### Phase 6: Configuration Options

**File**: `crates/ha-py-bridge/src/py_bridge/mod.rs`

Add configuration to control behavior:

```rust
pub struct PyBridgeConfig {
    /// Skip pip installation (for environments where it's not allowed)
    pub skip_pip: bool,
    /// Custom constraints file path
    pub constraints_file: Option<PathBuf>,
    /// Installation timeout in seconds
    pub pip_timeout: u64,
    /// Directory for installed packages (if not using venv)
    pub deps_dir: Option<PathBuf>,
}
```

Environment variables:
- `HA_SKIP_PIP=1` - Disable auto-installation
- `HA_PIP_TIMEOUT=120` - Custom timeout
- `HA_DEPS_DIR=/path/to/deps` - Custom installation directory

## File Changes Summary

| File | Changes |
|------|---------|
| `crates/ha-py-bridge/src/py_bridge/requirements.rs` | New file - RequirementsManager |
| `crates/ha-py-bridge/src/py_bridge/integration.rs` | Add `get_integration_requirements()` |
| `crates/ha-py-bridge/src/py_bridge/config_flow.rs` | Call `ensure_requirements()` before loading |
| `crates/ha-py-bridge/src/py_bridge/mod.rs` | Add RequirementsManager to PyBridge |

## Error Handling

1. **Package not found on PyPI**: Log error, add to failed set, return error to user
2. **Network timeout**: Retry once, then fail
3. **Permission denied**: Suggest using virtual environment or setting deps_dir
4. **Version conflict**: Log warning, attempt install anyway (pip will resolve)

## Testing

1. **Unit tests**:
   - `test_parse_requirement_string()` - version parsing
   - `test_is_package_installed()` - importlib check
   - `test_requirements_manager_caching()` - cache behavior

2. **Integration tests**:
   - Start flow for integration with missing dependency
   - Verify package is installed
   - Verify subsequent loads use cache
   - Verify failed packages are tracked

3. **Manual testing**:
   ```bash
   # Remove gcal-sync if installed
   pip uninstall gcal-sync -y

   # Start server and try google integration
   make run
   # Navigate to Settings > Integrations > Add > Google Calendar
   # Should auto-install gcal-sync and show config form
   ```

## Future Enhancements

1. **Constraints file**: Bundle `package_constraints.txt` from HA core for version compatibility
2. **Progress reporting**: WebSocket events for installation progress
3. **Rollback**: Track installed packages per session, allow rollback on failure
4. **Parallel installation**: Install multiple packages concurrently
5. **Use uv**: Prefer `uv pip` over `pip` for faster installs (if available)

## References

- Native HA implementation: `vendor/ha-core/homeassistant/requirements.py`
- Package installation: `vendor/ha-core/homeassistant/util/package.py`
- Example manifest: `vendor/ha-core/homeassistant/components/google/manifest.json`
