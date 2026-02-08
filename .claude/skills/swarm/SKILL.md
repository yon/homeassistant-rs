---
description: General-purpose agent team orchestration
---

# /swarm — General-Purpose Parallel Subagents

Spawn a team of subagents for any parallelizable task.

## Execution

1. Analyze the task and identify parallelizable subtasks
2. Choose subagent_type for each:
   - Research: `"Explore"` — fast codebase search
   - Planning: `"Plan"` — design implementation approach
   - Implementation: `"production-code-engineer"` — write code
   - Review: `"senior-code-reviewer"` — review code
   - Security: `"security-code-auditor"` — security analysis
   - Shell/infra: `"code-shell-expert"` — system tasks
3. Spawn all Task calls **in the same response** for parallel execution
4. Collect results and synthesize
5. If implementation was involved, run `make dev` to verify

## Common Patterns

### Research Swarm
```
Task 1: Explore — "trace data flow through crates/ha-api"
Task 2: Explore — "search git history for changes to state machine"
Task 3: Explore — "analyze Python HA behavior in vendor/ha-core for {feature}"
```

### Module-Parallel Implementation
```
Task 1: production-code-engineer — "implement X in crates/ha-config/"
Task 2: production-code-engineer — "implement Y in crates/ha-api/"
Task 3: production-code-engineer — "implement Z in crates/ha-components/"
```

### Debugging Swarm
```
Task 1: Explore — "check for race condition in event bus"
Task 2: Explore — "check for data corruption in state store"
Task 3: Explore — "check Python bridge for GIL deadlock"
```

## Arguments

- `/swarm research: how does automation trigger evaluation work?`
- `/swarm implement: add new input_select component across crates`
- `/swarm debug: entity state updates are intermittently lost`

## Guidelines

- 2-4 subagents is the sweet spot
- Each subagent should have clear, non-overlapping scope
- For implementation: assign file ownership explicitly
- Always verify combined output with `make dev` when code was changed
