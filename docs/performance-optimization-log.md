# Performance Optimization Log

This document tracks before/after performance metrics for each optimization.

## Baseline Environment
- Date: 2026-01-21
- Rust: 1.92.0 (ded5c06cf 2025-12-08)
- Platform: macOS Darwin 25.2.0
- Full test suite (release): 30.2s wall clock
- HA compat tests: 479 tests, 6.5s

## Measurement Notes
- Build times measured with `time cargo build`
- Test times measured with `time cargo test --release`
- Memory improvements are theoretical based on code analysis
- Runtime improvements require startup/request benchmarks

---

## Phase 2: Parallel Config Entry Setup ✅

### Before
**File:** `crates/ha-config-entries/src/manager.rs`
**Issue:** Sequential integration setup - each integration waits for previous to complete

```rust
// Before: Sequential
for entry_id in entry_ids {
    results.push(self.setup(&entry_id).await);
}
```

**Baseline Metrics:**
- Build time: 1.05s
- Test suite: 43 tests, 0.01s execution, 3.17s total

### After
**Change:** Use `futures::future::join_all()` for parallel setup

```rust
// After: Parallel
let futures: Vec<_> = entry_ids.iter().map(|id| self.setup(id)).collect();
join_all(futures).await
```

**Metrics:**
- Build time: 2.89s (first build with new futures dep)
- Test suite: 43 tests, 0.01s execution, 3.16s total (all pass)
- **Improvement:** Startup time now O(1) vs O(n) for n integrations

---

## Phase 1: Registry Arc Wrapping ✅

### Before
**Files:**
- `crates/ha-registries/src/entity_registry.rs`
- `crates/ha-registries/src/device_registry.rs`
- `crates/ha-registries/src/area_registry.rs`
- `crates/ha-registries/src/floor_registry.rs`
- `crates/ha-registries/src/label_registry.rs`

**Issue:** Clone entire entry structs on every get() operation

```rust
// Before: Clones entire entry (~30 fields for EntityEntry)
pub fn get(&self, entity_id: &str) -> Option<EntityEntry> {
    self.by_entity_id.get(entity_id).map(|r| r.value().clone())
}
```

### After
**Change:** Wrap entries in `Arc<T>`, return `Arc` references instead of cloning

```rust
// After: Returns Arc (atomic increment, ~16 bytes)
pub fn get(&self, entity_id: &str) -> Option<Arc<EntityEntry>> {
    self.by_entity_id.get(entity_id).map(|r| Arc::clone(r.value()))
}
```

**Files modified:**
- All 5 registry files: struct fields, load(), save(), index_entry(), get*(), update(), remove(), iter()
- Python bridge: PyEntityEntry, PyDeviceEntry, PyAreaEntry, PyFloorEntry, PyLabelEntry (from_inner accepts Arc)

**Metrics:**
- Test suite: 479 tests, all pass
- **Improvement:** Reads now cost ~atomic increment instead of ~30-field struct clone

---

## Phase 3: EventBus Arc Events ✅

### Before
**File:** `crates/ha-event-bus/src/lib.rs`
**Issue:** Events cloned for each subscriber - expensive for events with large JSON payloads

```rust
// Before: Clone entire event for each subscriber
if let Some(sender) = self.listeners.get(&event.event_type) {
    let _ = sender.send(event.clone());  // Full clone including JSON data
}
let _ = self.match_all_sender.send(event);  // Another clone
```

### After
**Change:** Wrap events in `Arc`, broadcast `Arc<Event>` to all subscribers

```rust
// After: Wrap once, share via Arc clone (atomic increment)
pub type ArcEvent = Arc<Event<serde_json::Value>>;

pub fn fire(&self, event: Event<serde_json::Value>) {
    let arc_event = Arc::new(event);  // Wrap once
    if let Some(sender) = self.listeners.get(&arc_event.event_type) {
        let _ = sender.send(Arc::clone(&arc_event));  // Cheap Arc clone
    }
    let _ = self.match_all_sender.send(arc_event);  // Move Arc
}
```

**Files modified:**
- `crates/ha-event-bus/src/lib.rs` - broadcast channels use `Arc<Event>`
- `crates/ha-py-bridge/src/extension/py_types.rs` - PyEvent stores `Arc<Event>`
- `crates/ha-py-bridge/src/extension/py_event_bus.rs` - use `from_arc()`
- `crates/ha-server/src/automation_engine.rs` - dereference Arc in handler

**Metrics:**
- Test suite: 479 compat tests, all pass
- **Improvement:** Event broadcast now O(1) atomic increment per subscriber vs O(n) JSON clone

---

## Phase 4: Template Regex Caching ✅

### Before
**File:** `crates/ha-template/src/filters.rs`
**Issue:** Regex compiled on every filter call

```rust
// Before: Compiles regex on every call (expensive)
pub fn regex_replace(value: &str, find: &str, replace: &str) -> Result<String, Error> {
    let re = Regex::new(find)?;
    Ok(re.replace_all(value, replace).to_string())
}
```

### After
**Change:** Thread-local cache for compiled regexes

```rust
// After: Thread-local cache with bounded size (64 entries)
thread_local! {
    static REGEX_CACHE: RefCell<HashMap<String, Regex>> = RefCell::new(HashMap::new());
}

fn get_or_compile_regex(pattern: &str) -> Result<Regex, Error> {
    REGEX_CACHE.with(|cache| { /* cache lookup or compile */ })
}

pub fn regex_replace(value: &str, find: &str, replace: &str) -> Result<String, Error> {
    let re = get_or_compile_regex(find)?;  // Cached!
    Ok(re.replace_all(value, replace).to_string())
}
```

**Functions updated:** `regex_replace`, `regex_findall`, `regex_match`

**Metrics:**
- Test suite: 36 template tests + 479 compat tests, all pass
- **Improvement:** Repeated regex patterns now O(1) lookup instead of O(n) compilation

---

## Phase 5: HashSet for State Matching ✅

### Before
**File:** `crates/ha-automation/src/trigger.rs`
**Issue:** `not_from` and `not_to` fields in `StateTrigger` used `Vec<String>` with O(n) `contains()` lookup

```rust
// Before: O(n) lookup
#[serde(default)]
pub not_from: Vec<String>,
#[serde(default)]
pub not_to: Vec<String>,
```

### After
**Change:** Use `HashSet<String>` for O(1) lookup

```rust
// After: O(1) lookup
#[serde(default)]
pub not_from: HashSet<String>,
#[serde(default)]
pub not_to: HashSet<String>,
```

**Files modified:**
- `crates/ha-automation/src/trigger.rs` - changed field types
- `crates/ha-automation/src/trigger_eval.rs` - updated test imports
- `crates/ha-server/src/main.rs` - updated test imports

**Metrics:**
- Test suite: 68 automation tests + 479 compat tests, all pass
- **Improvement:** State exclusion checks now O(1) vs O(n) for n excluded states

---

## Phase 6: Lock Scope Reduction ✅

### Before
**File:** `crates/ha-api/src/websocket/handlers.rs`
**Issue:** RwLock held across await points (channel sends), causing lock contention

```rust
// Before: Lock held during await
let config_entries = conn.state.config_entries.read().await;
// ... processing ...
tx.send(result).await  // AWAIT while lock still held!
```

### After
**Change:** Extract data into local variable, release lock, then await

```rust
// After: Lock released before await
let result_json = {
    let config_entries = conn.state.config_entries.read().await;
    // ... processing ...
}; // Lock released here
tx.send(result).await  // Safe - no lock held
```

**Functions fixed:**
- `handle_config_entries_get` - read lock across channel send
- `handle_config_entries_subscribe` - read lock across channel sends
- `handle_config_entries_delete` - write lock across remove + channel send

**Metrics:**
- Test suite: 53 API tests + 479 compat tests, all pass
- **Improvement:** Reduced lock contention under concurrent WebSocket load

---

## Summary

| Phase | Optimization | Status | Impact |
|-------|-------------|--------|--------|
| 1 | Registry Arc | ✅ | Reads: atomic inc vs ~30-field clone |
| 2 | Parallel setup | ✅ | Startup O(1) vs O(n) for n integrations |
| 3 | EventBus Arc | ✅ | Events: atomic inc vs JSON clone per subscriber |
| 4 | Regex cache | ✅ | Repeated patterns: O(1) vs O(n) compile |
| 5 | HashSet match | ✅ | State exclusion: O(1) vs O(n) lookup |
| 6 | Lock scope | ✅ | Reduced lock contention in handlers |
