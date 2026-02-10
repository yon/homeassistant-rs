//! Home Assistant Rust Server
//!
//! Main entry point for the Home Assistant Rust implementation.

mod automation_engine;
mod services;

use anyhow::Result;
use ha_api::{
    auth::AuthState, config_flow::ConfigFlowHandler, frontend::FrontendConfig,
    persistent_notification, AppState,
};
use ha_automation::AutomationConfig;
use ha_components::{register_system_log_services, SystemLog};
use ha_config::CoreConfig;
use ha_config_entries::ConfigEntries;
use ha_core::{
    Context, EntityId, ServiceCall, SupportsResponse, STATE_OFF, STATE_ON, STATE_UNKNOWN,
};
use ha_event_bus::EventBus;
use ha_registries::{Registries, Storage};
use ha_service_registry::{ServiceDescription, ServiceRegistry};
use ha_state_store::StateStore;
use ha_template::TemplateEngine;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

#[cfg(feature = "python")]
use ha_py_bridge::py_bridge::{load_allowlist_from_config, ConfigFlowManager, PyBridge};

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
    pub states: Arc<StateStore>,
    /// Template engine for rendering templates
    pub template_engine: Arc<TemplateEngine>,
    /// Python bridge for running Python integrations
    #[cfg(feature = "python")]
    pub python_bridge: Option<Arc<PyBridge>>,
}

impl HomeAssistant {
    /// Create a new Home Assistant instance
    ///
    /// # Arguments
    /// * `config_dir` - Path to the Home Assistant config directory
    /// * `registries` - Registries for entities, devices, areas, etc.
    pub fn new(config_dir: &Path, registries: Arc<Registries>) -> Self {
        let bus = Arc::new(EventBus::new());
        let states = Arc::new(StateStore::new(bus.clone()));
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
        // Derive HA core path from HA_COMPONENTS_PATH (parent of homeassistant/components)
        // or fall back to HA_PYTHON_PATH for pip-installed HA
        #[cfg(feature = "python")]
        let python_bridge = match {
            let ha_python_path = std::env::var("HA_COMPONENTS_PATH")
                .map(PathBuf::from)
                .ok()
                .and_then(|p| p.parent()?.parent().map(|p| p.to_path_buf()))
                .or_else(|| std::env::var("HA_PYTHON_PATH").map(PathBuf::from).ok());
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

                Some(Arc::new(bridge))
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
        use services::{register_state_service, register_stub_service, register_toggle_service};

        // State-modifying services
        register_state_service(
            &self.services,
            &self.states,
            "homeassistant",
            "turn_on",
            STATE_ON,
            None,
            false,
        );
        register_state_service(
            &self.services,
            &self.states,
            "homeassistant",
            "turn_off",
            STATE_OFF,
            None,
            false,
        );
        register_toggle_service(&self.services, &self.states, "homeassistant", None);

        // Stub services (alphabetized by service name)
        register_stub_service(&self.services, "homeassistant", "check_config", None, None);
        register_stub_service(
            &self.services,
            "homeassistant",
            "reload_all",
            None,
            Some("Reload all requested"),
        );
        register_stub_service(
            &self.services,
            "homeassistant",
            "reload_config_entry",
            Some(json!({})),
            Some("Reload config entry requested"),
        );
        register_stub_service(
            &self.services,
            "homeassistant",
            "reload_core_config",
            None,
            Some("Reloading core config"),
        );
        register_stub_service(
            &self.services,
            "homeassistant",
            "reload_custom_templates",
            None,
            Some("Reload custom templates requested"),
        );
        register_stub_service(
            &self.services,
            "homeassistant",
            "restart",
            None,
            Some("Restart requested (not implemented in test mode)"),
        );
        register_stub_service(
            &self.services,
            "homeassistant",
            "save_persistent_states",
            None,
            Some("Save persistent states requested"),
        );
        register_stub_service(
            &self.services,
            "homeassistant",
            "set_location",
            None,
            Some("Set location requested"),
        );
        register_stub_service(
            &self.services,
            "homeassistant",
            "stop",
            None,
            Some("Stop requested (not implemented in test mode)"),
        );
        register_stub_service(
            &self.services,
            "homeassistant",
            "update_entity",
            None,
            Some("update_entity called"),
        );

        info!("Core services registered");
    }

    /// Register automation domain services
    fn register_automation_services(&self) {
        use services::{
            register_automation_state_service, register_stub_service, AutomationAction,
        };

        let manager = self.automation_engine.manager();

        // State-modifying services (turn_on/turn_off/toggle)
        register_automation_state_service(
            &self.services,
            &self.states,
            &manager,
            "turn_on",
            AutomationAction::Enable,
        );
        register_automation_state_service(
            &self.services,
            &self.states,
            &manager,
            "turn_off",
            AutomationAction::Disable,
        );
        register_automation_state_service(
            &self.services,
            &self.states,
            &manager,
            "toggle",
            AutomationAction::Toggle,
        );

        // Register automation.trigger service - manually trigger an automation
        // (Different logic from turn_on/off/toggle: no state change, just logging)
        let manager_clone = manager.clone();
        self.services.register_with_description(
            ServiceDescription {
                domain: "automation".to_string(),
                description: None,
                name: None,
                schema: None,
                service: "trigger".to_string(),
                supports_response: SupportsResponse::None,
                target: Some(json!({"entity": {"domain": "automation"}})),
            },
            move |call: ServiceCall| {
                let manager = manager_clone.clone();
                async move {
                    if let Some(entity_id) = services::extract_entity_id(&call, Some("automation"))
                    {
                        let automation_id = entity_id.object_id().to_string();
                        let manager_guard = manager.read().await;
                        if let Some(automation) = manager_guard.get(&automation_id) {
                            info!(
                                "Triggering automation: {} ({})",
                                automation.display_name(),
                                automation_id
                            );
                        } else {
                            warn!("Automation not found: {}", automation_id);
                        }
                    }
                    Ok(None)
                }
            },
        );

        // automation.reload stub
        register_stub_service(
            &self.services,
            "automation",
            "reload",
            None,
            Some("Reloading automations (not fully implemented)"),
        );

        info!("Automation services registered");
    }

    /// Register script domain services
    fn register_script_services(&self) {
        use services::{register_state_service, register_stub_service, register_toggle_service};

        register_state_service(
            &self.services,
            &self.states,
            "script",
            "turn_on",
            STATE_ON,
            Some("script"),
            true,
        );
        register_state_service(
            &self.services,
            &self.states,
            "script",
            "turn_off",
            STATE_OFF,
            Some("script"),
            true,
        );
        register_toggle_service(&self.services, &self.states, "script", Some("script"));
        register_stub_service(
            &self.services,
            "script",
            "reload",
            None,
            Some("Reloading scripts"),
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
                .unwrap_or(STATE_UNKNOWN);

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
        // Helper to register entity in registry and set state
        let add_entity = |entity_id_str: &str,
                          platform: &str,
                          state: &str,
                          attrs: HashMap<String, serde_json::Value>,
                          device_class: Option<&str>,
                          original_name: Option<&str>| {
            let entity_id = match entity_id_str.split_once('.') {
                Some((domain, object_id)) => match EntityId::new(domain, object_id) {
                    Ok(id) => id,
                    Err(e) => {
                        warn!("Invalid entity ID '{}': {}", entity_id_str, e);
                        return;
                    }
                },
                None => {
                    warn!("Invalid entity ID format (no dot): '{}'", entity_id_str);
                    return;
                }
            };

            // Register in entity registry
            self.registries.entities.get_or_create(
                platform,
                entity_id_str,
                Some(&format!("demo_{}", entity_id_str)), // unique_id
                None,                                     // config_entry_id
                None,                                     // device_id
            );

            // Set device class and original name if provided
            if device_class.is_some() || original_name.is_some() {
                let _ = self.registries.entities.update(entity_id_str, |e| {
                    if let Some(dc) = device_class {
                        e.original_device_class = Some(dc.to_string());
                    }
                    if let Some(name) = original_name {
                        e.original_name = Some(name.to_string());
                    }
                });
            }

            // Set state
            self.states.set(entity_id, state, attrs, Context::new());
        };

        // Add demo lights
        add_entity(
            "light.living_room",
            "demo",
            STATE_ON,
            HashMap::from([
                ("brightness".to_string(), serde_json::json!(255)),
                (
                    "friendly_name".to_string(),
                    serde_json::json!("Living Room Light"),
                ),
            ]),
            None,
            Some("Living Room Light"),
        );

        add_entity(
            "light.bedroom",
            "demo",
            STATE_OFF,
            HashMap::from([
                ("brightness".to_string(), serde_json::json!(0)),
                (
                    "friendly_name".to_string(),
                    serde_json::json!("Bedroom Light"),
                ),
            ]),
            None,
            Some("Bedroom Light"),
        );

        // Add sensors
        add_entity(
            "sensor.temperature",
            "demo",
            "22.5",
            HashMap::from([
                ("unit_of_measurement".to_string(), serde_json::json!("°C")),
                (
                    "friendly_name".to_string(),
                    serde_json::json!("Temperature"),
                ),
                ("device_class".to_string(), serde_json::json!("temperature")),
            ]),
            Some("temperature"),
            Some("Temperature"),
        );

        add_entity(
            "sensor.humidity",
            "demo",
            "45",
            HashMap::from([
                ("unit_of_measurement".to_string(), serde_json::json!("%")),
                ("friendly_name".to_string(), serde_json::json!("Humidity")),
                ("device_class".to_string(), serde_json::json!("humidity")),
            ]),
            Some("humidity"),
            Some("Humidity"),
        );

        // Add a switch
        add_entity(
            "switch.coffee_maker",
            "demo",
            STATE_OFF,
            HashMap::from([(
                "friendly_name".to_string(),
                serde_json::json!("Coffee Maker"),
            )]),
            None,
            Some("Coffee Maker"),
        );

        // Add a binary sensor
        add_entity(
            "binary_sensor.front_door",
            "demo",
            STATE_OFF,
            HashMap::from([
                ("friendly_name".to_string(), serde_json::json!("Front Door")),
                ("device_class".to_string(), serde_json::json!("door")),
            ]),
            Some("door"),
            Some("Front Door"),
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
        "config".to_string(),
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

/// Extract a typed HashMap section from a YAML value by key.
///
/// Returns an empty map if the key is missing or the value cannot be
/// deserialized into `HashMap<String, T>`.
fn collect_yaml_section<T: serde::de::DeserializeOwned>(
    yaml: &serde_yaml::Value,
    key: &str,
) -> HashMap<String, T> {
    yaml.get(key)
        .and_then(|v| serde_yaml::from_value::<HashMap<String, T>>(v.clone()).ok())
        .unwrap_or_default()
}

/// Load input helpers (input_boolean, input_number) from configuration
fn load_input_helpers(config_dir: &Path, states: &StateStore) {
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

    // Collect from root level
    let mut all_input_booleans: HashMap<String, Option<ha_components::InputBooleanConfig>> =
        collect_yaml_section(&yaml, "input_boolean");
    let mut all_input_numbers: HashMap<String, ha_components::InputNumberConfig> =
        collect_yaml_section(&yaml, "input_number");

    // Collect from packages (homeassistant.packages contains merged package content)
    if let Some(homeassistant) = yaml.get("homeassistant") {
        if let Some(packages) = homeassistant.get("packages") {
            if let Some(packages_map) = packages.as_mapping() {
                for (_, package_content) in packages_map {
                    all_input_booleans
                        .extend(collect_yaml_section(package_content, "input_boolean"));
                    all_input_numbers.extend(collect_yaml_section(package_content, "input_number"));
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
/// If Python bridge is available, registers it as the setup handler for all domains.
#[cfg(feature = "python")]
async fn setup_config_entries(hass: &HomeAssistant) {
    use ha_config_entries::{SetupContext, WILDCARD_DOMAIN};
    // Load config entries from storage
    {
        let manager = hass.config_entries.write().await;
        if let Err(e) = manager.load().await {
            warn!("Failed to load config entries: {}", e);
            return;
        }
    }

    // Set up context and register Python bridge handlers
    {
        let manager = hass.config_entries.read().await;

        // Set context for setup/unload operations
        let context = SetupContext {
            bus: hass.bus.clone(),
            states: hass.states.clone(),
            services: hass.services.clone(),
        };
        manager.set_context(context).await;

        // Register Python bridge as wildcard handler if available
        if let Some(ref bridge) = hass.python_bridge {
            manager.register_setup_handler(
                WILDCARD_DOMAIN,
                PyBridge::create_setup_handler(bridge.clone()),
            );
            manager.register_unload_handler(
                WILDCARD_DOMAIN,
                PyBridge::create_unload_handler(bridge.clone()),
            );
        }
    }

    // Get entry IDs to setup
    let entry_ids: Vec<String> = {
        let manager = hass.config_entries.read().await;
        manager.entry_ids()
    };

    if entry_ids.is_empty() {
        debug!("No config entries to setup");
        return;
    }

    info!("Setting up {} config entries", entry_ids.len());

    // Setup each entry - FSM and handler dispatch handled by ConfigEntries
    let manager = hass.config_entries.read().await;
    for entry_id in entry_ids {
        if let Err(e) = manager.setup(&entry_id).await {
            // Errors are logged by the manager, but we can add context
            warn!("Failed to setup entry {}: {:?}", entry_id, e);
        }
    }

    // Start the Python background event loop AFTER all config entries are set up
    // This must happen after setup because run_until_complete() can't be called
    // while the loop is already running (from run_forever in background thread)
    if let Some(ref bridge) = hass.python_bridge {
        if let Err(e) = bridge.start_background_event_loop() {
            warn!("Failed to start Python background event loop: {:?}", e);
        }
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

/// Initialize entity states from the entity registry.
///
/// Collect unique controllable domains from the Rust entity registry.
///
/// Iterates over all entities in the registry, extracts domain names,
/// and filters out read-only domains (sensor, binary_sensor, etc.).
/// Used by register_python_entity_services (python feature) and tests.
#[cfg(any(feature = "python", test))]
fn collect_service_domains_from_registry(
    registries: &Registries,
) -> std::collections::HashSet<String> {
    use ha_core::domains;

    registries
        .entities
        .iter()
        .into_iter()
        .filter_map(|entry| {
            let domain = entry.entity_id.split('.').next().map(String::from)?;
            if domains::is_readonly_domain(&domain) {
                None
            } else {
                Some(domain)
            }
        })
        .collect()
}

/// For entities that exist in the registry but have no state in the state machine
/// (e.g., from integrations that failed to set up), create an "unavailable" state
/// with enriched attributes. This allows the frontend to display entity controls
/// even when the integration is offline.
fn initialize_entity_states_from_registry(hass: &HomeAssistant) {
    let mut count = 0;
    for entry in hass.registries.entities.iter() {
        let entity_id: EntityId = match entry.entity_id.parse() {
            Ok(id) => id,
            Err(_) => {
                debug!("Skipping invalid entity_id: {}", entry.entity_id);
                continue;
            }
        };
        // Skip if state already exists (set by Python integration or demo entities)
        if hass.states.get(&entry.entity_id).is_some() {
            continue;
        }
        // Create "unavailable" state with enriched attributes from registry
        let mut attrs = HashMap::new();
        entry.enrich_attributes(&mut attrs);
        hass.states
            .set(entity_id, "unavailable", attrs, Context::default());
        count += 1;
    }
    info!(
        "Initialized {} entity states from registry as unavailable",
        count
    );
}

/// Handle entity service calls directly in Rust via state store manipulation.
///
/// Returns `Some(true)` if the service was handled, `Some(false)` if the service
/// is not supported for this entity, or `None` if the entity doesn't exist.
#[cfg(any(feature = "python", test))]
fn handle_entity_service_rust(
    states: &StateStore,
    entity_id: &str,
    service: &str,
    service_data: &serde_json::Value,
    context: Context,
) -> Option<bool> {
    let current = states.get(entity_id)?;
    let domain = entity_id.split('.').next().unwrap_or("");

    let new_state = match service {
        "turn_on" => Some("on".to_string()),
        "turn_off" => Some("off".to_string()),
        "toggle" => match domain {
            "light" | "switch" | "fan" | "siren" | "humidifier" | "input_boolean" => {
                Some(if current.state == "on" { "off" } else { "on" }.to_string())
            }
            "lock" => Some(
                if current.state == "locked" {
                    "unlocked"
                } else {
                    "locked"
                }
                .to_string(),
            ),
            "cover" => Some(
                if current.state == "open" {
                    "closed"
                } else {
                    "open"
                }
                .to_string(),
            ),
            _ => Some(if current.state == "on" { "off" } else { "on" }.to_string()),
        },
        "lock" => Some("locked".to_string()),
        "unlock" => Some("unlocked".to_string()),
        "open_cover" | "open_valve" => Some("open".to_string()),
        "close_cover" | "close_valve" => Some("closed".to_string()),
        _ => None,
    };

    let new_state = match new_state {
        Some(s) => s,
        None => return Some(false),
    };

    // Preserve existing attributes, merge in any new ones from service_data
    let mut attrs = current.attributes.clone();
    if service == "turn_on" {
        if let Some(brightness) = service_data.get("brightness").and_then(|v| v.as_u64()) {
            attrs.insert("brightness".to_string(), json!(brightness));
        }
    }

    let eid: EntityId = match entity_id.parse() {
        Ok(id) => id,
        Err(_) => return Some(false),
    };

    states.set(eid, new_state, attrs, context);
    Some(true)
}

/// Register entity domain services based on Rust entity registry
///
/// After loading entities from disk, we register services like
/// `light.turn_on`, `light.turn_off` etc. in the Rust ServiceRegistry so that
/// service calls route to the Python entity methods or state store fallback.
#[cfg(feature = "python")]
fn register_python_entity_services(
    services: &ServiceRegistry,
    registries: &Registries,
    states: Arc<StateStore>,
) {
    use ha_core::domains;

    // Get domains from RUST entity registry (loaded from disk), NOT Python's _entity_registry.
    // Python's _entity_registry is often empty because integrations fail to connect to devices.
    // The Rust entity registry is always populated from core.entity_registry storage.
    let domains = collect_service_domains_from_registry(registries);

    if domains.is_empty() {
        debug!("No controllable entities found, skipping domain service registration");
        return;
    }

    info!(
        "Registering services for Python entity domains: {:?}",
        domains
    );

    for domain in &domains {
        // Get services for this domain from ha-core domain metadata
        // Returns None for read-only domains (sensor, binary_sensor, etc.)
        let services_list = match domains::get_domain_services(domain.as_str()) {
            Some(services) => services,
            None => {
                debug!("Skipping read-only domain: {}", domain);
                continue;
            }
        };

        for service_name in services_list {
            // Skip if already registered
            if services.has_service(domain, service_name) {
                continue;
            }

            let domain_clone = domain.clone();
            let service_clone = service_name.to_string();
            let states_clone = states.clone();

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
                    let states = states_clone.clone();
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
                        for entity_id in &entity_ids {
                            // Verify the entity belongs to this domain
                            if !entity_id.starts_with(&format!("{}.", domain)) {
                                continue;
                            }

                            // Handle entity service via Rust state store
                            if let Some(handled) = handle_entity_service_rust(
                                &states,
                                entity_id,
                                &service,
                                &call.service_data,
                                call.context.clone(),
                            ) {
                                if handled {
                                    info!(
                                        "Service {}.{} handled by Rust on {}",
                                        domain, service, entity_id
                                    );
                                } else {
                                    warn!(
                                        "Service {}.{} not supported on {}",
                                        domain, service, entity_id
                                    );
                                }
                            } else {
                                warn!(
                                    "Entity {} not found in state store for {}.{}",
                                    entity_id, domain, service
                                );
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
fn register_python_entity_services(
    _services: &ServiceRegistry,
    _registries: &Registries,
    _states: Arc<StateStore>,
) {
    // No-op when Python is not enabled
}

/// Load CoreConfig from the configuration directory, falling back to defaults.
fn load_config(config_dir: &Path) -> CoreConfig {
    if config_dir.join("configuration.yaml").exists() {
        info!("Loading configuration from {:?}", config_dir);
        match CoreConfig::load(config_dir) {
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
    }
}

/// Register all domain services (core, automation, script, input helpers).
fn register_all_services(hass: &HomeAssistant, config_dir: &Path) {
    hass.register_core_services();
    hass.register_automation_services();
    hass.register_script_services();
    ha_components::register_input_boolean_services(&hass.services, hass.states.clone());
    ha_components::register_input_number_services(&hass.services, hass.states.clone());
    load_input_helpers(config_dir, &hass.states);
}

/// Load automations from config, create state entities, and start the engine.
async fn load_and_start_automations(hass: &HomeAssistant, config_dir: &Path) {
    let automation_configs = load_automations(config_dir);
    if !automation_configs.is_empty() {
        // Create automation entities in state machine
        for config in &automation_configs {
            let automation_id = config
                .id
                .clone()
                .unwrap_or_else(|| config.alias.clone().unwrap_or_default());
            if !automation_id.is_empty() {
                let entity_id = match EntityId::new("automation", &automation_id) {
                    Ok(id) => id,
                    Err(e) => {
                        warn!("Invalid automation ID '{}': {}", automation_id, e);
                        continue;
                    }
                };
                let state = if config.enabled { STATE_ON } else { STATE_OFF };
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

    hass.automation_engine.start().await;
}

/// Read an optional path from an environment variable.
///
/// Returns `None` if the variable is unset or the path does not exist.
fn env_path_if_exists(var: &str) -> Option<PathBuf> {
    std::env::var(var).ok().and_then(|p| {
        let path = PathBuf::from(&p);
        if path.exists() {
            info!("{} enabled: {:?}", var, path);
            Some(path)
        } else {
            warn!("{} does not exist: {:?}", var, p);
            None
        }
    })
}

/// Build the API state from HomeAssistant instance and loaded data.
fn build_api_state(
    hass: &HomeAssistant,
    config: CoreConfig,
    config_dir: &Path,
    components: Vec<String>,
    services_cache: Option<Arc<serde_json::Value>>,
    events_cache: Option<Arc<serde_json::Value>>,
) -> AppState {
    let frontend_config =
        env_path_if_exists("HA_FRONTEND_PATH").map(|frontend_path| FrontendConfig {
            frontend_path,
            theme_color: "#18BCF2".to_string(),
        });

    let components_path = env_path_if_exists("HA_COMPONENTS_PATH");

    // Create managers and register their services
    let notifications = persistent_notification::create_manager();
    register_persistent_notification_services(&hass.services, notifications.clone());

    let system_log = Arc::new(SystemLog::with_defaults());
    register_system_log_services(&hass.services, system_log.clone());

    let application_credentials = ha_api::new_application_credentials_store();

    // Create config flow handler (Python-only)
    #[cfg(feature = "python")]
    let config_flow_handler: Option<Arc<dyn ConfigFlowHandler>> =
        hass.python_bridge
            .as_ref()
            .map(|bridge| -> Arc<dyn ConfigFlowHandler> {
                Arc::new(ConfigFlowManager::new(
                    hass.bus.clone(),
                    hass.states.clone(),
                    hass.services.clone(),
                    hass.registries.clone(),
                    Some(config_dir.to_path_buf()),
                    bridge.async_bridge.clone(),
                    application_credentials.clone(),
                    bridge.requirements.clone(),
                ))
            });
    #[cfg(not(feature = "python"))]
    let config_flow_handler: Option<Arc<dyn ConfigFlowHandler>> = None;

    // Suppress unused warning when python feature is disabled
    #[cfg(not(feature = "python"))]
    let _ = config_dir;

    AppState {
        application_credentials,
        auth_state: AuthState::new_onboarded(),
        components: Arc::new(components),
        components_path,
        config: Arc::new(config),
        config_entries: hass.config_entries.clone(),
        config_flow_handler,
        event_bus: hass.bus.clone(),
        events_cache,
        frontend_config,
        notifications,
        registries: hass.registries.clone(),
        service_registry: hass.services.clone(),
        services_cache,
        state_machine: hass.states.clone(),
        system_log,
    }
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

    let config_dir = std::env::var("HA_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/config"));
    let config = load_config(&config_dir);

    // Create registries and HomeAssistant instance
    let registries = Arc::new(Registries::new(&config_dir));
    if let Err(e) = registries.load_all().await {
        warn!("Failed to load registries: {}", e);
    }
    let hass = HomeAssistant::new(&config_dir, registries);

    // Register services and load entities
    register_all_services(&hass, &config_dir);
    hass.load_entities(&config_dir);

    // Load caches for comparison testing
    let components = load_components(&config_dir);
    info!("Loaded {} components", components.len());
    let services_cache = load_services_cache(&config_dir);
    let events_cache = load_events_cache(&config_dir);

    // Load automations and start engine
    load_and_start_automations(&hass, &config_dir).await;

    // Setup config entries and Python entity services
    setup_config_entries(&hass).await;
    register_python_entity_services(&hass.services, &hass.registries, hass.states.clone());

    // Initialize states for registry entities that have no state yet
    initialize_entity_states_from_registry(&hass);
    info!("Home Assistant initialized");

    // Build API state and start server
    let api_state = build_api_state(
        &hass,
        config,
        &config_dir,
        components,
        services_cache,
        events_cache,
    );

    let port = std::env::var("HA_PORT").unwrap_or_else(|_| "8123".to_string());
    let addr = format!("0.0.0.0:{}", port);
    info!("Starting API server on http://{}", addr);

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

    hass.automation_engine.stop();
    info!("Home Assistant stopped");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
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
            STATE_OFF,
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
                from: Some(StateMatch::Single(STATE_OFF.to_string())),
                to: Some(StateMatch::Single(STATE_ON.to_string())),
                not_from: HashSet::new(),
                not_to: HashSet::new(),
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
            STATE_ON,
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
            STATE_UNKNOWN,
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
            STATE_OFF, // Condition will check for "on"
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
                state: StateMatch::Single(STATE_ON.to_string()),
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
            STATE_ON,
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

    #[test]
    fn collect_yaml_section_extracts_typed_map() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(
            r#"
input_boolean:
  bedroom_lamp:
    name: Bedroom Lamp
  garage_door: ~
"#,
        )
        .unwrap();
        let result: HashMap<String, Option<ha_components::InputBooleanConfig>> =
            collect_yaml_section(&yaml, "input_boolean");
        assert_eq!(result.len(), 2);
        assert!(result.contains_key("bedroom_lamp"));
        assert!(result.contains_key("garage_door"));
    }

    #[test]
    fn collect_yaml_section_returns_empty_for_missing_key() {
        let yaml: serde_yaml::Value = serde_yaml::from_str("other_key: 42").unwrap();
        let result: HashMap<String, Option<ha_components::InputBooleanConfig>> =
            collect_yaml_section(&yaml, "input_boolean");
        assert!(result.is_empty());
    }

    #[test]
    fn collect_yaml_section_returns_empty_for_invalid_type() {
        let yaml: serde_yaml::Value = serde_yaml::from_str("input_boolean: not_a_map").unwrap();
        let result: HashMap<String, Option<ha_components::InputBooleanConfig>> =
            collect_yaml_section(&yaml, "input_boolean");
        assert!(result.is_empty());
    }

    // ---- Tests for collect_service_domains_from_registry ----

    #[test]
    fn test_collect_service_domains_includes_controllable() {
        let temp_dir = TempDir::new().unwrap();
        let registries = Registries::new(temp_dir.path());

        registries
            .entities
            .get_or_create("test_platform", "light.living_room", None, None, None);
        registries
            .entities
            .get_or_create("test_platform", "switch.garage", None, None, None);

        let domains = collect_service_domains_from_registry(&registries);

        assert!(
            domains.contains("light"),
            "Expected domains to contain 'light', got: {:?}",
            domains
        );
        assert!(
            domains.contains("switch"),
            "Expected domains to contain 'switch', got: {:?}",
            domains
        );
    }

    #[test]
    fn test_collect_service_domains_skips_readonly() {
        let temp_dir = TempDir::new().unwrap();
        let registries = Registries::new(temp_dir.path());

        registries
            .entities
            .get_or_create("test_platform", "sensor.temperature", None, None, None);
        registries.entities.get_or_create(
            "test_platform",
            "binary_sensor.motion",
            None,
            None,
            None,
        );

        let domains = collect_service_domains_from_registry(&registries);

        assert!(
            domains.is_empty(),
            "Expected empty domains for read-only entities, got: {:?}",
            domains
        );
    }

    #[test]
    fn test_collect_service_domains_empty_registry() {
        let temp_dir = TempDir::new().unwrap();
        let registries = Registries::new(temp_dir.path());

        let domains = collect_service_domains_from_registry(&registries);

        assert!(
            domains.is_empty(),
            "Expected empty domains for empty registry, got: {:?}",
            domains
        );
    }

    #[test]
    fn test_collect_service_domains_mixed() {
        let temp_dir = TempDir::new().unwrap();
        let registries = Registries::new(temp_dir.path());

        registries
            .entities
            .get_or_create("test_platform", "light.kitchen", None, None, None);
        registries
            .entities
            .get_or_create("test_platform", "sensor.humidity", None, None, None);
        registries
            .entities
            .get_or_create("test_platform", "lock.front_door", None, None, None);

        let domains = collect_service_domains_from_registry(&registries);

        assert!(
            domains.contains("light"),
            "Expected domains to contain 'light', got: {:?}",
            domains
        );
        assert!(
            domains.contains("lock"),
            "Expected domains to contain 'lock', got: {:?}",
            domains
        );
        assert!(
            !domains.contains("sensor"),
            "Expected domains to NOT contain read-only 'sensor', got: {:?}",
            domains
        );
    }

    #[test]
    fn test_collect_service_domains_deduplicates() {
        let temp_dir = TempDir::new().unwrap();
        let registries = Registries::new(temp_dir.path());

        registries.entities.get_or_create(
            "test_platform",
            "light.living_room",
            Some("unique_1"),
            None,
            None,
        );
        registries.entities.get_or_create(
            "test_platform",
            "light.bedroom",
            Some("unique_2"),
            None,
            None,
        );

        let domains = collect_service_domains_from_registry(&registries);

        assert_eq!(
            domains.len(),
            1,
            "Expected exactly 1 unique domain, got {} domains: {:?}",
            domains.len(),
            domains
        );
        assert!(
            domains.contains("light"),
            "Expected domains to contain 'light', got: {:?}",
            domains
        );
    }

    // =========================================================================
    // handle_entity_service_rust tests
    // =========================================================================

    fn create_state_store_with_entity(
        entity_id: &str,
        state: &str,
        attrs: HashMap<String, serde_json::Value>,
    ) -> Arc<StateStore> {
        let bus = Arc::new(EventBus::new());
        let store = Arc::new(StateStore::new(bus));
        let eid: EntityId = entity_id.parse().unwrap();
        store.set(eid, state, attrs, Context::new());
        store
    }

    #[test]
    fn test_handle_entity_service_rust_toggle_light_on_to_off() {
        let store = create_state_store_with_entity("light.test", "on", HashMap::new());
        let result =
            handle_entity_service_rust(&store, "light.test", "toggle", &json!({}), Context::new());
        assert_eq!(result, Some(true));
        assert_eq!(store.get("light.test").unwrap().state, "off");
    }

    #[test]
    fn test_handle_entity_service_rust_toggle_light_off_to_on() {
        let store = create_state_store_with_entity("light.test", "off", HashMap::new());
        let result =
            handle_entity_service_rust(&store, "light.test", "toggle", &json!({}), Context::new());
        assert_eq!(result, Some(true));
        assert_eq!(store.get("light.test").unwrap().state, "on");
    }

    #[test]
    fn test_handle_entity_service_rust_turn_on() {
        let store = create_state_store_with_entity("light.test", "off", HashMap::new());
        let result =
            handle_entity_service_rust(&store, "light.test", "turn_on", &json!({}), Context::new());
        assert_eq!(result, Some(true));
        assert_eq!(store.get("light.test").unwrap().state, "on");
    }

    #[test]
    fn test_handle_entity_service_rust_turn_off() {
        let store = create_state_store_with_entity("light.test", "on", HashMap::new());
        let result = handle_entity_service_rust(
            &store,
            "light.test",
            "turn_off",
            &json!({}),
            Context::new(),
        );
        assert_eq!(result, Some(true));
        assert_eq!(store.get("light.test").unwrap().state, "off");
    }

    #[test]
    fn test_handle_entity_service_rust_turn_on_with_brightness() {
        let mut attrs = HashMap::new();
        attrs.insert("brightness".to_string(), json!(255));
        let store = create_state_store_with_entity("light.test", "off", attrs);
        let result = handle_entity_service_rust(
            &store,
            "light.test",
            "turn_on",
            &json!({"brightness": 128}),
            Context::new(),
        );
        assert_eq!(result, Some(true));
        let state = store.get("light.test").unwrap();
        assert_eq!(state.state, "on");
        assert_eq!(state.attributes.get("brightness"), Some(&json!(128)));
    }

    #[test]
    fn test_handle_entity_service_rust_toggle_lock() {
        let store = create_state_store_with_entity("lock.front_door", "locked", HashMap::new());
        let result = handle_entity_service_rust(
            &store,
            "lock.front_door",
            "toggle",
            &json!({}),
            Context::new(),
        );
        assert_eq!(result, Some(true));
        assert_eq!(store.get("lock.front_door").unwrap().state, "unlocked");
    }

    #[test]
    fn test_handle_entity_service_rust_entity_not_found() {
        let bus = Arc::new(EventBus::new());
        let store = Arc::new(StateStore::new(bus));
        let result = handle_entity_service_rust(
            &store,
            "light.nonexistent",
            "toggle",
            &json!({}),
            Context::new(),
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_handle_entity_service_rust_unsupported_service() {
        let store = create_state_store_with_entity("light.test", "on", HashMap::new());
        let result = handle_entity_service_rust(
            &store,
            "light.test",
            "set_color_temp",
            &json!({}),
            Context::new(),
        );
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_handle_entity_service_rust_preserves_attributes() {
        let mut attrs = HashMap::new();
        attrs.insert("friendly_name".to_string(), json!("My Light"));
        attrs.insert("brightness".to_string(), json!(200));
        let store = create_state_store_with_entity("light.test", "on", attrs);
        let result = handle_entity_service_rust(
            &store,
            "light.test",
            "turn_off",
            &json!({}),
            Context::new(),
        );
        assert_eq!(result, Some(true));
        let state = store.get("light.test").unwrap();
        assert_eq!(state.state, "off");
        assert_eq!(
            state.attributes.get("friendly_name"),
            Some(&json!("My Light"))
        );
    }

    #[test]
    fn test_handle_entity_service_rust_toggle_cover() {
        let store = create_state_store_with_entity("cover.garage", "open", HashMap::new());
        let result = handle_entity_service_rust(
            &store,
            "cover.garage",
            "toggle",
            &json!({}),
            Context::new(),
        );
        assert_eq!(result, Some(true));
        assert_eq!(store.get("cover.garage").unwrap().state, "closed");
    }

    #[test]
    fn test_handle_entity_service_rust_propagates_context() {
        let store = create_state_store_with_entity("light.test", "off", HashMap::new());
        let ctx = Context::new();
        let ctx_id = ctx.id.to_string();
        let result = handle_entity_service_rust(&store, "light.test", "turn_on", &json!({}), ctx);
        assert_eq!(result, Some(true));
        let state = store.get("light.test").unwrap();
        assert_eq!(state.state, "on");
        assert_eq!(
            state.context.id.to_string(),
            ctx_id,
            "State should have the same context as the service call"
        );
    }
}
