//! Automation execution engine
//!
//! This module provides the `AutomationEngine` which orchestrates the complete
//! trigger→condition→action pipeline for automations. It listens for events,
//! matches them against triggers, evaluates conditions, and executes action sequences.

#![allow(clippy::too_many_arguments)]

use ha_automation::{
    Automation, AutomationManager, ConditionEvaluator, EvalContext, ExecutionMode, TriggerData,
    TriggerEvalContext, TriggerEvaluator,
};
use ha_core::Event;
use ha_event_bus::EventBus;
use ha_service_registry::ServiceRegistry;
use ha_state_store::StateStore;
use ha_template::TemplateEngine;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, trace, warn};

/// Automation engine that orchestrates trigger→condition→action flow
pub struct AutomationEngine {
    /// Event bus for subscribing to events
    event_bus: Arc<EventBus>,
    /// State machine for entity state
    state_machine: Arc<StateStore>,
    /// Service registry for calling services
    service_registry: Arc<ServiceRegistry>,
    /// Template engine for rendering templates
    template_engine: Arc<TemplateEngine>,
    /// Automation manager with all registered automations
    manager: Arc<RwLock<AutomationManager>>,
    /// Trigger evaluator
    trigger_evaluator: Arc<TriggerEvaluator>,
    /// Condition evaluator
    condition_evaluator: Arc<ConditionEvaluator>,
    /// Running flag
    running: Arc<AtomicBool>,
    /// Shutdown signal sender
    shutdown_tx: broadcast::Sender<()>,
    /// Currently executing automations (keyed by automation ID)
    executing: Arc<RwLock<HashMap<String, usize>>>,
}

impl AutomationEngine {
    /// Create a new automation engine
    pub fn new(
        event_bus: Arc<EventBus>,
        state_machine: Arc<StateStore>,
        service_registry: Arc<ServiceRegistry>,
        template_engine: Arc<TemplateEngine>,
    ) -> Self {
        let trigger_evaluator = Arc::new(TriggerEvaluator::new(
            state_machine.clone(),
            template_engine.clone(),
        ));
        let condition_evaluator = Arc::new(ConditionEvaluator::new(
            state_machine.clone(),
            template_engine.clone(),
        ));

        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            event_bus,
            state_machine,
            service_registry,
            template_engine,
            manager: Arc::new(RwLock::new(AutomationManager::new())),
            trigger_evaluator,
            condition_evaluator,
            running: Arc::new(AtomicBool::new(false)),
            shutdown_tx,
            executing: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get a reference to the automation manager for configuration
    pub fn manager(&self) -> Arc<RwLock<AutomationManager>> {
        self.manager.clone()
    }

    /// Start the automation engine
    ///
    /// This subscribes to all events and begins processing triggers.
    pub async fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            warn!("Automation engine already running");
            return;
        }

        info!("Starting automation engine");

        // Subscribe to all events
        let mut event_rx = self.event_bus.subscribe_all();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        let event_bus = self.event_bus.clone();
        let state_machine = self.state_machine.clone();
        let service_registry = self.service_registry.clone();
        let template_engine = self.template_engine.clone();
        let manager = self.manager.clone();
        let trigger_evaluator = self.trigger_evaluator.clone();
        let condition_evaluator = self.condition_evaluator.clone();
        let running = self.running.clone();
        let executing = self.executing.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    event_result = event_rx.recv() => {
                        match event_result {
                            Ok(event) => {
                                Self::process_event(
                                    &event,
                                    &event_bus,
                                    &state_machine,
                                    &service_registry,
                                    &template_engine,
                                    &manager,
                                    &trigger_evaluator,
                                    &condition_evaluator,
                                    &executing,
                                ).await;
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!("Automation engine lagged by {} events", n);
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                info!("Event bus closed, stopping automation engine");
                                break;
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Received shutdown signal");
                        break;
                    }
                }
            }

            running.store(false, Ordering::SeqCst);
            info!("Automation engine stopped");
        });
    }

    /// Stop the automation engine
    pub fn stop(&self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        info!("Stopping automation engine");
        let _ = self.shutdown_tx.send(());
    }

    /// Check if the engine is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Manually trigger an automation by ID
    pub async fn trigger(&self, automation_id: &str, trigger_data: Option<TriggerData>) {
        let manager = self.manager.read().await;
        if let Some(automation) = manager.get(automation_id) {
            if !automation.enabled {
                debug!(automation_id, "Automation is disabled, not triggering");
                return;
            }

            let trigger_data = trigger_data.unwrap_or_else(|| TriggerData::new("manual"));

            Self::run_automation(
                &automation,
                trigger_data,
                &self.event_bus,
                &self.state_machine,
                &self.service_registry,
                &self.template_engine,
                &self.condition_evaluator,
                &self.executing,
            )
            .await;
        } else {
            warn!(automation_id, "Automation not found");
        }
    }

    /// Process an incoming event against all automations
    async fn process_event(
        event: &Event<serde_json::Value>,
        event_bus: &Arc<EventBus>,
        state_machine: &Arc<StateStore>,
        service_registry: &Arc<ServiceRegistry>,
        template_engine: &Arc<TemplateEngine>,
        manager: &Arc<RwLock<AutomationManager>>,
        trigger_evaluator: &Arc<TriggerEvaluator>,
        condition_evaluator: &Arc<ConditionEvaluator>,
        executing: &Arc<RwLock<HashMap<String, usize>>>,
    ) {
        trace!(event_type = %event.event_type, "Processing event");

        let manager_guard = manager.read().await;
        let automations = manager_guard.all();

        for automation in automations {
            if !automation.enabled {
                continue;
            }

            // Check each trigger
            for trigger in &automation.triggers {
                let ctx = TriggerEvalContext::new();

                match trigger_evaluator.evaluate(trigger, event, &ctx) {
                    Ok(Some(trigger_data)) => {
                        debug!(
                            automation_id = %automation.id,
                            trigger_platform = %trigger_data.platform,
                            "Trigger matched"
                        );

                        // Run automation in the background
                        let automation = automation.clone();
                        let trigger_data = trigger_data.clone();
                        let event_bus = event_bus.clone();
                        let state_machine = state_machine.clone();
                        let service_registry = service_registry.clone();
                        let template_engine = template_engine.clone();
                        let condition_evaluator = condition_evaluator.clone();
                        let executing = executing.clone();

                        tokio::spawn(async move {
                            Self::run_automation(
                                &automation,
                                trigger_data,
                                &event_bus,
                                &state_machine,
                                &service_registry,
                                &template_engine,
                                &condition_evaluator,
                                &executing,
                            )
                            .await;
                        });
                    }
                    Ok(None) => {
                        // Trigger didn't match
                    }
                    Err(e) => {
                        warn!(
                            automation_id = %automation.id,
                            error = %e,
                            "Error evaluating trigger"
                        );
                    }
                }
            }
        }
    }

    /// Run a single automation
    async fn run_automation(
        automation: &Automation,
        trigger_data: TriggerData,
        event_bus: &Arc<EventBus>,
        state_machine: &Arc<StateStore>,
        service_registry: &Arc<ServiceRegistry>,
        template_engine: &Arc<TemplateEngine>,
        condition_evaluator: &Arc<ConditionEvaluator>,
        executing: &Arc<RwLock<HashMap<String, usize>>>,
    ) {
        let automation_id = automation.id.clone();

        // Check execution mode
        {
            let mut exec_guard = executing.write().await;
            let current_runs = exec_guard.get(&automation_id).copied().unwrap_or(0);

            // Use the can_run() logic from automation, simulated here
            let can_run = match &automation.mode {
                ExecutionMode::Single => current_runs == 0,
                ExecutionMode::Restart => true, // Always allows, would cancel existing
                ExecutionMode::Queued { max } => current_runs < *max,
                ExecutionMode::Parallel { max } => current_runs < *max,
            };

            if !can_run {
                debug!(
                    automation_id = %automation_id,
                    mode = ?automation.mode,
                    current_runs,
                    "Cannot run automation due to execution mode limits"
                );
                return;
            }

            // Increment run count
            *exec_guard.entry(automation_id.clone()).or_insert(0) += 1;
        }

        debug!(
            automation_id = %automation_id,
            "Running automation"
        );

        // Create evaluation context
        let eval_ctx = EvalContext::with_trigger(trigger_data.clone());

        // Evaluate conditions
        let conditions_pass = if automation.conditions.is_empty() {
            true
        } else {
            match condition_evaluator.evaluate_all(&automation.conditions, &eval_ctx) {
                Ok(result) => result,
                Err(e) => {
                    error!(
                        automation_id = %automation_id,
                        error = %e,
                        "Error evaluating conditions"
                    );
                    false
                }
            }
        };

        if !conditions_pass {
            debug!(
                automation_id = %automation_id,
                "Conditions not met, skipping action execution"
            );
            // Decrement run count
            let mut exec_guard = executing.write().await;
            if let Some(count) = exec_guard.get_mut(&automation_id) {
                *count = count.saturating_sub(1);
            }
            return;
        }

        // Execute actions
        info!(
            automation_id = %automation_id,
            "Executing automation actions"
        );

        // Create script executor
        let executor = ha_script::executor::ScriptExecutor::new(
            state_machine.clone(),
            service_registry.clone(),
            template_engine.clone(),
            event_bus.clone(),
        );

        let mut exec_ctx = ha_script::executor::ExecutionContext::with_trigger(trigger_data);

        let result = executor.execute(&automation.actions, &mut exec_ctx).await;

        match result {
            Ok(_) => {
                debug!(
                    automation_id = %automation_id,
                    "Automation completed successfully"
                );
            }
            Err(e) => {
                error!(
                    automation_id = %automation_id,
                    error = %e,
                    "Automation execution failed"
                );
            }
        }

        // Decrement run count
        {
            let mut exec_guard = executing.write().await;
            if let Some(count) = exec_guard.get_mut(&automation_id) {
                *count = count.saturating_sub(1);
            }
        }
    }
}
