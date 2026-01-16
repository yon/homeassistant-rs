//! Template engine for Home Assistant
//!
//! Provides Jinja2-compatible template rendering with Home Assistant-specific
//! functions and filters.

use crate::error::TemplateResult;
use crate::filters;
use crate::globals;
use crate::states::{self, StatesObject};
use ha_state_machine::StateMachine;
use minijinja::{Environment, Value};
use std::sync::Arc;
use tracing::debug;

/// Template engine with Home Assistant extensions
///
/// The engine provides:
/// - Access to entity states via the `states` object
/// - Time functions like `now()`, `utcnow()`, `relative_time()`
/// - State functions like `is_state()`, `state_attr()`, `has_value()`
/// - Filters like `round`, `regex_replace`, `to_json`, `slugify`
pub struct TemplateEngine {
    env: Environment<'static>,
    states: Arc<StatesObject>,
}

impl TemplateEngine {
    /// Create a new template engine with access to the state machine
    pub fn new(state_machine: Arc<StateMachine>) -> Self {
        let states = Arc::new(StatesObject::new(state_machine));
        let mut env = Environment::new();

        // Configure environment
        env.set_debug(true);

        // Register filters
        Self::register_filters(&mut env);

        // Register global functions
        Self::register_globals(&mut env, states.clone());

        // Register tests
        Self::register_tests(&mut env);

        Self { env, states }
    }

    fn register_filters(env: &mut Environment<'static>) {
        // String filters
        env.add_filter("slugify", filters::slugify);
        env.add_filter("regex_replace", filters::regex_replace);
        env.add_filter("regex_findall", filters::regex_findall);
        env.add_filter("regex_match", filters::regex_match);

        // Type conversion
        env.add_filter("float", filters::to_float);
        env.add_filter("int", filters::to_int);
        env.add_filter("bool", filters::to_bool);

        // Type checking
        env.add_filter("is_number", filters::is_number);
        env.add_filter("is_string", filters::is_string);
        env.add_filter("is_list", filters::is_list);
        env.add_filter("contains", filters::contains);

        // Math
        env.add_filter("round", filters::round_filter);
        env.add_filter("abs", filters::abs_filter);
        env.add_filter("sqrt", filters::sqrt);
        env.add_filter("log", filters::log_filter);
        env.add_filter("sin", filters::sin);
        env.add_filter("cos", filters::cos);
        env.add_filter("tan", filters::tan);
        env.add_filter("asin", filters::asin);
        env.add_filter("acos", filters::acos);
        env.add_filter("atan", filters::atan);
        env.add_filter("atan2", filters::atan2);

        // Aggregates
        env.add_filter("average", filters::average);
        env.add_filter("median", filters::median);

        // JSON
        env.add_filter("to_json", filters::to_json);
        env.add_filter("from_json", filters::from_json);

        // Encoding
        env.add_filter("base64_encode", filters::base64_encode);
        env.add_filter("base64_decode", filters::base64_decode);
        env.add_filter("urlencode", filters::urlencode);
        env.add_filter("ordinal", filters::ordinal);

        // Lists
        env.add_filter("flatten", filters::flatten);
    }

    fn register_globals(env: &mut Environment<'static>, states: Arc<StatesObject>) {
        // States object
        let states_clone = states.clone();
        env.add_global("states", Value::from_object((*states_clone).clone()));

        // Time functions
        env.add_function("now", globals::now);
        env.add_function("utcnow", globals::utcnow);
        env.add_function("today_at", globals::today_at);
        env.add_function("as_timestamp", globals::as_timestamp);
        env.add_function("as_datetime", globals::as_datetime);
        env.add_function("as_local", globals::as_local);
        env.add_function("strptime", globals::strptime);
        env.add_function("timedelta", globals::timedelta);
        env.add_function("as_timedelta", globals::as_timedelta);
        env.add_function("relative_time", globals::relative_time);
        env.add_function("time_since", globals::time_since);
        env.add_function("time_until", globals::time_until);

        // State functions - wrap with states reference
        let states_for_is_state = states.clone();
        env.add_function("is_state", move |entity_id: &str, state: Value| {
            states::is_state_fn(states_for_is_state.clone(), entity_id, state)
        });

        let states_for_state_attr = states.clone();
        env.add_function("state_attr", move |entity_id: &str, attribute: &str| {
            states::state_attr_fn(states_for_state_attr.clone(), entity_id, attribute)
        });

        let states_for_is_state_attr = states.clone();
        env.add_function(
            "is_state_attr",
            move |entity_id: &str, attribute: &str, value: Value| {
                states::is_state_attr_fn(
                    states_for_is_state_attr.clone(),
                    entity_id,
                    attribute,
                    value,
                )
            },
        );

        let states_for_has_value = states.clone();
        env.add_function("has_value", move |entity_id: &str| {
            states::has_value_fn(states_for_has_value.clone(), entity_id)
        });

        // Utility functions
        env.add_function("iif", globals::iif);
        env.add_function("distance", globals::distance);
        env.add_function("typeof", globals::typeof_fn);
        env.add_function("range", globals::range_fn);

        // Math functions as globals too
        env.add_function("min", |values: Value| -> Result<Value, minijinja::Error> {
            if let Ok(iter) = values.try_iter() {
                let nums: Vec<f64> = iter
                    .filter_map(|v| {
                        f64::try_from(v.clone())
                            .ok()
                            .or_else(|| v.as_i64().map(|i| i as f64))
                    })
                    .collect();
                if nums.is_empty() {
                    Ok(Value::UNDEFINED)
                } else {
                    Ok(Value::from(nums.into_iter().fold(f64::INFINITY, f64::min)))
                }
            } else {
                Ok(Value::UNDEFINED)
            }
        });

        env.add_function("max", |values: Value| -> Result<Value, minijinja::Error> {
            if let Ok(iter) = values.try_iter() {
                let nums: Vec<f64> = iter
                    .filter_map(|v| {
                        f64::try_from(v.clone())
                            .ok()
                            .or_else(|| v.as_i64().map(|i| i as f64))
                    })
                    .collect();
                if nums.is_empty() {
                    Ok(Value::UNDEFINED)
                } else {
                    Ok(Value::from(
                        nums.into_iter().fold(f64::NEG_INFINITY, f64::max),
                    ))
                }
            } else {
                Ok(Value::UNDEFINED)
            }
        });
    }

    fn register_tests(env: &mut Environment<'static>) {
        env.add_test("number", filters::is_number);
        env.add_test("string", filters::is_string);
        env.add_test("list", filters::is_list);
        env.add_test("defined", filters::is_defined);
        env.add_test("match", filters::match_test);
        env.add_test("contains", filters::contains);
    }

    /// Render a template string
    pub fn render(&self, template: &str) -> TemplateResult<String> {
        debug!("Rendering template: {}", template);

        let tmpl = self.env.template_from_str(template)?;
        let result = tmpl.render(())?;

        Ok(result)
    }

    /// Render a template with additional context variables
    pub fn render_with_context(
        &self,
        template: &str,
        context: impl serde::Serialize,
    ) -> TemplateResult<String> {
        let tmpl = self.env.template_from_str(template)?;
        let result = tmpl.render(context)?;
        Ok(result)
    }

    /// Evaluate a template and return the value
    pub fn evaluate(&self, template: &str) -> TemplateResult<Value> {
        let expr = self.env.compile_expression(template)?;
        let result = expr.eval(())?;
        Ok(result)
    }

    /// Evaluate a template with context and return the value
    pub fn evaluate_with_context(
        &self,
        template: &str,
        context: impl serde::Serialize,
    ) -> TemplateResult<Value> {
        let expr = self.env.compile_expression(template)?;
        let result = expr.eval(context)?;
        Ok(result)
    }

    /// Check if a template string contains template syntax
    pub fn is_template(template: &str) -> bool {
        template.contains("{{") || template.contains("{%") || template.contains("{#")
    }

    /// Get a reference to the states object
    pub fn states(&self) -> &StatesObject {
        &self.states
    }
}

/// Create a standalone template engine for testing
pub fn create_test_engine() -> TemplateEngine {
    use ha_event_bus::EventBus;

    let event_bus = Arc::new(EventBus::new());
    let state_machine = Arc::new(StateMachine::new(event_bus));
    TemplateEngine::new(state_machine)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ha_core::{Context, EntityId};
    use ha_event_bus::EventBus;
    use std::collections::HashMap;

    fn make_test_engine() -> TemplateEngine {
        let event_bus = Arc::new(EventBus::new());
        let state_machine = Arc::new(StateMachine::new(event_bus));

        // Add test states
        state_machine.set(
            EntityId::new("light", "living_room").unwrap(),
            "on",
            HashMap::from([
                ("brightness".to_string(), serde_json::json!(255)),
                (
                    "friendly_name".to_string(),
                    serde_json::json!("Living Room"),
                ),
            ]),
            Context::new(),
        );

        state_machine.set(
            EntityId::new("sensor", "temperature").unwrap(),
            "23.5",
            HashMap::from([("unit_of_measurement".to_string(), serde_json::json!("°C"))]),
            Context::new(),
        );

        state_machine.set(
            EntityId::new("switch", "kitchen").unwrap(),
            "off",
            HashMap::new(),
            Context::new(),
        );

        TemplateEngine::new(state_machine)
    }

    // ==================== Basic Rendering Tests ====================

    #[test]
    fn test_simple_render() {
        let engine = make_test_engine();
        let result = engine.render("Hello, World!").unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_variable_substitution() {
        let engine = make_test_engine();
        let result = engine
            .render_with_context("Hello, {{ name }}!", serde_json::json!({"name": "Test"}))
            .unwrap();
        assert_eq!(result, "Hello, Test!");
    }

    // ==================== States Tests ====================

    #[test]
    fn test_states_function() {
        let engine = make_test_engine();
        let result = engine.render("{{ states('light.living_room') }}").unwrap();
        assert_eq!(result, "on");
    }

    #[test]
    fn test_states_object_access() {
        let engine = make_test_engine();
        let result = engine
            .render("{{ states.light.living_room.state }}")
            .unwrap();
        assert_eq!(result, "on");
    }

    #[test]
    fn test_is_state() {
        let engine = make_test_engine();
        assert_eq!(
            engine
                .render("{{ is_state('light.living_room', 'on') }}")
                .unwrap(),
            "true"
        );
        assert_eq!(
            engine
                .render("{{ is_state('light.living_room', 'off') }}")
                .unwrap(),
            "false"
        );
    }

    #[test]
    fn test_state_attr() {
        let engine = make_test_engine();
        let result = engine
            .render("{{ state_attr('light.living_room', 'brightness') }}")
            .unwrap();
        assert_eq!(result, "255");
    }

    #[test]
    fn test_has_value() {
        let engine = make_test_engine();
        assert_eq!(
            engine
                .render("{{ has_value('light.living_room') }}")
                .unwrap(),
            "true"
        );
        assert_eq!(
            engine
                .render("{{ has_value('nonexistent.entity') }}")
                .unwrap(),
            "false"
        );
    }

    // ==================== Time Tests ====================

    #[test]
    fn test_now() {
        let engine = make_test_engine();
        let result = engine.render("{{ now().year }}").unwrap();
        let year: i32 = result.parse().unwrap();
        assert!(year >= 2024);
    }

    #[test]
    fn test_utcnow() {
        let engine = make_test_engine();
        let result = engine.render("{{ utcnow().year }}").unwrap();
        let year: i32 = result.parse().unwrap();
        assert!(year >= 2024);
    }

    // ==================== Filter Tests ====================

    #[test]
    fn test_round_filter() {
        let engine = make_test_engine();
        assert_eq!(engine.render("{{ 3.14159 | round(2) }}").unwrap(), "3.14");
    }

    #[test]
    fn test_abs_filter() {
        let engine = make_test_engine();
        assert_eq!(engine.render("{{ -5 | abs }}").unwrap(), "5.0");
    }

    #[test]
    fn test_slugify_filter() {
        let engine = make_test_engine();
        assert_eq!(
            engine.render("{{ 'Hello World' | slugify }}").unwrap(),
            "hello_world"
        );
    }

    #[test]
    fn test_to_json_filter() {
        let engine = make_test_engine();
        let result = engine
            .render_with_context(
                "{{ data | to_json }}",
                serde_json::json!({"data": {"key": "value"}}),
            )
            .unwrap();
        assert!(result.contains("key"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_regex_replace() {
        let engine = make_test_engine();
        assert_eq!(
            engine
                .render("{{ 'hello world' | regex_replace('\\\\s+', '-') }}")
                .unwrap(),
            "hello-world"
        );
    }

    // ==================== Math Tests ====================

    #[test]
    fn test_min_max() {
        let engine = make_test_engine();
        assert_eq!(engine.render("{{ min([1, 2, 3]) }}").unwrap(), "1.0");
        assert_eq!(engine.render("{{ max([1, 2, 3]) }}").unwrap(), "3.0");
    }

    #[test]
    fn test_sqrt() {
        let engine = make_test_engine();
        assert_eq!(engine.render("{{ 16 | sqrt }}").unwrap(), "4.0");
    }

    // ==================== Utility Tests ====================

    #[test]
    fn test_iif() {
        let engine = make_test_engine();
        assert_eq!(
            engine.render("{{ iif(true, 'yes', 'no') }}").unwrap(),
            "yes"
        );
        assert_eq!(
            engine.render("{{ iif(false, 'yes', 'no') }}").unwrap(),
            "no"
        );
    }

    #[test]
    fn test_typeof() {
        let engine = make_test_engine();
        assert_eq!(engine.render("{{ typeof(42) }}").unwrap(), "integer");
        assert_eq!(engine.render("{{ typeof(3.14) }}").unwrap(), "float");
        assert_eq!(engine.render("{{ typeof('hello') }}").unwrap(), "string");
    }

    #[test]
    fn test_range() {
        let engine = make_test_engine();
        assert_eq!(
            engine.render("{{ range(5) | list }}").unwrap(),
            "[0, 1, 2, 3, 4]"
        );
    }

    // ==================== Tests (Jinja2 tests) ====================

    #[test]
    fn test_is_number() {
        let engine = make_test_engine();
        assert_eq!(engine.render("{{ 42 is number }}").unwrap(), "true");
        assert_eq!(engine.render("{{ 'hello' is number }}").unwrap(), "false");
    }

    #[test]
    fn test_is_defined() {
        let engine = make_test_engine();
        let result = engine
            .render_with_context("{{ x is defined }}", serde_json::json!({"x": 1}))
            .unwrap();
        assert_eq!(result, "true");
    }

    // ==================== Integration Tests ====================

    #[test]
    fn test_complex_template() {
        let engine = make_test_engine();
        let template = r#"
{%- if is_state('light.living_room', 'on') -%}
Light is on at {{ state_attr('light.living_room', 'brightness') }}%
{%- else -%}
Light is off
{%- endif -%}
"#;
        let result = engine.render(template).unwrap();
        assert_eq!(result.trim(), "Light is on at 255%");
    }

    #[test]
    fn test_for_loop_with_states() {
        let engine = make_test_engine();
        let template = "{% for i in range(3) %}{{ i }}{% endfor %}";
        assert_eq!(engine.render(template).unwrap(), "012");
    }

    #[test]
    fn test_is_template() {
        assert!(TemplateEngine::is_template("{{ foo }}"));
        assert!(TemplateEngine::is_template("{% if true %}{% endif %}"));
        assert!(TemplateEngine::is_template("{# comment #}"));
        assert!(!TemplateEngine::is_template("plain text"));
    }
}
