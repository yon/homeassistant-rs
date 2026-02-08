# Plan: Existing Codebase Improvements

**Date:** 2026-02-08
**Status:** APPROVED — IN PROGRESS
**Task:** Analyze existing implementation and plan improvements before new development

## Baseline Metrics

| Metric | Value |
|--------|-------|
| Build | PASS (zero errors) |
| Tests | 771 pass, 0 fail |
| Lint | PASS (clippy + fmt) |
| Quality Score | 83/100 (COMMIT threshold met) |
| HA Compat | 76/77 (99%) |
| Alphabetization | 1 error (scripts/quality_score.py) |

### Quality Score Deductions

| Deduction | Points | Category |
|-----------|--------|----------|
| 48 god functions (>50 lines) | -10 | MAJOR: complexity |
| 19 TODOs without ticket numbers | -5 | MINOR: maintainability |
| 1 alphabetization error | -2 | MAJOR: alphabetization |

---

## Findings Summary

### F1: God Functions — 129 functions over 50 lines

**Severity: MAJOR** | **Impact: -10 quality points**

129 functions exceed the 50-line limit. The worst offenders:

| Rank | Lines | Crate | Function |
|:----:|------:|-------|----------|
| 1 | 1,114 | ha-py-bridge | `create_config_entries_wrapper` |
| 2 | 478 | ha-py-bridge | `async_update_device` |
| 3 | 393 | ha-api | `handle_message` |
| 4 | 281 | ha-server | `register_core_services` |
| 5 | 232 | ha-server | `register_automation_services` |
| 6 | 231 | ha-py-bridge | `async_get_or_create` (device) |
| 7 | 226 | ha-server | `main` |
| 8 | 189 | ha-py-bridge | `async_update_entity` |
| 9 | 183 | ha-py-bridge | `initialize_ha_registries` |
| 10 | 177 | ha-py-bridge | `async_get_or_create` (entity) |

**Distribution by crate:**

| Crate | Count | Root Cause Pattern |
|-------|------:|-------------------|
| ha-py-bridge | 38 | Many optional FFI params, embedded Python strings |
| ha-api | 31 | Large match/dispatch, JSON response construction |
| ha-server | 18 | Service registration boilerplate, procedural setup |
| ha-registries | 15 | Many-field update functions |
| ha-automation | 5 | Trigger/condition evaluation logic |
| ha-template | 5 | Global registration, method dispatch |
| ha-test-comparison | 5 | Test harness setup |
| ha-script | 4 | Action execution logic |
| ha-components | 4 | Service registration boilerplate |
| ha-config-entries | 2 | FSM state transition setup |
| ha-core | 1 | Domain-service match statement |
| ha-state-store | 1 | State set with event emission |

### F2: Error Handling — ~85 functions use `Result<T, String>`

**Severity: MAJOR** | **Impact: Convention violation, reliability risk**

The project convention requires `thiserror` error enums per crate. However:

- **ha-api**: ~68 functions return `Result<(), String>`. No error enum exists. This is the largest convention violation.
- **ha-registries**: 7 functions in area/floor/label registries use `Result<T, String>` despite `StorageError` existing in the same crate.
- **ha-py-bridge**: ~12 config flow and requirements functions use `String` errors despite `PyBridgeError` existing.
- **ha-test-comparison**: 3 functions. Acceptable for test infra.

### F3: Test Coverage Gaps — 5 crates with zero Rust tests

**Severity: MAJOR** | **Impact: 3,541 lines of core infrastructure untested by `cargo test`**

| Crate | Prod Lines | Tests | Note |
|-------|------------|-------|------|
| ha-registries | 2,994 | 0 | "covered by HA native tests" |
| ha-py-bridge | 11,524 | 0* | Feature-gated, don't run with default features |
| ha-service-registry | 207 | 0 | "covered by HA native tests" |
| ha-state-store | 174 | 0 | "covered by HA native tests" |
| ha-event-bus | 166 | 0 | "covered by HA native tests" |

*ha-py-bridge has 29 feature-gated tests + Python tests via `make test-python`

These crates rely entirely on the external Python-based ha-compat suite. If `make test-rust` passes but `make test-ha-compat` isn't run, regressions go undetected.

### F4: TODOs Without Tickets — 13 in Rust code

**Severity: MINOR** | **Impact: -5 quality points**

All 13 TODOs lack ticket numbers (convention: `TODO(#123)`):

| Location | TODO |
|----------|------|
| ha-api/src/websocket/connection.rs:220 | `accept any token (TODO: implement proper auth)` |
| ha-api/src/lib.rs:860 | `Actually abort the flow in the manager` |
| ha-api/src/websocket/handlers.rs:710 | `Implement entity_id rename` |
| ha-automation/src/trigger_eval.rs:217,391,537 | `Handle 'for' duration constraint` (x3) |
| ha-automation/src/eval.rs:189 | `Handle 'for' duration check` |
| ha-config-entries/src/manager.rs:444 | `Trigger reauth flow` |
| ha-config-entries/src/manager.rs:466 | `Schedule retry with exponential backoff` |
| ha-py-bridge/src/py_bridge/wrappers/services.rs:61 | `Extract context from PyObject` |
| ha-py-bridge/src/py_bridge/wrappers/config_entry.rs:300 | `Track the task and cancel it` |
| ha-recorder/src/lib.rs:1 | `ha-recorder - TODO: implement` |
| ha-script/src/executor.rs:790 | `Full implementation requires trigger matching` |

**Security-relevant TODO**: The auth bypass at connection.rs:220 accepts any token without validation.

### F5: Unsafe unwrap() Usage — 4 high-risk instances

**Severity: HIGH** | **Impact: Potential panics on external input**

| File | Line | Risk |
|------|------|------|
| ha-server/src/main.rs | 870 | `EntityId::new(parts[0], parts[1]).unwrap()` — panics on malformed entity ID |
| ha-server/src/main.rs | 1633 | `EntityId::new("automation", &automation_id).unwrap()` — panics on invalid chars |
| ha-state-store/src/lib.rs | 105 | `old_state.as_ref().unwrap()` — implicit invariant, fragile |
| ha-components/src/system_log.rs | 257 | `*self.index.get_mut(&key).unwrap()` — panics if index desyncs |

### F6: Dead Code — 5 entirely unused items

**Severity: MINOR** | **Impact: Code hygiene**

| File | Item |
|------|------|
| ha-py-bridge/src/py_bridge/runtime.rs:186 | `GilGuard` struct + impl (entirely unused type) |
| ha-py-bridge/src/py_bridge/service_bridge.rs:133 | `service_call_to_python()` function |
| ha-template/src/globals.rs:18 | `value_to_bool()` function |
| ha-template/src/filters.rs:334 | `clamp()` function |
| ha-py-bridge/src/py_bridge/runtime.rs:16,19 | `PythonRuntime.ha_path` and `.initialized` fields |

### F7: Missing Constants — hardcoded magic strings

**Severity: MINOR** | **Impact: Maintainability**

- State strings (`"on"`, `"off"`, `"unknown"`, `"unavailable"`) appear 25+ times in ha-server as raw literals. Python HA defines `STATE_ON`, `STATE_OFF`, etc. in `const.py`.
- Port `"8123"` hardcoded in ha-server/src/main.rs.
- Version `"2026.1.1"` hardcoded in 3 test/config locations.

### F8: Clippy Suppressions Without Justification — 16 of 17

**Severity: MINOR** | **Impact: Code hygiene**

14 `#[allow(clippy::too_many_arguments)]` suppressions, all without comments. Concentrated in ha-py-bridge extension functions (9 of 14) where they mirror PyO3 method signatures.

### F9: Missing Doc Comments — ~14 public types in ha-api

**Severity: MINOR** | **Impact: Documentation quality**

ha-api has ~14 undocumented public types (WebSocket message structs, auth types). All other crates have reasonable documentation.

### F10: Alphabetization Error — 1 violation

**Severity: MINOR** | **Impact: -2 quality points**

`scripts/quality_score.py:34`: Method `deduct` appears before `bonus` in class `QualityScore`.

---

## Improvement Plan

### Tier 1: Quick Wins (quality score 83 -> 90+)

These can be done in 1-2 sessions and immediately improve the quality score.

#### T1.1: Fix alphabetization error
- **Files**: `scripts/quality_score.py`
- **Effort**: 5 minutes
- **Impact**: +2 quality points

#### T1.2: Fix unsafe unwrap() calls
- **Files**: `ha-server/src/main.rs`, `ha-state-store/src/lib.rs`, `ha-components/src/system_log.rs`
- **Effort**: 30 minutes
- **Impact**: Prevents potential panics on external input

#### T1.3: Remove dead code
- **Files**: `ha-py-bridge/src/py_bridge/runtime.rs`, `ha-py-bridge/src/py_bridge/service_bridge.rs`, `ha-template/src/globals.rs`, `ha-template/src/filters.rs`
- **Effort**: 15 minutes
- **Impact**: Cleaner codebase, fewer `#[allow(dead_code)]` annotations

#### T1.4: Add ticket numbers to TODOs
- **Files**: 7 files across 5 crates
- **Effort**: 30 minutes (create GitHub issues, update TODO comments)
- **Impact**: +5 quality points

#### T1.5: Extract state constants to ha-core
- **Files**: `ha-core/src/lib.rs` (add constants), `ha-server/src/main.rs` (use constants)
- **Effort**: 45 minutes
- **Impact**: Eliminates 25+ magic strings, matches Python HA convention

### Tier 2: Structural Improvements (quality score 90 -> 95)

These require focused sessions but significantly improve code quality.

#### T2.1: Add typed error enum to ha-api
- **Files**: New `ha-api/src/error.rs`, update `ha-api/src/websocket/handlers.rs`, `ha-api/src/auth.rs`
- **Effort**: 2-3 hours
- **Impact**: Fixes largest convention violation (~68 functions), better error reporting, enables proper error propagation
- **Approach**:
  1. Define `ApiError` and `WebSocketError` enums with `thiserror`
  2. Replace `Result<(), String>` signatures one handler at a time
  3. Add `impl IntoResponse for ApiError` for Axum integration

#### T2.2: Fix Result<T, String> in ha-registries
- **Files**: `ha-registries/src/area_registry.rs`, `floor_registry.rs`, `label_registry.rs`
- **Effort**: 1 hour
- **Impact**: Consistent error handling within the crate (7 functions)
- **Approach**: Extend existing `StorageError` or add `RegistryError` variants

#### T2.3: Fix Result<T, String> in ha-py-bridge
- **Files**: `ha-py-bridge/src/py_bridge/config_flow.rs`, `requirements.rs`
- **Effort**: 1 hour
- **Impact**: Use existing `PyBridgeError` (12 functions)

#### T2.4: Add Rust unit tests for zero-test crates
- **Files**: `ha-event-bus/src/lib.rs`, `ha-state-store/src/lib.rs`, `ha-service-registry/src/lib.rs`
- **Effort**: 3-4 hours
- **Impact**: 547 lines of core infrastructure covered by `cargo test`
- **Approach**: These are small single-file crates. Write basic unit tests for public APIs. Don't duplicate ha-compat coverage, but ensure `cargo test` alone catches regressions.
- **Note**: ha-registries (2,994 lines) deserves its own dedicated session.

#### T2.5: Add justification comments to clippy suppressions
- **Files**: 14 locations across ha-api, ha-py-bridge, ha-registries
- **Effort**: 30 minutes
- **Impact**: Documents why suppressions exist, prevents blind cargo-cult suppression

### Tier 3: Architectural Refactors (quality score 95+, long-term)

These are significant efforts that should be planned individually.

#### T3.1: Split ha-py-bridge into focused crates (ARCHITECTURAL)
- **Problem**: ha-py-bridge is 11,524 lines — more than all other crates combined. It contains FFI wrappers, embedded Python source strings, config flow orchestration, entity registration, service bridging, and the shim layer. CLAUDE.md says "keep the bridge thin" but the bridge is the largest crate.
- **Proposed split**:
  ```
  crates/ha-py-bridge/     → thin PyO3 #[pyclass] wrappers only (~1K lines)
  crates/ha-py-shim/       → Python shim layer (already separate on disk)
  crates/ha-py-codegen/    → macro/build.rs that generates FFI boilerplate
  ```
- **Key insight**: The 1,114-line `create_config_entries_wrapper` writes Python source as Rust string literals. This is a code generation problem solved by hand. A `build.rs` or proc macro generating Python wrappers from trait definitions would eliminate the largest class of god functions.
- **Effort**: 3-5 sessions (plan individually before starting)
- **Impact**: Eliminates 38 god functions, enforces thin bridge principle, makes FFI layer maintainable

#### T3.2: Handler registry pattern for ha-api WebSocket dispatch (ARCHITECTURAL)
- **Problem**: 393-line `handle_message` is a giant match — every new WebSocket command touches the dispatch function. Violates Open/Closed principle.
- **Proposed pattern**:
  ```rust
  // Each handler registers itself
  registry.register("get_states", GetStatesHandler);
  registry.register("call_service", CallServiceHandler);

  // Dispatch becomes one line
  let handler = registry.get(msg_type)?;
  handler.handle(context, payload).await
  ```
- **Benefits**: Each handler independently testable, new commands don't touch dispatch, handler metadata (auth requirements, schema) co-located with implementation.
- **Effort**: 2-3 sessions
- **Impact**: Eliminates ha-api's largest god function, makes WebSocket handlers independently testable

#### T3.3: Refactor ha-server service registration
- **Scope**: `register_core_services` (281 lines), `register_automation_services` (232 lines), etc.
- **Effort**: 1-2 sessions
- **Approach**: Extract service registration into a declarative table/macro. Each service becomes a struct with metadata + handler.

#### T3.4: Add Rust unit tests for ha-registries (defense in depth)
- **Problem**: 2,994 lines with zero Rust tests. Relies entirely on external Python HA compat tests. `cargo test` is blind to regressions in 5 registry types + storage layer.
- **Approach**: Keep HA compat tests AND add focused Rust unit tests. They serve different purposes: HA compat verifies *behavior matches Python HA*, Rust unit tests verify *internal invariants hold*.
- **Scope**: EntityRegistry, DeviceRegistry, AreaRegistry, FloorRegistry, LabelRegistry, Storage
- **Effort**: 2-3 sessions
- **Impact**: Coverage target is 95%+

#### T3.5: Add doc comments to ha-api public types
- **Scope**: ~14 types in websocket/types.rs, auth.rs, lib.rs
- **Effort**: 1 hour

---

## Recommended Execution Order

### Session 1: Quick Wins (T1.1 through T1.5)
**Goal**: Quality score from 83 to 90+
**Time**: ~2 hours
**Verification**: `make dev`, `python3 scripts/quality_score.py --verbose`

### Session 2: Error Handling (T2.1)
**Goal**: Typed errors in ha-api
**Time**: 2-3 hours
**Verification**: `cargo test -p ha-api`, `make lint`

### Session 3: Error Handling + Tests (T2.2, T2.3, T2.5)
**Goal**: Fix remaining String errors, add suppression justifications
**Time**: 2-3 hours
**Verification**: `make dev`

### Session 4: Core Crate Tests (T2.4)
**Goal**: Unit tests for event-bus, state-store, service-registry
**Time**: 3-4 hours
**Verification**: `cargo test -p ha-event-bus -p ha-state-store -p ha-service-registry`

### Session 5: ha-py-bridge split (T3.1)
**Goal**: Split monolith into ha-py-bridge + ha-py-shim + ha-py-codegen
**Time**: 3-5 sessions (plan first)
**Verification**: `make build`, `make test-python`, `make test-ha-compat`

### Session 6: WebSocket handler registry (T3.2)
**Goal**: Replace giant match dispatch with registry pattern in ha-api
**Time**: 2-3 sessions
**Verification**: `cargo test -p ha-api`, `make test-integration`

### Sessions 7+: Remaining Refactors (T3.3-T3.5)
**Goal**: Service registration refactor, ha-registries tests, doc comments
**Time**: Multiple sessions, each planned individually

---

## Verification

- [ ] `make build` passes
- [ ] `make test-rust` — all tests green
- [ ] `make lint` — zero warnings
- [ ] `./scripts/lint-alpha.py --all` — zero violations
- [ ] `python3 scripts/quality_score.py --summary` — score >= 90 after Tier 1+2
- [ ] `make test-ha-compat` — no regressions (76/77)

## Risks & Notes

- **ha-registries zero tests**: The comment says "covered by HA native tests". This is true (76/77 pass), but it means `cargo test` alone is blind to registry regressions. Adding Rust unit tests provides defense-in-depth without removing the ha-compat tests.
- **ha-api error refactor**: Changing 68 function signatures is high-churn. Do it in a single focused PR to minimize merge conflicts. Consider using the agent team pattern (T2.1 is a good candidate for `/team-implement`).
- **Auth bypass TODO**: The `connection.rs:220` TODO that accepts any token is a security issue. It should be prioritized regardless of quality score impact.
- **God function threshold**: The quality_score.py counts 48 (not all 129) because it likely uses a different metric or sampling. The 50-line rule from CLAUDE.md is the project convention.
