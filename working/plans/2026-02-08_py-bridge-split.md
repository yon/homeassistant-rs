# Plan: T3.1 — Split ha-py-bridge into Focused Crates

**Date:** 2026-02-08
**Status:** DRAFT
**Task:** Refactor the ha-py-bridge monolith (15,602 Rust lines, 41 files) into focused crates

## Problem

`ha-py-bridge` is the largest crate in the workspace, serving two mutually exclusive purposes:
- **Extension mode** (`extension` feature): Compiles as a Python extension module (cdylib) — Python imports Rust
- **Bridge mode** (`py_bridge` feature): Embeds Python in the Rust binary — Rust calls Python

These modes share some conversion utilities but are otherwise independent codepaths behind `#[cfg(feature = "...")]` gates. The crate also contains:
- ~1,350 lines of embedded Python as raw string literals in Rust functions
- 5 duplicate `json_to_pyobject`/`pyobject_to_json` conversion functions
- 8 god functions (>100 lines each), the worst being `create_config_entries_wrapper()` at 1,116 lines

## Approach: 4 Sessions, Incremental Refactoring

Split into **2 crates** (not 3):
- `ha-py-ext` — Extension mode (Python imports Rust)
- `ha-py-bridge` — Bridge mode (Rust embeds Python), retains the name since `ha-server` depends on it

**Pre-split work** (Sessions 1-2) cleans up internal structure to make the split trivial.

---

## Session 1: Extract Embedded Python to .py Files

### Problem
~1,350 lines of Python live as `r#"..."#` string literals inside Rust functions in `hass_wrapper.rs`. This makes them:
- Invisible to Python linters/formatters
- Hard to read and maintain
- Impossible to test independently

### What Changes

| File | Change |
|------|--------|
| `crates/ha-py-bridge/python/config_entries_wrapper.py` | **NEW** — extracted from `create_config_entries_wrapper()` |
| `crates/ha-py-bridge/python/entity_service.py` | **NEW** — extracted from `call_python_entity_service()` |
| `crates/ha-py-bridge/python/application_credentials.py` | **NEW** — extracted from `inject_application_credentials()` |
| `crates/ha-py-bridge/python/registries_init.py` | **NEW** — extracted from `initialize_ha_registries()` |
| `crates/ha-py-bridge/python/python_loader.py` | **NEW** — extracted from `load_python_integration()` |
| `crates/ha-py-bridge/src/py_bridge/hass_wrapper.rs` | Replace inline Python with `include_str!("../../python/<name>.py")` |

### How
1. For each function with embedded Python:
   - Extract the Python string to a `.py` file
   - Replace the inline string with `include_str!()`
   - The Rust format!() calls that inject variables become Python function parameters
2. Run `make build && make test-rust && make lint`
3. Run `make run` to verify server still starts and config flows work

### Estimated Impact
- `hass_wrapper.rs`: 2,136 → ~800 lines (-1,336 lines of embedded Python)
- 5 new `.py` files totaling ~1,350 lines
- Net: same total, but Python is now in proper Python files

---

## Session 2: Deduplicate Conversions + Break God Functions

### Problem
- 5 copies of `json_to_pyobject`/`pyobject_to_json` scattered across files
- `create_config_entries_wrapper()` is 1,116 lines even after Python extraction (~76 lines Rust setup)

### What Changes

| File | Change |
|------|--------|
| `crates/ha-py-bridge/src/py_bridge/py_utils.rs` | Canonical conversion functions (already has `pyobject_to_json`) |
| `crates/ha-py-bridge/src/py_bridge/hass_wrapper.rs` | Break up remaining god functions, use shared conversions |
| `crates/ha-py-bridge/src/extension/services.rs` | Use shared conversions instead of local copies |
| `crates/ha-py-bridge/src/extension/entity.rs` | Use shared conversions instead of local copies |
| `crates/ha-py-bridge/src/extension/config_flow.rs` | Use shared conversions instead of local copies |

### How
1. Add missing conversion functions to `py_utils.rs` as the single source of truth
2. Replace all local copies with imports from `py_utils`
3. Break `create_config_entries_wrapper()` into smaller functions (setup, flow handling, entry creation)
4. Run full verification

### Estimated Impact
- ~200 lines of duplicate code removed
- Largest function drops from 1,116 to ~200 lines

---

## Session 3: Split Extension Mode into ha-py-ext

### Problem
Extension mode and bridge mode are independent codepaths that share only `py_utils.rs`. They should be separate crates.

### What Changes

| File | Change |
|------|--------|
| `crates/ha-py-ext/Cargo.toml` | **NEW** — extension mode crate (cdylib) |
| `crates/ha-py-ext/src/lib.rs` | **NEW** — `#[pymodule]` definition |
| `crates/ha-py-ext/src/` | **MOVE** — all files from `ha-py-bridge/src/extension/` |
| `crates/ha-py-bridge/Cargo.toml` | Remove `extension` feature, simplify deps |
| `crates/ha-py-bridge/src/lib.rs` | Remove extension-mode code, keep bridge-only |
| `Cargo.toml` (workspace) | Add `ha-py-ext` member |

### How
1. Create `ha-py-ext` crate with files moved from `ha-py-bridge/src/extension/`
2. Move shared `py_utils.rs` to a location both crates can use (or duplicate the small file)
3. Update `ha-py-bridge` to remove the `extension` feature flag
4. Update workspace Cargo.toml
5. Run full verification including `make install-dev` (extension build)

### Estimated Impact
- `ha-py-bridge`: 15,602 → ~8,500 lines (bridge-only)
- `ha-py-ext`: ~7,100 lines (extension-only)
- Feature flag complexity eliminated

---

## Session 4: End-to-End Verification + Cleanup

### What
1. Run full test suite: `make dev && make test-ha-compat`
2. Test extension mode: `make install-dev && make test-python`
3. Test bridge mode: `make run` (start server, verify config flows)
4. Update documentation in `docs/architecture.md`
5. Run quality score and code review

---

## Verification (per session)

- [ ] `make build` — zero errors
- [ ] `make test-rust` — all tests pass
- [ ] `make lint` — zero warnings
- [ ] `./scripts/lint-alpha.py --all` — zero violations
- [ ] `make test-ha-compat` — no regressions (Sessions 1-2)
- [ ] `make install-dev` — extension builds (Session 3)
- [ ] `make run` — server starts (Session 3-4)

## Key Design Decisions

1. **2 crates, not 3** — `ha-py-codegen` would be premature; the codegen is tightly coupled to what it generates
2. **Pre-split cleanup first** — Sessions 1-2 make Session 3 trivial by cleaning up shared code
3. **`include_str!()` for Python** — compiles Python into binary at build time, no runtime file loading needed
4. **Keep `ha-py-bridge` name** — `ha-server` already depends on it; renaming would touch more files for no benefit
5. **Leave shim layer as-is** — `python/shim/` is already a separate directory, no structural change needed

## Risks

- **Format strings**: Embedded Python uses Rust `format!()` to inject variables. Extracting to `.py` files requires converting these to Python function parameters or template variables
- **Build system**: Maturin builds the extension; splitting may require updating `pyproject.toml`
- **Feature flag interactions**: Some `#[cfg]` gates may be more entangled than exploration revealed
