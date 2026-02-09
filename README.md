# Home Assistant Rust

A Rust implementation of Home Assistant's core, designed as a drop-in replacement that:

- Loads existing HA configurations unchanged
- Runs existing Python integrations via embedded interpreter
- Presents identical APIs (REST, WebSocket) to frontends
- Provides better performance and lower memory usage

## Status

**Target HA Version**: 2026.1.1

| Component | Status |
|-----------|--------|
| Core (EventBus, StateStore, ServiceRegistry) | ✅ |
| Configuration (YAML, !include, !secret) | ✅ |
| Registries (Entity, Device, Area, Floor, Label) | ✅ |
| Template Engine (Jinja2-compatible) | ✅ |
| Config Entries (with FSM lifecycle) | ✅ |
| Automation & Script Engine | ✅ |
| REST API | ✅ |
| WebSocket API | ✅ |
| Frontend Serving | ✅ |
| Config Flows (via Python bridge) | ✅ |
| Python Shim Layer (ModuleRegistry) | ✅ |
| Auto-install Integration Dependencies | ✅ |
| Authentication | 🔶 (OAuth2 works, tokens in-memory) |
| Python Integration Entity Setup | 🚧 |

## Quick Start

### Prerequisites

```bash
# Clone with submodules
git clone --recursive https://github.com/yon/homeassistant-rs.git
cd homeassistant-rs

# Setup Python environment with all dependencies
make ha-compat-setup
```

### Build

```bash
make build          # Debug build
make build-release  # Release build
```

### Run

```bash
make run  # or: make run-release for optimized build
```

Or manually:
```bash
PYTHONPATH="$(pwd)/crates/ha-py-bridge/python:$(pwd)/.venv/lib/python3.13/site-packages" \
  HA_CONFIG_DIR="$(pwd)/tests/config" \
  HA_FRONTEND_PATH="$(pwd)/.venv/lib/python3.13/site-packages/hass_frontend" \
  ./target/debug/homeassistant
```

Open http://localhost:8123

### Testing with a Production Backup

You can run the Rust server against a real Home Assistant backup to verify integration loading and frontend behavior.

1. **Restore a backup** into a local directory (e.g. via HA's backup download):
   ```bash
   mkdir -p /tmp/ha-config && cd /tmp/ha-config
   tar xzf ~/Downloads/your-ha-backup.tar.gz
   # The backup extracts a data/ subdirectory containing configuration.yaml, .storage/, etc.
   ```

2. **Create an integration allowlist** to control which Python integrations load:
   ```bash
   cat > /tmp/ha-config/data/ha_python_integration_allowlist.yaml <<'EOF'
   integrations:
     - sun
     - uptime
     # Add your integrations here
   EOF
   ```

3. **Run the server** pointing at the `data/` subdirectory (not the parent):
   ```bash
   CONFIG_DIR=/tmp/ha-config/data make run-release
   ```
   This sets `PYTHONPATH`, `HA_FRONTEND_PATH`, and `HA_COMPONENTS_PATH` automatically via the Makefile. To use a different port:
   ```bash
   CONFIG_DIR=/tmp/ha-config/data HA_PORT=8124 make run-release
   ```

4. Open http://localhost:8123 (or your custom port). The vendored frontend from the `hass_frontend` pip package is served automatically.

**Important**: `HA_CONFIG_DIR` must point to the directory containing `configuration.yaml` and `.storage/`. In a backup, that's the `data/` subdirectory — not the backup root.

### Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `PYTHONPATH` | Paths for embedded Python interpreter | - |
| `HA_CONFIG_DIR` | Configuration directory | `./config` |
| `HA_FRONTEND_PATH` | Path to hass_frontend package | - |
| `HA_COMPONENTS_PATH` | Path to HA Python components | - |
| `HA_PORT` | Server port | `8123` |

## Development

### Testing

```bash
make test              # Run all Rust tests
make python-test       # Build wheel and run pytest
make ha-compat-test    # Run HA compatibility tests (76/77 passing)
make dev               # Run all dev checks (fmt, clippy, test)
```

### Project Structure

```
crates/
├── ha-api/               # REST + WebSocket API (axum)
├── ha-automation/        # Trigger-Condition-Action engine
├── ha-components/        # Built-in components (persistent_notification, system_log, input_*)
├── ha-config/            # YAML loading, !include, !secret
├── ha-config-entries/    # ConfigEntry lifecycle with FSM
├── ha-core/              # Core types (EntityId, State, Event, Context)
├── ha-event-bus/         # Pub/sub event system
├── ha-py-bridge/         # PyO3 bridge and Python shim layer
├── ha-recorder/          # SQLite history storage
├── ha-registries/        # Entity/Device/Area/Floor/Label registries
├── ha-script/            # Script executor
├── ha-server/            # Main binary
├── ha-service-registry/  # Service registration and dispatch
├── ha-state-store/       # Entity state management with domain indexing
├── ha-template/          # Jinja2-compatible templates (minijinja)
└── ha-test-comparison/   # Comparison test infrastructure
```

## Architecture

The server runs as a standalone Rust binary with an embedded Python interpreter for loading existing Home Assistant integrations.

```
┌─────────────────────────────────────────────────────────────┐
│                    Rust Server (ha-server)                  │
├─────────────────────────────────────────────────────────────┤
│  Frontend Serving     │  REST API         │  WebSocket API  │
├─────────────────────────────────────────────────────────────┤
│  EventBus  │  StateStore    │  ServiceRegistry  │  Config   │
├─────────────────────────────────────────────────────────────┤
│                 Python Bridge (PyO3)                        │
│  Loads integrations from homeassistant.components.*         │
└─────────────────────────────────────────────────────────────┘
```

## License

See [LICENSE](LICENSE) for details.
