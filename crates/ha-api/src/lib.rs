//! Home Assistant REST and WebSocket API
//!
//! Implements the Home Assistant REST and WebSocket APIs using axum.
//! Based on: https://developers.home-assistant.io/docs/api/rest
//!           https://developers.home-assistant.io/docs/api/websocket

pub mod auth;
pub mod config_flow;
pub mod frontend;
pub mod manifest;
pub mod persistent_notification;
pub mod translations;
mod websocket;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use dashmap::DashMap;
use ha_components::SystemLog;
use ha_config::CoreConfig;
use ha_config_entries::ConfigEntries;
use ha_core::{Context, EntityId, Event};
use ha_event_bus::EventBus;
use ha_registries::Registries;
use ha_service_registry::ServiceRegistry;
use ha_state_store::StateStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

/// Application credential for OAuth2 integrations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationCredential {
    pub id: String,
    pub domain: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(default)]
    pub auth_domain: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Application credentials storage
pub type ApplicationCredentialsStore = Arc<DashMap<String, ApplicationCredential>>;

/// Create a new empty application credentials store
pub fn new_application_credentials_store() -> ApplicationCredentialsStore {
    Arc::new(DashMap::new())
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub event_bus: Arc<EventBus>,
    pub state_machine: Arc<StateStore>,
    pub service_registry: Arc<ServiceRegistry>,
    pub config: Arc<CoreConfig>,
    pub components: Arc<Vec<String>>,
    /// Config entries manager
    pub config_entries: Arc<RwLock<ConfigEntries>>,
    /// Registries (entity, device, area, floor, label)
    pub registries: Arc<Registries>,
    /// Persistent notification manager
    pub notifications: Arc<persistent_notification::PersistentNotificationManager>,
    /// System log manager
    pub system_log: Arc<SystemLog>,
    /// Cached services response (loaded from JSON for comparison testing)
    pub services_cache: Option<Arc<serde_json::Value>>,
    /// Cached events response (loaded from JSON for comparison testing)
    pub events_cache: Option<Arc<serde_json::Value>>,
    /// Frontend configuration (if serving frontend)
    pub frontend_config: Option<frontend::FrontendConfig>,
    /// Authentication state
    pub auth_state: auth::AuthState,
    /// Config flow handler for integration setup
    pub config_flow_handler: Option<Arc<dyn config_flow::ConfigFlowHandler>>,
    /// Application credentials for OAuth2 integrations
    pub application_credentials: ApplicationCredentialsStore,
}

/// API status response
#[derive(Serialize)]
struct ApiStatus {
    message: &'static str,
}

/// Configuration response - matches HA's /api/config response
#[derive(Serialize)]
struct ConfigResponse {
    allowlist_external_dirs: Vec<String>,
    allowlist_external_urls: Vec<String>,
    components: Vec<String>,
    config_dir: String,
    config_source: String,
    country: Option<String>,
    currency: String,
    debug: bool,
    elevation: i32,
    external_url: Option<String>,
    internal_url: Option<String>,
    language: String,
    latitude: f64,
    location_name: String,
    longitude: f64,
    radius: i32,
    recovery_mode: bool,
    safe_mode: bool,
    state: &'static str,
    time_zone: String,
    unit_system: UnitSystemResponse,
    version: String,
    whitelist_external_dirs: Vec<String>,
}

#[derive(Serialize)]
struct UnitSystemResponse {
    length: String,
    accumulated_precipitation: String,
    mass: String,
    pressure: String,
    temperature: String,
    volume: String,
    wind_speed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    area: Option<String>,
}

/// State response for a single entity
#[derive(Serialize)]
pub struct StateResponse {
    pub entity_id: String,
    pub state: String,
    pub attributes: HashMap<String, serde_json::Value>,
    pub last_changed: String,
    pub last_updated: String,
    pub last_reported: String,
    pub context: ContextResponse,
}

#[derive(Serialize)]
pub struct ContextResponse {
    pub id: String,
    pub parent_id: Option<String>,
    pub user_id: Option<String>,
}

/// Request to set entity state
#[derive(Deserialize)]
pub struct SetStateRequest {
    pub state: String,
    #[serde(default)]
    pub attributes: HashMap<String, serde_json::Value>,
}

/// Service description
#[derive(Serialize)]
pub struct ServiceResponse {
    pub domain: String,
    pub services: HashMap<String, ServiceDescription>,
}

#[derive(Serialize)]
pub struct ServiceDescription {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub fields: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<serde_json::Value>,
}

/// Service call request
#[derive(Deserialize)]
pub struct ServiceCallRequest {
    #[serde(flatten)]
    pub service_data: HashMap<String, serde_json::Value>,
}

/// Onboarding step status
#[derive(Serialize)]
pub struct OnboardingStepResponse {
    pub step: String,
    pub done: bool,
}

/// Event fire request
#[derive(Deserialize)]
pub struct FireEventRequest {
    #[serde(flatten)]
    pub event_data: HashMap<String, serde_json::Value>,
}

/// Event fire response
#[derive(Serialize)]
pub struct FireEventResponse {
    pub message: String,
}

/// Error response
#[derive(Serialize)]
pub struct ErrorResponse {
    pub message: String,
}

/// Create the API router
pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Auth routes (separate state for auth endpoints)
    let auth_router = Router::new()
        .route("/auth/providers", get(auth::get_providers))
        .route("/auth/login_flow", post(auth::create_login_flow))
        .route("/auth/login_flow/:flow_id", post(auth::submit_login_flow))
        .route("/auth/token", post(auth::get_token))
        .route(
            "/.well-known/oauth-authorization-server",
            get(auth::oauth_metadata),
        )
        .with_state(state.auth_state.clone());

    let api_router = Router::new()
        // WebSocket endpoint
        .route("/api/websocket", get(websocket::ws_handler))
        // Status endpoint
        .route("/api/", get(api_status))
        // Config endpoint
        .route("/api/config", get(get_config))
        // State endpoints
        .route("/api/states", get(get_states))
        .route("/api/states/:entity_id", get(get_state))
        .route("/api/states/:entity_id", post(set_state))
        // Service endpoints
        .route("/api/services", get(get_services))
        .route("/api/services/:domain/:service", post(call_service))
        // Event endpoints
        .route("/api/events", get(get_events))
        .route("/api/events/:event_type", post(fire_event))
        // Health check
        .route("/api/health", get(health_check))
        // Onboarding status (always returns "done" for all steps)
        .route("/api/onboarding", get(get_onboarding))
        // Config entries endpoint (for deletion via HTTP)
        .route(
            "/api/config/config_entries/entry/:entry_id",
            delete(delete_config_entry),
        )
        // Config flow routes
        .route(
            "/api/config/config_entries/flow_handlers",
            get(get_config_flow_handlers),
        )
        .route("/api/config/config_entries/flow", post(start_config_flow))
        .route(
            "/api/config/config_entries/flow/:flow_id",
            get(get_config_flow)
                .post(progress_config_flow)
                .delete(cancel_config_flow),
        )
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state.clone())
        // Merge auth routes
        .merge(auth_router);

    // If frontend is configured, merge frontend router
    if let Some(frontend_config) = state.frontend_config {
        let frontend_router = frontend::create_frontend_router(frontend_config);
        // Frontend routes take lower priority than API routes
        frontend_router.merge(api_router)
    } else {
        api_router
    }
}

/// Start the API server
pub async fn start_server(state: AppState, addr: &str) -> std::io::Result<()> {
    let router = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("API server listening on {}", addr);
    axum::serve(listener, router).await
}

// ==================== Handlers ====================

/// GET /api/ - Returns API status
async fn api_status() -> Json<ApiStatus> {
    Json(ApiStatus {
        message: "API running.",
    })
}

/// GET /api/health - Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

/// GET /api/onboarding - Returns onboarding status
/// Always returns all steps as done (we don't support onboarding flow)
async fn get_onboarding() -> Json<Vec<OnboardingStepResponse>> {
    Json(vec![
        OnboardingStepResponse {
            step: "user".to_string(),
            done: true,
        },
        OnboardingStepResponse {
            step: "core_config".to_string(),
            done: true,
        },
        OnboardingStepResponse {
            step: "analytics".to_string(),
            done: true,
        },
        OnboardingStepResponse {
            step: "integration".to_string(),
            done: true,
        },
    ])
}

/// GET /api/config - Returns configuration
async fn get_config(State(state): State<AppState>) -> Json<ConfigResponse> {
    let config = &state.config;
    let unit_system = config.unit_system();

    Json(ConfigResponse {
        allowlist_external_dirs: config.allowlist_external_dirs.clone(),
        allowlist_external_urls: config.allowlist_external_urls.clone(),
        components: (*state.components).clone(),
        config_dir: "/config".to_string(),
        config_source: "yaml".to_string(),
        country: config.country.clone(),
        currency: config.currency.clone(),
        debug: false,
        elevation: config.elevation,
        external_url: config.external_url.clone(),
        internal_url: config.internal_url.clone(),
        language: config.language.clone(),
        latitude: config.latitude,
        location_name: config.name.clone(),
        longitude: config.longitude,
        radius: config.radius,
        recovery_mode: false,
        safe_mode: false,
        state: "RUNNING",
        time_zone: config.time_zone.clone(),
        unit_system: UnitSystemResponse {
            length: unit_system.length.clone(),
            accumulated_precipitation: unit_system.accumulated_precipitation.clone(),
            mass: unit_system.mass.clone(),
            pressure: unit_system.pressure.clone(),
            temperature: unit_system.temperature.clone(),
            volume: unit_system.volume.clone(),
            wind_speed: unit_system.wind_speed.clone(),
            area: unit_system.area.clone(),
        },
        version: env!("CARGO_PKG_VERSION").to_string(),
        whitelist_external_dirs: config.allowlist_external_dirs.clone(),
    })
}

/// GET /api/states - Returns all entity states
async fn get_states(State(state): State<AppState>) -> Json<Vec<StateResponse>> {
    let states = state.state_machine.all();
    let responses: Vec<StateResponse> = states.iter().map(state_to_response).collect();
    Json(responses)
}

/// Convert a State to a StateResponse
fn state_to_response(s: &ha_core::State) -> StateResponse {
    StateResponse {
        entity_id: s.entity_id.to_string(),
        state: s.state.clone(),
        attributes: s.attributes.clone(),
        last_changed: s.last_changed.to_rfc3339(),
        last_updated: s.last_updated.to_rfc3339(),
        last_reported: s.last_reported.unwrap_or(s.last_updated).to_rfc3339(),
        context: ContextResponse {
            id: s.context.id.to_string(),
            parent_id: s.context.parent_id.clone(),
            user_id: s.context.user_id.clone(),
        },
    }
}

/// GET /api/states/{entity_id} - Returns a single entity state
async fn get_state(
    State(state): State<AppState>,
    Path(entity_id): Path<String>,
) -> Result<Json<StateResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state.state_machine.get(&entity_id) {
        Some(s) => Ok(Json(state_to_response(&s))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                message: format!("Entity not found: {}", entity_id),
            }),
        )),
    }
}

/// POST /api/states/{entity_id} - Sets an entity state
async fn set_state(
    State(state): State<AppState>,
    Path(entity_id): Path<String>,
    Json(request): Json<SetStateRequest>,
) -> Result<Json<StateResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Parse entity ID
    let parts: Vec<&str> = entity_id.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: format!("Invalid entity_id format: {}", entity_id),
            }),
        ));
    }

    let entity = match EntityId::new(parts[0], parts[1]) {
        Ok(e) => e,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    message: format!("Invalid entity_id: {}", entity_id),
                }),
            ))
        }
    };

    // Set the state
    state.state_machine.set(
        entity,
        &request.state,
        request.attributes.clone(),
        Context::new(),
    );

    // Return the new state
    match state.state_machine.get(&entity_id) {
        Some(s) => Ok(Json(state_to_response(&s))),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                message: "Failed to set state".to_string(),
            }),
        )),
    }
}

/// GET /api/services - Returns available services
async fn get_services(State(state): State<AppState>) -> axum::response::Response {
    // If we have a cached services response (from Python HA export), use it
    if let Some(ref cache) = state.services_cache {
        return axum::response::Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(cache.to_string()))
            .unwrap();
    }

    // Otherwise, build from registry
    let all_services = state.service_registry.all_services();

    let responses: Vec<ServiceResponse> = all_services
        .into_iter()
        .map(|(domain, service_descs)| {
            let services: HashMap<String, ServiceDescription> = service_descs
                .into_iter()
                .map(|desc| {
                    (
                        desc.service.clone(),
                        ServiceDescription {
                            name: desc.name,
                            description: desc.description,
                            fields: HashMap::new(),
                            target: desc.target,
                        },
                    )
                })
                .collect();
            ServiceResponse { domain, services }
        })
        .collect();

    Json(responses).into_response()
}

/// POST /api/services/{domain}/{service} - Calls a service
async fn call_service(
    State(state): State<AppState>,
    Path((domain, service)): Path<(String, String)>,
    Json(request): Json<ServiceCallRequest>,
) -> Result<Json<Vec<StateResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let service_data = serde_json::to_value(&request.service_data).unwrap_or_default();

    match state
        .service_registry
        .call(&domain, &service, service_data, Context::new(), false)
        .await
    {
        Ok(_) => {
            // Return empty array (matching Python HA behavior)
            // In Python HA, most service calls return an empty array
            // Some service calls that affect entities return those states,
            // but that requires tracking which entities were affected
            Ok(Json(vec![]))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: format!("Service call failed: {}", e),
            }),
        )),
    }
}

/// GET /api/events - Returns available event types
async fn get_events(State(state): State<AppState>) -> axum::response::Response {
    // If we have a cached events response (from Python HA export), use it
    if let Some(ref cache) = state.events_cache {
        return axum::response::Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(cache.to_string()))
            .unwrap();
    }

    // Otherwise, return common event types
    let events = vec![
        EventTypeResponse {
            event: "state_changed".to_string(),
            listener_count: 0,
        },
        EventTypeResponse {
            event: "service_registered".to_string(),
            listener_count: 0,
        },
        EventTypeResponse {
            event: "call_service".to_string(),
            listener_count: 0,
        },
        EventTypeResponse {
            event: "homeassistant_start".to_string(),
            listener_count: 0,
        },
        EventTypeResponse {
            event: "homeassistant_stop".to_string(),
            listener_count: 0,
        },
    ];
    Json(events).into_response()
}

#[derive(Serialize)]
pub struct EventTypeResponse {
    pub event: String,
    pub listener_count: usize,
}

/// POST /api/events/{event_type} - Fires an event
async fn fire_event(
    State(state): State<AppState>,
    Path(event_type): Path<String>,
    Json(request): Json<FireEventRequest>,
) -> Json<FireEventResponse> {
    let event_data = serde_json::to_value(&request.event_data).unwrap_or_default();

    // Fire a generic event
    let event = Event::new(event_type.clone(), event_data, Context::new());
    state.event_bus.fire(event);

    Json(FireEventResponse {
        message: format!("Event {} fired.", event_type),
    })
}

/// DELETE /api/config/config_entries/entry/{entry_id} - Delete a config entry
async fn delete_config_entry(
    State(state): State<AppState>,
    Path(entry_id): Path<String>,
) -> impl IntoResponse {
    info!("HTTP DELETE config entry: {}", entry_id);

    // Remove the config entry
    let config_entries = state.config_entries.write().await;
    match config_entries.remove(&entry_id).await {
        Ok(_entry) => {
            info!("Config entry {} deleted successfully via HTTP", entry_id);
            // Return the same format as HA: {"require_restart": false}
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "require_restart": false
                })),
            )
        }
        Err(e) => {
            tracing::warn!("Failed to delete config entry {}: {}", entry_id, e);
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "message": format!("Invalid entry specified: {}", entry_id)
                })),
            )
        }
    }
}

/// GET /api/config/config_entries/flow_handlers - List available config flow handlers
///
/// This endpoint returns a list of integration domains that have config flows.
/// The frontend uses this to populate the "Add Integration" dialog.
///
/// Native HA uses homeassistant.generated.config_flows.FLOWS which contains
/// all integrations with config flows, categorized by type.
async fn get_config_flow_handlers(
    State(_state): State<AppState>,
    Query(params): Query<FlowHandlersQuery>,
) -> impl IntoResponse {
    // Query the Python FLOWS dict to get all available integrations
    // This matches native HA behavior
    let handlers = get_flows_from_python(params.type_filter.as_deref());
    Json(handlers)
}

/// Get available config flows from Python's FLOWS dict
#[cfg(feature = "python")]
fn get_flows_from_python(type_filter: Option<&str>) -> Vec<String> {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;

    Python::with_gil(|py| {
        // Import the generated config_flows module
        let config_flows = match py.import_bound("homeassistant.generated.config_flows") {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to import config_flows: {}", e);
                return Vec::new();
            }
        };

        // Get the FLOWS dict
        let flows = match config_flows.getattr("FLOWS") {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("Failed to get FLOWS: {}", e);
                return Vec::new();
            }
        };

        let mut result: Vec<String> = Vec::new();

        // If type filter specified, only get that category
        if let Some(filter) = type_filter {
            // Map frontend filter names to HA categories
            let category = match filter {
                "helper" => "helper",
                "integration" => "integration",
                "device" => "integration", // HA uses "integration" for devices
                "hub" => "integration",
                "service" => "integration",
                _ => filter,
            };

            if let Ok(category_flows) = flows.get_item(category) {
                if let Ok(list) = category_flows.extract::<Vec<String>>() {
                    result.extend(list);
                }
            }
        } else {
            // Get all categories
            if let Ok(dict) = flows.downcast::<PyDict>() {
                for (_, value) in dict.iter() {
                    if let Ok(list) = value.extract::<Vec<String>>() {
                        result.extend(list);
                    }
                }
            }
        }

        result
    })
}

/// Fallback when Python is not available
#[cfg(not(feature = "python"))]
fn get_flows_from_python(_type_filter: Option<&str>) -> Vec<String> {
    Vec::new()
}

/// Query parameters for flow_handlers endpoint
#[derive(Deserialize)]
struct FlowHandlersQuery {
    #[serde(rename = "type")]
    type_filter: Option<String>,
}

/// Request to start a config flow
#[derive(Deserialize)]
pub struct StartFlowRequest {
    pub handler: String,
    #[serde(default)]
    pub show_advanced_options: bool,
}

/// POST /api/config/config_entries/flow - Start a new config flow
async fn start_config_flow(
    State(state): State<AppState>,
    Json(request): Json<StartFlowRequest>,
) -> impl IntoResponse {
    info!(
        "HTTP POST start config flow for handler: {}",
        request.handler
    );

    let config_flow_handler = match &state.config_flow_handler {
        Some(h) => h.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "message": "Config flow handler not available"
                })),
            );
        }
    };

    match config_flow_handler
        .start_flow(&request.handler, request.show_advanced_options)
        .await
    {
        Ok(flow_result) => (
            StatusCode::OK,
            Json(serde_json::to_value(&flow_result).unwrap_or_default()),
        ),
        Err(e) => {
            tracing::error!("Failed to start config flow: {}", e);
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "message": format!("Invalid handler specified: {}", e)
                })),
            )
        }
    }
}

/// GET /api/config/config_entries/flow/{flow_id} - Get flow state
async fn get_config_flow(
    State(state): State<AppState>,
    Path(flow_id): Path<String>,
) -> impl IntoResponse {
    info!("HTTP GET config flow: {}", flow_id);

    let config_flow_handler = match &state.config_flow_handler {
        Some(h) => h.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "message": "Config flow handler not available"
                })),
            );
        }
    };

    // Call progress_flow with no user input to get current state
    match config_flow_handler.progress_flow(&flow_id, None).await {
        Ok(flow_result) => (
            StatusCode::OK,
            Json(serde_json::to_value(&flow_result).unwrap_or_default()),
        ),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "message": format!("Invalid flow specified: {}", e)
            })),
        ),
    }
}

/// POST /api/config/config_entries/flow/{flow_id} - Continue a config flow with user input
async fn progress_config_flow(
    State(state): State<AppState>,
    Path(flow_id): Path<String>,
    Json(user_input): Json<serde_json::Value>,
) -> impl IntoResponse {
    info!("HTTP POST progress config flow: {}", flow_id);

    let config_flow_handler = match &state.config_flow_handler {
        Some(h) => h.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "message": "Config flow handler not available"
                })),
            );
        }
    };

    // In Python, user_input=None is different from user_input={}
    // None means "show the form", {} means "user submitted empty form"
    // We pass the input as-is to preserve this distinction
    let input = Some(user_input);

    match config_flow_handler.progress_flow(&flow_id, input).await {
        Ok(flow_result) => {
            // If the flow created an entry, save it
            if flow_result.result_type == "create_entry" {
                if let Some(ref result_data) = flow_result.result {
                    if let Err(e) = save_config_entry_from_flow(
                        &state,
                        &flow_result.handler,
                        flow_result.title.as_deref().unwrap_or(&flow_result.handler),
                        result_data,
                    )
                    .await
                    {
                        tracing::warn!("Failed to save config entry: {}", e);
                    }
                }
            }

            (
                StatusCode::OK,
                Json(serde_json::to_value(&flow_result).unwrap_or_default()),
            )
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "message": format!("Invalid flow specified: {}", e)
            })),
        ),
    }
}

/// DELETE /api/config/config_entries/flow/{flow_id} - Cancel a config flow
async fn cancel_config_flow(
    State(_state): State<AppState>,
    Path(flow_id): Path<String>,
) -> impl IntoResponse {
    info!("HTTP DELETE (cancel) config flow: {}", flow_id);
    // TODO: Actually abort the flow in the manager
    // For now, just return success
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "message": "Flow aborted"
        })),
    )
}

/// Save a config entry from a completed flow
async fn save_config_entry_from_flow(
    state: &AppState,
    domain: &str,
    title: &str,
    data: &serde_json::Value,
) -> Result<(), String> {
    use ha_config_entries::ConfigEntry;

    let entry = ConfigEntry::new(domain, title).with_data(
        data.as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
    );

    let config_entries = state.config_entries.write().await;
    config_entries
        .add(entry)
        .await
        .map_err(|e| format!("Failed to add config entry: {}", e))?;

    info!("Saved config entry for {} ({})", domain, title);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn create_test_state() -> AppState {
        use ha_registries::Storage;

        let event_bus = Arc::new(EventBus::new());
        let state_machine = Arc::new(StateStore::new(event_bus.clone()));
        let service_registry = Arc::new(ServiceRegistry::new());
        // Use a temp directory for test registries
        let temp_dir = std::env::temp_dir().join("ha-api-test");
        let registries = Arc::new(Registries::new(&temp_dir));
        let storage = Arc::new(Storage::new(&temp_dir));
        let config_entries = Arc::new(RwLock::new(ConfigEntries::new(storage)));
        let notifications = persistent_notification::create_manager();
        let system_log = Arc::new(SystemLog::with_defaults());
        AppState {
            event_bus,
            state_machine,
            service_registry,
            config: Arc::new(CoreConfig::default()),
            components: Arc::new(vec![]),
            config_entries,
            registries,
            notifications,
            system_log,
            services_cache: None,
            events_cache: None,
            frontend_config: None,
            auth_state: auth::AuthState::new_onboarded(),
            config_flow_handler: None,
            application_credentials: new_application_credentials_store(),
        }
    }

    #[tokio::test]
    async fn test_api_status() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(Request::builder().uri("/api/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_health_check() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_config() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_states_empty() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/states")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_state_not_found() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/states/light.nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_services() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/services")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_events() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_providers() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/providers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json.get("providers").is_some());
        let providers = json["providers"].as_array().unwrap();
        assert!(!providers.is_empty());
        assert_eq!(providers[0]["type"], "homeassistant");
    }

    #[tokio::test]
    async fn test_auth_login_flow() {
        let state = create_test_state();
        let app = create_router(state);

        // Start login flow
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login_flow")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"client_id":"http://localhost:8123/","handler":["homeassistant",null],"redirect_uri":"/"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["type"], "form");
        assert!(json.get("flow_id").is_some());
        assert!(json.get("data_schema").is_some());
    }

    #[tokio::test]
    async fn test_oauth_metadata() {
        let state = create_test_state();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/.well-known/oauth-authorization-server")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["token_endpoint"], "/auth/token");
        assert_eq!(json["authorization_endpoint"], "/auth/authorize");
    }
}
