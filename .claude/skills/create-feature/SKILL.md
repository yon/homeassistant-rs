---
description: Full TDD feature workflow with planning
---

# /create-feature — Create a New Feature

Complete workflow: plan → test → implement → verify → review.

## Execution

### Phase 1: Plan
1. Enter plan mode (`EnterPlanMode`)
2. Draft implementation plan — which crate(s), HA compat impact, tests needed
3. Save plan to `quality_reports/plans/YYYY-MM-DD_description.md`
4. Get user approval before proceeding

### Phase 2: TDD Red (Write Failing Tests)
5. Write tests that describe the desired behavior
6. Run `make test-rust` — confirm tests **FAIL**
7. **If tests pass, STOP** — the feature already exists or the test is wrong

### Phase 3: TDD Green (Implement)
8. Write minimum code to make tests pass
9. Run `make test-rust` — confirm tests **PASS**

### Phase 4: Refactor
10. Clean up while keeping tests green
11. Run `make test-rust` after each change

### Phase 5: Verify
12. Run full verification: `make build && make test-rust && make lint`
13. Run `./scripts/lint-alpha.py --all`

### Phase 6: Review (via subagents)
14. Spawn review subagents using Task tool:
```
Task(subagent_type="senior-code-reviewer",
     prompt="<.claude/agents/code-reviewer.md content>\n\nReview: {changed_files}")
```
For API changes, also spawn:
```
Task(subagent_type="security-code-auditor",
     prompt="<.claude/agents/security-reviewer.md content>\n\nAudit: {changed_files}")
```

### Phase 7: Fix & Score
15. Address Critical/Major review findings (max 3 rounds)
16. Run `python3 scripts/quality_score.py --summary`
17. Present summary with quality score

## Arguments

- `/create-feature entity-history` — feature name/description

## HA Compatibility

For features affecting the API:
- Check `vendor/ha-core/` for Python HA behavior
- Run `make test-ha-compat` to verify compatibility
