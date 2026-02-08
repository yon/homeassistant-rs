---
description: General-purpose agent team orchestration
---

# /swarm — General-Purpose Agent Team

Spawn a team of agents for any parallelizable task.

## Steps

1. Analyze the task and identify parallelizable subtasks
2. Determine team composition and file ownership
3. Write detailed prompts for each teammate
4. Spawn team
5. Monitor progress
6. Synthesize results
7. Verify combined output: `make dev`

## Common Patterns

### Research Swarm
Multiple agents investigate different angles simultaneously:
- Teammate A: trace data flow through crates
- Teammate B: search git history for related changes
- Teammate C: analyze Python HA behavior in vendor/ha-core
- Teammate D: review test coverage for the area

### Module-Parallel Implementation
Each teammate implements in a different crate:
- Teammate A: crates/ha-config/ changes
- Teammate B: crates/ha-api/ changes
- Teammate C: crates/ha-components/ changes
- All: coordinated via shared task list

### Debugging Swarm
Competing hypotheses investigated simultaneously:
- Teammate A: check for race condition
- Teammate B: check for data corruption
- Teammate C: check for Python bridge issue
- Lead: synthesize findings into diagnosis

## Arguments

- `/swarm research: how does automation trigger evaluation work?`
- `/swarm implement: add new input_select component across crates`
- `/swarm debug: entity state updates are intermittently lost`

## Requirements

- Agent teams must be enabled
- 2-4 teammates for most tasks (sweet spot)
- Clear file ownership per teammate
