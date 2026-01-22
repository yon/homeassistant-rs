//! WebSocket message types
//!
//! Defines all incoming and outgoing WebSocket message types.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

// =============================================================================
// Incoming Messages
// =============================================================================

/// Incoming WebSocket message from client
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IncomingMessage {
    Auth {
        access_token: Option<String>,
        #[serde(default)]
        api_password: Option<String>,
    },
    #[serde(rename = "auth/current_user")]
    AuthCurrentUser {
        id: u64,
    },
    #[serde(rename = "automation/config")]
    AutomationConfig {
        id: u64,
        entity_id: String,
    },
    CallService {
        id: u64,
        domain: String,
        service: String,
        #[serde(default)]
        target: Option<ServiceTarget>,
        #[serde(default)]
        service_data: Option<serde_json::Value>,
        #[serde(default)]
        return_response: bool,
    },
    #[serde(rename = "config/entity_registry/get")]
    EntityRegistryGet {
        id: u64,
        entity_id: String,
    },
    #[serde(rename = "config/entity_registry/list")]
    EntityRegistryList {
        id: u64,
    },
    #[serde(rename = "config/entity_registry/remove")]
    EntityRegistryRemove {
        id: u64,
        entity_id: String,
    },
    #[serde(rename = "config/entity_registry/update")]
    EntityRegistryUpdate {
        id: u64,
        entity_id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        icon: Option<String>,
        #[serde(default)]
        area_id: Option<String>,
        #[serde(default)]
        disabled_by: Option<String>,
        #[serde(default)]
        hidden_by: Option<String>,
        #[serde(default)]
        new_entity_id: Option<String>,
        #[serde(default)]
        aliases: Option<HashSet<String>>,
        #[serde(default)]
        labels: Option<HashSet<String>>,
    },
    #[serde(rename = "config/entity_registry/list_for_display")]
    EntityRegistryListForDisplay {
        id: u64,
    },
    #[serde(rename = "config/device_registry/list")]
    DeviceRegistryList {
        id: u64,
    },
    #[serde(rename = "config/area_registry/list")]
    AreaRegistryList {
        id: u64,
    },
    #[serde(rename = "config/floor_registry/list")]
    FloorRegistryList {
        id: u64,
    },
    #[serde(rename = "config/label_registry/list")]
    LabelRegistryList {
        id: u64,
    },
    FireEvent {
        id: u64,
        event_type: String,
        #[serde(default)]
        event_data: Option<serde_json::Value>,
    },
    #[serde(rename = "frontend/get_themes")]
    FrontendGetThemes {
        id: u64,
    },
    #[serde(rename = "frontend/get_translations")]
    FrontendGetTranslations {
        id: u64,
        #[serde(default)]
        language: Option<String>,
        #[serde(default)]
        category: Option<String>,
        #[serde(default)]
        integration: Option<Vec<String>>,
        #[serde(default)]
        config_flow: Option<bool>,
    },
    #[serde(rename = "frontend/subscribe_user_data")]
    FrontendSubscribeUserData {
        id: u64,
        #[serde(default)]
        key: Option<String>,
    },
    #[serde(rename = "frontend/subscribe_system_data")]
    FrontendSubscribeSystemData {
        id: u64,
        #[serde(default)]
        key: Option<String>,
    },
    GetConfig {
        id: u64,
    },
    GetPanels {
        id: u64,
    },
    GetServices {
        id: u64,
    },
    GetStates {
        id: u64,
    },
    #[serde(rename = "lovelace/config")]
    LovelaceConfig {
        id: u64,
        #[serde(default)]
        url_path: Option<String>,
    },
    #[serde(rename = "lovelace/resources")]
    LovelaceResources {
        id: u64,
    },
    Ping {
        id: u64,
    },
    #[serde(rename = "recorder/info")]
    RecorderInfo {
        id: u64,
    },
    #[serde(rename = "repairs/list_issues")]
    RepairsListIssues {
        id: u64,
    },
    #[serde(rename = "persistent_notification/subscribe")]
    PersistentNotificationSubscribe {
        id: u64,
    },
    #[serde(rename = "labs/subscribe")]
    LabsSubscribe {
        id: u64,
    },
    #[serde(rename = "config_entries/get")]
    ConfigEntriesGet {
        id: u64,
        #[serde(default)]
        entry_id: Option<String>,
        #[serde(default)]
        domain: Option<String>,
    },
    #[serde(rename = "config_entries/subentries/list")]
    ConfigEntriesSubentriesList {
        id: u64,
        entry_id: String,
    },
    #[serde(rename = "config_entries/subscribe")]
    ConfigEntriesSubscribe {
        id: u64,
        #[serde(default)]
        type_filter: Option<Vec<String>>,
    },
    #[serde(rename = "config_entries/flow")]
    ConfigEntriesFlow {
        id: u64,
        /// Handler (integration domain) to start the flow for
        handler: String,
        /// Show advanced options (optional)
        #[serde(default)]
        show_advanced_options: bool,
    },
    #[serde(rename = "config_entries/flow/progress")]
    ConfigEntriesFlowProgress {
        id: u64,
        /// The flow_id to continue (optional - if None, list all flows in progress)
        #[serde(default)]
        flow_id: Option<String>,
        /// User input for this step (optional)
        #[serde(default)]
        user_input: Option<serde_json::Value>,
    },
    #[serde(rename = "config_entries/flow/subscribe")]
    ConfigEntriesFlowSubscribe {
        id: u64,
    },
    #[serde(rename = "config_entries/delete")]
    ConfigEntriesDelete {
        id: u64,
        entry_id: String,
    },
    #[serde(rename = "application_credentials/config")]
    ApplicationCredentialsConfig {
        id: u64,
    },
    #[serde(rename = "application_credentials/config_entry")]
    ApplicationCredentialsConfigEntry {
        id: u64,
        config_entry_id: String,
    },
    #[serde(rename = "application_credentials/list")]
    ApplicationCredentialsList {
        id: u64,
    },
    #[serde(rename = "application_credentials/create")]
    ApplicationCredentialsCreate {
        id: u64,
        domain: String,
        client_id: String,
        client_secret: String,
        #[serde(default)]
        auth_domain: Option<String>,
        #[serde(default)]
        name: Option<String>,
    },
    #[serde(rename = "application_credentials/delete")]
    ApplicationCredentialsDelete {
        id: u64,
        application_credentials_id: String,
    },
    #[serde(rename = "integration/descriptions")]
    IntegrationDescriptions {
        id: u64,
        #[serde(default)]
        integrations: Option<Vec<String>>,
    },
    #[serde(rename = "logger/log_info")]
    LoggerLogInfo {
        id: u64,
    },
    #[serde(rename = "manifest/get")]
    ManifestGet {
        id: u64,
        integration: String,
    },
    #[serde(rename = "manifest/list")]
    ManifestList {
        id: u64,
    },
    #[serde(rename = "entity/source")]
    EntitySource {
        id: u64,
        #[serde(default)]
        entity_id: Option<Vec<String>>,
    },
    #[serde(rename = "config/category_registry/list")]
    CategoryRegistryList {
        id: u64,
        #[serde(default)]
        scope: Option<String>,
    },
    #[serde(rename = "blueprint/list")]
    BlueprintList {
        id: u64,
        domain: String,
    },
    RenderTemplate {
        id: u64,
        template: String,
        #[serde(default)]
        variables: Option<HashMap<String, serde_json::Value>>,
        #[serde(default)]
        #[allow(dead_code)] // Reserved for future use
        timeout: Option<f64>,
        #[serde(default)]
        #[allow(dead_code)] // Reserved for future use
        report_errors: Option<bool>,
    },
    #[serde(rename = "script/config")]
    ScriptConfig {
        id: u64,
        entity_id: String,
    },
    #[serde(rename = "system_log/list")]
    SystemLogList {
        id: u64,
    },
    SubscribeEntities {
        id: u64,
        #[serde(default)]
        entity_ids: Option<Vec<String>>,
    },
    SubscribeEvents {
        id: u64,
        #[serde(default)]
        event_type: Option<String>,
    },
    SupportedFeatures {
        id: u64,
        #[allow(dead_code)] // Deserialized but not currently used
        features: HashMap<String, serde_json::Value>,
    },
    UnsubscribeEvents {
        id: u64,
        subscription: u64,
    },
}

// =============================================================================
// Service Target Types
// =============================================================================

/// Service call target
#[derive(Debug, Deserialize, Default)]
pub struct ServiceTarget {
    #[serde(default)]
    pub entity_id: Option<EntityIds>,
    #[serde(default)]
    pub device_id: Option<Vec<String>>,
    #[serde(default)]
    pub area_id: Option<Vec<String>>,
}

/// Entity IDs can be a single string or array
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EntityIds {
    Single(String),
    Multiple(Vec<String>),
}

impl EntityIds {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            EntityIds::Multiple(v) => v.clone(),
            EntityIds::Single(s) => vec![s.clone()],
        }
    }
}

// =============================================================================
// Outgoing Messages
// =============================================================================

/// Outgoing WebSocket message to client
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum OutgoingMessage {
    AuthRequired(AuthRequiredMessage),
    AuthOk(AuthOkMessage),
    AuthInvalid(AuthInvalidMessage),
    Pong(PongMessage),
    Result(ResultMessage),
    Event(EventMessage),
}

#[derive(Debug, Serialize)]
pub struct AuthRequiredMessage {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub ha_version: String,
}

#[derive(Debug, Serialize)]
pub struct AuthOkMessage {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub ha_version: String,
}

#[derive(Debug, Serialize)]
pub struct AuthInvalidMessage {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct PongMessage {
    pub id: u64,
    #[serde(rename = "type")]
    pub msg_type: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ResultMessage {
    pub id: u64,
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

#[derive(Debug, Serialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct EventMessage {
    pub id: u64,
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub event: serde_json::Value,
}
