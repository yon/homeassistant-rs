//! Script executor
//!
//! Executes script actions with proper context, variable handling, and control flow.

use crate::action::Action;
use ha_automation::TriggerData;
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, trace};

/// Script executor errors
#[derive(Debug, Error)]
pub enum ScriptExecutorError {
    #[error("Invalid action: {0}")]
    InvalidAction(String),

    #[error("Service call failed: {0}")]
    ServiceCallFailed(String),

    #[error("Template error: {0}")]
    Template(String),

    #[error("Condition failed")]
    ConditionFailed,

    #[error("Timeout waiting for trigger")]
    Timeout,

    #[error("Script stopped: {0}")]
    Stopped(String),

    #[error("Action error: {0}")]
    ActionError(String),

    #[error("Max runs exceeded")]
    MaxRunsExceeded,
}

/// Result type for script execution
pub type ScriptExecutorResult<T> = Result<T, ScriptExecutorError>;

/// Execution context for scripts
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Variables available in templates
    pub variables: HashMap<String, Value>,

    /// Trigger data (if started by automation)
    pub trigger: Option<TriggerData>,

    /// Response from last service call
    pub response: Option<Value>,

    /// Whether to stop on next condition failure
    pub stop_on_condition_fail: bool,

    /// Repeat loop context
    pub repeat: Option<RepeatContext>,

    /// Wait context from last wait_for_trigger
    pub wait: Option<WaitContext>,
}

impl ExecutionContext {
    /// Create new execution context
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            trigger: None,
            response: None,
            stop_on_condition_fail: true,
            repeat: None,
            wait: None,
        }
    }

    /// Create with trigger data
    pub fn with_trigger(trigger: TriggerData) -> Self {
        Self {
            trigger: Some(trigger),
            ..Self::new()
        }
    }

    /// Set a variable
    pub fn set_var(&mut self, key: impl Into<String>, value: Value) {
        self.variables.insert(key.into(), value);
    }

    /// Get a variable
    pub fn get_var(&self, key: &str) -> Option<&Value> {
        self.variables.get(key)
    }

    /// Convert to template variables
    pub fn to_template_vars(&self) -> Value {
        let mut vars = serde_json::Map::new();

        // Add all variables
        for (k, v) in &self.variables {
            vars.insert(k.clone(), v.clone());
        }

        // Add trigger data if available
        if let Some(trigger) = &self.trigger {
            vars.insert(
                "trigger".to_string(),
                serde_json::to_value(trigger).unwrap_or(Value::Null),
            );
        }

        // Add repeat context if available
        if let Some(repeat) = &self.repeat {
            let mut repeat_obj = serde_json::Map::new();
            repeat_obj.insert("index".to_string(), Value::Number(repeat.index.into()));
            repeat_obj.insert("first".to_string(), Value::Bool(repeat.first));
            repeat_obj.insert("last".to_string(), Value::Bool(repeat.last));
            if let Some(item) = &repeat.item {
                repeat_obj.insert("item".to_string(), item.clone());
            }
            vars.insert("repeat".to_string(), Value::Object(repeat_obj));
        }

        // Add wait context if available
        if let Some(wait) = &self.wait {
            let mut wait_obj = serde_json::Map::new();
            if let Some(trigger) = &wait.trigger {
                wait_obj.insert(
                    "trigger".to_string(),
                    serde_json::to_value(trigger).unwrap_or(Value::Null),
                );
            } else {
                wait_obj.insert("trigger".to_string(), Value::Null);
            }
            wait_obj.insert(
                "remaining".to_string(),
                Value::Number((wait.remaining_secs as i64).into()),
            );
            wait_obj.insert("completed".to_string(), Value::Bool(wait.completed));
            vars.insert("wait".to_string(), Value::Object(wait_obj));
        }

        Value::Object(vars)
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Repeat loop context
#[derive(Debug, Clone)]
pub struct RepeatContext {
    /// Current iteration index (1-based)
    pub index: usize,
    /// Whether this is the first iteration
    pub first: bool,
    /// Whether this is the last iteration
    pub last: bool,
    /// Current item (for for_each loops)
    pub item: Option<Value>,
}

/// Wait context from wait_for_trigger
#[derive(Debug, Clone)]
pub struct WaitContext {
    /// The trigger that fired (None if timed out)
    pub trigger: Option<TriggerData>,
    /// Remaining seconds (0 if completed normally)
    pub remaining_secs: f64,
    /// Whether completed without timeout
    pub completed: bool,
}

/// Script executor
///
/// Note: This is a minimal implementation that defines the types and structure.
/// Full execution logic requires integration with the event bus, state machine,
/// service registry, and template engine, which will be wired up in ha-server.
pub struct ScriptExecutor {
    // In a full implementation, this would hold references to:
    // - EventBus
    // - StateMachine
    // - ServiceRegistry
    // - TemplateEngine
}

impl ScriptExecutor {
    /// Create a new script executor
    pub fn new() -> Self {
        Self {}
    }

    /// Execute a sequence of actions
    ///
    /// Note: This is a stub. Full implementation requires event bus, state machine, etc.
    pub async fn execute(
        &self,
        actions: &[Value],
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<Option<Value>> {
        debug!("Executing {} actions", actions.len());

        for (i, action_value) in actions.iter().enumerate() {
            trace!("Executing action {}: {:?}", i, action_value);

            // Parse action
            let action: Action = serde_json::from_value(action_value.clone())
                .map_err(|e| ScriptExecutorError::InvalidAction(e.to_string()))?;

            // Execute based on action type
            match action {
                Action::Service(service) => {
                    if !service.enabled {
                        continue;
                    }
                    debug!("Service call: {}", service.service);
                    // In full implementation: call service registry
                }
                Action::Delay(delay) => {
                    if !delay.enabled {
                        continue;
                    }
                    if let Some(duration) = delay.delay.to_duration() {
                        debug!("Delaying for {:?}", duration);
                        tokio::time::sleep(duration).await;
                    }
                }
                Action::Variables(vars) => {
                    if !vars.enabled {
                        continue;
                    }
                    for (key, value) in vars.variables {
                        // In full implementation: render template if string
                        ctx.set_var(key, value);
                    }
                }
                Action::Condition(cond) => {
                    if !cond.enabled {
                        continue;
                    }
                    // In full implementation: evaluate condition
                    // If false and stop_on_condition_fail, return
                }
                Action::Stop(stop) => {
                    if !stop.enabled {
                        continue;
                    }
                    if stop.error {
                        return Err(ScriptExecutorError::Stopped(stop.stop));
                    }
                    debug!("Script stopped: {}", stop.stop);
                    return Ok(None);
                }
                // Other action types would be handled similarly
                _ => {
                    trace!("Action type not fully implemented yet");
                }
            }
        }

        Ok(ctx.response.clone())
    }
}

impl Default for ScriptExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_context() {
        let mut ctx = ExecutionContext::new();
        ctx.set_var("brightness", serde_json::json!(255));

        assert_eq!(ctx.get_var("brightness"), Some(&serde_json::json!(255)));
        assert_eq!(ctx.get_var("nonexistent"), None);
    }

    #[test]
    fn test_context_to_template_vars() {
        let mut ctx = ExecutionContext::new();
        ctx.set_var("test", serde_json::json!("value"));
        ctx.repeat = Some(RepeatContext {
            index: 2,
            first: false,
            last: false,
            item: Some(serde_json::json!("item_value")),
        });

        let vars = ctx.to_template_vars();
        assert!(vars.is_object());

        let obj = vars.as_object().unwrap();
        assert_eq!(obj.get("test"), Some(&serde_json::json!("value")));

        let repeat = obj.get("repeat").unwrap().as_object().unwrap();
        assert_eq!(repeat.get("index"), Some(&serde_json::json!(2)));
        assert_eq!(repeat.get("first"), Some(&serde_json::json!(false)));
        assert_eq!(repeat.get("item"), Some(&serde_json::json!("item_value")));
    }

    #[test]
    fn test_context_with_trigger() {
        let trigger = TriggerData::new("state")
            .with_id("motion")
            .with_var("entity_id", serde_json::json!("binary_sensor.motion"));

        let ctx = ExecutionContext::with_trigger(trigger.clone());
        assert!(ctx.trigger.is_some());

        let vars = ctx.to_template_vars();
        let obj = vars.as_object().unwrap();
        assert!(obj.contains_key("trigger"));
    }

    #[test]
    fn test_wait_context() {
        let mut ctx = ExecutionContext::new();
        ctx.wait = Some(WaitContext {
            trigger: None,
            remaining_secs: 0.0,
            completed: false,
        });

        let vars = ctx.to_template_vars();
        let obj = vars.as_object().unwrap();
        let wait = obj.get("wait").unwrap().as_object().unwrap();

        assert_eq!(wait.get("trigger"), Some(&Value::Null));
        assert_eq!(wait.get("completed"), Some(&serde_json::json!(false)));
    }

    #[tokio::test]
    async fn test_executor_delay() {
        let executor = ScriptExecutor::new();
        let mut ctx = ExecutionContext::new();

        let actions = vec![serde_json::json!({
            "delay": {"seconds": 0}  // 0 second delay for test
        })];

        let result = executor.execute(&actions, &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_stop() {
        let executor = ScriptExecutor::new();
        let mut ctx = ExecutionContext::new();

        let actions = vec![serde_json::json!({
            "stop": "Test stop",
            "error": false
        })];

        let result = executor.execute(&actions, &mut ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_executor_stop_error() {
        let executor = ScriptExecutor::new();
        let mut ctx = ExecutionContext::new();

        let actions = vec![serde_json::json!({
            "stop": "Test error",
            "error": true
        })];

        let result = executor.execute(&actions, &mut ctx).await;
        assert!(matches!(result, Err(ScriptExecutorError::Stopped(_))));
    }

    #[tokio::test]
    async fn test_executor_variables() {
        let executor = ScriptExecutor::new();
        let mut ctx = ExecutionContext::new();

        let actions = vec![serde_json::json!({
            "variables": {
                "brightness": 255,
                "color": "red"
            }
        })];

        executor.execute(&actions, &mut ctx).await.unwrap();

        assert_eq!(ctx.get_var("brightness"), Some(&serde_json::json!(255)));
        assert_eq!(ctx.get_var("color"), Some(&serde_json::json!("red")));
    }
}
