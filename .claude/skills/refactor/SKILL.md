---
description: Safe refactoring with test-first verification
---

# /refactor — Safe Refactoring

Refactor code safely: verify tests pass before AND after.

## Steps

1. **Baseline** — Run `make test-rust` and capture results
2. **Plan** — Enter plan mode, identify what to refactor and why
3. **Save plan** — Write to `quality_reports/plans/`
4. **Approval** — Present plan, wait for user OK
5. **Add tests** — If coverage gaps exist, add tests FIRST
6. **Refactor** — Make changes incrementally
7. **Verify after each step** — `make test-rust` stays green
8. **Full verify** — `make dev`
9. **Review** — Run architecture-reviewer and code-reviewer

## Arguments

- `/refactor crates/ha-state-store` — scope of refactoring
- `/refactor extract-service` — description of refactoring

## Rules

- NO behavior changes — tests must pass identically before and after
- If a test breaks, the refactoring changed behavior — fix or reconsider
- Refactor in small, verified steps — not one big change
