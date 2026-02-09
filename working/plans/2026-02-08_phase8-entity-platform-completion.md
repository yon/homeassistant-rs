# Plan: Phase 8 — Entity Platform Completion

**Date:** 2026-02-08
**Status:** DRAFT
**Task:** Complete entity platform support: polling, service dispatch, and more integrations loading

## Current State

Entity platform infrastructure is ~95% complete. 33/108 integrations load against production backup. Key gaps: no entity polling after initial state, service dispatch unverified end-to-end, 72 integrations failing for unknown reasons.

**What already works:**
- Platform forwarding (`async_forward_entry_setups`)
- Entity registration in Rust registries
- Initial state setting via `StatesWrapper`
- Import fallthrough to native HA via `__path__` extension
- `ServicesWrapper.async_register()` for Python→Rust service registration
- `register_python_entity_services()` in main.rs
- `call_python_entity_service()` bridge
- `StatesWrapper.async_set_internal()` for entity state writes

## Approach: Diagnosis First, Then Fix Patterns

---

### Session 1: Diagnose Integration Failures

**Goal:** Categorize the 72 failing integrations to find fixable patterns.

**Steps:**
1. Run server against production backup with verbose logging:
   ```bash
   CONFIG_DIR=/tmp/ha-config/data RUST_LOG=debug make run-release 2>&1 | tee /tmp/ha-setup.log
   ```
2. Extract and categorize failures from log output:
   - Missing hass wrapper methods/attributes (FIXABLE)
   - Missing Python dependencies (MAYBE FIXABLE — pip install)
   - Network/hardware unreachable (EXPECTED — skip)
   - Auth/credential issues (EXPECTED — skip)
   - Async bridge errors (FIXABLE)
   - Import errors from shim layer (FIXABLE)
3. Count integrations per failure pattern
4. Identify top 3-5 patterns that unlock the most integrations
5. Save categorized report to `working/logs/`

**Verification:** Report exists with clear priorities.
**Expected outcome:** Actionable list of 3-5 patterns, each affecting 5+ integrations.

---

### Session 2: Fix High-Impact Failure Patterns

**Goal:** Fix the top failure patterns from Session 1 to load 15-20 more integrations.

**Likely candidates** (to be confirmed by diagnosis):

| Pattern | Likely Fix | Files |
|---------|-----------|-------|
| Missing `hass.*` method | Add to HassWrapper | `wrappers/hass.rs` |
| Missing `hass.data` key | Populate in setup | `hass_wrapper.rs` |
| DataUpdateCoordinator fails | Fix async bridge gap | `wrappers/hass.rs` or shim |
| Missing `hass.bus.*` method | Add to EventBusWrapper | `wrappers/events.rs` |
| Component setup failure | Add component shim | `ha-py-ext/python/...` |

**Steps:**
1. For each fixable pattern (TDD):
   - Write a test that reproduces the failure
   - Implement the fix
   - Verify the test passes
2. Run `make build && make test-rust && make lint`
3. Run server against production backup, count loaded integrations
4. Iterate on next pattern

**Verification:** `make build && make test-rust && make lint` pass. Integration count increases.
**Expected outcome:** 33 → 50+ integrations loading.

---

### Session 3: Entity Polling Verification

**Goal:** Prove entities update after initial state (DataUpdateCoordinator or manual polling).

**Steps:**
1. Pick a loaded integration that uses DataUpdateCoordinator (e.g., `sun`, `weather`, or an API-based integration)
2. Run server and observe whether entity states change over time
3. If states DON'T change, trace the failure:
   - Does `DataUpdateCoordinator.__init__()` succeed?
   - Does `async_config_entry_first_refresh()` run?
   - Does the coordinator's `_async_update_data()` get scheduled?
   - Does `entity.async_write_ha_state()` reach `StatesWrapper.async_set_internal()`?
4. Fix whatever breaks (likely missing hass methods for scheduling)
5. If DataUpdateCoordinator works, verify with WebSocket subscription:
   ```json
   {"id": 1, "type": "subscribe_events", "event_type": "state_changed"}
   ```
   Watch for state_changed events from Python entities.

**Key files to check/modify:**
- `crates/ha-py-ext/python/homeassistant/core.py` — hass wrapper (async_create_task, async_add_executor_job)
- `crates/ha-py-bridge/src/py_bridge/wrappers/hass.rs` — HassWrapper methods
- `crates/ha-py-bridge/embedded_python/config_entries_wrapper.py` — entity state updates

**Verification:** Entity states change over time when observed via WebSocket.
**Expected outcome:** Entities from coordinator-based integrations update every SCAN_INTERVAL.

---

### Session 4: Service Dispatch End-to-End

**Goal:** Verify WebSocket service calls reach Python entities and update state.

**Steps:**
1. Connect via WebSocket and authenticate
2. Find a controllable entity (light, switch) from a loaded integration
3. Send service call:
   ```json
   {"id": 10, "type": "call_service", "domain": "light", "service": "turn_on",
    "service_data": {"entity_id": "light.some_entity"}}
   ```
4. Verify state changes in response
5. If it fails, trace the path:
   - Does ServiceRegistry have `light.turn_on` registered?
   - Does `register_python_entity_services()` run?
   - Does `call_python_entity_service()` find the entity?
   - Does `entity_service.py::_call_entity_service_sync()` modify the entity?
   - Does `_update_entity_state_sync()` push to StatesWrapper?
6. Fix whatever breaks
7. Also verify: `_register_domain_services()` in config_entries_wrapper.py — currently logs but doesn't register. Either make it call `hass.services.async_register()` or remove it (since main.rs already handles this).

**Key files:**
- `crates/ha-api/src/websocket/handlers/service.rs` — WebSocket entry point
- `crates/ha-server/src/main.rs` — `register_python_entity_services()` (lines 934-1076)
- `crates/ha-py-bridge/src/py_bridge/hass_wrapper.rs` — `call_python_entity_service()`
- `crates/ha-py-bridge/embedded_python/entity_service.py` — entity method dispatch
- `crates/ha-py-bridge/embedded_python/config_entries_wrapper.py` — `_register_domain_services()`

**Verification:** WebSocket service call changes entity state. Frontend toggle works.
**Expected outcome:** Entity services work from frontend.

---

## Success Criteria

| Session | Metric | Target |
|---------|--------|--------|
| 1 | Failure categorization | 3-5 fixable patterns identified |
| 2 | Integration count | 33 → 50+ loading |
| 3 | Entity polling | States change over time via WebSocket |
| 4 | Service dispatch | Frontend toggle changes entity state |

## Verification (per session)

- [ ] `make build` — zero errors
- [ ] `make test-rust` — all tests pass
- [ ] `make lint` — zero warnings
- [ ] `./scripts/lint-alpha.py --all` — zero violations
- [ ] Server starts against production backup
- [ ] Session-specific verification passes

## Risks

- **Diagnosis may reveal unexpected patterns** — pivot plan accordingly
- **DataUpdateCoordinator may need deep async bridge work** — could expand Session 3
- **Some integrations may need network access** — can't test all in local environment
- **Service dispatch may need entity_id routing changes** — could expand Session 4
