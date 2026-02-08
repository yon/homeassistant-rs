# Orchestrator Protocol: Contractor Mode

**After a plan is approved, the orchestrator runs autonomously: implement → verify → review → fix → score.**

---

## When Contractor Mode Activates

The orchestrator takes over after:
1. A plan is approved (Plan-First Workflow Rule 1, step 7)
2. The user says "just do it" or "implement this"
3. A `/create-feature`, `/fix-bug`, or `/refactor` skill is invoked

## The Orchestrator Loop

### Step 1: Write Tests (TDD Red Phase)
- Write failing tests that describe the desired behavior
- Run `make test-rust` — confirm tests FAIL (red)
- If tests pass, the feature already exists or the test is wrong

### Step 2: Implement (TDD Green Phase)
- Write the minimum code to make tests pass
- Run `make test-rust` — confirm tests PASS (green)

### Step 3: Refactor
- Clean up while keeping tests green
- Run `make test-rust` after each refactor step

### Step 4: Verify
- Run `make build` — zero errors
- Run `make test-rust` — all green
- Run `make lint` — zero warnings
- Run `./scripts/lint-alpha.py --all` — alphabetization clean

### Step 5: Review
Select relevant agents based on change type:
| Change Type | Agents |
|-------------|--------|
| Logic/algorithms | code-reviewer |
| API endpoints | code-reviewer, security-reviewer |
| Architecture/new crate | architecture-reviewer |
| Python bridge | code-reviewer, security-reviewer |
| Tests | test-reviewer |
| Performance-sensitive | performance-reviewer |
| Documentation | doc-reviewer |

### Step 6: Fix Issues
- Address Critical issues immediately
- Address Major issues before presenting
- Note Minor issues for user decision
- Max 5 review-fix rounds

### Step 7: Score
Run `python3 scripts/quality_score.py --summary` for quality gate assessment.

### Step 8: Present Results
Show the user:
- Summary of changes made
- Quality score
- Files modified
- Tests added/changed
- Any unresolved minor issues

## "Just Do It" Mode

When the user says "just do it": skip the final approval pause and auto-commit if score >= 80.

## Team-Based Orchestration

For multi-crate changes, the orchestrator may spawn agent teams:
- Each teammate owns specific crate(s)
- Lead runs `make check-all` on combined result
- See `agent-teams.md` for coordination rules
