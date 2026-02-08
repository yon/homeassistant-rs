# Plan-First Workflow & Context Preservation

**These rules apply to ALL tasks, regardless of language or file type.**

---

## Rule 1: Plan Before You Build

**For any non-trivial task, enter Plan Mode FIRST before writing code or making edits.**

A task is "non-trivial" if it involves:
- Creating or modifying more than one file
- Implementing a new feature or fixing a non-trivial bug
- Refactoring existing code
- Any task the user describes with multiple steps
- Any task where the approach is not immediately obvious

### The Plan-First Protocol

1. **Enter Plan Mode** — use `EnterPlanMode` to switch to planning
2. **Draft the plan** — outline what will change, which files are affected, and in what order
3. **Save the plan** — write it to `quality_reports/plans/` (see Rule 2)
4. **Present to user** — explain the plan and wait for approval
5. **Only after approval** — exit plan mode
6. **Immediately save initial session log** — capture the goal, plan summary, and key context while it's fresh (see Rule 5)
7. **Implement via orchestrator** — the orchestrator protocol takes over (see `orchestrator-protocol.md`): implement → verify → review → fix → score → present results

### What a Good Plan Includes

- **Task description** — what are we trying to accomplish?
- **Acceptance criteria** — how do we know when it's done?
- **Files to modify** — which crates/files will be created, edited, or deleted?
- **Tests to write** — what tests are needed BEFORE implementation? (TDD)
- **Approach** — step-by-step implementation strategy
- **Dependencies** — what must happen before what?
- **Verification steps** — how will we confirm it worked? (build, test, lint)
- **Risks** — what could go wrong? What's the rollback plan?
- **HA compatibility** — does this change affect API compatibility with Python HA?

### When to Skip Planning

You may skip plan mode for:
- Single-file edits with a clear scope (fix a typo, rename a variable)
- Running existing skills/commands (`/build`, `/test`, `/lint`)
- Purely informational questions
- Tasks the user explicitly says to do immediately

---

## Rule 2: Save Plans to Disk

**Every plan must be saved to a file so it survives context compression.**

### Where to Save

```
quality_reports/plans/
├── 2026-02-07_add-websocket-endpoint.md
├── 2026-02-07_refactor-state-store.md
└── ...
```

### Naming Convention

`YYYY-MM-DD_short-description.md`

### Plan File Format

```markdown
# Plan: [Short Description]

**Date:** [YYYY-MM-DD HH:MM]
**Status:** DRAFT | APPROVED | IN PROGRESS | COMPLETED
**Task:** [What the user asked for]

## Acceptance Criteria

- [ ] [Criterion 1]
- [ ] [Criterion 2]

## Tests to Write First (TDD)

- [ ] [Test 1 — describe what it validates]
- [ ] [Test 2 — describe what it validates]

## Approach

1. [Step 1 — Write failing tests]
2. [Step 2 — Implement minimum code to pass]
3. [Step 3 — Refactor]
4. ...

## Crates & Files to Modify

- `crates/ha-xxx/src/file.rs` — [what changes]
- `crates/ha-yyy/src/file.rs` — [what changes]

## Verification

- [ ] `make build` passes
- [ ] `make test-rust` — all tests green
- [ ] `make lint` — zero warnings
- [ ] `./scripts/lint-alpha.py --all` — alphabetization clean
- [ ] `make test-ha-compat` — no regressions (if applicable)

## Risks & Rollback

[Any risks, open questions, or decisions made]
```

---

## Rule 3: Never /clear — Rely on Auto-Compression

**NEVER use `/clear` to reset the conversation. Use auto-compression instead.**

### Why This Matters

- `/clear` is a **nuclear option** — destroys all context and design decisions
- Auto-compression provides **graceful degradation** that preserves critical context
- Saved plans offer a **safety net** with full strategy documented

### Session Recovery Protocol

If starting a new session (or after heavy compression):

1. Read `CLAUDE.md` for project context
2. Read the most recent plan in `quality_reports/plans/`
3. Check `git log --oneline -10` for recent changes
4. Check `git diff` for any uncommitted work
5. State what you understand the current task to be

---

## Rule 4: Continuous Learning with [LEARN] Tags

**When a mistake is corrected, immediately save a `[LEARN:tag]` entry to MEMORY.md.**

Format: `[LEARN:category] Incorrect assumption → correct fact`

Common categories: `pattern`, `rust`, `python`, `test`, `convention`, `ha-compat`, `performance`, `security`, `workflow`.

---

## Rule 5: Session Logging

**Session logs live at `quality_reports/session_logs/YYYY-MM-DD_description.md`.**

### 5a. Post-Plan Log — Immediately after plan approval, create the session log file
### 5b. Incremental Logging — Append 1-3 line entries as significant events happen
### 5c. End-of-Session Log — Add summary, open questions, unresolved issues

**Do not wait to be asked for any of these.** All three behaviors are proactive.
