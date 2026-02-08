# Architecture — Home Assistant Rust

## Overview

Rewrite Home Assistant's Python core in Rust for performance, memory efficiency, reliability, and maintainability. This is a **drop-in replacement** that loads existing HA configurations unchanged, runs Python integrations via embedded interpreter, and presents identical APIs to frontends.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                 Pure Rust Core (no PyO3)                    │
├─────────────┬─────────────┬─────────────┬──────────────────┤
│  Event Bus  │ State       │ Service     │ Config Entries   │
│  (pub/sub)  │ Machine     │ Registry    │ (lifecycle)      │
├─────────────┴─────────────┴─────────────┴──────────────────┤
│  Registries (Entity, Device, Area, Floor, Label)           │
├────────────────────────────────────────────────────────────┤
│  Automation Engine │ Script Engine │ Template Engine       │
├────────────────────────────────────────────────────────────┤
│  REST API (axum) │ WebSocket API │ Auth                    │
└────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
┌─────────────────────────┐     ┌─────────────────────────┐
│  Mode 1: Extension      │     │  Mode 2: Standalone     │
│  #[pyclass] wrappers    │     │  Python bridge via      │
│  Rust → Python export   │     │  embedded interpreter   │
│  (cdylib)               │     │  (binary)               │
└─────────────────────────┘     └─────────────────────────┘
```

## Dual Deployment Modes

**Mode 1: Python Extension** — Safe production testing
```
Python HA (existing) → imports ha_core_rs → Rust components
```
- Install via pip, import Rust components into existing HA
- Feature flag controls which components use Rust
- Zero risk — disable to revert to Python

**Mode 2: Standalone Binary** — Clean architecture
```
Rust HA (main) → embeds Python → runs integrations
```
- Rust is the main process
- Unimplemented components fall back to embedded Python
- Gradually remove fallbacks as coverage grows

## Core Design Principles

1. **Pure Rust core** (no PyO3) in dedicated crates
2. **PyO3 bridge** only in `ha-py-bridge` crate (`#[pyclass]` wrappers for direct Rust access)
3. **Trait abstractions** for Python-dependent features (e.g., `ConfigFlowHandler` trait in ha-api, implementation in ha-py-bridge)
4. **Event-driven** — EventBus is the backbone
5. **Async-first** — Tokio runtime, matches HA's asyncio patterns
6. **Domain indexing** — StateMachine indexes by domain for fast lookups

## Python Shim Layer

Python integrations import from `homeassistant.*` (e.g., `from homeassistant.core import HomeAssistant`).
To ensure these imports use Rust implementations while maintaining compatibility:

```
crates/ha-py-bridge/python/
└── homeassistant/              # Shim package (on PYTHONPATH first)
    ├── __init__.py             # Installs import fallback via __path__
    ├── _native_loader.py       # Loads modules from vendor/ha-core
    ├── core.py                 # HomeAssistant, Event, State + native re-exports
    ├── const.py                # Re-exports all constants from native
    ├── exceptions.py           # Re-exports all exceptions from native
    ├── config_entries.py       # Re-exports ConfigEntry, etc.
    ├── core_config.py          # Re-exports core config utilities
    ├── helpers/
    │   ├── __init__.py         # Extends __path__ to native helpers/
    │   ├── entity.py           # Entity with RustStateMixin
    │   ├── entity_platform.py  # Re-exports from native
    │   └── typing.py           # Re-exports type aliases
    └── components/
        ├── __init__.py         # Extends __path__ to native components/
        ├── light/              # LightEntity with RustStateMixin
        ├── switch/             # SwitchEntity with RustStateMixin
        └── sensor/             # SensorEntity with RustStateMixin
```

**Strategy:**
1. **Re-export safe modules**: Constants, types, exceptions from `vendor/ha-core` (no logic to override)
2. **Inherit + Override**: Entity classes inherit from native HA, add `RustStateMixin` for state routing
3. **Rust-backed core**: HomeAssistant class backed by PyO3 wrappers (states, bus, services)
4. **Fallback via `__path__`**: Unknown submodules found in `vendor/ha-core`

**Why RustStateMixin?**
- Native HA's Entity uses a `CachedProperties` metaclass for `_attr_*` property caching
- Multiple inheritance with different metaclasses causes conflicts
- Mixin approach: `RustStateMixin` has no metaclass, so it can be combined with native Entity

**Key override — `async_write_ha_state()`:**
```python
class RustStateMixin:
    def async_write_ha_state(self) -> None:
        # Routes state updates to Rust StateMachine instead of Python HA
        self.hass.states.async_set(self.entity_id, self.state, attributes)
```

**Result**: When demo integration creates a `DemoLight`, its MRO is:
```
DemoLight -> LightEntity -> RustStateMixin -> LightEntity -> ToggleEntity -> Entity
```
The `RustStateMixin.async_write_ha_state()` takes precedence, routing all state writes to Rust.

**Native Fallback:**
The shim extends `__path__` to include `vendor/ha-core` for native fallback, but shim modules
always take precedence. This allows Python integrations to import modules we haven't shimmed
while ensuring core types (HomeAssistant, EventBus, Entity, etc.) route to Rust.

Note: `vendor/ha-core` is NOT on PYTHONPATH directly — only accessible via `__path__` extension.

## Key Dependencies

| Purpose | Crate | Notes |
|---------|-------|-------|
| Async runtime | `tokio` | Full features, rt-multi-thread |
| Web framework | `axum` | Tower middleware, WebSocket built-in |
| YAML | `serde_yaml` | Serde ecosystem |
| JSON | `serde_json` | Standard |
| Templates | `minijinja` | Jinja2-compatible, fast |
| Python | `pyo3` | Bidirectional Python <-> Rust bridge |
| Python async | `pyo3-asyncio-0-21` | Tokio <-> asyncio interop |
| Build wheels | `maturin` | Build Python wheels from Rust |
| SQLite | `rusqlite` | For recorder |
| Concurrent maps | `dashmap` | Lock-free concurrent HashMap |
| ULID | `ulid` | ID generation (Context, ConfigEntry) |
| DateTime | `chrono` | Timezone-aware |
| Schema validation | `jsonschema` | Service schema validation |
| Logging | `tracing` | Structured logging |

## Vendored Home Assistant Core

HA core is vendored as a git submodule at `vendor/ha-core` for:
- **Research**: Study HA's implementation patterns
- **Compatibility testing**: Use real HA test configs to verify our types

```bash
git submodule update --init vendor/ha-core
```

**Key source files we reference:**
- `homeassistant/core.py` — EventBus, StateMachine, ServiceRegistry
- `homeassistant/helpers/condition.py` — Condition evaluation
- `homeassistant/helpers/trigger.py` — Trigger system
- `homeassistant/helpers/script.py` — Script executor
- `homeassistant/helpers/storage.py` — JSON persistence
- `homeassistant/components/automation/` — Automation component

## Upstream Tracking Strategy

1. **API Schema Tests**: Extract API schemas from Python HA, validate Rust responses
2. **Event Format Tests**: Capture events from Python HA, replay against Rust
3. **Config Compatibility**: Test loading configs from latest HA version
4. **CI Pipeline**: Run comparison tests against HA `dev` branch nightly
5. **Release Tracking**: Tag Rust releases to corresponding HA versions

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| PyO3 async complexity | Use `pyo3-asyncio` crate, extensive testing |
| Template compatibility | Comprehensive filter test suite, fuzz testing |
| API drift from upstream | Automated nightly tests against HA dev |
| Integration breakage | Start with simple integrations, expand gradually |
| Performance regression | Benchmark suite, memory profiling |

## Success Criteria

1. **Compatibility**: Load existing HA config without modification
2. **Integration Support**: Run 90%+ of popular integrations via PyO3
3. **API Parity**: Frontend works without changes
4. **Performance**: 50%+ memory reduction vs Python HA
5. **Reliability**: No crashes in 7-day continuous run
