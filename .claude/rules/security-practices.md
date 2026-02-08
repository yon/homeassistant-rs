# Security Practices

**Security is a property of the system, not a feature to add later.**

---

## Core Security Principles

### Principle 1: Never Trust External Input

All data from outside the system boundary is untrusted until validated.

- **Network input** — HTTP requests, WebSocket messages, MQTT payloads
- **File input** — configuration files, YAML, JSON, user uploads
- **Environment variables** — may be set by untrusted processes
- **Python bridge data** — treat as external input (crosses FFI boundary)

Validate at the boundary, then pass validated types internally.

### Principle 2: Secrets Management

Secrets must never appear in logs, error messages, debug output, or version control.

- Use `secrecy::Secret<T>` or equivalent wrappers for sensitive values
- Implement `Debug` manually to redact sensitive fields
- Never log authentication tokens, passwords, or API keys
- Never commit `.env` files, credentials, or private keys
- Use `#[serde(skip_serializing)]` for sensitive fields in serializable structs

### Principle 3: Least Privilege

Every component should have the minimum permissions necessary.

- File system access: only the directories the component needs
- Network access: only the endpoints the component needs
- Database access: only the tables/operations the component needs
- OS permissions: run as unprivileged user when possible

### Principle 4: Defense in Depth

Do not rely on a single layer of security.

- Input validation at the boundary AND within processing logic
- Authentication AND authorization checks
- Encryption in transit AND at rest
- Rate limiting AND abuse detection

---

## Dependency Security

### Auditing

```bash
make audit    # Runs cargo audit
```

- Run on every dependency addition or update
- Run periodically (at minimum before each release)
- Zero tolerance for critical vulnerabilities
- Document any accepted advisories with justification

### Dependency Review Checklist

Before adding a new dependency:

1. **Is it necessary?** Can we accomplish this with existing deps or std?
2. **Is it maintained?** When was the last commit? Are issues addressed?
3. **Is it audited?** Check for known vulnerabilities
4. **Is it minimal?** Does it pull in a large dependency tree?
5. **Is it trustworthy?** Who maintains it? Is it widely used?
6. **License compatible?** Check license compatibility

### Supply Chain

- Pin dependency versions in `Cargo.lock` (committed to repo)
- Review `Cargo.lock` changes in PRs
- Use `cargo deny` for license and advisory checking when available

---

## Authentication & Authorization

### Authentication Rules

- Use constant-time comparison for tokens and secrets
- Implement rate limiting on authentication endpoints
- Log authentication failures (without logging the attempted credential)
- Support token rotation without downtime

### Authorization Rules

- Check authorization on every request (never cache auth decisions indefinitely)
- Use deny-by-default policies
- Validate permissions at the handler level, not just middleware
- Audit-log authorization failures

---

## Data Handling

### Sensitive Data

- Classify data by sensitivity level
- Encrypt sensitive data at rest
- Use TLS for all network communication
- Implement proper key management
- Sanitize data before logging

### Serialization Safety

- Validate deserialized data (do not trust that serialized data is valid)
- Set size limits on incoming payloads
- Use `#[serde(deny_unknown_fields)]` where strict schemas are required
- Implement `TryFrom` instead of `From` for fallible conversions

---

## FFI / Python Bridge Security

The Python bridge is a critical security boundary.

- Validate all data crossing the FFI boundary
- Do not pass raw pointers without safety documentation
- Handle panics at the FFI boundary (do not unwind across FFI)
- Sanitize error messages returned to Python (no internal details)
- Rate-limit calls from Python if applicable

---

## Error Handling Security

- Never expose internal system details in user-facing errors
- Log detailed errors internally, return generic errors externally
- Do not include stack traces in production error responses
- Include correlation IDs for debugging without exposing internals

---

## Security Review Triggers

The following changes MUST include a security review:

- Adding or modifying authentication/authorization logic
- Adding or modifying the Python bridge / FFI boundary
- Adding new network endpoints or protocols
- Adding or updating dependencies
- Modifying cryptographic operations
- Changing file system access patterns
- Modifying user input handling

---

## Security Checklist for PRs

- [ ] No secrets in code, logs, or error messages
- [ ] All external input is validated
- [ ] Authentication/authorization checks are in place
- [ ] Dependencies have been audited (`make audit`)
- [ ] Error messages do not leak internal details
- [ ] FFI boundary data is validated
- [ ] Sensitive fields use appropriate wrappers
- [ ] Rate limiting is considered for new endpoints
