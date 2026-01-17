# Home Assistant Rust Rewrite

## Intent

Rewrite Home Assistant's Python core in Rust for:
- **Performance**: Faster startup, lower latency, no GC pauses
- **Memory efficiency**: Critical for IoT devices and Raspberry Pi
- **Reliability**: Strong typing catches bugs at compile time
- **Maintainability**: Clear ownership model, no runtime surprises

This is NOT a fork. The goal is a drop-in replacement that:
- Loads existing HA configurations unchanged
- Runs existing Python integrations via embedded interpreter
- Presents identical APIs (REST, WebSocket) to frontends
- Maintains full backward compatibility

## Version Tracking

**Target HA Version**: 2026.1.1 (tracked in `tests/comparison/ha-versions.toml`)

Version scheme: We match HA's version exactly for compatibility:
```
Cargo.toml:     version = "2026.1.1"
pyproject.toml: version = "2026.1.1"
```

When HA releases 2026.2.0, we update both files to match.

The comparison test infrastructure (`make test-compare`) validates API compatibility against the target HA version running in Docker.

## Architecture Principles

### Dual Deployment Modes

**Mode 1: Python Extension** - Safe production testing
```
Python HA (existing) → imports ha_core_rs → Rust components
```
- Install via pip, import Rust components into existing HA
- Feature flag controls which components use Rust
- Zero risk - disable to revert to Python

**Mode 2: Standalone Binary** - Clean architecture
```
Rust HA (main) → embeds Python → runs integrations
```
- Rust is the main process
- Unimplemented components fall back to embedded Python
- Gradually remove fallbacks as coverage grows

### Core Design

1. **Pure Rust core** (no PyO3) in dedicated crates
2. **PyO3 bridge** only in `ha-python-bridge` crate
3. **Event-driven** - EventBus is the backbone
4. **Async-first** - Tokio runtime, matches HA's asyncio patterns
5. **Domain indexing** - StateMachine indexes by domain for fast lookups

## Testing Strategy

We use a three-tier testing approach where each tier serves a distinct purpose:

### Tier 1: Rust Unit Tests (Internal Logic)

Test pure Rust implementation directly. These catch:
- Memory safety issues (lifetimes, ownership)
- Async/threading bugs
- Internal API contracts
- Edge cases in Rust code

```bash
make test            # Run all Rust tests
cargo test -p ha-core  # Test specific crate
```

**What they test**: ~130 tests across crates
- `ha-core`: EntityId validation, Context, State, Event, ServiceCall (31 tests)
- `ha-event-bus`: Subscribe/fire, MATCH_ALL, typed events (6 tests)
- `ha-state-machine`: Set/get, domain indexing, state changes (7 tests)
- `ha-service-registry`: Register/call, schema validation (9 tests)
- `ha-config`: YAML loading, !include, !secret (34 tests)
- `ha-template`: Jinja2 filters, globals, state access (336 tests)

### Tier 2: Python Tests (PyO3 Bindings)

Test that PyO3 bindings expose the correct API to Python. These catch:
- Type conversion bugs (Rust ↔ Python)
- GIL-related issues
- API mismatches with Python HA
- Missing or incorrect method signatures

```bash
make python-test     # Build wheel and run pytest
```

**What they test**: 53 tests in `tests/python/test_ha_core_rs.py`
- `HomeAssistant`: Properties, component access
- `StateMachine`: set/get/remove/entity_ids
- `EventBus`: fire, async_fire, listener_count
- `ServiceRegistry`: register/unregister/call
- `EntityId`, `Context`, `State`: All properties

### Tier 3: Comparison Tests (API Compatibility)

Run identical operations against Python HA (Docker) and Rust HA, compare responses.
This validates end-to-end API compatibility.

```bash
make ha-start        # Start Python HA in Docker
make test-compare    # Run comparison tests
make ha-stop         # Stop Python HA
```

### Why All Three Tiers?

| Tier | Catches | Speed | Scope |
|------|---------|-------|-------|
| Rust unit tests | Rust bugs, memory issues | Fast (~2s) | Internal APIs |
| Python tests | Binding bugs, API shape | Medium (~5s) | PyO3 interface |
| Comparison tests | Behavior differences | Slow (~60s) | Full system |

**They are NOT redundant.** Example: A bug where `StateMachine.set()` works in Rust
but the PyO3 binding converts attributes incorrectly would:
- ✅ Pass Rust unit tests (Rust code is correct)
- ❌ Fail Python tests (binding is broken)
- ❌ Fail comparison tests (behavior differs)

### Tier 4: Running HA's Own Tests (Compatibility)

HA's pytest tests are the ultimate specification. We run them with Rust components patched in:

```bash
# Setup (one-time) - clones HA core, installs dependencies
make ha-compat-setup

# Run with Rust patching (State/Context replaced with Rust-backed wrappers)
.venv/bin/python tests/ha_compat/run_ha_tests.py "test_state" -v

# Run without patching (baseline)
.venv/bin/python tests/ha_compat/run_ha_tests.py "test_state" --no-rust -v
```

**Current status** (with Rust patching enabled):
- State tests: **28/29 pass** (97%) - 1 skipped
- Event tests: **22/22 pass** (100%)
- Service tests: **26/26 pass** (100%)
- **Combined: 76/77 pass (99%)**

The 1 skipped test (`test_state_changed_events_to_not_leak_contexts`) requires internal HA fixtures.

**Files**:
- `tests/ha_compat/setup.sh` - Setup script (clones HA, installs deps)
- `tests/ha_compat/run_ha_tests.py` - Test runner with Rust patching
- `tests/ha_compat/conftest.py` - RustState/RustContext wrappers

## Crate Structure

```
crates/
├── ha-core/           # Core types (EntityId, State, Event, Context)
├── ha-event-bus/      # Pub/sub event system
├── ha-state-machine/  # Entity state management
├── ha-service-registry/  # Service registration and dispatch
├── ha-config/         # YAML loading, !include, !secret
├── ha-template/       # Jinja2-compatible templates (minijinja)
├── ha-api/            # REST API (axum)
├── ha-python-bridge/  # PyO3 bidirectional bridge
├── ha-server/         # Main binary
└── ha-test-comparison/  # Comparison test infrastructure
```

## Vendored Home Assistant Core

We maintain a clone of `home-assistant/core` at `vendor/ha-core` for:

**Research & Development:**
- Study HA's implementation patterns (registries, storage, events)
- Reference when implementing Rust equivalents
- Understand data structures for compatibility

**Testing:**
- Run HA's own test suite with Rust components patched in
- Validate API compatibility against the real implementation
- Ensure our Rust types match Python HA's behavior exactly

**Setup:**
```bash
# The setup script clones HA core automatically
make ha-compat-setup

# Or manually:
git clone --depth 1 --branch 2026.1.1 \
    https://github.com/home-assistant/core.git ../core
```

**Key directories in HA core we reference:**
- `homeassistant/core.py` - EventBus, StateMachine, ServiceRegistry
- `homeassistant/helpers/storage.py` - JSON persistence with versioning
- `homeassistant/helpers/entity_registry.py` - Entity tracking
- `homeassistant/helpers/device_registry.py` - Device tracking
- `homeassistant/helpers/template.py` - Jinja2 template engine
- `tests/` - Test fixtures and patterns we mirror

**Note:** The vendored core is NOT committed to this repo - it's cloned on demand during setup.

## Key Files

- `tests/comparison/ha-versions.toml` - Target HA versions
- `tests/comparison/docker-compose.yml` - Python HA test instance
- `tests/ha_compat/setup.sh` - Clones and configures HA core for testing
- `Makefile` - Build and test commands
- `.claude/plans/moonlit-nibbling-snail.md` - Detailed implementation plan

## Git Workflow

**We follow gitflow.** Never push directly to main.

1. Create a feature branch: `git checkout -b feature/description`
2. Make commits on the feature branch
3. Push and create a PR: `gh pr create`
4. Merge via PR after review

Example:
```bash
git checkout -b feature/phase-3-registries
# ... make changes ...
git add -A && git commit -m "Implement registry system"
git push -u origin feature/phase-3-registries
gh pr create --title "Phase 3: Registry System" --body "..."
```

## Commands

```bash
# Build
make build           # Build all crates (debug)
make build-release   # Build all crates (release)
make build-wheel     # Build Python wheel (Mode 1)

# Test
make test            # Run Rust unit tests
make python-test     # Run Python pytest tests
make ha-start        # Start Python HA test instance
make test-compare    # Run API comparison tests
make ha-stop         # Stop Python HA

# Development
make install-dev     # Install wheel in dev mode (editable)
make dev             # Run all dev checks (fmt, clippy, test)
```

## Current Status

Working:
- Core types, EventBus, StateMachine, ServiceRegistry
- Template engine (Jinja2-compatible)
- REST API (basic endpoints)
- WebSocket API (auth, get_states, get_config, get_services, call_service, subscribe_events, ping/pong)
- Comparison test infrastructure
- Registry system (EntityRegistry, DeviceRegistry, AreaRegistry, FloorRegistry, LabelRegistry)
- Storage abstraction (JSON persistence with versioning)

In Progress:
- API parity with Python HA
- Python bridge (Mode 1 and Mode 2)

Not Started:
- Recorder (SQLite history)
- Automation/Script engine execution
- Config entries lifecycle
