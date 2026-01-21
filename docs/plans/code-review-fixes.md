# Code Review Fixes: HA Compatibility & Rust Idioms

## Overview

Based on code review comparing Rust implementation to Python HA reference (`vendor/ha-core`).
Issues organized by priority for systematic fixing.

---

## Phase 1: High Priority - Compatibility Bugs

### 1.1 Add State Length Validation (MAX 255 chars)

**File:** `crates/ha-state-machine/src/lib.rs`
**Reference:** `vendor/ha-core/homeassistant/core.py:2357-2365`

Python HA validates state length and falls back to "unknown" if exceeded:
```python
MAX_LENGTH_STATE_STATE = 255
if len(new_state) > MAX_LENGTH_STATE_STATE:
    new_state = STATE_UNKNOWN
```

**Fix:**
- Add `MAX_STATE_LENGTH: usize = 255` constant to `ha-core`
- In `StateMachine::set()`, check length and replace with "unknown" if exceeded
- Log warning when truncation occurs

---

### 1.2 Add EVENT_STATE_REPORTED Event

**File:** `crates/ha-state-machine/src/lib.rs`
**Reference:** `vendor/ha-core/homeassistant/core.py:2333-2350`

Python HA fires `EVENT_STATE_REPORTED` when state is unchanged but reported:
```python
if same_state and same_attr:
    old_state.last_reported = now
    self._bus.async_fire_internal(EVENT_STATE_REPORTED, {...})
    return
```

**Fix:**
- Add `EVENT_STATE_REPORTED` constant to `ha-core`
- Update `StateMachine::set()` to fire this event when state unchanged
- Update `last_reported` timestamp on existing state

---

### 1.3 Fix EntityId Validation Regex

**File:** `crates/ha-core/src/entity_id.rs:78-81`
**Reference:** `vendor/ha-core/homeassistant/core.py:179-182`

Python regex rules:
- No leading/trailing underscores in object_id
- No double underscores (`__`) in domain
- Pattern: `^(?!.+__)(?!_)[\da-z_]+(?<!_)\.(?!_)[\da-z_]+(?<!_)$`

**Fix:**
- Update `EntityId::new()` validation to match Python exactly
- Add unit tests for edge cases: `__domain.test`, `domain.test_`, `do__main.test`

---

### 1.4 Add `force_update` Parameter to StateMachine::set()

**File:** `crates/ha-state-machine/src/lib.rs`
**Reference:** `vendor/ha-core/homeassistant/core.py:2313`

Python's `async_set` has `force_update` that forces `last_changed` update:
```python
same_state = old_state.state == new_state and not force_update
```

**Fix:**
- Add `force_update: bool` parameter to `set()` and `async_set()`
- When true, treat state as changed even if value is same
- Update all call sites (default to `false`)

---

### 1.5 Replace Panic with Result in EntityRegistry::update()

**File:** `crates/ha-registries/src/entity_registry.rs:536`

Current code panics:
```rust
} else {
    panic!("Entity not found: {}", entity_id);
}
```

**Fix:**
- Change return type to `Result<EntityEntry, RegistryError>`
- Add `RegistryError::EntityNotFound` variant
- Update all call sites to handle the Result

---

## Phase 2: Medium Priority - Missing Features

### 2.1 Add `origin_event` to Context

**File:** `crates/ha-core/src/context.rs`
**Reference:** `vendor/ha-core/homeassistant/core.py:1211,1223,1311-1312`

**Fix:**
- Add `origin_event: Option<Event>` field to `Context`
- Set it when event spawns a new context

---

### 2.2 Add MATCH_ALL Event Filtering

**File:** `crates/ha-event-bus/src/lib.rs`
**Reference:** `vendor/ha-core/homeassistant/core.py`

Python excludes certain events from MATCH_ALL:
```python
EVENTS_EXCLUDED_FROM_MATCH_ALL = {
    EVENT_HOMEASSISTANT_CLOSE,
    EVENT_STATE_REPORTED,
}
```

**Fix:**
- Add exclusion set constant
- Filter these events when dispatching to MATCH_ALL subscribers

---

### 2.3 Add SupportsResponse::ONLY Validation

**File:** `crates/ha-service-registry/src/lib.rs:188`

Missing validation: error if service is `ONLY` and `return_response=False`.

**Fix:**
```rust
if !return_response && registered.description.supports_response == SupportsResponse::Only {
    return Err(ServiceError::ResponseRequired);
}
```

---

### 2.4 Add `error_reason_translation_key` to ConfigEntry

**File:** `crates/ha-config-entries/src/entry.rs`

**Fix:**
- Add `error_reason_translation_key: Option<String>` field
- Update serialization/deserialization

---

## Phase 3: Code Quality - Rust Idioms

### 3.1 Reduce unwrap()/expect() Usage

**Scope:** 1014+ occurrences across 55 files

**Approach:**
- Audit critical paths in order of importance:
  1. `ha-py-bridge` (Python interop - crashes are bad UX)
  2. `ha-api` (HTTP/WS handlers - should return errors)
  3. `ha-state-machine` (core functionality)
- Replace with `?` operator or proper error handling
- Keep `expect()` only where panic is truly impossible

---

### 3.2 Extract Duplicate Config Flow Result Conversion

**File:** `crates/ha-py-bridge/src/py_bridge/config_flow.rs`

`convert_flow_result_standalone()` (lines 113-251) and `convert_flow_result()` (lines 520-667) are nearly identical.

**Fix:**
- Create shared `fn convert_flow_result_common(...)`
- Both functions call the common implementation

---

### 3.3 Add Newtypes for IDs

**Current:** Entry IDs, device IDs, flow IDs are all `String`.

**Fix:** Create newtypes to prevent mixing:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntryId(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowId(String);
```

---

### 3.4 Fix Naming: `ActiveFlow.handler` → `domain`

**File:** `crates/ha-py-bridge/src/py_bridge/config_flow.rs:29`

```rust
struct ActiveFlow {
    handler: String,  // Name says "handler" but it's actually domain
```

**Fix:** Rename to `domain` for clarity.

---

## Phase 4: Robustness

### 4.1 Fix Race Condition in ConfigFlowManager

**File:** `crates/ha-py-bridge/src/py_bridge/config_flow.rs`

Flow state is read, then `spawn_blocking` runs, then state is written - race if concurrent calls.

**Fix:**
- Use a per-flow lock or mark flow as "in progress" before releasing read lock
- Or use `RwLock::upgradable_read()` pattern

---

### 4.2 Optimize Event Loop Creation

**File:** `crates/ha-py-bridge/src/py_bridge/config_flow.rs:83-105`

Creating new Python event loop per flow step is expensive.

**Fix:**
- Create a dedicated Python executor with persistent event loop
- Reuse across all flow operations

---

## Verification

After each phase:
```bash
cargo test --workspace
make test-integration
make run  # Manual smoke test
```

---

## Implementation Order

| Order | Task | Priority | Files |
|-------|------|----------|-------|
| 1 | State length validation | High | ha-state-machine/src/lib.rs, ha-core |
| 2 | EVENT_STATE_REPORTED | High | ha-state-machine/src/lib.rs, ha-core |
| 3 | EntityId validation fix | High | ha-core/src/entity_id.rs |
| 4 | force_update parameter | High | ha-state-machine/src/lib.rs |
| 5 | EntityRegistry panic → Result | High | ha-registries/src/entity_registry.rs |
| 6 | Context origin_event | Medium | ha-core/src/context.rs |
| 7 | MATCH_ALL filtering | Medium | ha-event-bus/src/lib.rs |
| 8 | SupportsResponse::ONLY | Medium | ha-service-registry/src/lib.rs |
| 9 | DRY flow result conversion | Medium | ha-py-bridge |
| 10 | Naming fixes | Low | Multiple |
| 11 | Newtype IDs | Low | Multiple |
| 12 | Race condition fix | Low | ha-py-bridge |

---

## Critical Files

| File | Changes |
|------|---------|
| `crates/ha-core/src/lib.rs` | Add constants (MAX_STATE_LENGTH, EVENT_STATE_REPORTED) |
| `crates/ha-core/src/entity_id.rs` | Fix validation regex |
| `crates/ha-core/src/context.rs` | Add origin_event field |
| `crates/ha-state-machine/src/lib.rs` | State validation, force_update, STATE_REPORTED event |
| `crates/ha-event-bus/src/lib.rs` | MATCH_ALL filtering |
| `crates/ha-service-registry/src/lib.rs` | SupportsResponse::ONLY validation |
| `crates/ha-registries/src/entity_registry.rs` | Panic → Result |
| `crates/ha-py-bridge/src/py_bridge/config_flow.rs` | DRY, naming, race condition |
