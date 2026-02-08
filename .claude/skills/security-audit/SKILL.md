---
description: Security review — OWASP, deps, secrets, permissions
---

# /security-audit — Security Audit

Run a comprehensive security assessment.

## Steps

1. Run `make audit` (cargo audit for dependency vulnerabilities)
2. Scan for hardcoded secrets in source files
3. Review authentication and authorization patterns in ha-api
4. Check unsafe blocks in Rust code
5. Review Python bridge boundary for injection risks
6. Invoke security-reviewer agent for deep analysis

## Scopes

| Argument | Focus |
|----------|-------|
| (none) | Full audit |
| `deps` | Dependency vulnerabilities only |
| `secrets` | Hardcoded credentials scan only |
| `code` | Code-level security review only |

## Output

Security assessment report with findings by severity and remediation steps.
