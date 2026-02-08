---
description: Parallel agent team implementation with adversarial review
---

# /team-implement — Parallel Implementation with Adversarial Review

Spawn implementation subagents working on different crates in parallel, with separate review subagents.

## Execution

1. Read the approved plan from `working/plans/`
2. Partition work by crate — each subagent owns specific files
3. Spawn implementation Task calls **in the same response**:

**Per crate/module:**
```
Task(subagent_type="production-code-engineer",
     prompt="You own crates/{crate}/. Implement the following from the plan:\n{task_description}\n\nRules:\n- Only modify files in your assigned crate\n- Follow TDD: write failing tests first, then implement\n- Run: cargo build -p {crate} && cargo test -p {crate}\n- Report what you changed and test results")
```

4. After all implementers complete, spawn review subagent:

```
Task(subagent_type="senior-code-reviewer",
     prompt="<content of .claude/agents/code-reviewer.md>\n\nReview the changes in:\n{changed_files}")
```

5. If reviewer finds Critical/Major issues, route fixes to the responsible implementer (max 3 rounds)
6. Run `make dev` on combined result
7. Present summary

## Arguments

- `/team-implement` — use most recent approved plan
- `/team-implement working/plans/2026-02-08_feature.md` — specific plan

## File Ownership Rules

- Each subagent owns specific crate(s) — no overlapping file edits
- Root config files (Cargo.toml, Makefile): only main context modifies
- **Iron Rule**: The subagent that writes code NEVER reviews it
