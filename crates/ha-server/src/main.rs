//! Home Assistant Rust Server
//!
//! Main entry point for the Home Assistant Rust implementation.

use anyhow::Result;
use ha_event_bus::EventBus;
use ha_service_registry::ServiceRegistry;
use ha_state_machine::StateMachine;
use std::sync::Arc;
use tracing::{info, Level};
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

        Self { bus, states, services }
    }
}

impl Default for HomeAssistant {
    fn default() -> Self {
        Self::new()
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

    let _hass = HomeAssistant::new();

    info!("Home Assistant initialized");

    // TODO: Load configuration
    // TODO: Start API server
    // TODO: Load integrations
    // TODO: Start automation engine

    info!("Home Assistant is running");

    // Keep the server running
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    Ok(())
}
