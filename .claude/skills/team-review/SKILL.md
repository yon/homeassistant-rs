---
description: Parallel agent team code review (each reviewer in own session)
---

# /team-review — Parallel Agent Team Review

Spawn multiple review agents simultaneously, each with full context.

## Steps

1. Identify scope (changed files or specified scope)
2. Spawn review team:
   - Teammate A: security-reviewer (auth, input handling, secrets)
   - Teammate B: architecture-reviewer (module boundaries, SOLID)
   - Teammate C: code-reviewer (correctness, readability, principles)
   - Teammate D: test-reviewer (coverage, quality, TDD compliance)
3. Wait for all teammates to complete
4. Synthesize into unified review report
5. Present merged findings by severity

## Arguments

- `/team-review` — review uncommitted changes
- `/team-review crates/ha-api` — review specific crate

## Requirements

- Agent teams must be enabled: `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=true`
- All agents are READ-ONLY — they cannot modify files

## Why Use Team Review?

Each reviewer has its own context window, allowing deeper analysis than sequential review. No context dilution between review dimensions.
