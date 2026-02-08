# Code Conventions

**Conventions specific to the homeassistant-rs project. Follow these consistently.**

---

## Rust Formatting

### rustfmt

All Rust code is formatted with `rustfmt` via:

```bash
make fmt        # Apply formatting
make lint       # Includes fmt --check
```

Key `rustfmt.toml` settings respected by this project:
- Use the project's `rustfmt.toml` — do not override with personal settings
- Format all files consistently (no exceptions)
- Run `make fmt` before committing

### Line Length

- Target 100 characters per line (as configured in `rustfmt.toml`)
- Comments may exceed slightly for readability, but prefer wrapping
- URLs in comments are exempt

---

## Alphabetization (Enforced by Linter)

**Alphabetical ordering is enforced by `./scripts/lint-alpha.py` and checked in pre-commit hooks.**

### What Must Be Alphabetized

1. **`use` declarations** — within each group (std, external, crate-local)
2. **Enum variants** — alphabetical order
3. **Struct fields** — alphabetical order (where semantically appropriate)
4. **Match arms** — alphabetical by pattern (where patterns are identifiers)
5. **Module declarations** — `mod` statements in alphabetical order
6. **Cargo.toml `[dependencies]`** — alphabetical order

### Example: use Declarations

```rust
// Group 1: std library (alphabetized)
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

// Group 2: external crates (alphabetized)
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// Group 3: workspace crates (alphabetized)
use ha_entity::EntityId;
use ha_event::Event;

// Group 4: crate-local (alphabetized)
use crate::config::Config;
use crate::error::CoreError;
```

### Example: Enum Variants

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum EntityDomain {
    Automation,
    BinarySensor,
    Climate,
    Cover,
    Fan,
    Light,
    Lock,
    MediaPlayer,
    Sensor,
    Switch,
}
```

### Example: Match Arms

```rust
match domain {
    EntityDomain::Automation => handle_automation(entity),
    EntityDomain::BinarySensor => handle_binary_sensor(entity),
    EntityDomain::Climate => handle_climate(entity),
    EntityDomain::Light => handle_light(entity),
    EntityDomain::Switch => handle_switch(entity),
}
```

### Running the Linter

```bash
./scripts/lint-alpha.py --all       # Check all files
./scripts/lint-alpha.py --staged    # Check staged files only (pre-commit)
./scripts/lint-alpha.py crates/ha-entity/src/lib.rs  # Check specific file
```

---

## Clippy

Clippy is run with deny-all-warnings:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

### Common Clippy Guidance

- **Do not suppress warnings without justification** — if `#[allow(clippy::...)]` is needed, add a comment explaining why
- Prefer `clippy::pedantic` suggestions when they improve clarity
- Address `clippy::nursery` suggestions on a case-by-case basis

---

## Naming Conventions

### Crates

All workspace crates use the `ha-` prefix:

```
crates/
├── ha-automation/
├── ha-config/
├── ha-core/
├── ha-entity/
├── ha-event/
├── ha-integration/
├── ha-python-bridge/
├── ha-registry/
├── ha-rest-api/
├── ha-scheduler/
├── ha-service/
├── ha-state/
├── ha-storage/
├── ha-test-support/
├── ha-websocket/
└── ha-yaml/
```

Cargo.toml package names use hyphens: `ha-entity`. Module names use underscores: `ha_entity`.

### Types

- **Structs**: `PascalCase` — `EntityState`, `ConfigEntry`, `ServiceCall`
- **Enums**: `PascalCase` — `EntityDomain`, `EventType`, `StateChange`
- **Traits**: `PascalCase`, often adjective-like — `Configurable`, `Serializable`, `EventEmitter`
- **Functions/methods**: `snake_case` — `get_state`, `register_service`, `emit_event`
- **Constants**: `SCREAMING_SNAKE_CASE` — `MAX_RETRY_COUNT`, `DEFAULT_TIMEOUT`
- **Modules**: `snake_case` — `state_machine`, `config_entry`

### Domain-Specific Naming

- Entity IDs: `EntityId` (newtype around `String`, format: `domain.object_id`)
- Service targets: `ServiceTarget` (entity, device, or area)
- Config entries: `ConfigEntry` (integration configuration unit)
- State objects: `EntityState` (current state + attributes + timestamps)

---

## Error Handling

### Use `thiserror` for Library Errors

Each crate defines its own error type using `thiserror`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EntityError {
    #[error("entity not found: {entity_id}")]
    NotFound { entity_id: String },

    #[error("invalid entity ID format: {input}")]
    InvalidId { input: String },

    #[error("state update failed for {entity_id}: {reason}")]
    StateUpdateFailed { entity_id: String, reason: String },

    #[error(transparent)]
    Storage(#[from] ha_storage::StorageError),
}
```

### Error Conventions

- Each crate has its own error enum in `src/error.rs`
- Error variants are alphabetized
- Use `#[error(transparent)]` for wrapped errors from other crates
- Include context in error messages (entity IDs, file paths, etc.)
- Use `anyhow` only in binary targets and tests, never in library crates
- Prefer `Result<T, CrateError>` in public APIs
- Use `#[from]` for automatic conversion from dependency errors

### Error Propagation

```rust
// Good: use ? with context
let config = load_config(path)
    .map_err(|e| ConfigError::LoadFailed {
        path: path.to_owned(),
        source: e,
    })?;

// Bad: unwrap in library code
let config = load_config(path).unwrap();
```

---

## Struct Design

### Derive Order

Follow this standard derive order (alphabetized within groups):

```rust
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct EntityId(String);
```

Common groups:
1. `Clone`, `Copy`
2. `Debug`, `Display`
3. `Deserialize`, `Serialize`
4. `Eq`, `Hash`, `Ord`, `PartialEq`, `PartialOrd`
5. `Default`
6. `Error` (thiserror)

### Builder Pattern

For structs with many optional fields, use the builder pattern:

```rust
let entity = Entity::builder("light.living_room")
    .with_name("Living Room Light")
    .with_state("on")
    .with_attribute("brightness", 255)
    .build()?;
```

### Newtype Pattern

Use newtypes for domain-specific identifiers:

```rust
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct EntityId(String);

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct DeviceId(String);

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct AreaId(String);
```

---

## Module Organization

### Standard Crate Layout

```
crates/ha-entity/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Public API re-exports
│   ├── entity.rs       # Core entity types
│   ├── error.rs        # Error types
│   ├── id.rs           # EntityId and related types
│   ├── registry.rs     # Entity registry
│   └── state.rs        # State management
└── tests/
    ├── entity_tests.rs
    └── registry_tests.rs
```

### lib.rs Convention

`lib.rs` should primarily contain module declarations and re-exports:

```rust
//! Entity management for Home Assistant.
//!
//! This crate provides the core entity types, state management,
//! and entity registry.

mod entity;
mod error;
mod id;
mod registry;
mod state;

pub use entity::Entity;
pub use error::EntityError;
pub use id::EntityId;
pub use registry::EntityRegistry;
pub use state::EntityState;
```

Note: module declarations (`mod`) and re-exports (`pub use`) are each alphabetized.

---

## Documentation

### Doc Comments

Every public item must have a doc comment:

```rust
/// Represents a single Home Assistant entity.
///
/// An entity is the fundamental unit of state in Home Assistant.
/// Each entity belongs to a domain (e.g., `light`, `sensor`) and
/// has a unique identifier within that domain.
///
/// # Examples
///
/// ```
/// use ha_entity::Entity;
///
/// let entity = Entity::new("light.living_room", "off");
/// assert_eq!(entity.domain(), "light");
/// ```
pub struct Entity { ... }
```

### Module-Level Documentation

Each module should have a `//!` doc comment at the top explaining its purpose.

---

## Async Conventions

- Use `tokio` as the async runtime
- Prefer `async fn` over manual `Future` implementations
- Use `tokio::sync` primitives (`RwLock`, `Mutex`, `broadcast`, `mpsc`)
- Avoid holding locks across `.await` points
- Use `tokio::select!` for concurrent operations with cancellation
- Document cancellation safety for public async functions

---

## Logging and Tracing

Use the `tracing` crate for structured logging:

```rust
use tracing::{debug, error, info, instrument, warn};

#[instrument(skip(self), fields(entity_id = %entity_id))]
pub async fn update_state(&self, entity_id: &EntityId, new_state: &str) -> Result<()> {
    info!("updating entity state");
    // ...
    debug!(old_state = %current, new_state = %new_state, "state changed");
    Ok(())
}
```

- Use `info!` for significant state changes and operations
- Use `debug!` for detailed operational information
- Use `warn!` for recoverable issues
- Use `error!` for failures that need attention
- Use `#[instrument]` on public functions for automatic span creation
- Never log secrets or sensitive data (see `security-practices.md`)

---

## Python Bridge Conventions

The `ha-python-bridge` crate uses PyO3 for Python interop:

- All Python-facing functions must validate inputs (treat as external)
- Use `#[pyfunction]` and `#[pyclass]` with explicit names
- Convert Rust errors to Python exceptions at the boundary
- Document Python-side usage in docstrings
- Keep the bridge layer thin — logic lives in core crates
- Test both sides: Rust unit tests AND Python integration tests
