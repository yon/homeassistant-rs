//! Automation management
//!
//! An automation ties together triggers, conditions, and actions.
//! The AutomationManager handles the lifecycle of all automations.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};

use crate::condition::Condition;
use crate::trigger::Trigger;

/// Automation errors
#[derive(Debug, Error)]
pub enum AutomationError {
    #[error("Automation not found: {0}")]
    NotFound(String),

    #[error("Invalid automation configuration: {0}")]
    InvalidConfig(String),

    #[error("Trigger error: {0}")]
    Trigger(#[from] crate::trigger::TriggerError),

    #[error("Condition error: {0}")]
    Condition(#[from] crate::condition::ConditionError),

    #[error("Action error: {0}")]
    Action(String),

    #[error("Automation is disabled: {0}")]
    Disabled(String),
}

/// Result type for automation operations
pub type AutomationResult<T> = Result<T, AutomationError>;

/// Execution mode for automations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Default - ignore new triggers while running
    #[default]
    Single,

    /// Restart from beginning on new trigger
    Restart,

    /// Queue triggers (up to max)
    Queued {
        #[serde(default = "default_max_queued")]
        max: usize,
    },

    /// Run all simultaneously (up to max)
    Parallel {
        #[serde(default = "default_max_parallel")]
        max: usize,
    },
}

fn default_max_queued() -> usize {
    10
}

fn default_max_parallel() -> usize {
    10
}

/// Automation configuration from YAML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationConfig {
    /// Unique ID (optional, auto-generated if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Human-readable name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Triggers that start the automation
    #[serde(default, alias = "trigger")]
    pub triggers: Vec<Trigger>,

    /// Conditions that must be met
    #[serde(default, alias = "condition")]
    pub conditions: Vec<Condition>,

    /// Actions to execute (raw JSON, handled by ha-script)
    #[serde(default, alias = "action")]
    pub actions: Vec<serde_json::Value>,

    /// Execution mode
    #[serde(default)]
    pub mode: ExecutionMode,

    /// Maximum number of runs (for queued/parallel modes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<usize>,

    /// Whether the automation is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Variables available to the automation
    #[serde(default)]
    pub variables: serde_json::Value,

    /// Trace settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<TraceConfig>,
}

fn default_enabled() -> bool {
    true
}

/// Trace configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceConfig {
    /// Number of traces to store
    #[serde(default = "default_stored_traces")]
    pub stored_traces: usize,
}

fn default_stored_traces() -> usize {
    5
}

/// A running automation instance
#[derive(Debug, Clone)]
pub struct Automation {
    /// Unique identifier
    pub id: String,

    /// Human-readable name
    pub alias: Option<String>,

    /// Description
    pub description: Option<String>,

    /// Triggers that start the automation
    pub triggers: Vec<Trigger>,

    /// Conditions that must be met
    pub conditions: Vec<Condition>,

    /// Actions to execute (raw JSON for ha-script)
    pub actions: Vec<serde_json::Value>,

    /// Execution mode
    pub mode: ExecutionMode,

    /// Whether enabled
    pub enabled: bool,

    /// Variables
    pub variables: serde_json::Value,

    /// Last triggered time
    pub last_triggered: Option<DateTime<Utc>>,

    /// Current running count
    pub current_runs: usize,

    /// Trace configuration
    pub trace_config: TraceConfig,
}

impl Automation {
    /// Create from config
    pub fn from_config(config: AutomationConfig) -> Self {
        let id = config.id.unwrap_or_else(|| ulid::Ulid::new().to_string());

        Self {
            id,
            alias: config.alias,
            description: config.description,
            triggers: config.triggers,
            conditions: config.conditions,
            actions: config.actions,
            mode: config.mode,
            enabled: config.enabled,
            variables: config.variables,
            last_triggered: None,
            current_runs: 0,
            trace_config: config.trace.unwrap_or(TraceConfig {
                stored_traces: default_stored_traces(),
            }),
        }
    }

    /// Get display name (alias or ID)
    pub fn display_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.id)
    }

    /// Check if can start new run based on mode
    pub fn can_run(&self) -> bool {
        if !self.enabled {
            return false;
        }

        match self.mode {
            ExecutionMode::Single => self.current_runs == 0,
            ExecutionMode::Restart => true,
            ExecutionMode::Queued { max } => self.current_runs < max,
            ExecutionMode::Parallel { max } => self.current_runs < max,
        }
    }
}

/// Manages all automations
pub struct AutomationManager {
    /// All automations by ID
    automations: DashMap<String, Automation>,
}

impl AutomationManager {
    /// Create a new automation manager
    pub fn new() -> Self {
        Self {
            automations: DashMap::new(),
        }
    }

    /// Load automations from configs
    pub fn load(&self, configs: Vec<AutomationConfig>) -> AutomationResult<()> {
        for config in configs {
            let automation = Automation::from_config(config);
            info!(
                "Loaded automation: {} ({})",
                automation.display_name(),
                automation.id
            );
            self.automations.insert(automation.id.clone(), automation);
        }
        Ok(())
    }

    /// Get an automation by ID
    pub fn get(&self, id: &str) -> Option<Automation> {
        self.automations.get(id).map(|a| a.value().clone())
    }

    /// Get all automations
    pub fn all(&self) -> Vec<Automation> {
        self.automations.iter().map(|a| a.value().clone()).collect()
    }

    /// Get automation count
    pub fn count(&self) -> usize {
        self.automations.len()
    }

    /// Enable an automation
    pub fn enable(&self, id: &str) -> AutomationResult<()> {
        let mut automation = self
            .automations
            .get_mut(id)
            .ok_or_else(|| AutomationError::NotFound(id.to_string()))?;

        automation.enabled = true;
        info!("Enabled automation: {}", automation.display_name());
        Ok(())
    }

    /// Disable an automation
    pub fn disable(&self, id: &str) -> AutomationResult<()> {
        let mut automation = self
            .automations
            .get_mut(id)
            .ok_or_else(|| AutomationError::NotFound(id.to_string()))?;

        automation.enabled = false;
        info!("Disabled automation: {}", automation.display_name());
        Ok(())
    }

    /// Toggle an automation
    pub fn toggle(&self, id: &str) -> AutomationResult<bool> {
        let mut automation = self
            .automations
            .get_mut(id)
            .ok_or_else(|| AutomationError::NotFound(id.to_string()))?;

        automation.enabled = !automation.enabled;
        info!(
            "{} automation: {}",
            if automation.enabled {
                "Enabled"
            } else {
                "Disabled"
            },
            automation.display_name()
        );
        Ok(automation.enabled)
    }

    /// Remove an automation
    pub fn remove(&self, id: &str) -> AutomationResult<Automation> {
        self.automations
            .remove(id)
            .map(|(_, a)| a)
            .ok_or_else(|| AutomationError::NotFound(id.to_string()))
    }

    /// Add a new automation
    pub fn add(&self, config: AutomationConfig) -> AutomationResult<String> {
        let automation = Automation::from_config(config);
        let id = automation.id.clone();

        if self.automations.contains_key(&id) {
            return Err(AutomationError::InvalidConfig(format!(
                "Automation with ID {} already exists",
                id
            )));
        }

        info!(
            "Added automation: {} ({})",
            automation.display_name(),
            automation.id
        );
        self.automations.insert(id.clone(), automation);
        Ok(id)
    }

    /// Update last triggered time
    pub fn mark_triggered(&self, id: &str) {
        if let Some(mut automation) = self.automations.get_mut(id) {
            automation.last_triggered = Some(Utc::now());
            debug!(
                "Marked automation {} as triggered",
                automation.display_name()
            );
        }
    }

    /// Increment run count
    pub fn increment_runs(&self, id: &str) {
        if let Some(mut automation) = self.automations.get_mut(id) {
            automation.current_runs += 1;
            debug!(
                "Automation {} runs: {}",
                automation.display_name(),
                automation.current_runs
            );
        }
    }

    /// Decrement run count
    pub fn decrement_runs(&self, id: &str) {
        if let Some(mut automation) = self.automations.get_mut(id) {
            automation.current_runs = automation.current_runs.saturating_sub(1);
            debug!(
                "Automation {} runs: {}",
                automation.display_name(),
                automation.current_runs
            );
        }
    }

    /// Reload automations from configs
    pub fn reload(&self, configs: Vec<AutomationConfig>) -> AutomationResult<()> {
        // Clear existing
        self.automations.clear();

        // Load new
        self.load(configs)?;

        info!("Reloaded {} automations", self.automations.len());
        Ok(())
    }
}

impl Default for AutomationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> AutomationConfig {
        serde_json::from_str(
            r#"{
                "id": "test_automation",
                "alias": "Test Automation",
                "triggers": [
                    {"trigger": "state", "entity_id": "light.test", "to": "on"}
                ],
                "conditions": [
                    {"condition": "time", "after": "08:00:00"}
                ],
                "actions": [
                    {"service": "light.turn_off", "target": {"entity_id": "light.test"}}
                ]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn test_automation_from_config() {
        let config = sample_config();
        let automation = Automation::from_config(config);

        assert_eq!(automation.id, "test_automation");
        assert_eq!(automation.alias, Some("Test Automation".to_string()));
        assert!(automation.enabled);
        assert_eq!(automation.triggers.len(), 1);
        assert_eq!(automation.conditions.len(), 1);
        assert_eq!(automation.actions.len(), 1);
    }

    #[test]
    fn test_automation_manager_load() {
        let manager = AutomationManager::new();
        manager.load(vec![sample_config()]).unwrap();

        assert_eq!(manager.count(), 1);
        assert!(manager.get("test_automation").is_some());
    }

    #[test]
    fn test_automation_enable_disable() {
        let manager = AutomationManager::new();
        manager.load(vec![sample_config()]).unwrap();

        manager.disable("test_automation").unwrap();
        assert!(!manager.get("test_automation").unwrap().enabled);

        manager.enable("test_automation").unwrap();
        assert!(manager.get("test_automation").unwrap().enabled);
    }

    #[test]
    fn test_automation_toggle() {
        let manager = AutomationManager::new();
        manager.load(vec![sample_config()]).unwrap();

        let enabled = manager.toggle("test_automation").unwrap();
        assert!(!enabled);

        let enabled = manager.toggle("test_automation").unwrap();
        assert!(enabled);
    }

    #[test]
    fn test_execution_mode_single() {
        let mut automation = Automation::from_config(sample_config());
        automation.mode = ExecutionMode::Single;

        assert!(automation.can_run());
        automation.current_runs = 1;
        assert!(!automation.can_run());
    }

    #[test]
    fn test_execution_mode_parallel() {
        let mut automation = Automation::from_config(sample_config());
        automation.mode = ExecutionMode::Parallel { max: 3 };

        assert!(automation.can_run());
        automation.current_runs = 2;
        assert!(automation.can_run());
        automation.current_runs = 3;
        assert!(!automation.can_run());
    }

    #[test]
    fn test_execution_mode_restart() {
        let mut automation = Automation::from_config(sample_config());
        automation.mode = ExecutionMode::Restart;

        assert!(automation.can_run());
        automation.current_runs = 5;
        assert!(automation.can_run()); // Restart always allows
    }

    #[test]
    fn test_disabled_automation_cannot_run() {
        let mut automation = Automation::from_config(sample_config());
        automation.enabled = false;

        assert!(!automation.can_run());
    }

    #[test]
    fn test_auto_generated_id() {
        let config: AutomationConfig = serde_json::from_str(
            r#"{
                "alias": "No ID Automation",
                "triggers": [],
                "actions": []
            }"#,
        )
        .unwrap();

        let automation = Automation::from_config(config);
        assert!(!automation.id.is_empty());
        // ULID format check
        assert_eq!(automation.id.len(), 26);
    }

    #[test]
    fn test_run_count_tracking() {
        let manager = AutomationManager::new();
        manager.load(vec![sample_config()]).unwrap();

        manager.increment_runs("test_automation");
        assert_eq!(manager.get("test_automation").unwrap().current_runs, 1);

        manager.increment_runs("test_automation");
        assert_eq!(manager.get("test_automation").unwrap().current_runs, 2);

        manager.decrement_runs("test_automation");
        assert_eq!(manager.get("test_automation").unwrap().current_runs, 1);
    }
}
