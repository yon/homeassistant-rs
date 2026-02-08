# CLAUDE.MD — Home Assistant Rust (homeassistant-rs)

**Last Updated:** 2026-02-08
**Project:** homeassistant-rs
**Language/Stack:** Rust 2021 (MSRV 1.75) / Python 3.13 (embedded via PyO3)
**Target HA Version:** 2026.1.1
**Working Branch:** main

---

## Quick Reference: Available Skills & Agents

| Command | What It Does |
|---------|-------------|
| `/build` | Build the project (compile all workspace crates) |
| `/test [scope]` | Run test suite — rust, python, integration, ha-compat, or all |
| `/lint [files]` | Run linters: rustfmt, clippy, alphabetization, Makefile lint |
| `/review [files]` | Combined multi-agent code review (6 dimensions) |
| `/security-audit [scope]` | Security review: OWASP, deps (cargo audit), secrets, permissions |
| `/deploy [env]` | Deploy to target environment (staging/production) |
| `/create-feature [name]` | Full feature creation workflow with planning |
| `/fix-bug [issue]` | Structured bug fix workflow with root cause analysis |
| `/refactor [scope]` | Safe refactoring with test-first verification |
| `/team-review [scope]` | Parallel agent team code review (each reviewer in own session) |
| `/team-implement [plan]` | Parallel agent team implementation with adversarial review |
| `/swarm [task]` | General-purpose agent team orchestration |

**Agents** (available for delegation): `code-reviewer`, `security-reviewer`, `architecture-reviewer`, `test-reviewer`, `performance-reviewer`, `doc-reviewer`, `verifier`, `team-lead`

**Rules** (auto-loaded): See `.claude/rules/` for rules on planning, quality gates, testing, security, code conventions, engineering principles, git workflow, verification, and agent teams.

---

## Project Overview

### Intent

Rewrite Home Assistant's Python core in Rust for:
- **Performance**: Faster startup, lower latency, no GC pauses
- **Memory efficiency**: Critical for IoT devices and Raspberry Pi
- **Reliability**: Strong typing catches bugs at compile time
- **Maintainability**: Clear ownership model, no runtime surprises

This is NOT a fork. The goal is a **drop-in replacement** that:
- Loads existing HA configurations unchanged
- Runs existing Python integrations via embedded interpreter
- Presents identical APIs (REST, WebSocket) to frontends
- Maintains full backward compatibility

The project tracks Home Assistant version **2026.1.1** and currently passes **76/77** of the real HA compatibility test suite.

### Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Language | Rust | Strongly typed, fast, low memory, no GC pauses, IoT-optimized |
| Python Bridge | Bidirectional (PyO3) | Support both deployment modes with same codebase |
| Deployment | Dual-mode | Extension mode for safe testing, standalone for production |
| Initial Scope | Core only | Integrations remain Python initially, rewrite over time |
| Runtime | Tokio | Mature async runtime, maps well to HA's asyncio patterns |

### Version Tracking

**Target HA Version**: 2026.1.1 (tracked in `tests/comparison/ha-versions.toml`)

Version scheme: We match HA's version exactly for compatibility:
```
Cargo.toml:     version = "2026.1.1"
pyproject.toml: version = "2026.1.1"
```

### Tech Stack

| Layer | Technology | Version |
|-------|-----------|---------|
| Language | Rust | 2021 edition, MSRV 1.75 |
| Language (bridge) | Python | 3.13 |
| Web Framework | Axum | 0.7 |
| Async Runtime | Tokio | 1.40 |
| Python FFI | PyO3 / pyo3-asyncio | 0.22 / 0.21 |
| Template Engine | MiniJinja | 2.0 |
| Database | rusqlite (bundled SQLite) | 0.32 |
| Testing (Rust) | cargo test | built-in |
| Testing (Python) | pytest / pytest-asyncio | — |
| Linting | rustfmt + clippy + lint-alpha.py | — |
| CI/CD | GitHub Actions | 5 jobs |
| Build Automation | GNU Make + Cargo workspace | — |

### Key Dependencies

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

---

## Architecture

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

### Dual Deployment Modes

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

### Core Design Principles

1. **Pure Rust core** (no PyO3) in dedicated crates
2. **PyO3 bridge** only in `ha-py-bridge` crate (`#[pyclass]` wrappers for direct Rust access)
3. **Trait abstractions** for Python-dependent features (e.g., `ConfigFlowHandler` trait in ha-api, implementation in ha-py-bridge)
4. **Event-driven** — EventBus is the backbone
5. **Async-first** — Tokio runtime, matches HA's asyncio patterns
6. **Domain indexing** — StateMachine indexes by domain for fast lookups

---

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

---

## Folder Structure

```
homeassistant-rs/
├── Makefile                           # Self-documenting build commands
├── Cargo.toml                         # Workspace root (16 member crates)
├── rust-toolchain.toml                # Stable Rust with rustfmt + clippy
├── rustfmt.toml                       # Formatting rules (100 char width)
├── .claude/                           # Claude Code configuration
│   ├── CLAUDE.md                      # This file — Claude's project guide
│   ├── settings.local.json            # Local permissions + hooks (gitignored)
│   ├── rules/                         # Engineering rules (auto-loaded)
│   ├── skills/                        # Slash commands (/build, /test, etc.)
│   └── agents/                        # Specialized agents (review + team-lead)
├── crates/                            # 16 workspace crates
│   ├── ha-core/                       # Shared types: EntityId, State, Event, Context
│   ├── ha-event-bus/                  # Pub/sub event system
│   ├── ha-service-registry/           # Service registration and dispatch
│   ├── ha-state-store/                # Entity state storage
│   ├── ha-registries/                 # Entity, Device, Area, Floor, Label registries
│   ├── ha-config/                     # YAML loading (!include, !secret)
│   ├── ha-template/                   # Jinja2-compatible templates (MiniJinja)
│   ├── ha-components/                 # Built-in components (input_*, system_log)
│   ├── ha-config-entries/             # ConfigEntry lifecycle with FSM
│   ├── ha-automation/                 # Trigger-Condition-Action engine
│   ├── ha-script/                     # Script executor
│   ├── ha-api/                        # REST + WebSocket API (Axum)
│   ├── ha-server/                     # Main binary (homeassistant)
│   ├── ha-py-bridge/                  # PyO3 Python bridge + shim layer
│   ├── ha-recorder/                   # SQLite history storage (stub)
│   └── ha-test-comparison/            # API comparison test infrastructure
├── tests/                             # Test suites
│   ├── integration/                   # WebSocket API integration tests
│   ├── ha_compat/                     # HA compatibility test harness
│   ├── comparison/                    # Docker-based API comparison tests
│   └── fixtures/                      # Test fixtures and mock data
├── vendor/                            # Git submodule: Home Assistant core
├── scripts/                           # Custom tools
│   ├── lint-alpha.py                  # Alphabetization linter
│   └── quality_score.py               # Automated quality scoring (0-100)
├── config/                            # Sample HA configuration
├── docs/                              # Documentation
├── .githooks/                         # Git pre-commit hooks
├── .github/workflows/                 # CI/CD pipeline
└── quality_reports/                   # Plans and session logs
    ├── plans/                         # Implementation plans
    └── session_logs/                  # Session history and decision logs
```

---

## Working Philosophy

### Collaborative Partnership Approach

Claude serves as your **engineering partner**, not a code generator:

- **You define requirements** — provide specs, context, and constraints
- **Claude proposes designs** — architecture, implementation approach, trade-offs
- **You iterate together** — refine until the solution is right
- **You maintain control** — final decisions always rest with you

### Communication Style

- **Challenge assumptions** — question design choices and explore alternatives
- **Explain trade-offs** — never present a single option without discussing alternatives
- **Correctness over speed** — getting it right matters more than getting it fast
- **Teach while building** — explain the "why" behind engineering decisions

### TDD — Test-Driven Development (MANDATORY)

All implementation follows the **Red-Green-Refactor** cycle:

1. **RED** — Write a failing test that describes the desired behavior
2. **GREEN** — Write the minimum code to make the test pass
3. **REFACTOR** — Clean up while keeping tests green
4. **Repeat** — Next behavior, next test

Tests are not an afterthought. Tests are the FIRST code written for every feature and bug fix. See `.claude/rules/testing-protocol.md` for the full protocol.

#### TDD with HA's Own Tests

This project uses Home Assistant's own test suite from `vendor/ha-core` as the primary TDD driver:

1. **Identify tests** — Find relevant tests in `vendor/ha-core/tests/`
   ```bash
   grep -r "def test_" vendor/ha-core/tests/helpers/test_<module>.py
   ```

2. **Add to compat suite** — Add test patterns to `tests/ha_compat/run_tests.py` TEST_CATEGORIES
   ```python
   "<category>": [
       "helpers/test_<module>.py::test_function_name",
   ],
   ```

3. **Run tests (expect failures)** — Red phase
   ```bash
   make install-dev
   python tests/ha_compat/run_tests.py -c <category> -v
   ```

4. **Implement until tests pass** — Green phase

5. **Verify full suite** — Refactor phase
   ```bash
   python tests/ha_compat/run_tests.py -a -v
   ```

### Plan-First Approach

For any non-trivial task, Claude enters **plan mode first** before writing code:

1. **Plan** — draft an approach, list files to modify, identify risks, define tests
2. **Save** — write the plan to `quality_reports/plans/` so it survives context compression
3. **Review** — present the plan and wait for your approval
4. **Test** — write failing tests FIRST (TDD red phase)
5. **Implement** — write minimum code to pass tests (TDD green phase)
6. **Refactor** — clean up while tests stay green

See `.claude/rules/plan-first-workflow.md` for the full protocol.

> **Never use `/clear`.** Rely on auto-compression to manage long conversations. `/clear` destroys all context; auto-compression preserves what matters.

### Contractor Mode (Orchestrator)

After a plan is approved, Claude operates in **contractor mode**: implement, verify (build + test + lint), review with agents, fix issues, and re-verify — all autonomously. The user sees a summary when the work meets quality standards or review rounds are exhausted. See `.claude/rules/orchestrator-protocol.md`.

When you say "just do it", the orchestrator skips the final approval pause and auto-commits if the score is 80+.

### Agent Teams (Parallel Multi-Session)

For tasks that span multiple independent modules, Claude can spawn **agent teams** — multiple independent sessions working in parallel with peer-to-peer communication:

- `/team-review` — spawn parallel reviewers (security + architecture + tests + code) each in their own context window
- `/team-implement` — spawn parallel implementers with adversarial review (implementers != reviewers, always)
- `/swarm` — general-purpose team for research, debugging, or migration

**The Iron Rule:** The agent that writes code NEVER approves it. The agent that reviews NEVER edits. This adversarial separation is non-negotiable.

See `.claude/rules/agent-teams.md` for patterns, file ownership rules, and team coordination protocols.

> **Enable agent teams:** Set `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=true` in `.claude/settings.local.json` or as an environment variable.

### Continuous Learning with [LEARN] Tags

When Claude makes a mistake or you correct a misconception, tag the correction:

```
[LEARN:pattern] Don't use singleton here — use dependency injection instead
[LEARN:rust] This crate uses thiserror, not anyhow — keep error enums concrete
[LEARN:convention] Match arms must be alphabetized (enforced by lint-alpha.py)
[LEARN:ha-compat] Always check Python HA behavior first before implementing
```

These corrections persist in `MEMORY.md` across sessions and prevent the same mistake from recurring.

---

## Engineering Principles (MANDATORY)

These principles are non-negotiable. They apply to every line of code written in this project.

### DRY — Don't Repeat Yourself
- Extract shared logic into functions, modules, or shared libraries
- If you find yourself copying code, stop and refactor
- Single source of truth for configuration, constants, and business rules
- **Exception:** Prefer duplication over the wrong abstraction (see KISS)

### KISS — Keep It Simple, Stupid
- The simplest solution that works correctly is the best solution
- Avoid premature abstraction — wait until you see the pattern three times
- Every layer of indirection must justify its existence
- If a junior engineer can't understand it in 5 minutes, simplify it

### SOLID Principles
- **S**ingle Responsibility — each crate/module does one thing well
- **O**pen/Closed — extend behavior without modifying existing code
- **L**iskov Substitution — subtypes must be substitutable for their base types
- **I**nterface Segregation — prefer small, focused traits over large ones
- **D**ependency Inversion — depend on abstractions (traits), not concretions

### Immutability by Default
- Prefer immutable bindings (`let` not `let mut`) unless mutation is needed
- Return new objects instead of modifying inputs
- Use `Arc<T>` for shared state, `DashMap` for concurrent access

### Strong Typing
- Use the type system to make illegal states unrepresentable
- Prefer enums/newtypes over stringly-typed code (`EntityId` not `String`)
- Define explicit types for domain concepts
- Use `thiserror` for typed error enums, not `anyhow`

### Dependency Injection
- Pass dependencies explicitly — never reach into global state
- Constructor injection via struct fields
- Makes testing trivial: swap real deps for mocks

### Additional Principles
- **Fail fast** — validate inputs at boundaries, return errors early
- **Composition over inheritance** — build behavior by combining small pieces via traits
- **Least privilege** — grant minimum necessary permissions and access
- **Explicit over implicit** — no magic, no hidden side effects, no surprises
- **Idempotency** — operations should be safe to retry
- **Separation of concerns** — I/O at the edges, pure logic in the core

See `.claude/rules/engineering-principles.md` for detailed enforcement rules.

---

## Quality Gates

| Threshold | When | What It Means |
|-----------|------|--------------|
| **80/100** | Commit | Tests pass, no lint errors, no security issues |
| **90/100** | PR/Merge | High coverage, clean architecture, documented |
| **95/100** | Release | Production-ready, performance validated, fully reviewed |

See `.claude/rules/quality-gates.md` for full scoring rubric.

---

## Task Completion Verification Protocol

**At the end of EVERY task, Claude MUST verify the output works correctly.** A Stop hook enforces this automatically.

See `.claude/rules/verification-protocol.md` for the full checklist.

**Quick summary:**
- **Build:** Run `make build`, verify zero errors
- **Tests:** Run `make test-rust`, verify all pass
- **Lint:** Run `make lint`, verify zero warnings
- **Always** run `make dev` (fmt + clippy + test) before presenting results

---

## Testing Strategy

### Tier 1: Rust Unit Tests
```bash
make test-rust         # Run all Rust tests
cargo test -p ha-core  # Test specific crate
```

### Tier 2: Python Tests (PyO3 Bindings)
```bash
make test-python       # Build wheel and run pytest
```

### Tier 3: Comparison Tests
```bash
make ha-start          # Start Python HA in Docker
make test-compare      # Run comparison tests
make ha-stop           # Stop Python HA
```

### Tier 4: HA's Own Tests
```bash
make ha-setup
python tests/ha_compat/run_tests.py --all -v
```

**Current HA test compatibility**: 76/77 pass (99%)

### Coverage Requirements

| Component | Target |
|-----------|--------|
| ha-core | 100% |
| ha-event-bus | 100% |
| ha-state-store | 100% |
| ha-service-registry | 100% |
| ha-config | 95%+ |
| ha-registries | 95%+ |
| ha-template | 95%+ |
| ha-automation | 90%+ |
| ha-py-bridge | 80%+ |
| ha-api | 90%+ |

### Current Test Categories

The ha-compat test suite runs 249+ tests across categories including:
- `state`, `statemachine`, `eventbus`, `service` — Core infrastructure
- `condition`, `trigger`, `script` — Automation engine
- `storage`, `area_registry`, `device_registry`, `entity_registry` — Registries
- `template` — Template rendering
- `api`, `websocket_commands` — External APIs
- `config_entries`, `config_flow` — Config entry lifecycle

---

## Design Patterns (Preferred)

| Pattern | When to Use | Example in This Project |
|---------|-------------|------------------------|
| Registry | Central lookup for dynamic items | `ServiceRegistry`, `EntityRegistry` |
| Observer/Events | Decoupled notifications | `EventBus` with typed subscriptions |
| State Machine | Lifecycle management | `ConfigEntry` FSM states |
| Builder | Complex object construction | Query builders, config objects |
| Strategy | Swappable implementations | Python bridge modes (extension vs embedded) |
| Result/Either | Error handling | `Result<T, CrateError>` everywhere |
| Newtype | Type safety for domain concepts | `EntityId`, `AreaId` |

**Anti-patterns to avoid:**
- God functions (> 50 lines — enforced by quality_score.py)
- Stringly-typed interfaces (use `EntityId` not `String`)
- Mutable shared state without `DashMap`/`Arc<RwLock>` discipline
- Deep module nesting (prefer flat crate structures)
- `anyhow` in library crates (use `thiserror` for concrete error types)
- `unwrap()` on user-provided data (use `?` or explicit matching)

---

## Project-Specific Conventions

### Alphabetization (enforced by lint-alpha.py + pre-commit hook)

- **Rust match arms**: Must be alphabetized (exceptions: Ok/Err, Some/None, Trigger/Action enums)
- **Rust `mod` declarations**: Must be alphabetized within groups
- **Cargo.toml dependencies**: Must be alphabetized within sections
- **Python class members**: Properties, methods, and dunders each alphabetized
- **Makefile targets**: Must be alphabetized within `##@` sections

### Error Handling

- Use `thiserror` for error types (not `anyhow`)
- One error enum per crate: `#[derive(Debug, Error)]`
- Result type aliases: `pub type ConfigResult<T> = Result<T, ConfigError>;`
- API handlers: `Result<Json<T>, (StatusCode, Json<ErrorResponse>)>`

### Crate Architecture

- 16 crates in `crates/`, each with focused responsibility
- Crate names: `ha-{name}` pattern
- `ha-core` is the leaf dependency (shared types, no workspace deps)
- New functionality should go in existing crates when possible
- New crates need workspace registration in root `Cargo.toml` (alphabetized)

### Python Bridge

- `ha-py-bridge` has two modes: extension (Rust->Python) and embedded (Python->Rust)
- Python shim layer in `crates/ha-py-bridge/python/` (see [Python Shim Layer](#python-shim-layer) above)
- Minimize GIL hold time; release before async operations
- Use `pyo3-asyncio` for bridging Tokio <-> asyncio
- Keep the bridge layer thin — logic lives in core crates
- All Python-facing functions must validate inputs (treat as external)

### HA Compatibility

- Every feature must maintain API compatibility with Python HA 2026.1.1
- Check `vendor/ha-core/` for reference Python implementation
- Run `make test-ha-compat` to validate against real HA test suite
- Target: 77/77 tests passing (currently 76/77)

---

## Vendored Home Assistant Core

HA core is vendored as a git submodule at `vendor/ha-core` for:
- **Research**: Study HA's implementation patterns
- **Compatibility testing**: Use real HA test configs to verify our types

```bash
# Initialize the submodule (required before use)
git submodule update --init vendor/ha-core

# Key test files for compatibility
vendor/ha-core/tests/helpers/test_condition.py  # Condition configs
vendor/ha-core/tests/helpers/test_trigger.py    # Trigger configs
vendor/ha-core/tests/helpers/test_script.py     # Script/action configs
```

**Key source files we reference:**
- `homeassistant/core.py` — EventBus, StateMachine, ServiceRegistry
- `homeassistant/helpers/condition.py` — Condition evaluation
- `homeassistant/helpers/trigger.py` — Trigger system
- `homeassistant/helpers/script.py` — Script executor
- `homeassistant/helpers/storage.py` — JSON persistence
- `homeassistant/components/automation/` — Automation component

### Upstream Tracking Strategy

1. **API Schema Tests**: Extract API schemas from Python HA, validate Rust responses
2. **Event Format Tests**: Capture events from Python HA, replay against Rust
3. **Config Compatibility**: Test loading configs from latest HA version
4. **CI Pipeline**: Run comparison tests against HA `dev` branch nightly
5. **Release Tracking**: Tag Rust releases to corresponding HA versions

---

## Running the Server

```bash
make run                    # Debug build with Python support
make run-release            # Release build with Python support
```

Then open http://localhost:8123

**Manual build and run:**
```bash
# Build with Python support
PYO3_PYTHON=$(pwd)/.venv/bin/python cargo build -p ha-server --features python

# Run with correct environment
PYTHONPATH="$(pwd)/crates/ha-py-bridge/python:$(pwd)/.venv/lib/python3.13/site-packages" \
  HA_CONFIG_DIR="$(pwd)/tests/config" \
  HA_FRONTEND_PATH="$(pwd)/.venv/lib/python3.13/site-packages/hass_frontend" \
  ./target/debug/homeassistant
```

### Environment Variables

| Variable | Purpose |
|----------|---------|
| `PYTHONPATH` | Paths for embedded Python (shim first, then venv packages) |
| `HA_CONFIG_DIR` | Home Assistant configuration directory |
| `HA_FRONTEND_PATH` | Path to hass_frontend package for UI |
| `HA_COMPONENTS_PATH` | Path to HA components (vendor/ha-core/homeassistant/components) |
| `HA_PORT` | Server port (default: 8123) |
| `PYO3_PYTHON` | Python interpreter for PyO3 builds |

### Python Environment

Always use the project venv (`.venv`) for Python-related work:
```bash
# Set PYO3_PYTHON for PyO3 builds (from project root)
export PYO3_PYTHON=$(pwd)/.venv/bin/python

# Or inline
PYO3_PYTHON=$(pwd)/.venv/bin/python cargo build -p ha-py-bridge
```

---

## Makefile Quick Reference

```bash
make help              # Show all available commands with descriptions
make build             # Build all crates (debug)
make build-release     # Build all crates (release)
make build-wheel       # Build Python wheel (Mode 1: extension)
make check             # Fast syntax/type check (cargo check)
make check-all         # Full pipeline: build + test + lint
make dev               # Format + clippy + test (recommended before commit)
make test              # ALL tests (Rust + Python + integration + HA compat)
make test-rust         # Rust unit/integration tests only
make test-python       # Python shim + extension tests
make test-integration  # WebSocket API integration tests
make test-ha-compat    # HA compatibility test suite
make test-compare      # API comparison tests (requires Docker)
make lint              # All linters (fmt-check + clippy + lint-makefile)
make fmt               # Auto-format all code
make clippy            # Clippy with -D warnings
make audit             # Security audit (cargo-audit)
make quality-score     # Run quality score (0-100)
make clean             # Remove build artifacts
make run               # Run the HA server
make run-release       # Run the HA server (release mode)
make install-dev       # Install Python extension in dev mode
make ha-start          # Start Python HA test instance (Docker)
make ha-stop           # Stop Python HA test instance
```

---

## Git Workflow

- **Main branch:** `main` — always deployable
- **Feature branches:** `feature/[ticket]-short-description`
- **Bug fix branches:** `fix/[ticket]-short-description`
- **Commit style:** Conventional Commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`)
- **Before every PR:** Run `make dev` and `/review`
- **Merge strategy:** Squash and merge for features, regular merge for releases
- **Pre-commit hook:** Runs fmt-check, lint-alpha.py --staged, clippy

**Never push directly to main.** Always use feature branches and PRs.

See `.claude/rules/git-workflow.md` for the full protocol.

---

## Session Startup Ritual

Start each session with:

```
Claude, please:
1. Read CLAUDE.md to understand our workflow
2. Check recent git commits to see what changed
3. Check quality_reports/plans/ for any in-progress plans
4. Check quality_reports/session_logs/ for the most recent session log
5. Look at the code area we're working on
6. State what you understand our goals to be
```

### Session End Protocol

Before ending a session:
1. Save a session log to `quality_reports/session_logs/YYYY-MM-DD_description.md`
2. Commit significant changes with descriptive messages
3. Update CLAUDE.md if workflow changed
4. Note any unresolved questions in the session log

---

## Implementation Phases & Current State

### Phase 1: Core Foundation -- DONE
EventBus, StateMachine, ServiceRegistry, HomeAssistant struct

### Phase 2: Configuration System -- DONE
YAML loader with `!include`, `!include_dir_*`, `!secret` substitution

### Phase 3: Registry System -- DONE
Storage abstraction, EntityRegistry, DeviceRegistry, AreaRegistry, FloorRegistry, LabelRegistry

### Phase 4: Template Engine -- DONE
minijinja with HA filters (`is_state`, `state_attr`, etc.) and globals (`states`, `now()`, etc.)

### Phase 5: Config Entries -- DONE
ConfigEntry lifecycle (NotLoaded -> SetupInProgress -> Loaded), CRUD, persistence

### Phase 6: Automation & Script Engine -- DONE
- Trigger platform system (state, time, event, template, device, etc.)
- Condition evaluation (and, or, not, state, numeric_state, template)
- Script action executor (service calls, delays, wait_for_trigger, choose, repeat, parallel)
- Variable scoping and template rendering in actions
- Execution tracing for debugging

### Phase 7: External APIs & Frontend -- DONE
- REST API (basic endpoints)
- WebSocket API (auth, get_states, get_config, get_services, call_service, subscribe_events)
- Frontend serving (static files, template processing, SPA routes)
- Config flows (trait-based abstraction, Python bridge implementation, frontend forms working)
- Authentication (OAuth2 flow works, tokens in-memory only, no persistence, credentials not validated)

### Phase 8: Python Integration Loading -- IN PROGRESS
- Config flow execution -- done (start flow, progress flow, form rendering)
- Integration whitelist system — not started (control which integrations load via Python)
- Entity platform setup — not started (load entities from Python integrations)
- Service registration from Python — not started
- Device/entity registry population — not started

### Phase 9: Built-in Components -- PARTIAL
- `homeassistant` component (core services: restart, reload) — not started
- `persistent_notification` component -- done
- `system_log` component -- done
- `input_boolean`, `input_number` helpers -- done
- `recorder` — SQLite storage, history — not started
- `logbook` — event logging — not started
- `history` — state history queries — not started

### Phase 10: Migration & Validation -- NOT STARTED
- Migration CLI tool
- Comparison test suite (byte-for-byte API responses)
- Performance benchmarks
- Documentation

**HA Compat Tests:** 76/77 passing (99%)

---

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

---

**Ready to begin? Start with `make help` to see available commands!**
