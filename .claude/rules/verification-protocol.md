# Verification Protocol

**Every change must be verified before it is presented to the user. No exceptions.**

---

## The Verification Sequence

After any code change, run the following checks in order. Stop at the first failure and fix before continuing.

### 1. Build Check

```bash
make build
```

Must exit with code 0. If it fails, the change is broken — fix it immediately.

### 2. Test Check

```bash
make test-rust
```

All tests must pass. If any test fails:
- Read the failure message carefully
- Determine if the test is correct (regression) or needs updating (intentional change)
- Fix the code or update the test with justification
- Re-run until green

For Python bridge changes, also run:
```bash
make test-python
```

For cross-boundary changes:
```bash
make test-integration
```

For API or schema changes:
```bash
make test-ha-compat
```

### 3. Lint Check

```bash
make lint
```

Zero warnings required. This runs:
- `cargo fmt --all -- --check` (formatting)
- `cargo clippy --workspace --all-targets -- -D warnings` (lints)
- Makefile lint checks

### 4. Alphabetization Check

```bash
./scripts/lint-alpha.py --all
```

Zero violations required. This checks alphabetical ordering of:
- `use` declarations
- Enum variants
- Struct fields (where applicable)
- Match arms (where applicable)
- Module declarations

### 5. Format Check

If lint fails on formatting:
```bash
make fmt
```

Then re-run `make lint` to confirm.

### 6. Full Verification

For final verification before presenting to the user:
```bash
make dev
```

This runs the complete suite: `fmt` + `clippy` + `test`. Equivalent to what CI runs.

---

## Verification Rules

### Rule 1: Never Present Unverified Code

Do not show code to the user that has not passed at least Build + Test + Lint. If time pressure exists, say so explicitly.

### Rule 2: Verify After Every Significant Change

A "significant change" is anything that modifies logic, adds/removes code, or changes dependencies. Run at minimum:
```bash
make build && make test-rust && make lint
```

### Rule 3: Read Error Messages Carefully

When a check fails:
1. **Read the full error output** — do not guess
2. **Identify the root cause** — not just the symptom
3. **Fix the actual problem** — do not apply band-aids
4. **Re-run the check** — confirm the fix works
5. **Run all checks again** — ensure the fix did not break something else

### Rule 4: Do Not Silence Warnings

- Never add `#[allow(...)]` without explicit justification
- Never add `// clippy::...` suppression without documenting why
- If a warning seems wrong, investigate before suppressing

### Rule 5: Track Verification State

In session logs, record:
- Which checks were run
- Whether they passed or failed
- What was done to fix failures

---

## Common Verification Patterns

### After Adding a New Crate

```bash
make build                       # Verify workspace compiles
make test-rust                   # Verify no regressions
make lint                        # Verify lint passes
./scripts/lint-alpha.py --all    # Verify alphabetization
```

### After Modifying a Public API

```bash
make build                       # Verify workspace compiles
make test-rust                   # Verify existing tests
make test-ha-compat              # Verify HA compatibility
make lint                        # Verify lint passes
```

### After Updating Dependencies

```bash
make build                       # Verify everything compiles
make test-rust                   # Verify no regressions
make audit                       # Check for vulnerabilities
```

### Quick Check During Development

```bash
make dev                         # fmt + clippy + test (all-in-one)
```

---

## Pre-Commit Hook

The pre-commit hook runs automatically on `git commit`:

1. `cargo fmt --all -- --check` — formatting
2. `./scripts/lint-alpha.py --staged` — alphabetization on staged files
3. `cargo clippy --workspace --all-targets -- -D warnings` — lints

If any check fails, the commit is rejected. Fix the issues and try again.
