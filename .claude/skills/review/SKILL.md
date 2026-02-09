---
description: Combined multi-agent code review (6 dimensions)
---

# /review — Multi-Agent Code Review

Run a comprehensive code review using specialized subagents.

## Execution

1. Identify changed files: `git diff --name-only HEAD~1` (or specified scope)
2. Select agents based on change type (see table below)
3. Read each selected agent definition from `.claude/agents/{name}.md`
4. Spawn Task tool calls for selected agents — **all in the same response** for parallel execution:

```
Task(subagent_type="senior-code-reviewer",
     prompt="<content of .claude/agents/code-reviewer.md>\n\nScope: Review these files:\n{file_list}\n\nRead each file and produce your review report.")
```

5. Collect all subagent results
6. Synthesize into single review report by severity (Critical > Major > Minor)

## Agent Selection

| Change Type | Agents | subagent_type |
|-------------|--------|---------------|
| Any code change | code-reviewer | senior-code-reviewer |
| API endpoints (ha-api) | + security-reviewer | security-code-auditor |
| New crate / major refactor | + architecture-reviewer | senior-code-reviewer |
| Test files | + test-reviewer | senior-code-reviewer |
| Performance-sensitive | + performance-reviewer | senior-code-reviewer |

## Arguments

- `/review` — review uncommitted changes
- `/review HEAD~3` — review last 3 commits
- `/review crates/ha-api` — review specific crate
