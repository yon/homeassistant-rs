---
description: Full TDD feature workflow with planning
---

# /create-feature — Create a New Feature

Complete workflow: plan → test → implement → verify → review.

## Steps

1. **Plan** — Enter plan mode, draft implementation plan
   - Which crate(s) will be affected?
   - Does this affect HA API compatibility?
   - What tests are needed?
2. **Save plan** — Write to `quality_reports/plans/`
3. **Get approval** — Present plan, wait for user OK
4. **Write tests** (TDD Red) — Failing tests first
5. **Implement** (TDD Green) — Minimum code to pass
6. **Refactor** — Clean up while green
7. **Verify** — `make dev` (fmt + clippy + test)
8. **Review** — Run relevant review agents
9. **Fix** — Address review findings
10. **Score** — Run quality assessment
11. **Present** — Summary with quality score

## Arguments

- `/create-feature entity-history` — feature name/description
- The argument seeds the plan description

## HA Compatibility Check

For features that affect the API:
- Check `vendor/ha-core/` for Python HA behavior
- Run `make test-ha-compat` to verify compatibility
- Document any intentional deviations
