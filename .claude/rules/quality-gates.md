# Quality Gates

**Every change must pass through quality gates before being presented to the user.**

---

## Gate Levels

### Gate 1: Build (Mandatory)

The project must compile without errors.

```bash
make build
```

- **Zero tolerance** for build errors
- If the build fails, fix it before proceeding to any other gate
- Never present broken code to the user

### Gate 2: Tests (Mandatory)

All existing tests must pass, and new code must have tests.

```bash
make test-rust        # Rust unit + integration tests
make test-python      # Python bridge tests
make test-integration # Cross-boundary integration tests
make test-ha-compat   # Home Assistant compatibility tests
```

- **Zero tolerance** for test regressions
- New features require new tests (written FIRST per TDD)
- Bug fixes require regression tests
- Aim for meaningful coverage, not coverage percentage theater

### Gate 3: Lint & Style (Mandatory)

Code must pass all linters with zero warnings.

```bash
make lint                        # fmt-check + clippy + lint-makefile
./scripts/lint-alpha.py --all    # Alphabetization enforcement
```

- **Zero tolerance** for lint warnings
- Fix warnings, do not suppress them (unless documented and justified)
- Alphabetization is enforced on all ordered lists (use declarations, match arms, struct fields, enum variants, etc.)

### Gate 4: Security (Required for Sensitive Changes)

```bash
make audit    # cargo audit for known vulnerabilities
```

- Required when adding/updating dependencies
- Required when modifying authentication, authorization, or data handling
- Required when touching the Python bridge or FFI boundary

### Gate 5: Quality Score (Advisory)

```bash
python3 scripts/score.py --summary
```

- Score >= 80: auto-commit eligible
- Score 60-79: present to user with notes
- Score < 60: requires review and remediation before presenting

---

## Gate Enforcement Rules

1. **Gates are sequential** — do not skip ahead if an earlier gate fails
2. **Fix forward** — when a gate fails, fix the issue, do not disable the gate
3. **Document exceptions** — if a gate must be skipped, document why in the commit message
4. **Never suppress warnings globally** — address them individually with justification
5. **Run all gates before presenting** — the user should never see code that fails a mandatory gate

## Quick Reference

| Gate | Command | Tolerance | When Required |
|------|---------|-----------|---------------|
| Build | `make build` | Zero errors | Always |
| Test (Rust) | `make test-rust` | Zero failures | Always |
| Test (Python) | `make test-python` | Zero failures | Python bridge changes |
| Test (Integration) | `make test-integration` | Zero failures | Cross-boundary changes |
| Test (HA Compat) | `make test-ha-compat` | Zero failures | API/schema changes |
| Lint | `make lint` | Zero warnings | Always |
| Alphabetization | `./scripts/lint-alpha.py --all` | Zero violations | Always |
| Security | `make audit` | Zero critical | Dependency/auth changes |
| Quality Score | `scripts/score.py` | >= 80 preferred | Always (advisory) |
