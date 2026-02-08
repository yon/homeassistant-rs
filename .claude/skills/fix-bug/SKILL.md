---
description: Structured bug fix workflow with root cause analysis
---

# /fix-bug — Fix a Bug

Structured bug fix: reproduce → root cause → test → fix → verify.

## Steps

1. **Understand** — Read the bug report/description
2. **Reproduce** — Write a failing test that demonstrates the bug
3. **Root cause** — Analyze why the bug occurs
4. **Plan fix** — Minimal change to fix the root cause
5. **Implement** — Fix the bug (test should now pass)
6. **Regression check** — `make test-rust` (all tests pass)
7. **Verify** — `make dev` (fmt + clippy + test)
8. **Review** — Run code-reviewer agent on the fix

## Arguments

- `/fix-bug #42` — GitHub issue number
- `/fix-bug entity state not updating` — bug description

## Rules

- ALWAYS write a regression test BEFORE fixing
- The test must FAIL before the fix and PASS after
- Minimal change: fix the bug, don't refactor surrounding code
- Commit the test WITH the fix — they travel together
