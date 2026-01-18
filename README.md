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
| Core (EventBus, StateMachine, ServiceRegistry) | ✅ |
| Configuration (YAML, !include, !secret) | ✅ |
| Registries (Entity, Device, Area, Floor, Label) | ✅ |
| Template Engine (Jinja2-compatible) | ✅ |
| Config Entries | ✅ |
| Automation & Script Engine | ✅ |
| REST API | ✅ |
| WebSocket API | ✅ |
| Frontend Serving | ✅ |
| Python Integration Loading | ✅ |
| Authentication | 🚧 |

## Quick Start

### Prerequisites

```bash
# Clone with submodules
git clone --recursive https://github.com/yon/homeassistant-rs.git
cd homeassistant-rs

# Create Python venv
python3 -m venv .venv
.venv/bin/pip install home-assistant-frontend
.venv/bin/pip install -e vendor/ha-core
```

### Build

```bash
PYO3_PYTHON=$(pwd)/.venv/bin/python cargo build -p ha-server --features python
```

### Run

```bash
PYTHONPATH="$(pwd)/.venv/lib/python3.13/site-packages:$(pwd)/vendor/ha-core" \
  HA_CONFIG_DIR="$(pwd)/tests/config" \
  HA_FRONTEND_PATH="$(pwd)/.venv/lib/python3.13/site-packages/hass_frontend" \
  ./target/debug/homeassistant
```

Open http://localhost:8123

### Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `PYTHONPATH` | Paths for embedded Python interpreter | - |
| `HA_CONFIG_DIR` | Configuration directory | `./config` |
| `HA_FRONTEND_PATH` | Path to hass_frontend package | - |
| `HA_PORT` | Server port | `8123` |

## Development

### Testing

```bash
# Run all Rust tests
cargo test --workspace --exclude ha-core-rs

# Run tests with Python support
PYO3_PYTHON=$(pwd)/.venv/bin/python cargo test -p ha-core-rs --features fallback --no-default-features --lib

# Run HA compatibility tests
.venv/bin/python tests/ha_compat/run_tests.py --all -v
```

### Project Structure

```
crates/
├── ha-core/              # Core types (EntityId, State, Event, Context)
├── ha-event-bus/         # Pub/sub event system
├── ha-state-machine/     # Entity state management
├── ha-service-registry/  # Service registration and dispatch
├── ha-config/            # YAML loading, !include, !secret
├── ha-config-entries/    # ConfigEntry lifecycle
├── ha-registries/        # Entity/Device/Area/Floor/Label registries
├── ha-template/          # Jinja2-compatible templates (minijinja)
├── ha-automation/        # Trigger-Condition-Action engine
├── ha-script/            # Script executor
├── ha-core-rs/           # PyO3 bridge for Python integrations
├── ha-api/               # REST + WebSocket API (axum)
├── ha-server/            # Main binary
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
│  EventBus  │  StateMachine  │  ServiceRegistry  │  Config   │
├─────────────────────────────────────────────────────────────┤
│                 Python Bridge (PyO3)                        │
│  Loads integrations from homeassistant.components.*         │
└─────────────────────────────────────────────────────────────┘
```

## License

See [LICENSE](LICENSE) for details.
