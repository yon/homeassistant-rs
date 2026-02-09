//! Config Flow Handler Trait
//!
//! Defines the interface for config flow handlers. The implementation
//! (e.g., Python-based via ha-py-bridge) is provided externally.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Result type for a config flow step
#[derive(Debug, Clone, Serialize)]
pub struct FlowResult {
    /// Flow ID
    pub flow_id: String,
    /// Handler (integration domain)
    pub handler: String,
    /// Result type: form, create_entry, abort, external_step, show_progress
    #[serde(rename = "type")]
    pub result_type: String,
    /// Current step ID (for form type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    /// Data schema for the form (for form type)
    /// Always present, empty array if no schema
    pub data_schema: Vec<FormField>,
    /// Errors from the previous submission (always present, null if none)
    pub errors: Option<HashMap<String, String>>,
    /// Description placeholders for the form (always present, null if none)
    pub description_placeholders: Option<HashMap<String, String>>,
    /// Title (for create_entry type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Abort reason (for abort type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Version (for create_entry)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// Minor version (for create_entry)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minor_version: Option<u32>,
    /// Result data (for create_entry - the config entry data)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Whether this is the last step (controls submit vs next button in frontend)
    pub last_step: Option<bool>,
    /// Preview component to display in frontend
    pub preview: Option<String>,
}

/// Form field schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

/// Errors that can occur during config flow operations
#[derive(Debug, Error)]
pub enum ConfigFlowError {
    /// The requested flow was not found
    #[error("Flow not found: {flow_id}")]
    FlowNotFound { flow_id: String },

    /// The requested handler/integration was not found
    #[error("Handler not found: {handler}")]
    HandlerNotFound { handler: String },

    /// Internal error (Python exceptions, async failures, etc.)
    #[error("{0}")]
    Internal(String),
}

impl ConfigFlowError {
    /// Returns the WebSocket error code for this error variant
    pub fn error_code(&self) -> &str {
        match self {
            ConfigFlowError::FlowNotFound { .. } => "flow_not_found",
            ConfigFlowError::HandlerNotFound { .. } => "handler_not_found",
            ConfigFlowError::Internal(_) => "flow_error",
        }
    }
}

/// Trait for handling configuration flows
///
/// This trait defines the interface for config flow operations.
/// Implementations can use Python (via ha-py-bridge) or native Rust.
#[async_trait]
pub trait ConfigFlowHandler: Send + Sync {
    /// Start a new configuration flow for an integration
    ///
    /// # Arguments
    /// * `handler` - The integration domain (e.g., "hue", "zwave")
    /// * `show_advanced_options` - Whether to show advanced options in the flow
    ///
    /// # Returns
    /// The initial flow result (usually a form to fill out)
    async fn start_flow(
        &self,
        handler: &str,
        show_advanced_options: bool,
    ) -> Result<FlowResult, ConfigFlowError>;

    /// Continue a flow with user input
    ///
    /// # Arguments
    /// * `flow_id` - The ID of the flow to continue
    /// * `user_input` - The user's input for the current step
    ///
    /// # Returns
    /// The next flow result (form, create_entry, or abort)
    async fn progress_flow(
        &self,
        flow_id: &str,
        user_input: Option<serde_json::Value>,
    ) -> Result<FlowResult, ConfigFlowError>;

    /// Get list of active flows
    ///
    /// # Returns
    /// A list of active flow information as JSON values
    async fn list_flows(&self) -> Vec<serde_json::Value>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_flow_error_flow_not_found_display() {
        let err = ConfigFlowError::FlowNotFound {
            flow_id: "abc123".to_string(),
        };
        assert_eq!(err.to_string(), "Flow not found: abc123");
    }

    #[test]
    fn config_flow_error_handler_not_found_display() {
        let err = ConfigFlowError::HandlerNotFound {
            handler: "hue".to_string(),
        };
        assert_eq!(err.to_string(), "Handler not found: hue");
    }

    #[test]
    fn config_flow_error_internal_display() {
        let err = ConfigFlowError::Internal("something broke".to_string());
        assert_eq!(err.to_string(), "something broke");
    }

    #[test]
    fn config_flow_error_codes_match_variants() {
        let flow_err = ConfigFlowError::FlowNotFound {
            flow_id: "x".to_string(),
        };
        assert_eq!(flow_err.error_code(), "flow_not_found");

        let handler_err = ConfigFlowError::HandlerNotFound {
            handler: "y".to_string(),
        };
        assert_eq!(handler_err.error_code(), "handler_not_found");

        let internal_err = ConfigFlowError::Internal("oops".to_string());
        assert_eq!(internal_err.error_code(), "flow_error");
    }

    #[test]
    fn config_flow_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ConfigFlowError>();
    }
}
