# Session 1: Integration Failure Diagnosis

**Date:** 2026-02-08
**Plan:** Phase 8 — Entity Platform Completion
**Goal:** Categorize 55 failing integrations to find fixable patterns

## Test Configuration

- **Command:** `CONFIG_DIR=/tmp/ha-config/data RUST_LOG=debug make run-release`
- **Duration:** ~45 seconds of startup
- **Log:** `/tmp/ha-setup.log` (1856 lines)

## Results Summary

| Category | Entries | Unique Integrations | Fixable? |
|----------|---------|---------------------|----------|
| Successfully loaded | — | 29 | N/A |
| `aiousbwatcher` dependency | 15 | 5 | MAYBE (pip install) |
| Custom integrations (not in vendor/) | 15 | 9 | NO (expected) |
| Context tuple error (`.id`) | 10 | 1 (mobile_app x10) | YES |
| `async_update_entry` missing | 3 | 3 (tesla_fleet, ecobee, cast) | YES |
| `add_update_listener` missing | 3 | 3 (openai_conversation, homekit, openai_conversation) | YES |
| Registry method missing | 2 | 2 (reolink, unifi) | YES |
| Missing `hass.data` key | 2 | 2 (apple_tv, samsungtv) | MAYBE |
| `ConfigEntryNotReady` (network) | 2 | 2 | NO (expected) |
| `ValueError: Implementation not available` | 2 | 2 | NO (hardware) |
| `ConfigEntryAuthFailed` | 1 | 1 | NO (expected) |

**Total:** 55 failed entries across ~28 unique integrations

## Successfully Loaded Integrations (29)

airthings, apple_tv, backup, cast, ecobee, econet, ecowitt, flo,
forecast_solar, homekit, lutron_caseta, matter, miele, mobile_app,
myuplink, openai_conversation, opower, proximity, reolink, roborock,
samsungtv, sense, sun, switch_as_x, tesla_fleet, thread, unifi,
uptime, wemo

*Note: Some loaded partially — mobile_app loaded device_tracker but
sensor/binary_sensor platforms failed. openai_conversation loaded
but ai_task/conversation platforms failed.*

## Fixable Patterns (Priority Order)

### P1: Context Tuple Error — 10 entries, 1 integration (mobile_app)

**Error:** `'tuple' object has no attribute 'id'`
**Affected:** mobile_app (all 10 entries are separate mobile devices)

**Root cause:** In `services.rs` and `service_bridge.rs`, the Context is
created as a plain Python dict instead of a proper Context object. Python
code accessing `context.id` gets a dict/tuple instead of an object with
attributes.

**Fix:** Create a proper Context object (from `homeassistant.core.Context`)
instead of a dict when constructing context for Python callbacks.

**Files:**
- `crates/ha-py-bridge/src/py_bridge/wrappers/services.rs` (lines 198-203)
- `crates/ha-py-bridge/src/py_bridge/service_bridge.rs` (context_to_pyobject)

**Impact:** Unlocks all 10 mobile_app entries (many sensor/binary_sensor entities)

### P2: `async_update_entry` Missing — 3 entries, 3 integrations

**Error:** `'types.SimpleNamespace' object has no attribute 'async_update_entry'`
**Affected:** tesla_fleet, ecobee, cast

**Root cause:** `hass.config_entries` is a SimpleNamespace created in
`hass_wrapper.rs::create_config_entries_wrapper()`. The `async_update_entry`
method is NOT set on the wrapper. It exists in HA's ConfigEntries class
but was never stubbed.

**Fix:** Add `async_update_entry` Python stub to `config_entries_wrapper.py`
and expose it on the SimpleNamespace in `hass_wrapper.rs`.

**Files:**
- `crates/ha-py-bridge/embedded_python/config_entries_wrapper.py`
- `crates/ha-py-bridge/src/py_bridge/hass_wrapper.rs` (line ~470)

**Impact:** Unlocks tesla_fleet, ecobee, cast (3 integrations)

### P3: `add_update_listener` Missing — 3 entries, 2 integrations

**Error:** `'builtins.ConfigEntry' object has no attribute 'add_update_listener'`
**Affected:** openai_conversation (x2), homekit

**Root cause:** `ConfigEntryWrapper` in `config_entry.rs` doesn't implement
`add_update_listener`. HA's ConfigEntry stores a list of callbacks that
fire when entry options change.

**Fix:** Add `add_update_listener` method to `ConfigEntryWrapper` that stores
callbacks and returns an unsubscribe callable.

**Files:**
- `crates/ha-py-bridge/src/py_bridge/wrappers/config_entry.rs`

**Impact:** Unlocks openai_conversation, homekit (2 integrations, but
openai_conversation already partially loads)

### P4: Registry Method Missing — 2 entries, 2 integrations

**Error (reolink):** `'dict' object has no attribute 'get_devices_for_config_entry_id'`
**Error (unifi):** `'dict' object has no attribute 'get_entries_for_config_entry_id'`
**Also (mobile_app platforms):** `'dict' object has no attribute 'get_entries_for_config_entry_id'`

**Root cause:** Device registry or entity registry exposed as a dict
instead of a proper wrapper with helper methods.

**Fix:** Add `get_devices_for_config_entry_id` and `get_entries_for_config_entry_id`
methods to the registry wrappers.

**Files:**
- `crates/ha-py-bridge/src/py_bridge/wrappers/registries.rs`

**Impact:** Unlocks reolink, unifi; fixes mobile_app sensor/binary_sensor platforms

### P5: Missing `hass.data` Keys — 2 entries, 2 integrations

**Error (apple_tv):** `KeyError: 'credentials'`
**Error (samsungtv):** `KeyError: 'ssdp'`

**Root cause:** These integrations expect certain HA subsystems to be loaded
and registered in `hass.data` (e.g., SSDP discovery, credential manager).
These subsystems don't exist in the Rust implementation.

**Fix:** Pre-populate `hass.data` with stub objects for common subsystems,
or add shims for ssdp/credentials. Lower priority — these are harder to
stub correctly.

**Impact:** Unlocks apple_tv, samsungtv (but may need network anyway)

## Not Fixable (Expected Failures)

### `aiousbwatcher` Missing (15 entries, 5 integrations)

bluetooth, homekit_controller, esphome, ibeacon, airthings_ble — all need
USB/Bluetooth hardware support (`aiousbwatcher` package). Could `pip install`
it but these integrations also need physical hardware.

### Custom Integrations (15 entries, 9 integrations)

composite, hacs, spook, samsungtv_smart, midea_ac_lan, etc. — these are
HACS/custom integrations installed by the user, not in `vendor/ha-core/`.
Not in scope for homeassistant-rs.

### Network/Auth/Hardware (5 entries)

ConfigEntryNotReady (network timeout), ConfigEntryAuthFailed (credentials),
ValueError: Implementation not available — all expected when running
without network/hardware access.

## Platform-Level Errors (Partial Successes)

These integrations loaded but some platforms failed:

| Integration | Platform | Error |
|------------|----------|-------|
| mobile_app | binary_sensor, sensor | `get_entries_for_config_entry_id` (P4) |
| openai_conversation | ai_task, conversation | `subentries` attribute missing |
| flo | switch | `Cannot get non-set current platform` |

## Session 1 Outcome

**Identified 5 fixable patterns.** Top 3 (P1-P3) are purely missing methods
that unlock 6+ integrations (15+ config entries). P4 fixes platform-level
errors for already-loaded integrations.

**Priority for Session 2:**
1. P1: Context object fix (10 entries, mobile_app)
2. P2: `async_update_entry` (3 integrations)
3. P3: `add_update_listener` (2 integrations)
4. P4: Registry helper methods (2+ integrations, mobile_app platforms)

**Expected impact of fixing P1-P4:** 29 → ~35 fully loaded integrations,
plus mobile_app sensors/binary_sensors working.

*Note: Some integrations that "loaded" (like ecobee, cast, tesla_fleet)
have BOTH a successful entry and a failing entry. The successful entry
might be a different config entry for the same integration. Fixing the
errors would make ALL entries for that integration work.*
