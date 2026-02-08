---
description: Parallel agent team code review (each reviewer in own session)
---

# /team-review — Parallel Subagent Code Review

Spawn 4 review subagents simultaneously, each with its own context window.

## Execution

1. Identify scope: `git diff --name-only HEAD~1` (or specified scope)
2. Read each agent definition from `.claude/agents/`
3. Spawn 4 Task tool calls **in the same response** for parallel execution:

**Task 1 — Security:**
```
Task(subagent_type="security-code-auditor",
     prompt="<content of .claude/agents/security-reviewer.md>\n\nReview these files:\n{file_list}")
```

**Task 2 — Architecture:**
```
Task(subagent_type="senior-code-reviewer",
     prompt="<content of .claude/agents/architecture-reviewer.md>\n\nReview these files:\n{file_list}")
```

**Task 3 — Code Quality:**
```
Task(subagent_type="senior-code-reviewer",
     prompt="<content of .claude/agents/code-reviewer.md>\n\nReview these files:\n{file_list}")
```

**Task 4 — Test Quality:**
```
Task(subagent_type="senior-code-reviewer",
     prompt="<content of .claude/agents/test-reviewer.md>\n\nReview these files:\n{file_list}")
```

4. Collect all 4 results
5. Synthesize into unified report sorted by severity (Critical > Major > Minor)

## Arguments

- `/team-review` — review uncommitted changes
- `/team-review crates/ha-api` — review specific crate

## Why Parallel?

Each reviewer has its own context window — no context dilution between dimensions. All 4 run simultaneously via Task tool.
