//! Home Assistant Rust Server
//!
//! Main entry point for the Home Assistant Rust implementation.

mod automation_engine;

use anyhow::Result;
use ha_api::{
    auth::AuthState, config_flow::ConfigFlowHandler, frontend::FrontendConfig,
    persistent_notification, AppState,
};
use ha_automation::AutomationConfig;
use ha_components::{register_system_log_services, SystemLog};
use ha_config::CoreConfig;
use ha_config_entries::ConfigEntries;
#[cfg(feature = "python")]
use ha_config_entries::ConfigEntryState;
use ha_core::{Context, EntityId, ServiceCall, SupportsResponse};
use ha_event_bus::EventBus;
use ha_registries::{Registries, Storage};
use ha_service_registry::{ServiceDescription, ServiceRegistry};
use ha_state_machine::StateMachine;
use ha_template::TemplateEngine;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

#[cfg(feature = "python")]
use ha_py_bridge::py_bridge::{
    call_python_entity_service, get_python_entities, load_allowlist_from_config, ConfigFlowManager,
    PyBridge,
};

/// The central Home Assistant instance
pub struct HomeAssistant {
    /// Automation engine for trigger→condition→action flow
    pub automation_engine: automation_engine::AutomationEngine,
    /// Event bus for pub/sub communication
    pub bus: Arc<EventBus>,
    /// Config entries manager
    pub config_entries: Arc<RwLock<ConfigEntries>>,
    /// Registries for entities, devices, areas, etc.
    pub registries: Arc<Registries>,
    /// Service registry for service calls
    pub services: Arc<ServiceRegistry>,
    /// State machine for entity states
    pub states: Arc<StateMachine>,
    /// Template engine for rendering templates
    pub template_engine: Arc<TemplateEngine>,
    /// Python bridge for running Python integrations
    #[cfg(feature = "python")]
    pub python_bridge: Option<PyBridge>,
}

impl HomeAssistant {
    /// Create a new Home Assistant instance
    ///
    /// # Arguments
    /// * `config_dir` - Path to the Home Assistant config directory
    /// * `registries` - Registries for entities, devices, areas, etc.
    pub fn new(config_dir: &Path, registries: Arc<Registries>) -> Self {
        let bus = Arc::new(EventBus::new());
        let states = Arc::new(StateMachine::new(bus.clone()));
        let services = Arc::new(ServiceRegistry::new());

        // Create template engine and load custom templates before wrapping in Arc
        let mut template_engine = TemplateEngine::new(states.clone());
        match template_engine.load_custom_templates(config_dir) {
            Ok(count) if count > 0 => {
                info!("Loaded {} custom templates", count);
            }
            Ok(_) => {}
            Err(e) => {
                warn!("Failed to load custom templates: {}", e);
            }
        }
        let template_engine = Arc::new(template_engine);

        let automation_engine = automation_engine::AutomationEngine::new(
            bus.clone(),
            states.clone(),
            services.clone(),
            template_engine.clone(),
        );

        // Create config entries manager with storage
        let storage = Arc::new(Storage::new(config_dir));
        let config_entries = Arc::new(RwLock::new(ConfigEntries::new(storage)));

        // Initialize Python bridge if feature is enabled
        // Use HA_PYTHON_PATH env var to point to a pip-installed Home Assistant
        #[cfg(feature = "python")]
        let python_bridge = match {
            let ha_python_path = std::env::var("HA_PYTHON_PATH").map(PathBuf::from).ok();
            PyBridge::new(
                ha_python_path.as_deref(),
                registries.clone(),
                Some(config_dir.to_path_buf()),
            )
        } {
            Ok(bridge) => {
                match bridge.python_version() {
                    Ok(version) => info!("Python bridge initialized: Python {}", version),
                    Err(_) => info!("Python bridge initialized"),
                }

                // Load Python integration allowlist from config
                let allowlist = load_allowlist_from_config(config_dir);
                bridge.set_allowlist(allowlist);

                Some(bridge)
            }
            Err(e) => {
                warn!(
                    "Python bridge not available: {}. Running in Rust-only mode.",
                    e
                );
                None
            }
        };

        Self {
            automation_engine,
            bus,
            config_entries,
            registries,
            services,
            states,
            template_engine,
            #[cfg(feature = "python")]
            python_bridge,
        }
    }

    /// Register core services
    fn register_core_services(&self) {
        let states = self.states.clone();

        // Helper to create entity target spec
        let entity_target = || Some(json!({}));

        // Register homeassistant.turn_on service
        let states_clone = states.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "turn_on".to_string(),
                name: None,
                description: None,
                schema: None,
                target: entity_target(),
                supports_response: SupportsResponse::None,
            },
            move |call: ServiceCall| {
                let states = states_clone.clone();
                async move {
                    if let Some(entity_id) =
                        call.service_data.get("entity_id").and_then(|v| v.as_str())
                    {
                        let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
                        if parts.len() == 2 {
                            if let Ok(entity) = EntityId::new(parts[0], parts[1]) {
                                states.set(entity, "on", HashMap::new(), Context::new());
                            }
                        }
                    }
                    Ok(None)
                }
            },
        );

        // Register homeassistant.turn_off service
        let states_clone = states.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "turn_off".to_string(),
                name: None,
                description: None,
                schema: None,
                target: entity_target(),
                supports_response: SupportsResponse::None,
            },
            move |call: ServiceCall| {
                let states = states_clone.clone();
                async move {
                    if let Some(entity_id) =
                        call.service_data.get("entity_id").and_then(|v| v.as_str())
                    {
                        let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
                        if parts.len() == 2 {
                            if let Ok(entity) = EntityId::new(parts[0], parts[1]) {
                                states.set(entity, "off", HashMap::new(), Context::new());
                            }
                        }
                    }
                    Ok(None)
                }
            },
        );

        // Register homeassistant.toggle service
        let states_clone = states.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "toggle".to_string(),
                name: None,
                description: None,
                schema: None,
                target: entity_target(),
                supports_response: SupportsResponse::None,
            },
            move |call: ServiceCall| {
                let states = states_clone.clone();
                async move {
                    if let Some(entity_id) =
                        call.service_data.get("entity_id").and_then(|v| v.as_str())
                    {
                        if let Some(state) = states.get(entity_id) {
                            let new_state = if state.state == "on" { "off" } else { "on" };
                            let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
                            if parts.len() == 2 {
                                if let Ok(entity) = EntityId::new(parts[0], parts[1]) {
                                    states.set(
                                        entity,
                                        new_state,
                                        state.attributes.clone(),
                                        Context::new(),
                                    );
                                }
                            }
                        }
                    }
                    Ok(None)
                }
            },
        );

        // Register homeassistant.update_entity service
        let states_clone = states.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "update_entity".to_string(),
                name: None,
                description: None,
                schema: None,
                target: None,
                supports_response: SupportsResponse::None,
            },
            move |call: ServiceCall| {
                let _states = states_clone.clone();
                async move {
                    // In a real implementation, this would request entities to update
                    info!("update_entity called: {:?}", call.service_data);
                    Ok(None)
                }
            },
        );

        // Register homeassistant.check_config service
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "check_config".to_string(),
                name: None,
                description: None,
                schema: None,
                target: None,
                supports_response: SupportsResponse::None,
            },
            |_call: ServiceCall| async move {
                // Return success - config check passed
                Ok(None)
            },
        );

        // Register homeassistant.reload_core_config service
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "reload_core_config".to_string(),
                name: None,
                description: None,
                schema: None,
                target: None,
                supports_response: SupportsResponse::None,
            },
            |_call: ServiceCall| async move {
                info!("Reloading core config");
                Ok(None)
            },
        );

        // Register homeassistant.restart service
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "restart".to_string(),
                name: None,
                description: None,
                schema: None,
                target: None,
                supports_response: SupportsResponse::None,
            },
            |_call: ServiceCall| async move {
                info!("Restart requested (not implemented in test mode)");
                Ok(None)
            },
        );

        // Register homeassistant.stop service
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "stop".to_string(),
                name: None,
                description: None,
                schema: None,
                target: None,
                supports_response: SupportsResponse::None,
            },
            |_call: ServiceCall| async move {
                info!("Stop requested (not implemented in test mode)");
                Ok(None)
            },
        );

        // Register homeassistant.reload_all service
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "reload_all".to_string(),
                name: None,
                description: None,
                schema: None,
                target: None,
                supports_response: SupportsResponse::None,
            },
            |_call: ServiceCall| async move {
                info!("Reload all requested");
                Ok(None)
            },
        );

        // Register homeassistant.reload_config_entry service
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "reload_config_entry".to_string(),
                name: None,
                description: None,
                schema: None,
                target: entity_target(),
                supports_response: SupportsResponse::None,
            },
            |_call: ServiceCall| async move {
                info!("Reload config entry requested");
                Ok(None)
            },
        );

        // Register homeassistant.reload_custom_templates service
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "reload_custom_templates".to_string(),
                name: None,
                description: None,
                schema: None,
                target: None,
                supports_response: SupportsResponse::None,
            },
            |_call: ServiceCall| async move {
                info!("Reload custom templates requested");
                Ok(None)
            },
        );

        // Register homeassistant.save_persistent_states service
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "save_persistent_states".to_string(),
                name: None,
                description: None,
                schema: None,
                target: None,
                supports_response: SupportsResponse::None,
            },
            |_call: ServiceCall| async move {
                info!("Save persistent states requested");
                Ok(None)
            },
        );

        // Register homeassistant.set_location service
        self.services.register_with_description(
            ServiceDescription {
                domain: "homeassistant".to_string(),
                service: "set_location".to_string(),
                name: None,
                description: None,
                schema: None,
                target: None,
                supports_response: SupportsResponse::None,
            },
            |_call: ServiceCall| async move {
                info!("Set location requested");
                Ok(None)
            },
        );

        info!("Core services registered");
    }

    /// Register automation domain services
    fn register_automation_services(&self) {
        let states = self.states.clone();
        let manager = self.automation_engine.manager();

        // Helper for automation entity target
        let automation_target = || {
            Some(json!({
                "entity": {
                    "domain": "automation"
                }
            }))
        };

        // Register automation.turn_on service - enable automation
        let states_clone = states.clone();
        let manager_clone = manager.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "automation".to_string(),
                service: "turn_on".to_string(),
                name: None,
                description: None,
                schema: None,
                target: automation_target(),
                supports_response: SupportsResponse::None,
            },
            move |call: ServiceCall| {
                let states = states_clone.clone();
                let manager = manager_clone.clone();
                async move {
                    if let Some(entity_id) =
                        call.service_data.get("entity_id").and_then(|v| v.as_str())
                    {
                        let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
                        if parts.len() == 2 && parts[0] == "automation" {
                            let automation_id = parts[1];

                            // Enable in automation manager
                            let manager_guard = manager.read().await;
                            if manager_guard.get(automation_id).is_some() {
                                drop(manager_guard);
                                let manager_guard = manager.write().await;
                                let _ = manager_guard.enable(automation_id);
                            }

                            // Update entity state
                            if let Some(state) = states.get(entity_id) {
                                if let Ok(entity) = EntityId::new(parts[0], parts[1]) {
                                    states.set(
                                        entity,
                                        "on",
                                        state.attributes.clone(),
                                        Context::new(),
                                    );
                                }
                            }
                        }
                    }
                    Ok(None)
                }
            },
        );

        // Register automation.turn_off service - disable automation
        let states_clone = states.clone();
        let manager_clone = manager.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "automation".to_string(),
                service: "turn_off".to_string(),
                name: None,
                description: None,
                schema: None,
                target: automation_target(),
                supports_response: SupportsResponse::None,
            },
            move |call: ServiceCall| {
                let states = states_clone.clone();
                let manager = manager_clone.clone();
                async move {
                    if let Some(entity_id) =
                        call.service_data.get("entity_id").and_then(|v| v.as_str())
                    {
                        let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
                        if parts.len() == 2 && parts[0] == "automation" {
                            let automation_id = parts[1];

                            // Disable in automation manager
                            let manager_guard = manager.read().await;
                            if manager_guard.get(automation_id).is_some() {
                                drop(manager_guard);
                                let manager_guard = manager.write().await;
                                let _ = manager_guard.disable(automation_id);
                            }

                            // Update entity state
                            if let Some(state) = states.get(entity_id) {
                                if let Ok(entity) = EntityId::new(parts[0], parts[1]) {
                                    states.set(
                                        entity,
                                        "off",
                                        state.attributes.clone(),
                                        Context::new(),
                                    );
                                }
                            }
                        }
                    }
                    Ok(None)
                }
            },
        );

        // Register automation.toggle service
        let states_clone = states.clone();
        let manager_clone = manager.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "automation".to_string(),
                service: "toggle".to_string(),
                name: None,
                description: None,
                schema: None,
                target: automation_target(),
                supports_response: SupportsResponse::None,
            },
            move |call: ServiceCall| {
                let states = states_clone.clone();
                let manager = manager_clone.clone();
                async move {
                    if let Some(entity_id) =
                        call.service_data.get("entity_id").and_then(|v| v.as_str())
                    {
                        let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
                        if parts.len() == 2 && parts[0] == "automation" {
                            let automation_id = parts[1];

                            // Toggle in automation manager and get new state
                            let manager_guard = manager.read().await;
                            let new_enabled = if manager_guard.get(automation_id).is_some() {
                                drop(manager_guard);
                                let manager_guard = manager.write().await;
                                manager_guard.toggle(automation_id).unwrap_or(true)
                            } else {
                                // Automation not in manager, toggle based on entity state
                                if let Some(state) = states.get(entity_id) {
                                    state.state != "on"
                                } else {
                                    true
                                }
                            };

                            // Update entity state
                            if let Some(state) = states.get(entity_id) {
                                let new_state = if new_enabled { "on" } else { "off" };
                                if let Ok(entity) = EntityId::new(parts[0], parts[1]) {
                                    states.set(
                                        entity,
                                        new_state,
                                        state.attributes.clone(),
                                        Context::new(),
                                    );
                                }
                            }
                        }
                    }
                    Ok(None)
                }
            },
        );

        // Register automation.trigger service - manually trigger an automation
        let manager_clone = manager.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "automation".to_string(),
                service: "trigger".to_string(),
                name: None,
                description: None,
                schema: None,
                target: automation_target(),
                supports_response: SupportsResponse::None,
            },
            move |call: ServiceCall| {
                let manager = manager_clone.clone();
                async move {
                    if let Some(entity_id) =
                        call.service_data.get("entity_id").and_then(|v| v.as_str())
                    {
                        let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
                        if parts.len() == 2 && parts[0] == "automation" {
                            let automation_id = parts[1];
                            let manager_guard = manager.read().await;
                            if let Some(automation) = manager_guard.get(automation_id) {
                                info!(
                                    "Triggering automation: {} ({})",
                                    automation.display_name(),
                                    automation_id
                                );
                                // Note: Full trigger would require access to the AutomationEngine
                                // For now, log the trigger request
                            } else {
                                warn!("Automation not found: {}", automation_id);
                            }
                        }
                    }
                    Ok(None)
                }
            },
        );

        // Register automation.reload service
        self.services.register_with_description(
            ServiceDescription {
                domain: "automation".to_string(),
                service: "reload".to_string(),
                name: None,
                description: None,
                schema: None,
                target: None,
                supports_response: SupportsResponse::None,
            },
            |_call: ServiceCall| async move {
                // Note: Full reload would require reloading from config
                // For now, just log the request
                info!("Reloading automations (not fully implemented)");
                Ok(None)
            },
        );

        info!("Automation services registered");
    }

    /// Register script domain services
    fn register_script_services(&self) {
        let states = self.states.clone();

        // Helper for script entity target
        let script_target = || {
            Some(json!({
                "entity": {
                    "domain": "script"
                }
            }))
        };

        // Register script.turn_on service - run the script
        let states_clone = states.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "script".to_string(),
                service: "turn_on".to_string(),
                name: None,
                description: None,
                schema: None,
                target: script_target(),
                supports_response: SupportsResponse::None,
            },
            move |call: ServiceCall| {
                let states = states_clone.clone();
                async move {
                    if let Some(entity_id) =
                        call.service_data.get("entity_id").and_then(|v| v.as_str())
                    {
                        if let Some(state) = states.get(entity_id) {
                            let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
                            if parts.len() == 2 && parts[0] == "script" {
                                if let Ok(entity) = EntityId::new(parts[0], parts[1]) {
                                    // Set to "on" while running (scripts run once then go to off)
                                    states.set(
                                        entity,
                                        "on",
                                        state.attributes.clone(),
                                        Context::new(),
                                    );
                                }
                            }
                        }
                    }
                    Ok(None)
                }
            },
        );

        // Register script.turn_off service - stop the script
        let states_clone = states.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "script".to_string(),
                service: "turn_off".to_string(),
                name: None,
                description: None,
                schema: None,
                target: script_target(),
                supports_response: SupportsResponse::None,
            },
            move |call: ServiceCall| {
                let states = states_clone.clone();
                async move {
                    if let Some(entity_id) =
                        call.service_data.get("entity_id").and_then(|v| v.as_str())
                    {
                        if let Some(state) = states.get(entity_id) {
                            let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
                            if parts.len() == 2 && parts[0] == "script" {
                                if let Ok(entity) = EntityId::new(parts[0], parts[1]) {
                                    states.set(
                                        entity,
                                        "off",
                                        state.attributes.clone(),
                                        Context::new(),
                                    );
                                }
                            }
                        }
                    }
                    Ok(None)
                }
            },
        );

        // Register script.toggle service
        let states_clone = states.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "script".to_string(),
                service: "toggle".to_string(),
                name: None,
                description: None,
                schema: None,
                target: script_target(),
                supports_response: SupportsResponse::None,
            },
            move |call: ServiceCall| {
                let states = states_clone.clone();
                async move {
                    if let Some(entity_id) =
                        call.service_data.get("entity_id").and_then(|v| v.as_str())
                    {
                        if let Some(state) = states.get(entity_id) {
                            let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
                            if parts.len() == 2 && parts[0] == "script" {
                                let new_state = if state.state == "on" { "off" } else { "on" };
                                if let Ok(entity) = EntityId::new(parts[0], parts[1]) {
                                    states.set(
                                        entity,
                                        new_state,
                                        state.attributes.clone(),
                                        Context::new(),
                                    );
                                }
                            }
                        }
                    }
                    Ok(None)
                }
            },
        );

        // Register script.reload service
        self.services.register_with_description(
            ServiceDescription {
                domain: "script".to_string(),
                service: "reload".to_string(),
                name: None,
                description: None,
                schema: None,
                target: None,
                supports_response: SupportsResponse::None,
            },
            |_call: ServiceCall| async move {
                info!("Reloading scripts");
                Ok(None)
            },
        );

        info!("Script services registered");
    }

    /// Load entities from JSON file or add hardcoded demo entities
    fn load_entities(&self, config_dir: &std::path::Path) {
        let entities_file = config_dir.join("demo-entities.json");

        if entities_file.exists() {
            match self.load_entities_from_file(&entities_file) {
                Ok(count) => {
                    info!("Loaded {} entities from {:?}", count, entities_file);
                    return;
                }
                Err(e) => {
                    warn!(
                        "Failed to load entities from {:?}: {}. Using defaults.",
                        entities_file, e
                    );
                }
            }
        }

        // Fallback to hardcoded demo entities
        self.add_hardcoded_demo_entities();
    }

    /// Load entities from a JSON file exported from Python HA
    fn load_entities_from_file(&self, path: &std::path::Path) -> Result<usize> {
        let content = std::fs::read_to_string(path)?;
        let entities: Vec<serde_json::Value> = serde_json::from_str(&content)?;

        let mut count = 0;
        for entity in entities {
            let entity_id_str = entity
                .get("entity_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing entity_id"))?;

            let state = entity
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let attributes: HashMap<String, serde_json::Value> = entity
                .get("attributes")
                .and_then(|v| v.as_object())
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();

            // Parse entity_id into domain.object_id
            let parts: Vec<&str> = entity_id_str.splitn(2, '.').collect();
            if parts.len() == 2 {
                if let Ok(entity_id) = EntityId::new(parts[0], parts[1]) {
                    self.states
                        .set(entity_id, state, attributes, Context::new());
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Add hardcoded demo entities (fallback)
    fn add_hardcoded_demo_entities(&self) {
        // Add some demo lights
        self.states.set(
            EntityId::new("light", "living_room").unwrap(),
            "on",
            HashMap::from([
                ("brightness".to_string(), serde_json::json!(255)),
                (
                    "friendly_name".to_string(),
                    serde_json::json!("Living Room Light"),
                ),
            ]),
            Context::new(),
        );

        self.states.set(
            EntityId::new("light", "bedroom").unwrap(),
            "off",
            HashMap::from([
                ("brightness".to_string(), serde_json::json!(0)),
                (
                    "friendly_name".to_string(),
                    serde_json::json!("Bedroom Light"),
                ),
            ]),
            Context::new(),
        );

        // Add some sensors
        self.states.set(
            EntityId::new("sensor", "temperature").unwrap(),
            "22.5",
            HashMap::from([
                ("unit_of_measurement".to_string(), serde_json::json!("°C")),
                (
                    "friendly_name".to_string(),
                    serde_json::json!("Temperature"),
                ),
                ("device_class".to_string(), serde_json::json!("temperature")),
            ]),
            Context::new(),
        );

        self.states.set(
            EntityId::new("sensor", "humidity").unwrap(),
            "45",
            HashMap::from([
                ("unit_of_measurement".to_string(), serde_json::json!("%")),
                ("friendly_name".to_string(), serde_json::json!("Humidity")),
                ("device_class".to_string(), serde_json::json!("humidity")),
            ]),
            Context::new(),
        );

        // Add a switch
        self.states.set(
            EntityId::new("switch", "coffee_maker").unwrap(),
            "off",
            HashMap::from([(
                "friendly_name".to_string(),
                serde_json::json!("Coffee Maker"),
            )]),
            Context::new(),
        );

        // Add a binary sensor
        self.states.set(
            EntityId::new("binary_sensor", "front_door").unwrap(),
            "off",
            HashMap::from([
                ("friendly_name".to_string(), serde_json::json!("Front Door")),
                ("device_class".to_string(), serde_json::json!("door")),
            ]),
            Context::new(),
        );

        info!("Demo entities added");
    }
}

// Note: HomeAssistant no longer implements Default since new() requires config_dir

/// Register persistent_notification services
fn register_persistent_notification_services(
    services: &ServiceRegistry,
    notifications: Arc<persistent_notification::PersistentNotificationManager>,
) {
    const DOMAIN: &str = persistent_notification::DOMAIN;

    // Register persistent_notification.create service
    let notifications_clone = notifications.clone();
    services.register_with_description(
        ServiceDescription {
            domain: DOMAIN.to_string(),
            service: "create".to_string(),
            name: Some("Create notification".to_string()),
            description: Some("Create a persistent notification".to_string()),
            schema: Some(json!({
                "message": {"required": true, "selector": {"text": {}}},
                "title": {"required": false, "selector": {"text": {}}},
                "notification_id": {"required": false, "selector": {"text": {}}}
            })),
            target: None,
            supports_response: SupportsResponse::None,
        },
        move |call: ServiceCall| {
            let notifications = notifications_clone.clone();
            async move {
                let message = call
                    .service_data
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let title = call
                    .service_data
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let notification_id = call
                    .service_data
                    .get("notification_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| ulid::Ulid::new().to_string().to_lowercase());

                notifications.create(notification_id, message, title);
                Ok(None)
            }
        },
    );

    // Register persistent_notification.dismiss service
    let notifications_clone = notifications.clone();
    services.register_with_description(
        ServiceDescription {
            domain: DOMAIN.to_string(),
            service: "dismiss".to_string(),
            name: Some("Dismiss notification".to_string()),
            description: Some("Dismiss a persistent notification".to_string()),
            schema: Some(json!({
                "notification_id": {"required": true, "selector": {"text": {}}}
            })),
            target: None,
            supports_response: SupportsResponse::None,
        },
        move |call: ServiceCall| {
            let notifications = notifications_clone.clone();
            async move {
                if let Some(notification_id) = call
                    .service_data
                    .get("notification_id")
                    .and_then(|v| v.as_str())
                {
                    notifications.dismiss(notification_id);
                }
                Ok(None)
            }
        },
    );

    // Register persistent_notification.dismiss_all service
    let notifications_clone = notifications.clone();
    services.register_with_description(
        ServiceDescription {
            domain: DOMAIN.to_string(),
            service: "dismiss_all".to_string(),
            name: Some("Dismiss all notifications".to_string()),
            description: Some("Dismiss all persistent notifications".to_string()),
            schema: None,
            target: None,
            supports_response: SupportsResponse::None,
        },
        move |_call: ServiceCall| {
            let notifications = notifications_clone.clone();
            async move {
                notifications.dismiss_all();
                Ok(None)
            }
        },
    );

    info!("Persistent notification services registered");
}

/// Load components list from JSON file or use defaults
fn load_components(config_dir: &std::path::Path) -> Vec<String> {
    let components_file = config_dir.join("components.json");

    if components_file.exists() {
        match std::fs::read_to_string(&components_file) {
            Ok(content) => match serde_json::from_str::<Vec<String>>(&content) {
                Ok(components) => return components,
                Err(e) => {
                    warn!("Failed to parse components.json: {}", e);
                }
            },
            Err(e) => {
                warn!("Failed to read components.json: {}", e);
            }
        }
    }

    // Default components
    vec![
        "api".to_string(),
        "automation".to_string(),
        "homeassistant".to_string(),
        "input_boolean".to_string(),
        "input_number".to_string(),
        "persistent_notification".to_string(),
        "scene".to_string(),
        "script".to_string(),
    ]
}

/// Load services cache from JSON file (for comparison testing)
fn load_services_cache(config_dir: &std::path::Path) -> Option<Arc<serde_json::Value>> {
    let services_file = config_dir.join("services.json");

    if services_file.exists() {
        match std::fs::read_to_string(&services_file) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(services) => return Some(Arc::new(services)),
                Err(e) => {
                    warn!("Failed to parse services.json: {}", e);
                }
            },
            Err(e) => {
                warn!("Failed to read services.json: {}", e);
            }
        }
    }

    None
}

/// Load events cache from JSON file (for comparison testing)
fn load_events_cache(config_dir: &std::path::Path) -> Option<Arc<serde_json::Value>> {
    let events_file = config_dir.join("events.json");

    if events_file.exists() {
        match std::fs::read_to_string(&events_file) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(events) => return Some(Arc::new(events)),
                Err(e) => {
                    warn!("Failed to parse events.json: {}", e);
                }
            },
            Err(e) => {
                warn!("Failed to read events.json: {}", e);
            }
        }
    }

    None
}

/// Load automations from configuration.yaml
fn load_automations(config_dir: &Path) -> Vec<AutomationConfig> {
    let config_file = config_dir.join("configuration.yaml");

    if !config_file.exists() {
        debug!("No configuration.yaml found, no automations to load");
        return Vec::new();
    }

    // Load the full YAML with includes resolved
    let yaml = match ha_config::load_yaml(config_dir, "configuration.yaml") {
        Ok(yaml) => yaml,
        Err(e) => {
            warn!("Failed to load configuration.yaml: {}", e);
            return Vec::new();
        }
    };

    // Extract the automation key
    let automation_value = match yaml.get("automation") {
        Some(v) => v.clone(),
        None => {
            debug!("No 'automation' key in configuration.yaml");
            return Vec::new();
        }
    };

    // Handle both single automation and list of automations
    let automations_array = if automation_value.is_sequence() {
        automation_value
    } else if automation_value.is_mapping() {
        // Single automation, wrap in array
        serde_yaml::Value::Sequence(vec![automation_value])
    } else {
        debug!("automation key is not a mapping or sequence");
        return Vec::new();
    };

    // Deserialize to Vec<AutomationConfig>
    match serde_yaml::from_value::<Vec<AutomationConfig>>(automations_array) {
        Ok(configs) => {
            info!("Loaded {} automation(s) from configuration", configs.len());
            configs
        }
        Err(e) => {
            warn!("Failed to parse automations: {}", e);
            Vec::new()
        }
    }
}

/// Load input helpers (input_boolean, input_number) from configuration
fn load_input_helpers(config_dir: &Path, states: &StateMachine) {
    let config_file = config_dir.join("configuration.yaml");

    if !config_file.exists() {
        debug!("No configuration.yaml found, no input helpers to load");
        return;
    }

    // Load the full YAML with includes resolved
    let yaml = match ha_config::load_yaml(config_dir, "configuration.yaml") {
        Ok(yaml) => yaml,
        Err(e) => {
            warn!("Failed to load configuration.yaml for input helpers: {}", e);
            return;
        }
    };

    // Collect all input_boolean configs (from root and packages)
    let mut all_input_booleans: HashMap<String, Option<ha_components::InputBooleanConfig>> =
        HashMap::new();
    let mut all_input_numbers: HashMap<String, ha_components::InputNumberConfig> = HashMap::new();

    // Load from root level
    if let Some(input_boolean_value) = yaml.get("input_boolean") {
        if let Ok(configs) = serde_yaml::from_value::<
            HashMap<String, Option<ha_components::InputBooleanConfig>>,
        >(input_boolean_value.clone())
        {
            all_input_booleans.extend(configs);
        }
    }

    if let Some(input_number_value) = yaml.get("input_number") {
        if let Ok(configs) = serde_yaml::from_value::<
            HashMap<String, ha_components::InputNumberConfig>,
        >(input_number_value.clone())
        {
            all_input_numbers.extend(configs);
        }
    }

    // Load from packages (homeassistant.packages contains merged package content)
    if let Some(homeassistant) = yaml.get("homeassistant") {
        if let Some(packages) = homeassistant.get("packages") {
            if let Some(packages_map) = packages.as_mapping() {
                for (_, package_content) in packages_map {
                    // Each package can have input_boolean and input_number sections
                    if let Some(input_boolean_value) = package_content.get("input_boolean") {
                        if let Ok(configs) = serde_yaml::from_value::<
                            HashMap<String, Option<ha_components::InputBooleanConfig>>,
                        >(input_boolean_value.clone())
                        {
                            all_input_booleans.extend(configs);
                        }
                    }

                    if let Some(input_number_value) = package_content.get("input_number") {
                        if let Ok(configs) = serde_yaml::from_value::<
                            HashMap<String, ha_components::InputNumberConfig>,
                        >(input_number_value.clone())
                        {
                            all_input_numbers.extend(configs);
                        }
                    }
                }
            }
        }
    }

    // Load the collected configs
    if !all_input_booleans.is_empty() {
        ha_components::load_input_booleans(&all_input_booleans, states);
    }

    if !all_input_numbers.is_empty() {
        ha_components::load_input_numbers(&all_input_numbers, states);
    }
}

/// Load and setup config entries
///
/// Loads config entries from storage and sets up each one.
/// If Python bridge is available, calls the integration's async_setup_entry.
#[cfg(feature = "python")]
async fn setup_config_entries(hass: &HomeAssistant) {
    // Load config entries from storage
    {
        let manager = hass.config_entries.write().await;
        if let Err(e) = manager.load().await {
            warn!("Failed to load config entries: {}", e);
            return;
        }
    }

    // Get entries to setup
    let entries: Vec<_> = {
        let manager = hass.config_entries.read().await;
        manager.iter().collect()
    };

    if entries.is_empty() {
        debug!("No config entries to setup");
        return;
    }

    info!("Setting up {} config entries", entries.len());

    // Setup each entry using the Python bridge
    if let Some(ref bridge) = hass.python_bridge {
        for entry in entries {
            let domain = entry.domain.clone();
            let entry_id = entry.entry_id.clone();

            // Skip already loaded entries
            if entry.state == ConfigEntryState::Loaded {
                debug!("Config entry {} already loaded", entry_id);
                continue;
            }

            // Set state to SetupInProgress
            {
                let manager = hass.config_entries.read().await;
                manager.set_state(&entry_id, ConfigEntryState::SetupInProgress, None);
            }

            // Call setup_config_entry via the bridge (handles all Python work internally)
            let result = bridge.setup_config_entry(
                &entry,
                hass.bus.clone(),
                hass.states.clone(),
                hass.services.clone(),
            );

            // Update state based on result
            let manager = hass.config_entries.read().await;
            match result {
                Ok(true) => {
                    info!("Setup config entry: {} ({})", domain, entry_id);
                    manager.set_state(&entry_id, ConfigEntryState::Loaded, None);
                }
                Ok(false) => {
                    debug!("Integration {} doesn't support config entries", domain);
                    manager.set_state(&entry_id, ConfigEntryState::NotLoaded, None);
                }
                Err(e) => {
                    warn!("Failed to setup {}: {}", domain, e);
                    manager.set_state(&entry_id, ConfigEntryState::SetupError, Some(e.to_string()));
                }
            }
        }
    } else {
        debug!("Python bridge not available, skipping config entry setup");
    }
}

#[cfg(not(feature = "python"))]
async fn setup_config_entries(hass: &HomeAssistant) {
    // Load config entries from storage (but don't setup since no Python)
    {
        let manager = hass.config_entries.write().await;
        if let Err(e) = manager.load().await {
            warn!("Failed to load config entries: {}", e);
            return;
        }
    }

    let count = {
        let manager = hass.config_entries.read().await;
        manager.len()
    };

    if count > 0 {
        info!(
            "Loaded {} config entries (Python bridge not available for setup)",
            count
        );
    }
}

/// Register entity domain services for Python entities
///
/// After Python integrations load entities, we need to register services like
/// `light.turn_on`, `light.turn_off` etc. in the Rust ServiceRegistry so that
/// service calls route to the Python entity methods.
#[cfg(feature = "python")]
fn register_python_entity_services(services: &ServiceRegistry) {
    use std::collections::HashSet;

    // Get all Python entities and extract unique domains
    let domains: HashSet<String> = match get_python_entities() {
        Ok(entities) => entities
            .iter()
            .filter_map(|entity_id| entity_id.split('.').next().map(String::from))
            .collect(),
        Err(e) => {
            warn!("Failed to get Python entities: {}", e);
            return;
        }
    };

    if domains.is_empty() {
        debug!("No Python entities found, skipping domain service registration");
        return;
    }

    info!(
        "Registering services for Python entity domains: {:?}",
        domains
    );

    // Define services for each domain type
    let domain_services: std::collections::HashMap<&str, Vec<&str>> = [
        ("light", vec!["turn_on", "turn_off", "toggle"]),
        ("switch", vec!["turn_on", "turn_off", "toggle"]),
        (
            "fan",
            vec![
                "turn_on",
                "turn_off",
                "toggle",
                "set_percentage",
                "set_preset_mode",
            ],
        ),
        (
            "cover",
            vec![
                "open_cover",
                "close_cover",
                "stop_cover",
                "set_cover_position",
            ],
        ),
        ("lock", vec!["lock", "unlock", "open"]),
        (
            "climate",
            vec!["set_temperature", "set_hvac_mode", "set_preset_mode"],
        ),
        (
            "media_player",
            vec![
                "turn_on",
                "turn_off",
                "play_media",
                "media_play",
                "media_pause",
                "media_stop",
                "volume_up",
                "volume_down",
                "volume_set",
                "volume_mute",
            ],
        ),
        ("vacuum", vec!["start", "stop", "pause", "return_to_base"]),
        ("button", vec!["press"]),
        ("number", vec!["set_value"]),
        ("select", vec!["select_option"]),
        (
            "humidifier",
            vec!["turn_on", "turn_off", "set_humidity", "set_mode"],
        ),
        ("siren", vec!["turn_on", "turn_off"]),
        ("valve", vec!["open_valve", "close_valve"]),
        (
            "water_heater",
            vec!["set_temperature", "set_operation_mode"],
        ),
        (
            "alarm_control_panel",
            vec![
                "alarm_arm_home",
                "alarm_arm_away",
                "alarm_arm_night",
                "alarm_disarm",
                "alarm_trigger",
            ],
        ),
    ]
    .into_iter()
    .collect();

    for domain in &domains {
        // Get services for this domain, or default to turn_on/turn_off/toggle
        let services_list = domain_services
            .get(domain.as_str())
            .cloned()
            .unwrap_or_else(|| vec!["turn_on", "turn_off", "toggle"]);

        for service_name in services_list {
            // Skip if already registered
            if services.has_service(domain, service_name) {
                continue;
            }

            let domain_clone = domain.clone();
            let service_clone = service_name.to_string();

            services.register_with_description(
                ServiceDescription {
                    domain: domain.clone(),
                    service: service_name.to_string(),
                    name: None,
                    description: Some(format!(
                        "Call {} on {} entities (Python)",
                        service_name, domain
                    )),
                    schema: None,
                    target: Some(json!({
                        "entity": {
                            "domain": domain
                        }
                    })),
                    supports_response: SupportsResponse::None,
                },
                move |call: ServiceCall| {
                    let domain = domain_clone.clone();
                    let service = service_clone.clone();
                    async move {
                        // Extract entity_id from service data
                        let entity_ids: Vec<String> =
                            if let Some(entity_id) = call.service_data.get("entity_id") {
                                if let Some(s) = entity_id.as_str() {
                                    vec![s.to_string()]
                                } else if let Some(arr) = entity_id.as_array() {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                } else {
                                    vec![]
                                }
                            } else {
                                vec![]
                            };

                        if entity_ids.is_empty() {
                            warn!("Service {}.{} called without entity_id", domain, service);
                            return Ok(None);
                        }

                        // Call the service on each entity
                        for entity_id in entity_ids {
                            // Verify the entity belongs to this domain
                            if !entity_id.starts_with(&format!("{}.", domain)) {
                                continue;
                            }

                            debug!(
                                "Calling Python entity service: {}.{} on {}",
                                domain, service, entity_id
                            );

                            // Build service data without entity_id
                            let mut service_data = call.service_data.clone();
                            if let Some(obj) = service_data.as_object_mut() {
                                obj.remove("entity_id");
                            }

                            match call_python_entity_service(&entity_id, &service, service_data) {
                                Ok(true) => {
                                    debug!(
                                        "Service {}.{} succeeded on {}",
                                        domain, service, entity_id
                                    );
                                }
                                Ok(false) => {
                                    warn!(
                                        "Service {}.{} not supported on {}",
                                        domain, service, entity_id
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        "Error calling {}.{} on {}: {}",
                                        domain, service, entity_id, e
                                    );
                                }
                            }
                        }

                        Ok(None)
                    }
                },
            );

            info!("Registered service: {}.{}", domain, service_name);
        }
    }
}

#[cfg(not(feature = "python"))]
fn register_python_entity_services(_services: &ServiceRegistry) {
    // No-op when Python is not enabled
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Home Assistant (Rust)");

    // Load configuration
    // Use HA_CONFIG_DIR env var or default to /config
    let config_dir = std::env::var("HA_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/config"));

    let config = if config_dir.join("configuration.yaml").exists() {
        info!("Loading configuration from {:?}", config_dir);
        match CoreConfig::load(&config_dir) {
            Ok(cfg) => {
                info!(
                    "Configuration loaded: name={}, location=({}, {})",
                    cfg.name, cfg.latitude, cfg.longitude
                );
                cfg
            }
            Err(e) => {
                warn!("Failed to load configuration: {}. Using defaults.", e);
                CoreConfig::default()
            }
        }
    } else {
        info!("No configuration.yaml found, using defaults");
        CoreConfig::default()
    };

    // Create registries before HomeAssistant so Python bridge can use them
    let registries = Arc::new(Registries::new(&config_dir));
    if let Err(e) = registries.load_all().await {
        warn!("Failed to load registries: {}", e);
    }

    let hass = HomeAssistant::new(&config_dir, registries);

    // Register core services
    hass.register_core_services();

    // Register automation and script services
    hass.register_automation_services();
    hass.register_script_services();

    // Register input helper services
    ha_components::register_input_boolean_services(&hass.services, hass.states.clone());
    ha_components::register_input_number_services(&hass.services, hass.states.clone());

    // Load input helpers from configuration
    load_input_helpers(&config_dir, &hass.states);

    // Load entities from config or use demo entities
    hass.load_entities(&config_dir);

    // Load components list from file or use defaults
    let components = load_components(&config_dir);
    info!("Loaded {} components", components.len());

    // Load services cache from file (for comparison testing)
    let services_cache = load_services_cache(&config_dir);
    if services_cache.is_some() {
        info!("Loaded services cache from file");
    }

    // Load events cache from file (for comparison testing)
    let events_cache = load_events_cache(&config_dir);
    if events_cache.is_some() {
        info!("Loaded events cache from file");
    }

    // Load automations from configuration
    let automation_configs = load_automations(&config_dir);
    if !automation_configs.is_empty() {
        // Create automation entities in state machine
        for config in &automation_configs {
            let automation_id = config
                .id
                .clone()
                .unwrap_or_else(|| config.alias.clone().unwrap_or_default());
            if !automation_id.is_empty() {
                let entity_id = EntityId::new("automation", &automation_id).unwrap();
                let state = if config.enabled { "on" } else { "off" };
                let mut attributes = HashMap::new();
                if let Some(alias) = &config.alias {
                    attributes.insert("friendly_name".to_string(), json!(alias));
                }
                hass.states
                    .set(entity_id, state, attributes, Context::new());
            }
        }

        // Load automations into the engine
        let manager = hass.automation_engine.manager();
        let manager_guard = manager.write().await;
        if let Err(e) = manager_guard.load(automation_configs) {
            warn!("Failed to load automations into engine: {}", e);
        }
    }

    // Start the automation engine
    hass.automation_engine.start().await;

    // Load and setup config entries
    setup_config_entries(&hass).await;

    // Register Python entity domain services (light.turn_on, etc.)
    // This must happen after config entries are set up so Python entities are registered
    register_python_entity_services(&hass.services);

    info!("Home Assistant initialized");

    // Configure frontend if HA_FRONTEND_PATH is set
    let frontend_config = std::env::var("HA_FRONTEND_PATH").ok().and_then(|path| {
        let frontend_path = PathBuf::from(&path);
        if frontend_path.exists() {
            info!("Frontend enabled: {:?}", frontend_path);
            Some(FrontendConfig {
                frontend_path,
                theme_color: "#18BCF2".to_string(),
            })
        } else {
            warn!("Frontend path does not exist: {:?}", path);
            None
        }
    });

    // Create persistent notification manager
    let notifications = persistent_notification::create_manager();

    // Register persistent_notification services
    register_persistent_notification_services(&hass.services, notifications.clone());

    // Create system log manager
    let system_log = Arc::new(SystemLog::with_defaults());

    // Register system_log services
    register_system_log_services(&hass.services, system_log.clone());

    // Create config flow handler for Python integration setup (only with python feature)
    #[cfg(feature = "python")]
    let config_flow_handler: Option<Arc<dyn ConfigFlowHandler>> =
        Some(Arc::new(ConfigFlowManager::new(
            hass.bus.clone(),
            hass.states.clone(),
            hass.services.clone(),
            hass.registries.clone(),
            Some(config_dir.clone()),
        )));
    #[cfg(not(feature = "python"))]
    let config_flow_handler: Option<Arc<dyn ConfigFlowHandler>> = None;

    // Create API state with auth (mark as onboarded for dev mode)
    let api_state = AppState {
        event_bus: hass.bus.clone(),
        state_machine: hass.states.clone(),
        service_registry: hass.services.clone(),
        config: Arc::new(config),
        components: Arc::new(components),
        config_entries: hass.config_entries.clone(),
        registries: hass.registries.clone(),
        notifications,
        system_log,
        services_cache,
        events_cache,
        frontend_config,
        auth_state: AuthState::new_onboarded(),
        config_flow_handler,
    };

    // Start API server
    // Use HA_PORT env var or default to 8123
    let port = std::env::var("HA_PORT").unwrap_or_else(|_| "8123".to_string());
    let addr = format!("0.0.0.0:{}", port);
    info!("Starting API server on http://{}", addr);

    // Run server until shutdown signal
    tokio::select! {
        result = ha_api::start_server(api_state, &addr) => {
            if let Err(e) = result {
                tracing::error!("Server error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Shutdown signal received");
        }
    }

    // Stop the automation engine
    hass.automation_engine.stop();

    info!("Home Assistant stopped");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a HomeAssistant instance with test registries
    fn create_test_hass(temp_dir: &TempDir) -> HomeAssistant {
        let registries = Arc::new(Registries::new(temp_dir.path()));
        HomeAssistant::new(temp_dir.path(), registries)
    }

    #[test]
    fn test_load_automations_no_config() {
        let temp_dir = TempDir::new().unwrap();
        let automations = load_automations(temp_dir.path());
        assert!(automations.is_empty());
    }

    #[test]
    fn test_load_automations_empty_list() {
        let temp_dir = TempDir::new().unwrap();
        let config_content = r#"
homeassistant:
  name: Test
automation: []
"#;
        fs::write(temp_dir.path().join("configuration.yaml"), config_content).unwrap();
        let automations = load_automations(temp_dir.path());
        assert!(automations.is_empty());
    }

    #[test]
    fn test_load_automations_single() {
        let temp_dir = TempDir::new().unwrap();
        let config_content = r#"
homeassistant:
  name: Test
automation:
  - id: test_automation
    alias: Test Automation
    trigger:
      - platform: state
        entity_id: sensor.test
    action:
      - action: homeassistant.turn_on
        target:
          entity_id: light.test
"#;
        fs::write(temp_dir.path().join("configuration.yaml"), config_content).unwrap();
        let automations = load_automations(temp_dir.path());
        assert_eq!(automations.len(), 1);
        assert_eq!(automations[0].id, Some("test_automation".to_string()));
        assert_eq!(automations[0].alias, Some("Test Automation".to_string()));
        assert!(automations[0].enabled);
    }

    #[test]
    fn test_load_automations_multiple() {
        let temp_dir = TempDir::new().unwrap();
        let config_content = r#"
homeassistant:
  name: Test
automation:
  - id: auto1
    alias: First
    trigger:
      - platform: state
        entity_id: sensor.a
    action: []
  - id: auto2
    alias: Second
    enabled: false
    trigger:
      - platform: state
        entity_id: sensor.b
    action: []
"#;
        fs::write(temp_dir.path().join("configuration.yaml"), config_content).unwrap();
        let automations = load_automations(temp_dir.path());
        assert_eq!(automations.len(), 2);
        assert_eq!(automations[0].id, Some("auto1".to_string()));
        assert!(automations[0].enabled);
        assert_eq!(automations[1].id, Some("auto2".to_string()));
        assert!(!automations[1].enabled);
    }

    #[test]
    fn test_load_automations_no_automation_key() {
        let temp_dir = TempDir::new().unwrap();
        let config_content = r#"
homeassistant:
  name: Test
script: []
"#;
        fs::write(temp_dir.path().join("configuration.yaml"), config_content).unwrap();
        let automations = load_automations(temp_dir.path());
        assert!(automations.is_empty());
    }

    #[test]
    fn test_home_assistant_new() {
        let temp_dir = TempDir::new().unwrap();
        let hass = create_test_hass(&temp_dir);
        assert!(!hass.automation_engine.is_running());
    }

    #[tokio::test]
    async fn test_automation_engine_start_stop() {
        let temp_dir = TempDir::new().unwrap();
        let hass = create_test_hass(&temp_dir);
        assert!(!hass.automation_engine.is_running());

        hass.automation_engine.start().await;
        assert!(hass.automation_engine.is_running());

        hass.automation_engine.stop();
        // Give the engine time to stop
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert!(!hass.automation_engine.is_running());
    }

    #[tokio::test]
    async fn test_automation_manager_load() {
        let temp_dir = TempDir::new().unwrap();
        let hass = create_test_hass(&temp_dir);

        let configs = vec![AutomationConfig {
            id: Some("test_auto".to_string()),
            alias: Some("Test Automation".to_string()),
            description: None,
            triggers: vec![],
            conditions: vec![],
            actions: vec![],
            mode: ha_automation::ExecutionMode::Single,
            max: None,
            enabled: true,
            variables: serde_json::Value::Null,
            trace: None,
        }];

        let manager = hass.automation_engine.manager();
        let manager_guard = manager.write().await;
        manager_guard.load(configs).unwrap();

        assert_eq!(manager_guard.count(), 1);
        let automation = manager_guard.get("test_auto").unwrap();
        assert_eq!(automation.alias, Some("Test Automation".to_string()));
        assert!(automation.enabled);
    }

    #[tokio::test]
    async fn test_automation_enable_disable() {
        let temp_dir = TempDir::new().unwrap();
        let hass = create_test_hass(&temp_dir);

        let configs = vec![AutomationConfig {
            id: Some("test_auto".to_string()),
            alias: Some("Test".to_string()),
            description: None,
            triggers: vec![],
            conditions: vec![],
            actions: vec![],
            mode: ha_automation::ExecutionMode::Single,
            max: None,
            enabled: true,
            variables: serde_json::Value::Null,
            trace: None,
        }];

        let manager = hass.automation_engine.manager();
        {
            let manager_guard = manager.write().await;
            manager_guard.load(configs).unwrap();
        }

        // Verify initially enabled
        {
            let manager_guard = manager.read().await;
            let automation = manager_guard.get("test_auto").unwrap();
            assert!(automation.enabled);
        }

        // Disable
        {
            let manager_guard = manager.write().await;
            manager_guard.disable("test_auto").unwrap();
        }

        // Verify disabled
        {
            let manager_guard = manager.read().await;
            let automation = manager_guard.get("test_auto").unwrap();
            assert!(!automation.enabled);
        }

        // Enable
        {
            let manager_guard = manager.write().await;
            manager_guard.enable("test_auto").unwrap();
        }

        // Verify enabled
        {
            let manager_guard = manager.read().await;
            let automation = manager_guard.get("test_auto").unwrap();
            assert!(automation.enabled);
        }
    }

    #[tokio::test]
    async fn test_event_trigger_fires_automation() {
        use ha_automation::trigger::{EventTrigger, Trigger};
        use ha_core::Event;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let temp_dir = TempDir::new().unwrap();
        let hass = create_test_hass(&temp_dir);

        // Track service calls
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        // Register a test service
        hass.services.register(
            "test",
            "automation_action",
            move |_call| {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(None)
                }
            },
            None,
            SupportsResponse::None,
        );

        // Create automation with event trigger
        let configs = vec![AutomationConfig {
            id: Some("event_auto".to_string()),
            alias: Some("Event Automation".to_string()),
            description: None,
            triggers: vec![Trigger::Event(EventTrigger {
                id: None,
                event_type: "test_event".to_string(),
                event_data: None,
                context: None,
            })],
            conditions: vec![],
            actions: vec![json!({
                "service": "test.automation_action"
            })],
            mode: ha_automation::ExecutionMode::Single,
            max: None,
            enabled: true,
            variables: serde_json::Value::Null,
            trace: None,
        }];

        // Load automation
        {
            let manager = hass.automation_engine.manager();
            let manager_guard = manager.write().await;
            manager_guard.load(configs).unwrap();
        }

        // Start the engine
        hass.automation_engine.start().await;

        // Give engine time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Fire the event
        let event = Event::new("test_event", json!({}), Context::new());
        hass.bus.fire(event);

        // Give automation time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Verify action was called
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "Action should have been called once"
        );

        // Fire again
        let event = Event::new("test_event", json!({}), Context::new());
        hass.bus.fire(event);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            2,
            "Action should have been called twice"
        );

        // Stop engine
        hass.automation_engine.stop();
    }

    #[tokio::test]
    async fn test_state_trigger_fires_automation() {
        use ha_automation::trigger::{EntityIdSpec, StateMatch, StateTrigger, Trigger};
        use std::sync::atomic::{AtomicUsize, Ordering};

        let temp_dir = TempDir::new().unwrap();
        let hass = create_test_hass(&temp_dir);

        // Track service calls
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        // Register a test service
        hass.services.register(
            "test",
            "state_action",
            move |_call| {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(None)
                }
            },
            None,
            SupportsResponse::None,
        );

        // Set up initial entity state
        hass.states.set(
            EntityId::new("sensor", "test").unwrap(),
            "off",
            HashMap::new(),
            Context::new(),
        );

        // Create automation with state trigger
        let configs = vec![AutomationConfig {
            id: Some("state_auto".to_string()),
            alias: Some("State Automation".to_string()),
            description: None,
            triggers: vec![Trigger::State(StateTrigger {
                id: None,
                entity_id: EntityIdSpec::Single("sensor.test".to_string()),
                attribute: None,
                from: Some(StateMatch::Single("off".to_string())),
                to: Some(StateMatch::Single("on".to_string())),
                not_from: vec![],
                not_to: vec![],
                r#for: None,
            })],
            conditions: vec![],
            actions: vec![json!({
                "service": "test.state_action"
            })],
            mode: ha_automation::ExecutionMode::Single,
            max: None,
            enabled: true,
            variables: serde_json::Value::Null,
            trace: None,
        }];

        // Load automation
        {
            let manager = hass.automation_engine.manager();
            let manager_guard = manager.write().await;
            manager_guard.load(configs).unwrap();
        }

        // Start the engine
        hass.automation_engine.start().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Change state from off to on - should trigger
        hass.states.set(
            EntityId::new("sensor", "test").unwrap(),
            "on",
            HashMap::new(),
            Context::new(),
        );

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "Action should have been called on state change"
        );

        // Change state to something else - should not trigger (wrong transition)
        hass.states.set(
            EntityId::new("sensor", "test").unwrap(),
            "unknown",
            HashMap::new(),
            Context::new(),
        );

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "Action should not be called for wrong transition"
        );

        // Stop engine
        hass.automation_engine.stop();
    }

    #[tokio::test]
    async fn test_disabled_automation_does_not_fire() {
        use ha_automation::trigger::{EventTrigger, Trigger};
        use ha_core::Event;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let temp_dir = TempDir::new().unwrap();
        let hass = create_test_hass(&temp_dir);

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        hass.services.register(
            "test",
            "disabled_action",
            move |_call| {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(None)
                }
            },
            None,
            SupportsResponse::None,
        );

        // Create disabled automation
        let configs = vec![AutomationConfig {
            id: Some("disabled_auto".to_string()),
            alias: Some("Disabled Automation".to_string()),
            description: None,
            triggers: vec![Trigger::Event(EventTrigger {
                id: None,
                event_type: "disabled_test_event".to_string(),
                event_data: None,
                context: None,
            })],
            conditions: vec![],
            actions: vec![json!({
                "service": "test.disabled_action"
            })],
            mode: ha_automation::ExecutionMode::Single,
            max: None,
            enabled: false, // Disabled!
            variables: serde_json::Value::Null,
            trace: None,
        }];

        {
            let manager = hass.automation_engine.manager();
            let manager_guard = manager.write().await;
            manager_guard.load(configs).unwrap();
        }

        hass.automation_engine.start().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Fire the event
        let event = Event::new("disabled_test_event", json!({}), Context::new());
        hass.bus.fire(event);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Should NOT have been called
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            0,
            "Disabled automation should not fire"
        );

        hass.automation_engine.stop();
    }

    #[tokio::test]
    async fn test_condition_blocks_automation() {
        use ha_automation::condition::{Condition, StateCondition};
        use ha_automation::trigger::{EntityIdSpec, EventTrigger, StateMatch, Trigger};
        use ha_core::Event;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let temp_dir = TempDir::new().unwrap();
        let hass = create_test_hass(&temp_dir);

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        hass.services.register(
            "test",
            "condition_action",
            move |_call| {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(None)
                }
            },
            None,
            SupportsResponse::None,
        );

        // Set up entity for condition check
        hass.states.set(
            EntityId::new("input_boolean", "gate").unwrap(),
            "off", // Condition will check for "on"
            HashMap::new(),
            Context::new(),
        );

        // Create automation with condition that won't pass
        let configs = vec![AutomationConfig {
            id: Some("condition_auto".to_string()),
            alias: Some("Condition Automation".to_string()),
            description: None,
            triggers: vec![Trigger::Event(EventTrigger {
                id: None,
                event_type: "condition_test_event".to_string(),
                event_data: None,
                context: None,
            })],
            conditions: vec![Condition::State(StateCondition {
                entity_id: EntityIdSpec::Single("input_boolean.gate".to_string()),
                state: StateMatch::Single("on".to_string()),
                attribute: None,
                r#for: None,
                match_regex: false,
            })],
            actions: vec![json!({
                "service": "test.condition_action"
            })],
            mode: ha_automation::ExecutionMode::Single,
            max: None,
            enabled: true,
            variables: serde_json::Value::Null,
            trace: None,
        }];

        {
            let manager = hass.automation_engine.manager();
            let manager_guard = manager.write().await;
            manager_guard.load(configs).unwrap();
        }

        hass.automation_engine.start().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Fire event - trigger matches but condition fails
        let event = Event::new("condition_test_event", json!({}), Context::new());
        hass.bus.fire(event);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            0,
            "Action should not fire when condition fails"
        );

        // Now set the gate to on
        hass.states.set(
            EntityId::new("input_boolean", "gate").unwrap(),
            "on",
            HashMap::new(),
            Context::new(),
        );

        // Fire event again - now condition should pass
        let event = Event::new("condition_test_event", json!({}), Context::new());
        hass.bus.fire(event);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "Action should fire when condition passes"
        );

        hass.automation_engine.stop();
    }
}
