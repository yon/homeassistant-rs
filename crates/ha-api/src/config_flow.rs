//! Config Flow Handler Trait
//!
//! Defines the interface for config flow handlers. The implementation
//! (e.g., Python-based via ha-py-bridge) is provided externally.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    ) -> Result<FlowResult, String>;

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
    ) -> Result<FlowResult, String>;

    /// Get list of active flows
    ///
    /// # Returns
    /// A list of active flow information as JSON values
    async fn list_flows(&self) -> Vec<serde_json::Value>;
}
