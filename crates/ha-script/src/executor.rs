//! Script executor
//!
//! Executes script actions with proper context, variable handling, and control flow.
//! The executor is wired to core systems: ServiceRegistry, StateStore, EventBus,
//! TemplateEngine, and ConditionEvaluator.

use crate::action::{Action, ChooseConditions, DelaySpec, RepeatConfig, RepeatCount};
use ha_automation::{ConditionEvaluator, EvalContext, TriggerData};
use ha_core::Context;
use ha_event_bus::EventBus;
use ha_service_registry::ServiceRegistry;
use ha_state_store::StateStore;
use ha_template::TemplateEngine;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, trace, warn};

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

    /// Convert to EvalContext for condition evaluation
    pub fn to_eval_context(&self) -> EvalContext {
        let mut eval_ctx = if let Some(trigger) = &self.trigger {
            EvalContext::with_trigger(trigger.clone())
        } else {
            EvalContext::new()
        };

        // Add variables
        for (k, v) in &self.variables {
            eval_ctx = eval_ctx.with_var(k.clone(), v.clone());
        }

        eval_ctx
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
/// Executes script actions with access to core Home Assistant systems.
pub struct ScriptExecutor {
    #[allow(dead_code)] // Reserved for future use (state access in actions)
    state_machine: Arc<StateStore>,
    service_registry: Arc<ServiceRegistry>,
    template_engine: Arc<TemplateEngine>,
    event_bus: Arc<EventBus>,
    condition_evaluator: Arc<ConditionEvaluator>,
}

impl ScriptExecutor {
    /// Create a new script executor with all required systems
    pub fn new(
        state_machine: Arc<StateStore>,
        service_registry: Arc<ServiceRegistry>,
        template_engine: Arc<TemplateEngine>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        let condition_evaluator = Arc::new(ConditionEvaluator::new(
            state_machine.clone(),
            template_engine.clone(),
        ));

        Self {
            state_machine,
            service_registry,
            template_engine,
            event_bus,
            condition_evaluator,
        }
    }

    /// Execute a sequence of actions
    pub fn execute<'a>(
        &'a self,
        actions: &'a [Value],
        ctx: &'a mut ExecutionContext,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = ScriptExecutorResult<Option<Value>>> + Send + 'a>,
    > {
        Box::pin(async move {
            debug!("Executing {} actions", actions.len());

            for (i, action_value) in actions.iter().enumerate() {
                trace!("Executing action {}: {:?}", i, action_value);

                // Parse action
                let action: Action = serde_json::from_value(action_value.clone())
                    .map_err(|e| ScriptExecutorError::InvalidAction(e.to_string()))?;

                // Execute based on action type
                match self.execute_action(&action, ctx).await {
                    Ok(ActionResult::Continue) => continue,
                    Ok(ActionResult::Stop) => return Ok(ctx.response.clone()),
                    Ok(ActionResult::StopWithResponse(response)) => return Ok(Some(response)),
                    Err(e) => return Err(e),
                }
            }

            Ok(ctx.response.clone())
        })
    }

    /// Execute a single action
    async fn execute_action(
        &self,
        action: &Action,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        match action {
            Action::Service(service) => {
                if !service.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_service(service, ctx).await
            }
            Action::Delay(delay) => {
                if !delay.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_delay(delay, ctx).await
            }
            Action::Variables(vars) => {
                if !vars.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_variables(vars, ctx).await
            }
            Action::Condition(cond) => {
                if !cond.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_condition(cond, ctx).await
            }
            Action::Stop(stop) => {
                if !stop.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_stop(stop, ctx).await
            }
            Action::Event(event) => {
                if !event.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_event(event, ctx).await
            }
            Action::Scene(scene) => {
                if !scene.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_scene(scene, ctx).await
            }
            Action::Choose(choose) => {
                if !choose.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_choose(choose, ctx).await
            }
            Action::If(if_action) => {
                if !if_action.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_if(if_action, ctx).await
            }
            Action::Repeat(repeat) => {
                if !repeat.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_repeat(repeat, ctx).await
            }
            Action::Sequence(seq) => {
                if !seq.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute(&seq.sequence, ctx).await?;
                Ok(ActionResult::Continue)
            }
            Action::Parallel(parallel) => {
                if !parallel.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_parallel(parallel, ctx).await
            }
            Action::WaitForTrigger(wait) => {
                if !wait.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_wait_for_trigger(wait, ctx).await
            }
            Action::WaitTemplate(wait) => {
                if !wait.enabled {
                    return Ok(ActionResult::Continue);
                }
                self.execute_wait_template(wait, ctx).await
            }
        }
    }

    // --- Individual action executors ---

    async fn execute_service(
        &self,
        service: &crate::action::ServiceAction,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        // Parse domain.service
        let (domain, svc_name) = service.service.split_once('.').ok_or_else(|| {
            ScriptExecutorError::InvalidAction(format!(
                "Invalid service format: {}",
                service.service
            ))
        })?;

        // Render template values in service data
        let mut service_data = serde_json::Map::new();
        let template_ctx = ctx.to_template_vars();

        for (key, value) in &service.data {
            let rendered_value = self.render_value(value, &template_ctx)?;
            service_data.insert(key.clone(), rendered_value);
        }

        // Add target to service data if present
        if let Some(target) = &service.target {
            if !target.entity_id.is_empty() {
                service_data.insert(
                    "entity_id".to_string(),
                    serde_json::to_value(&target.entity_id).unwrap(),
                );
            }
            if !target.device_id.is_empty() {
                service_data.insert(
                    "device_id".to_string(),
                    serde_json::to_value(&target.device_id).unwrap(),
                );
            }
            if !target.area_id.is_empty() {
                service_data.insert(
                    "area_id".to_string(),
                    serde_json::to_value(&target.area_id).unwrap(),
                );
            }
        }

        debug!("Calling service: {}.{}", domain, svc_name);

        let return_response = service.response_variable.is_some();
        let result = self
            .service_registry
            .call(
                domain,
                svc_name,
                Value::Object(service_data),
                Context::new(),
                return_response,
            )
            .await
            .map_err(|e| ScriptExecutorError::ServiceCallFailed(e.to_string()))?;

        // Store response if requested
        if let Some(var_name) = &service.response_variable {
            if let Some(response) = result {
                ctx.set_var(var_name.clone(), response.clone());
                ctx.response = Some(response);
            }
        }

        Ok(ActionResult::Continue)
    }

    async fn execute_delay(
        &self,
        delay: &crate::action::DelayAction,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        let duration = match &delay.delay {
            DelaySpec::Components {
                hours,
                minutes,
                seconds,
                milliseconds,
            } => Duration::from_millis(
                hours * 3600 * 1000 + minutes * 60 * 1000 + seconds * 1000 + milliseconds,
            ),
            DelaySpec::Template(template) => {
                // Render template to get duration string
                let template_ctx = ctx.to_template_vars();
                let rendered = self
                    .template_engine
                    .render_with_context(template, &template_ctx)
                    .map_err(|e| ScriptExecutorError::Template(e.to_string()))?;

                // Parse duration string (HH:MM:SS or seconds)
                parse_duration(&rendered).ok_or_else(|| {
                    ScriptExecutorError::Template(format!("Invalid duration: {}", rendered))
                })?
            }
        };

        debug!("Delaying for {:?}", duration);
        tokio::time::sleep(duration).await;
        Ok(ActionResult::Continue)
    }

    async fn execute_variables(
        &self,
        vars: &crate::action::VariablesAction,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        let template_ctx = ctx.to_template_vars();

        for (key, value) in &vars.variables {
            let rendered_value = self.render_value(value, &template_ctx)?;
            ctx.set_var(key.clone(), rendered_value);
        }

        Ok(ActionResult::Continue)
    }

    async fn execute_condition(
        &self,
        cond: &crate::action::ConditionAction,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        let eval_ctx = ctx.to_eval_context();

        let result = self
            .condition_evaluator
            .evaluate(&cond.condition, &eval_ctx)
            .map_err(|e| ScriptExecutorError::ActionError(e.to_string()))?;

        if !result {
            if ctx.stop_on_condition_fail {
                debug!("Condition failed, stopping script");
                return Err(ScriptExecutorError::ConditionFailed);
            }
            debug!("Condition failed, but continue_on_fail is set");
        }

        Ok(ActionResult::Continue)
    }

    async fn execute_stop(
        &self,
        stop: &crate::action::StopAction,
        _ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        debug!("Script stopped: {}", stop.stop);

        if stop.error {
            return Err(ScriptExecutorError::Stopped(stop.stop.clone()));
        }

        Ok(ActionResult::Stop)
    }

    async fn execute_event(
        &self,
        event: &crate::action::EventAction,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        // Render template values in event data
        let mut event_data = serde_json::Map::new();
        let template_ctx = ctx.to_template_vars();

        for (key, value) in &event.event_data {
            let rendered_value = self.render_value(value, &template_ctx)?;
            event_data.insert(key.clone(), rendered_value);
        }

        debug!("Firing event: {}", event.event);

        let ha_event = ha_core::Event::new(
            event.event.clone(),
            Value::Object(event_data),
            Context::new(),
        );
        self.event_bus.fire(ha_event);

        Ok(ActionResult::Continue)
    }

    async fn execute_scene(
        &self,
        scene: &crate::action::SceneAction,
        _ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        // Scene activation is done via scene.turn_on service
        debug!("Activating scene: {}", scene.scene);

        let service_data = serde_json::json!({
            "entity_id": scene.scene
        });

        self.service_registry
            .call("scene", "turn_on", service_data, Context::new(), false)
            .await
            .map_err(|e| ScriptExecutorError::ServiceCallFailed(e.to_string()))?;

        Ok(ActionResult::Continue)
    }

    async fn execute_choose(
        &self,
        choose: &crate::action::ChooseAction,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        let eval_ctx = ctx.to_eval_context();

        // Find first matching choice
        for option in &choose.choose {
            let matches = self.evaluate_choose_conditions(&option.conditions, &eval_ctx)?;

            if matches {
                debug!("Choose option matched, executing sequence");
                self.execute(&option.sequence, ctx).await?;
                return Ok(ActionResult::Continue);
            }
        }

        // No match, execute default
        if !choose.default.is_empty() {
            debug!("No choose option matched, executing default");
            self.execute(&choose.default, ctx).await?;
        }

        Ok(ActionResult::Continue)
    }

    async fn execute_if(
        &self,
        if_action: &crate::action::IfAction,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        let eval_ctx = ctx.to_eval_context();

        let matches = self.evaluate_choose_conditions(&if_action.r#if, &eval_ctx)?;

        if matches {
            debug!("If condition matched, executing then");
            self.execute(&if_action.then, ctx).await?;
        } else if !if_action.r#else.is_empty() {
            debug!("If condition didn't match, executing else");
            self.execute(&if_action.r#else, ctx).await?;
        }

        Ok(ActionResult::Continue)
    }

    async fn execute_repeat(
        &self,
        repeat: &crate::action::RepeatAction,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        match &repeat.repeat {
            RepeatConfig::Count { count, sequence } => {
                let count_value = match count {
                    RepeatCount::Number(n) => *n,
                    RepeatCount::Template(template) => {
                        let template_ctx = ctx.to_template_vars();
                        let rendered = self
                            .template_engine
                            .render_with_context(template, &template_ctx)
                            .map_err(|e| ScriptExecutorError::Template(e.to_string()))?;
                        rendered.parse().map_err(|_| {
                            ScriptExecutorError::Template(format!("Invalid count: {}", rendered))
                        })?
                    }
                };

                for i in 1..=count_value {
                    ctx.repeat = Some(RepeatContext {
                        index: i,
                        first: i == 1,
                        last: i == count_value,
                        item: None,
                    });
                    self.execute(sequence, ctx).await?;
                }
                ctx.repeat = None;
            }

            RepeatConfig::ForEach { for_each, sequence } => {
                // Render for_each if it's a template
                let items: Vec<Value> = if let Value::Array(arr) = for_each {
                    arr.clone()
                } else if let Value::String(template) = for_each {
                    let template_ctx = ctx.to_template_vars();
                    let rendered = self
                        .template_engine
                        .render_with_context(template, &template_ctx)
                        .map_err(|e| ScriptExecutorError::Template(e.to_string()))?;
                    serde_json::from_str(&rendered).map_err(|e| {
                        ScriptExecutorError::Template(format!("Invalid for_each list: {}", e))
                    })?
                } else {
                    vec![for_each.clone()]
                };

                let total = items.len();
                for (i, item) in items.into_iter().enumerate() {
                    ctx.repeat = Some(RepeatContext {
                        index: i + 1,
                        first: i == 0,
                        last: i == total - 1,
                        item: Some(item),
                    });
                    self.execute(sequence, ctx).await?;
                }
                ctx.repeat = None;
            }

            RepeatConfig::While { r#while, sequence } => {
                let mut index = 1;
                loop {
                    let eval_ctx = ctx.to_eval_context();
                    let should_continue = self
                        .condition_evaluator
                        .evaluate_all(r#while, &eval_ctx)
                        .map_err(|e| ScriptExecutorError::ActionError(e.to_string()))?;

                    if !should_continue {
                        break;
                    }

                    ctx.repeat = Some(RepeatContext {
                        index,
                        first: index == 1,
                        last: false, // Unknown for while loops
                        item: None,
                    });

                    self.execute(sequence, ctx).await?;
                    index += 1;

                    // Safety limit
                    if index > 10000 {
                        warn!("Repeat while loop exceeded 10000 iterations, stopping");
                        break;
                    }
                }
                ctx.repeat = None;
            }

            RepeatConfig::Until { until, sequence } => {
                let mut index = 1;
                loop {
                    ctx.repeat = Some(RepeatContext {
                        index,
                        first: index == 1,
                        last: false,
                        item: None,
                    });

                    self.execute(sequence, ctx).await?;

                    let eval_ctx = ctx.to_eval_context();
                    let should_stop = self
                        .condition_evaluator
                        .evaluate_all(until, &eval_ctx)
                        .map_err(|e| ScriptExecutorError::ActionError(e.to_string()))?;

                    if should_stop {
                        break;
                    }

                    index += 1;

                    // Safety limit
                    if index > 10000 {
                        warn!("Repeat until loop exceeded 10000 iterations, stopping");
                        break;
                    }
                }
                ctx.repeat = None;
            }
        }

        Ok(ActionResult::Continue)
    }

    async fn execute_parallel(
        &self,
        parallel: &crate::action::ParallelAction,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        // For parallel execution, we use tokio::join! macro style
        // Each parallel branch gets its own context clone
        // Results are collected and any error fails the whole parallel block

        let mut futures = Vec::new();

        for action_value in &parallel.parallel {
            // Wrap each action sequence in a vec if it's a single action
            let actions = if action_value.is_array() {
                action_value.as_array().unwrap().clone()
            } else {
                vec![action_value.clone()]
            };
            futures.push(actions);
        }

        // Execute all branches "in parallel" by interleaving
        // Note: True parallelism requires spawn which has Send constraints
        // This executes sequentially but maintains parallel semantics for simple cases
        let mut results = Vec::new();
        for actions in futures {
            let mut branch_ctx = ctx.clone();
            let result = self.execute(&actions, &mut branch_ctx).await;
            results.push(result);
        }

        // Check for any errors
        for result in results {
            result?;
        }

        Ok(ActionResult::Continue)
    }

    async fn execute_wait_for_trigger(
        &self,
        wait: &crate::action::WaitForTriggerAction,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        // TODO: Full implementation requires trigger matching against events
        // For now, just handle timeout
        let timeout = if let Some(timeout_str) = &wait.timeout {
            let template_ctx = ctx.to_template_vars();
            let rendered = self
                .template_engine
                .render_with_context(timeout_str, &template_ctx)
                .map_err(|e| ScriptExecutorError::Template(e.to_string()))?;
            parse_duration(&rendered)
        } else {
            None
        };

        debug!("Wait for trigger (timeout: {:?})", timeout);

        if let Some(duration) = timeout {
            tokio::time::sleep(duration).await;
        }

        ctx.wait = Some(WaitContext {
            trigger: None,
            remaining_secs: 0.0,
            completed: false,
        });

        if !wait.continue_on_timeout {
            return Err(ScriptExecutorError::Timeout);
        }

        Ok(ActionResult::Continue)
    }

    async fn execute_wait_template(
        &self,
        wait: &crate::action::WaitTemplateAction,
        ctx: &mut ExecutionContext,
    ) -> ScriptExecutorResult<ActionResult> {
        let timeout = if let Some(timeout_str) = &wait.timeout {
            let template_ctx = ctx.to_template_vars();
            let rendered = self
                .template_engine
                .render_with_context(timeout_str, &template_ctx)
                .map_err(|e| ScriptExecutorError::Template(e.to_string()))?;
            parse_duration(&rendered)
        } else {
            None
        };

        let start = std::time::Instant::now();
        let max_wait = timeout.unwrap_or(Duration::from_secs(3600)); // Default 1 hour max

        loop {
            let template_ctx = ctx.to_template_vars();
            let result = self
                .template_engine
                .render_with_context(&wait.wait_template, &template_ctx)
                .map_err(|e| ScriptExecutorError::Template(e.to_string()))?;

            if is_truthy(&result) {
                ctx.wait = Some(WaitContext {
                    trigger: None,
                    remaining_secs: 0.0,
                    completed: true,
                });
                return Ok(ActionResult::Continue);
            }

            if start.elapsed() >= max_wait {
                ctx.wait = Some(WaitContext {
                    trigger: None,
                    remaining_secs: 0.0,
                    completed: false,
                });

                if !wait.continue_on_timeout {
                    return Err(ScriptExecutorError::Timeout);
                }
                return Ok(ActionResult::Continue);
            }

            // Poll every 100ms
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    // --- Helper methods ---

    fn evaluate_choose_conditions(
        &self,
        conditions: &ChooseConditions,
        eval_ctx: &EvalContext,
    ) -> ScriptExecutorResult<bool> {
        match conditions {
            ChooseConditions::Template(template) => {
                let result = self
                    .template_engine
                    .render_with_context(template, eval_ctx.to_template_context())
                    .map_err(|e| ScriptExecutorError::Template(e.to_string()))?;
                Ok(is_truthy(&result))
            }
            ChooseConditions::List(conditions) => self
                .condition_evaluator
                .evaluate_all(conditions, eval_ctx)
                .map_err(|e| ScriptExecutorError::ActionError(e.to_string())),
        }
    }

    fn render_value(&self, value: &Value, template_ctx: &Value) -> ScriptExecutorResult<Value> {
        match value {
            Value::String(s) if TemplateEngine::is_template(s) => {
                let rendered = self
                    .template_engine
                    .render_with_context(s, template_ctx)
                    .map_err(|e| ScriptExecutorError::Template(e.to_string()))?;

                // Try to parse as JSON, otherwise keep as string
                Ok(serde_json::from_str(&rendered).unwrap_or(Value::String(rendered)))
            }
            Value::Object(obj) => {
                let mut new_obj = serde_json::Map::new();
                for (k, v) in obj {
                    new_obj.insert(k.clone(), self.render_value(v, template_ctx)?);
                }
                Ok(Value::Object(new_obj))
            }
            Value::Array(arr) => {
                let new_arr: Result<Vec<_>, _> = arr
                    .iter()
                    .map(|v| self.render_value(v, template_ctx))
                    .collect();
                Ok(Value::Array(new_arr?))
            }
            _ => Ok(value.clone()),
        }
    }
}

/// Result of executing a single action
#[allow(dead_code)] // StopWithResponse reserved for future use
enum ActionResult {
    /// Continue to next action
    Continue,
    /// Stop script execution
    Stop,
    /// Stop with a response value
    StopWithResponse(Value),
}

// --- Utility functions ---

/// Parse duration from string (HH:MM:SS or seconds)
fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();

    // Try as seconds
    if let Ok(secs) = s.parse::<f64>() {
        return Some(Duration::from_secs_f64(secs));
    }

    // Try as HH:MM:SS
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        2 => {
            let mins: u64 = parts[0].parse().ok()?;
            let secs: u64 = parts[1].parse().ok()?;
            Some(Duration::from_secs(mins * 60 + secs))
        }
        3 => {
            let hours: u64 = parts[0].parse().ok()?;
            let mins: u64 = parts[1].parse().ok()?;
            let secs: u64 = parts[2].parse().ok()?;
            Some(Duration::from_secs(hours * 3600 + mins * 60 + secs))
        }
        _ => None,
    }
}

/// Check if a string value is truthy
fn is_truthy(value: &str) -> bool {
    let trimmed = value.trim().to_lowercase();

    if trimmed.is_empty() {
        return false;
    }

    !matches!(trimmed.as_str(), "false" | "no" | "off" | "0" | "none")
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

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("60"), Some(Duration::from_secs(60)));
        assert_eq!(parse_duration("5:30"), Some(Duration::from_secs(330)));
        assert_eq!(parse_duration("1:30:00"), Some(Duration::from_secs(5400)));
        assert_eq!(parse_duration("invalid"), None);
    }

    #[test]
    fn test_is_truthy() {
        assert!(is_truthy("true"));
        assert!(is_truthy("True"));
        assert!(is_truthy("yes"));
        assert!(is_truthy("1"));
        assert!(is_truthy("hello"));

        assert!(!is_truthy("false"));
        assert!(!is_truthy("no"));
        assert!(!is_truthy("0"));
        assert!(!is_truthy(""));
    }
}
