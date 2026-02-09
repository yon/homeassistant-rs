---
description: Run test suite — rust, python, integration, ha-compat, or all
---

# /test — Run Tests

Run the specified test suite and report results.

## Steps

1. Determine scope from argument (default: rust)
2. Run the appropriate make target
3. Report: total, passed, failed, skipped
4. If failures, show failing test output

## Scopes

| Argument | Command | What It Tests |
|----------|---------|---------------|
| (none) / `rust` | `make test-rust` | All Rust unit/integration tests |
| `python` | `make test-python` | Python shim + PyO3 extension |
| `integration` | `make test-integration` | WebSocket API tests |
| `ha-compat` | `make test-ha-compat` | HA compatibility suite |
| `all` | `make test` | Everything |

## Success Criteria
- All tests pass
- No regressions from previous run
