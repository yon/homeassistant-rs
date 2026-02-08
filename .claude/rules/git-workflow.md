# Git Workflow

**Consistent git practices make collaboration easier and history more useful.**

---

## Commit Messages

### Format: Conventional Commits

```
<type>(<scope>): <short description>

[optional body]

[optional footer(s)]
```

### Types

| Type | When to Use |
|------|-------------|
| `feat` | A new feature |
| `fix` | A bug fix |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `test` | Adding or correcting tests |
| `docs` | Documentation only changes |
| `style` | Formatting, missing semicolons, etc. (no logic change) |
| `perf` | Performance improvement |
| `ci` | CI/CD configuration changes |
| `build` | Build system or dependency changes |
| `chore` | Maintenance tasks, tooling, etc. |

### Scope

Use the crate name or component as the scope:

```
feat(ha-entity): add state expiration support
fix(ha-websocket): handle reconnection on auth failure
refactor(ha-config): simplify entry validation logic
test(ha-automation): add trigger edge case tests
docs(ha-core): update module-level documentation
```

For changes spanning multiple crates:
```
refactor(workspace): rename EntityId to EntityKey across crates
```

### Rules

1. **Subject line**: imperative mood, lowercase, no period, max 72 characters
2. **Body**: explain WHY, not WHAT (the diff shows what changed)
3. **Footer**: reference issues (`Closes #123`, `Fixes #456`)
4. **Breaking changes**: add `BREAKING CHANGE:` footer or `!` after type

### Examples

```
feat(ha-entity): add TTL-based availability tracking

Entities can now declare a TTL. When the TTL expires without a state
update, the entity is marked as unavailable. This matches Python HA
behavior for device_tracker entities.

Closes #42
```

```
fix(ha-websocket)!: require authentication for all message types

Previously, some internal message types bypassed authentication.
This was a security oversight.

BREAKING CHANGE: All WebSocket messages now require a valid auth token.
Clients must authenticate before sending any commands.

Fixes #99
```

---

## Branching Strategy

### Branch Naming

```
<type>/<short-description>
```

Examples:
- `feat/entity-ttl`
- `fix/websocket-reconnect`
- `refactor/state-store-cleanup`
- `test/automation-triggers`

### Branch Rules

- `main` is always deployable
- Feature branches are short-lived (ideally < 1 week)
- Rebase on `main` before merging (keep linear history)
- Delete branches after merging

---

## Pre-Commit Hooks

The pre-commit hook runs automatically on every commit:

1. `cargo fmt --all -- --check` — formatting
2. `./scripts/lint-alpha.py --staged` — alphabetization on staged files
3. `cargo clippy --workspace --all-targets -- -D warnings` — lints

If any check fails, the commit is rejected. Fix the issues and commit again.

**Do not bypass hooks** with `--no-verify` unless there is a documented, exceptional reason.

---

## Commit Hygiene

### Do

- Make small, focused commits (one logical change per commit)
- Write meaningful commit messages
- Stage specific files, not `git add .`
- Review your diff before committing (`git diff --cached`)
- Squash WIP commits before merging to main

### Do Not

- Commit generated files (build artifacts, compiled output)
- Commit secrets, credentials, or `.env` files
- Mix formatting changes with logic changes
- Leave `println!` / `dbg!` debugging in committed code
- Force-push to shared branches

---

## Pull Request Guidelines

### PR Checklist

- [ ] All quality gates pass (`make dev`)
- [ ] Alphabetization clean (`./scripts/lint-alpha.py --all`)
- [ ] New tests for new behavior
- [ ] Commit messages follow conventional format
- [ ] No secrets in code or configuration
- [ ] Breaking changes documented
- [ ] HA compatibility verified (if applicable)

### PR Size

- Aim for < 400 lines changed
- If larger, split into a chain of PRs
- Each PR in the chain should be independently reviewable and mergable

### PR Description

Include:
- What changed and why
- How to test it
- Any decisions made and their rationale
- Screenshots/logs if applicable
- Related issues

---

## Tags and Releases

- Use semantic versioning: `vMAJOR.MINOR.PATCH`
- Tag releases on `main` only
- Include changelog in release notes
- Breaking changes bump MAJOR (or MINOR if pre-1.0)
