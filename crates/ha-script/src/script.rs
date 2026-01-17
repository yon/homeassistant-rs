//! Script definition
//!
//! A Script is a named sequence of actions that can be called as a service.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Script execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScriptMode {
    /// Default - ignore new calls while running
    #[default]
    Single,

    /// Restart from beginning on new call
    Restart,

    /// Queue calls (up to max)
    Queued,

    /// Run all simultaneously (up to max)
    Parallel,
}

/// Script configuration from YAML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptConfig {
    /// Script alias (human-readable name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Icon
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Execution mode
    #[serde(default)]
    pub mode: ScriptMode,

    /// Maximum runs (for queued/parallel modes)
    #[serde(default = "default_max")]
    pub max: usize,

    /// Maximum exceeded behavior
    #[serde(default)]
    pub max_exceeded: MaxExceeded,

    /// Input fields (parameters)
    #[serde(default)]
    pub fields: serde_json::Value,

    /// Variables
    #[serde(default)]
    pub variables: serde_json::Value,

    /// Action sequence
    pub sequence: Vec<serde_json::Value>,

    /// Trace configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<TraceConfig>,
}

fn default_max() -> usize {
    10
}

/// What to do when max runs exceeded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MaxExceeded {
    /// Log a warning
    #[default]
    Warning,
    /// Silently ignore
    Silent,
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

/// A loaded script
#[derive(Debug, Clone)]
pub struct Script {
    /// Script ID (entity part, e.g., "turn_on_lights" from "script.turn_on_lights")
    pub id: String,

    /// Human-readable name
    pub alias: Option<String>,

    /// Description
    pub description: Option<String>,

    /// Icon
    pub icon: Option<String>,

    /// Execution mode
    pub mode: ScriptMode,

    /// Maximum runs
    pub max: usize,

    /// Max exceeded behavior
    pub max_exceeded: MaxExceeded,

    /// Input fields
    pub fields: serde_json::Value,

    /// Variables
    pub variables: serde_json::Value,

    /// Action sequence (raw JSON)
    pub sequence: Vec<serde_json::Value>,

    /// Trace config
    pub trace_config: TraceConfig,

    /// Last triggered
    pub last_triggered: Option<DateTime<Utc>>,

    /// Current run count
    pub current_runs: usize,
}

impl Script {
    /// Create from config
    pub fn from_config(id: impl Into<String>, config: ScriptConfig) -> Self {
        Self {
            id: id.into(),
            alias: config.alias,
            description: config.description,
            icon: config.icon,
            mode: config.mode,
            max: config.max,
            max_exceeded: config.max_exceeded,
            fields: config.fields,
            variables: config.variables,
            sequence: config.sequence,
            trace_config: config.trace.unwrap_or(TraceConfig {
                stored_traces: default_stored_traces(),
            }),
            last_triggered: None,
            current_runs: 0,
        }
    }

    /// Get display name
    pub fn display_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.id)
    }

    /// Get full entity ID
    pub fn entity_id(&self) -> String {
        format!("script.{}", self.id)
    }

    /// Check if can start new run
    pub fn can_run(&self) -> bool {
        match self.mode {
            ScriptMode::Single => self.current_runs == 0,
            ScriptMode::Restart => true,
            ScriptMode::Queued | ScriptMode::Parallel => self.current_runs < self.max,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> ScriptConfig {
        serde_json::from_str(
            r#"{
                "alias": "Turn On Lights",
                "description": "Turns on all lights",
                "mode": "single",
                "sequence": [
                    {"service": "light.turn_on", "target": {"entity_id": ["light.living_room"]}}
                ]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn test_script_from_config() {
        let config = sample_config();
        let script = Script::from_config("turn_on_lights", config);

        assert_eq!(script.id, "turn_on_lights");
        assert_eq!(script.alias, Some("Turn On Lights".to_string()));
        assert_eq!(script.mode, ScriptMode::Single);
        assert_eq!(script.entity_id(), "script.turn_on_lights");
    }

    #[test]
    fn test_script_can_run() {
        let config = sample_config();
        let mut script = Script::from_config("test", config);

        // Single mode
        assert!(script.can_run());
        script.current_runs = 1;
        assert!(!script.can_run());

        // Restart mode always allows
        script.mode = ScriptMode::Restart;
        assert!(script.can_run());

        // Parallel mode respects max
        script.mode = ScriptMode::Parallel;
        script.max = 3;
        script.current_runs = 2;
        assert!(script.can_run());
        script.current_runs = 3;
        assert!(!script.can_run());
    }

    #[test]
    fn test_script_modes() {
        let json = r#"{"mode": "queued", "sequence": []}"#;
        let config: ScriptConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.mode, ScriptMode::Queued);

        let json = r#"{"mode": "parallel", "sequence": []}"#;
        let config: ScriptConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.mode, ScriptMode::Parallel);
    }

    #[test]
    fn test_script_with_fields() {
        let json = r#"{
            "alias": "Set Brightness",
            "fields": {
                "brightness": {
                    "description": "Brightness level",
                    "example": 255
                }
            },
            "sequence": [
                {"service": "light.turn_on", "data": {"brightness": "{{ brightness }}"}}
            ]
        }"#;

        let config: ScriptConfig = serde_json::from_str(json).unwrap();
        assert!(!config.fields.is_null());
    }
}
