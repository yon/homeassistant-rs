---
description: Safe refactoring with test-first verification
---

# /refactor — Safe Refactoring

Refactor code safely: verify tests pass before AND after.

## Execution

1. **Baseline** — Run `make test-rust` and capture results
2. **Plan** — Enter plan mode, identify what to refactor and why
3. **Save plan** — Write to `quality_reports/plans/YYYY-MM-DD_description.md`
4. **Approval** — Present plan, wait for user OK
5. **Add tests** — If coverage gaps exist, add tests FIRST
6. **Refactor** — Make changes incrementally
7. **Verify after each step** — `make test-rust` stays green
8. **Full verify** — `make build && make test-rust && make lint`
9. **Review** — Spawn review subagents:
```
Task(subagent_type="senior-code-reviewer",
     prompt="<.claude/agents/architecture-reviewer.md content>\n\nReview this refactoring:\n{changed_files}")
Task(subagent_type="senior-code-reviewer",
     prompt="<.claude/agents/code-reviewer.md content>\n\nReview this refactoring:\n{changed_files}")
```

## Arguments

- `/refactor crates/ha-state-store` — scope of refactoring
- `/refactor extract-service` — description of refactoring

## Rules

- NO behavior changes — tests must pass identically before and after
- If a test breaks, the refactoring changed behavior — fix or reconsider
- Refactor in small, verified steps — not one big change
