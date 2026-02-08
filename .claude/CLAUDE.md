# CLAUDE.MD — Home Assistant Rust (homeassistant-rs)

**Target HA Version**: 2026.1.1 | **HA Compat**: 76/77 (99%) | **Phase**: 8 (Python Integration Loading)

> Architecture, shim layer, and dependency docs: `docs/architecture.md`
> Behavioral rules and subagent patterns: MEMORY.md (auto-loaded)
> Engineering principles: `.claude/rules/engineering-principles.md`

---

## Quick Reference

| Command | What It Does |
|---------|-------------|
| `/build` | Build all workspace crates |
| `/test [scope]` | Run tests: rust, python, integration, ha-compat, or all |
| `/lint` | Linters: rustfmt, clippy, alphabetization |
| `/review [scope]` | Multi-agent code review (spawns subagents) |
| `/security-audit` | Security: OWASP, deps, secrets, permissions |
| `/create-feature` | Full TDD workflow: plan, test, implement, verify, review |
| `/fix-bug [issue]` | Bug fix: reproduce, root cause, test, fix, verify |
| `/refactor [scope]` | Safe refactoring with test-first verification |
| `/team-review` | Parallel subagent review (4 reviewers simultaneously) |
| `/team-implement` | Parallel subagent implementation with adversarial review |
| `/swarm [task]` | General-purpose parallel subagent orchestration |

**Agents** (`.claude/agents/`): code-reviewer, security-reviewer, architecture-reviewer, test-reviewer, performance-reviewer, doc-reviewer, verifier, team-lead

---

## Crate Structure

```
crates/
├── ha-core/              # Core types (EntityId, State, Event, Context)
├── ha-event-bus/         # Pub/sub event system
├── ha-state-store/       # Entity state storage
├── ha-service-registry/  # Service registration and dispatch
├── ha-config/            # YAML loading, !include, !secret
├── ha-config-entries/    # ConfigEntry lifecycle (FSM)
├── ha-registries/        # Entity/Device/Area/Floor/Label registries + Storage
├── ha-template/          # Jinja2-compatible templates (minijinja)
├── ha-automation/        # Trigger-Condition-Action engine
├── ha-script/            # Script executor
├── ha-components/        # Built-in components (input_*, system_log)
├── ha-py-bridge/         # PyO3 bidirectional bridge + Python shim layer
├── ha-api/               # REST + WebSocket API (axum)
├── ha-recorder/          # SQLite history (stub)
├── ha-server/            # Main binary (homeassistant)
└── ha-test-comparison/   # API comparison test infrastructure
```

## Folder Structure

```
homeassistant-rs/
├── Cargo.toml                  # Workspace root (16 member crates)
├── Makefile                    # Self-documenting build commands
├── .claude/                    # Claude Code configuration
│   ├── CLAUDE.md               # This file
│   ├── agents/                 # 8 specialized review agents
│   ├── rules/                  # 10 engineering rules (auto-loaded)
│   └── skills/                 # 13 slash commands
├── crates/                     # 16 workspace crates (see above)
├── tests/                      # Test suites
│   ├── integration/            # WebSocket API integration tests
│   ├── ha_compat/              # HA compatibility test harness
│   ├── comparison/             # Docker-based API comparison
│   └── fixtures/               # Test fixtures
├── vendor/ha-core/             # Git submodule: Home Assistant core
├── scripts/                    # lint-alpha.py, quality_score.py
├── docs/                       # Architecture, plans
├── quality_reports/            # Plans and session logs
│   ├── plans/                  # Implementation plans
│   └── session_logs/           # Session history
└── config/                     # Sample HA configuration
```

## Tech Stack

| Layer | Technology | Version |
|-------|-----------|---------|
| Language | Rust | 2021 edition, MSRV 1.75 |
| Bridge | Python (PyO3) | 3.13 |
| Web | Axum | 0.7 |
| Async | Tokio | 1.40 |
| Templates | MiniJinja | 2.0 |
| Database | rusqlite | 0.32 |

---

## Project Conventions

### Alphabetization (enforced by lint-alpha.py + pre-commit hook)
- `use` declarations, `mod` declarations, enum variants, match arms, Cargo.toml deps
- Run: `./scripts/lint-alpha.py --all`

### Error Handling
- `thiserror` for error types (never `anyhow` in library crates)
- One error enum per crate in `src/error.rs`
- Result type alias: `pub type XxxResult<T> = Result<T, XxxError>;`

### Naming
- Crates: `ha-{name}` (hyphens); modules: `ha_{name}` (underscores)
- Newtypes for domain IDs: `EntityId`, `DeviceId`, `AreaId`
- Derive order: `Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize`

### Python Bridge
- `ha-py-bridge` has two modes: extension (Rust->Python) and embedded (Python->Rust)
- Keep the bridge layer thin — logic lives in pure Rust core crates
- Validate all data crossing the FFI boundary

---

## Testing

### Test Tiers

| Tier | Command | When |
|------|---------|------|
| Rust unit tests | `make test-rust` | Every change |
| Python bridge | `make test-python` | Bridge changes |
| Integration | `make test-integration` | Cross-crate changes |
| HA compat | `make test-ha-compat` | API/schema changes |
| Comparison | `make test-compare` | API parity checks |

### TDD with HA's Own Tests

1. Find tests in `vendor/ha-core/tests/`
2. Add to `tests/ha_compat/run_tests.py` TEST_CATEGORIES
3. Run (expect failures): `python tests/ha_compat/run_tests.py -c <category> -v`
4. Implement until tests pass
5. Verify full suite: `python tests/ha_compat/run_tests.py -a -v`

**Add new categories**: Edit `tests/ha_compat/run_tests.py`, add to TEST_CATEGORIES dict.
**List categories**: `python tests/ha_compat/run_tests.py --list`

### Coverage Targets

| Crate | Target |
|-------|--------|
| ha-core, ha-event-bus, ha-state-store, ha-service-registry | 100% |
| ha-config, ha-registries, ha-template | 95%+ |
| ha-automation, ha-api | 90%+ |
| ha-py-bridge | 80%+ |

---

## Makefile Reference

```bash
make help              # All commands with descriptions
make build             # Build (debug)
make build-release     # Build (release)
make dev               # fmt + clippy + test (use before commit)
make test              # ALL tests
make test-rust         # Rust tests only
make test-python       # Python tests
make test-ha-compat    # HA compatibility suite
make lint              # All linters
make fmt               # Auto-format
make clippy            # Clippy with -D warnings
make audit             # Security audit
make quality-score     # Quality score (0-100)
make run               # Run HA server (debug + Python)
make run-release       # Run HA server (release)
make install-dev       # Install Python extension (dev mode)
```

---

## Git Workflow

- **Branch naming**: `feat/description`, `fix/description`, `refactor/description`
- **Commit style**: Conventional Commits — `feat(ha-core): add state expiration`
- **Never push directly to main** — always feature branches + PRs
- **Pre-commit hook**: fmt-check, lint-alpha.py --staged, clippy
- **Before PR**: Run `make dev`

---

## Running the Server

```bash
make run               # Debug build with Python support
make run-release       # Release build
```

Open http://localhost:8123

| Variable | Purpose |
|----------|---------|
| `PYTHONPATH` | Shim first, then venv packages |
| `HA_CONFIG_DIR` | Configuration directory |
| `HA_FRONTEND_PATH` | Path to hass_frontend |
| `HA_COMPONENTS_PATH` | Path to HA components |
| `HA_PORT` | Server port (default: 8123) |
| `PYO3_PYTHON` | Python interpreter for PyO3 builds |

### Python Environment

```bash
export PYO3_PYTHON=$(pwd)/.venv/bin/python  # Set for PyO3 builds
make ha-compat-setup                         # Install all dependencies
```

---

## Implementation Phases

| Phase | Status | What |
|-------|--------|------|
| 1. Core Foundation | Done | EventBus, StateMachine, ServiceRegistry |
| 2. Configuration | Done | YAML loader, !include, !secret |
| 3. Registries | Done | Entity, Device, Area, Floor, Label |
| 4. Templates | Done | minijinja with HA filters/globals |
| 5. Config Entries | Done | ConfigEntry lifecycle FSM |
| 6. Automation | Done | Triggers, conditions, script executor |
| 7. APIs & Frontend | Done | REST, WebSocket, frontend serving, config flows |
| 8. Python Loading | **In Progress** | Config flows done; entity platform, services pending |
| 9. Built-in Components | Partial | input_*, system_log, persistent_notification done |
| 10. Migration | Not Started | CLI tool, benchmarks, comparison tests |
