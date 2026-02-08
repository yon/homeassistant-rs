---
description: Run linters, formatters, and static analysis
---

# /lint — Code Quality Checks

Run all linting and report results.

## Steps

1. Run `make lint` (includes fmt-check + clippy + lint-makefile)
2. Run `./scripts/lint-alpha.py --all` (alphabetization check)
3. Report combined results

## What Gets Checked

| Tool | What It Checks |
|------|---------------|
| `cargo fmt --check` | Rust code formatting |
| `cargo clippy -D warnings` | Rust linting (warnings = errors) |
| `lint-makefile` | Makefile target alphabetization |
| `lint-alpha.py` | Match arms, mod decls, Cargo.toml deps, Python class members |

## Auto-Fix

If linting fails due to formatting:
1. Run `make fmt` to auto-fix
2. Re-run `make lint` to verify
3. Report what was fixed
