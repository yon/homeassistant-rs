# Testing Protocol

**Tests are mandatory. Every behavior must be tested. TDD is the default workflow.**

---

## Test-Driven Development (TDD)

### The TDD Cycle

1. **Red** — Write a failing test that describes the desired behavior
2. **Green** — Write the minimum code to make the test pass
3. **Refactor** — Clean up while keeping tests green
4. **Repeat** — For each new behavior or edge case

### Why TDD Is Required

- Forces you to define the API before implementing it
- Ensures every behavior has at least one test
- Prevents writing unnecessary code
- Creates a living specification of the system

---

## Test Organization (Rust)

### Unit Tests

Unit tests live in the same file as the code they test, inside a `#[cfg(test)]` module.

```rust
// In crates/ha-entity/src/state.rs

pub fn validate_state(state: &str) -> Result<(), StateError> {
    // implementation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_state_accepts_valid_input() {
        assert!(validate_state("on").is_ok());
    }

    #[test]
    fn validate_state_rejects_empty_string() {
        assert!(validate_state("").is_err());
    }
}
```

### Integration Tests

Integration tests live in `crates/<crate>/tests/` as separate files.

```
crates/ha-entity/
├── src/
│   └── lib.rs
└── tests/
    ├── entity_lifecycle.rs
    └── state_transitions.rs
```

### Test Naming Convention

Use descriptive names that read as sentences:

```rust
#[test]
fn entity_state_change_emits_event() { ... }

#[test]
fn service_call_with_invalid_domain_returns_error() { ... }

#[test]
fn config_entry_reload_preserves_user_settings() { ... }
```

Pattern: `<subject>_<action_or_condition>_<expected_result>`

### Test Helpers

Shared test utilities go in a dedicated test-support crate or in `tests/common/mod.rs`:

```rust
// crates/ha-test-support/src/lib.rs
pub fn create_test_entity(entity_id: &str) -> Entity { ... }
pub fn create_test_config() -> Config { ... }
```

---

## What to Test

### Always Test

- **Public API** — every public function, method, and type
- **Edge cases** — empty inputs, boundary values, overflow conditions
- **Error paths** — every `Err` variant should have a test that triggers it
- **State transitions** — valid transitions succeed, invalid transitions fail
- **Serialization** — round-trip serialize/deserialize for all wire-format types
- **HA compatibility** — any behavior that Python HA depends on

### Test Categories

| Category | Command | When to Run |
|----------|---------|-------------|
| Unit tests | `make test-rust` | Every change |
| Python bridge tests | `make test-python` | Python bridge changes |
| Integration tests | `make test-integration` | Cross-crate changes |
| HA compatibility | `make test-ha-compat` | API/schema changes |

---

## Test Quality Rules

### Rule 1: Tests Must Be Deterministic

- No random values without seeds
- No dependency on system time (use injectable clocks)
- No dependency on network or filesystem (use mocks/tempfiles)
- Tests must pass when run in any order

### Rule 2: Tests Must Be Fast

- Unit tests: < 100ms each
- Integration tests: < 1s each (ideally)
- Use `#[ignore]` for slow tests, run them separately
- Prefer in-memory implementations over disk/network I/O

### Rule 3: Tests Must Be Independent

- No shared mutable state between tests
- Each test sets up its own fixtures
- Use `tempdir()` for filesystem tests
- Reset any global state in setup/teardown

### Rule 4: Tests Must Be Readable

- Follow Arrange-Act-Assert (AAA) pattern
- One logical assertion per test (multiple `assert!` is fine if testing one concept)
- Use descriptive variable names in tests (verbosity is fine)
- Add comments for non-obvious test setup

```rust
#[test]
fn entity_with_expired_ttl_is_considered_unavailable() {
    // Arrange
    let mut entity = create_test_entity("sensor.temperature");
    entity.set_last_updated(Utc::now() - Duration::hours(2));
    entity.set_ttl(Duration::hours(1));

    // Act
    let available = entity.is_available();

    // Assert
    assert!(!available, "Entity with expired TTL should be unavailable");
}
```

### Rule 5: Test Error Messages Must Be Helpful

Always provide context in assertions:

```rust
// Bad
assert!(result.is_ok());

// Good
assert!(result.is_ok(), "Expected OK but got: {:?}", result.err());

// Also good
let value = result.expect("service call should succeed for valid domain");
```

---

## Coverage Guidelines

- Aim for meaningful coverage, not a percentage target
- Every public function should have at least one happy-path test and one error-path test
- Critical paths (state management, event handling, service calls) should have comprehensive tests
- Use `cargo tarpaulin` for coverage reports when needed

---

## Test Fixtures and Builders

For complex test setup, use the builder pattern:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct EntityBuilder {
        entity_id: String,
        state: String,
        attributes: HashMap<String, Value>,
    }

    impl EntityBuilder {
        fn new(entity_id: &str) -> Self {
            Self {
                entity_id: entity_id.to_string(),
                state: "unknown".to_string(),
                attributes: HashMap::new(),
            }
        }

        fn with_state(mut self, state: &str) -> Self {
            self.state = state.to_string();
            self
        }

        fn build(self) -> Entity {
            Entity::new(self.entity_id, self.state, self.attributes)
        }
    }
}
```

---

## Snapshot Testing

For complex output (JSON responses, error messages, serialized configs), consider snapshot testing with `insta`:

```rust
#[test]
fn entity_serializes_to_expected_json() {
    let entity = create_test_entity("light.living_room");
    let json = serde_json::to_value(&entity).unwrap();
    insta::assert_json_snapshot!(json);
}
```

---

## Property-Based Testing

For functions with wide input domains, use `proptest` or `quickcheck`:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn entity_id_roundtrips_through_serialization(
        domain in "[a-z_]{1,32}",
        object_id in "[a-z0-9_]{1,64}",
    ) {
        let id = format!("{}.{}", domain, object_id);
        let entity_id: EntityId = id.parse().unwrap();
        assert_eq!(entity_id.to_string(), id);
    }
}
```
