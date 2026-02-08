---
description: Combined multi-agent code review (6 dimensions)
---

# /review — Multi-Agent Code Review

Run a comprehensive code review using specialized agents.

## Steps

1. Identify changed files: `git diff --name-only HEAD~1` (or staged files)
2. Determine which agents to invoke based on change type
3. Run each agent sequentially (or in parallel via team if enabled)
4. Synthesize results into a single review report

## Agent Selection

| Change Type | Agents Used |
|-------------|-------------|
| Any code change | code-reviewer |
| API endpoints (ha-api) | + security-reviewer |
| New crate or major refactor | + architecture-reviewer |
| Test files | + test-reviewer |
| Performance-sensitive code | + performance-reviewer |
| Documentation | + doc-reviewer |

## Output

Combined review report with:
- Issues by severity (Critical → Minor)
- Engineering principles compliance matrix
- Positive highlights
- Overall quality assessment

## Variants

- `/review` — review uncommitted changes
- `/review HEAD~3` — review last 3 commits
- `/review crates/ha-api` — review specific crate
