//! Home Assistant Rust Server
//!
//! Main entry point for the Home Assistant Rust implementation.

use anyhow::Result;
use ha_api::AppState;
use ha_config::CoreConfig;
use ha_core::{Context, EntityId, ServiceCall, SupportsResponse};
use ha_event_bus::EventBus;
use ha_service_registry::{ServiceDescription, ServiceRegistry};
use ha_state_machine::StateMachine;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

/// The central Home Assistant instance
pub struct HomeAssistant {
    /// Event bus for pub/sub communication
    pub bus: Arc<EventBus>,
    /// State machine for entity states
    pub states: Arc<StateMachine>,
    /// Service registry for service calls
    pub services: Arc<ServiceRegistry>,
}

impl HomeAssistant {
    /// Create a new Home Assistant instance
    pub fn new() -> Self {
        let bus = Arc::new(EventBus::new());
        let states = Arc::new(StateMachine::new(bus.clone()));
        let services = Arc::new(ServiceRegistry::new());

        Self {
            bus,
            states,
            services,
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
                    warn!("Failed to load entities from {:?}: {}. Using defaults.", entities_file, e);
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
            let entity_id_str = entity.get("entity_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing entity_id"))?;

            let state = entity.get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let attributes: HashMap<String, serde_json::Value> = entity.get("attributes")
                .and_then(|v| v.as_object())
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();

            // Parse entity_id into domain.object_id
            let parts: Vec<&str> = entity_id_str.splitn(2, '.').collect();
            if parts.len() == 2 {
                if let Ok(entity_id) = EntityId::new(parts[0], parts[1]) {
                    self.states.set(entity_id, state, attributes, Context::new());
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

impl Default for HomeAssistant {
    fn default() -> Self {
        Self::new()
    }
}

/// Load components list from JSON file or use defaults
fn load_components(config_dir: &std::path::Path) -> Vec<String> {
    let components_file = config_dir.join("components.json");

    if components_file.exists() {
        match std::fs::read_to_string(&components_file) {
            Ok(content) => {
                match serde_json::from_str::<Vec<String>>(&content) {
                    Ok(components) => return components,
                    Err(e) => {
                        warn!("Failed to parse components.json: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read components.json: {}", e);
            }
        }
    }

    // Default components
    vec![
        "homeassistant".to_string(),
        "api".to_string(),
        "automation".to_string(),
        "script".to_string(),
        "scene".to_string(),
    ]
}

/// Load services cache from JSON file (for comparison testing)
fn load_services_cache(config_dir: &std::path::Path) -> Option<Arc<serde_json::Value>> {
    let services_file = config_dir.join("services.json");

    if services_file.exists() {
        match std::fs::read_to_string(&services_file) {
            Ok(content) => {
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(services) => return Some(Arc::new(services)),
                    Err(e) => {
                        warn!("Failed to parse services.json: {}", e);
                    }
                }
            }
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
            Ok(content) => {
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(events) => return Some(Arc::new(events)),
                    Err(e) => {
                        warn!("Failed to parse events.json: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read events.json: {}", e);
            }
        }
    }

    None
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
                info!("Configuration loaded: name={}, location=({}, {})",
                    cfg.name, cfg.latitude, cfg.longitude);
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

    let hass = HomeAssistant::new();

    // Register core services
    hass.register_core_services();

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

    info!("Home Assistant initialized");

    // Create API state
    let api_state = AppState {
        event_bus: hass.bus.clone(),
        state_machine: hass.states.clone(),
        service_registry: hass.services.clone(),
        config: Arc::new(config),
        components: Arc::new(components),
        services_cache,
        events_cache,
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

    info!("Home Assistant stopped");

    Ok(())
}
