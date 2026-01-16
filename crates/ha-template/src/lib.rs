//! Jinja2-compatible template engine for Home Assistant
//!
//! This crate provides a template engine built on minijinja with
//! Home Assistant-specific extensions:
//!
//! # State Access
//!
//! - `states('entity_id')` - Get entity state as string
//! - `states.light.living_room` - Access state object
//! - `is_state('entity_id', 'on')` - Check if entity is in state
//! - `state_attr('entity_id', 'brightness')` - Get attribute value
//! - `has_value('entity_id')` - Check if entity has valid value
//!
//! # Time Functions
//!
//! - `now()` - Current local time
//! - `utcnow()` - Current UTC time
//! - `today_at('14:30')` - Today at specific time
//! - `as_timestamp(datetime)` - Convert to UNIX timestamp
//! - `relative_time(datetime)` - Human-readable age ("2 hours")
//! - `timedelta(hours=2)` - Create duration
//!
//! # Filters
//!
//! - `| round(2)` - Round to precision
//! - `| int` / `| float` / `| bool` - Type conversion
//! - `| slugify` - Convert to slug
//! - `| to_json` / `| from_json` - JSON serialization
//! - `| regex_replace(pattern, replacement)` - Regex substitution
//! - `| average` / `| median` - Aggregate functions
//!
//! # Example
//!
//! ```ignore
//! use ha_template::TemplateEngine;
//! use ha_state_machine::StateMachine;
//!
//! let engine = TemplateEngine::new(state_machine);
//!
//! // Simple state access
//! let result = engine.render("{{ states('sensor.temperature') }}")?;
//!
//! // Conditional logic
//! let template = r#"
//! {% if is_state('light.living_room', 'on') %}
//!   Light is on at {{ state_attr('light.living_room', 'brightness') }}%
//! {% endif %}
//! "#;
//! let result = engine.render(template)?;
//! ```

mod engine;
mod error;
mod filters;
mod globals;
mod states;

pub use engine::{create_test_engine, TemplateEngine};
pub use error::{TemplateError, TemplateResult};
pub use globals::{DateTimeWrapper, TimeDeltaWrapper};
pub use states::{StateWrapper, StatesObject};

// Re-export minijinja Value for convenience
pub use minijinja::Value;
