# Agent Teams: Coordination Protocol

**When a task requires multiple agents working on different parts of the codebase, follow these coordination rules.**

---

## When to Use Agent Teams

Agent teams are activated when:
- A change spans 3+ crates or modules
- Parallel work is possible and beneficial
- The orchestrator determines that sequential work would be too slow
- A complex refactor touches many files

## Team Structure

### Roles

| Role | Responsibility |
|------|---------------|
| **Lead** | Coordinates work, resolves conflicts, runs final verification |
| **Teammate** | Owns specific crate(s)/file(s), implements assigned changes |
| **Reviewer** | Reviews teammate output, checks for cross-cutting concerns |

### Assignment Rules

1. Each file is owned by exactly ONE teammate (no conflicts)
2. Each teammate knows which files they own
3. Cross-crate dependencies are identified BEFORE work begins
4. The lead maintains the dependency graph

---

## Coordination Protocol

### Phase 1: Planning

1. Lead breaks the task into subtasks
2. Lead assigns subtasks to teammates with file ownership
3. Lead identifies cross-crate dependencies and ordering constraints
4. All teammates acknowledge their assignments

### Phase 2: Implementation

1. Teammates work on their assigned files
2. If a teammate needs a type/trait from another teammate's crate:
   - Request the interface (trait signature, type definition) from the lead
   - Lead coordinates to get the interface defined first
   - Implementation against the interface proceeds in parallel
3. Teammates report completion to the lead

### Phase 3: Integration

1. Lead combines all changes
2. Lead runs full verification suite:
   ```bash
   make build
   make test-rust
   make lint
   ./scripts/lint-alpha.py --all
   make test-integration
   ```
3. If integration fails, lead identifies which teammate's change caused the issue
4. Targeted fix by the responsible teammate

### Phase 4: Review

1. Lead runs the review agents (per orchestrator protocol)
2. Issues are assigned to the teammate who owns the affected file
3. Fixes follow the same ownership rules

---

## Conflict Resolution

### File Conflicts

- If two teammates need to modify the same file, the lead must:
  1. Determine if the changes can be sequenced (one after the other)
  2. If not, one teammate is designated as the owner, the other provides a change request
  3. The owner applies both changes and verifies

### Interface Conflicts

- If teammates disagree on an interface design:
  1. Both propose their interface
  2. Lead evaluates against the plan's requirements
  3. Lead makes the decision (or escalates to the user)
  4. Decision is documented in the session log

### Build Conflicts

- If combined changes break the build:
  1. Lead identifies the conflict source using compiler errors
  2. Responsible teammate fixes their code
  3. Lead re-runs verification

---

## Communication Rules

1. **Be explicit** — state what you changed, what you need, what you are blocked on
2. **Be concise** — avoid lengthy explanations; use bullet points
3. **Be timely** — report completion or blockers immediately
4. **Use structured updates**:

```
[TEAMMATE: ha-entity] COMPLETED: Added EntityId::new_validated()
[TEAMMATE: ha-state] BLOCKED: Need EntityId type from ha-entity
[LEAD] RESOLVED: EntityId is available, ha-state can proceed
[TEAMMATE: ha-state] COMPLETED: State store uses EntityId
[LEAD] INTEGRATING: Running make check-all
```

---

## Quality Standards for Teams

- Each teammate must run crate-level checks before reporting completion:
  ```bash
  cargo build -p ha-<crate>
  cargo test -p ha-<crate>
  cargo clippy -p ha-<crate> -- -D warnings
  ```
- The lead runs workspace-level checks after integration
- No teammate may modify files outside their assignment without lead approval
- All teammates follow the same code conventions (`code-conventions.md`)

---

## Escalation

If a team cannot resolve an issue:
1. Lead documents the issue clearly
2. Lead presents options to the user
3. User makes the decision
4. Lead communicates the decision to all teammates
5. Work resumes

Never let a team disagreement block progress for more than one round of discussion.
