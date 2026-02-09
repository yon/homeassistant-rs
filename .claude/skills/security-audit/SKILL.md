---
description: Security review — OWASP, deps, secrets, permissions
---

# /security-audit — Security Audit

Run a comprehensive security assessment using specialized subagent.

## Execution

1. Run `make audit` (cargo audit for dependency vulnerabilities)
2. Spawn security review subagent:
```
Task(subagent_type="security-code-auditor",
     prompt="<content of .claude/agents/security-reviewer.md>\n\nPerform a security audit of: {scope}\n\nCheck:\n- Hardcoded secrets in source files\n- Authentication/authorization patterns in ha-api\n- Unsafe blocks in Rust code\n- Python bridge boundary for injection risks\n- Input validation at all system boundaries")
```
3. Collect and present findings by severity

## Scopes

| Argument | Focus |
|----------|-------|
| (none) | Full audit |
| `deps` | Dependency vulnerabilities only (`make audit`) |
| `secrets` | Hardcoded credentials scan only |
| `code` | Code-level security review only |

## Output

Security assessment report with findings by severity and remediation steps.
