//! Home Assistant REST and WebSocket API
//!
//! Implements the Home Assistant REST and WebSocket APIs using axum.
//! Based on: https://developers.home-assistant.io/docs/api/rest
//!           https://developers.home-assistant.io/docs/api/websocket

mod websocket;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use ha_config::CoreConfig;
use ha_core::{Context, EntityId, Event};
use ha_event_bus::EventBus;
use ha_service_registry::ServiceRegistry;
use ha_state_machine::StateMachine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub event_bus: Arc<EventBus>,
    pub state_machine: Arc<StateMachine>,
    pub service_registry: Arc<ServiceRegistry>,
    pub config: Arc<CoreConfig>,
    pub components: Arc<Vec<String>>,
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

    Router::new()
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
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
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
async fn get_services(State(state): State<AppState>) -> Json<Vec<ServiceResponse>> {
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

    Json(responses)
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
async fn get_events() -> Json<Vec<EventTypeResponse>> {
    // Return common event types
    Json(vec![
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
    ])
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn create_test_state() -> AppState {
        let event_bus = Arc::new(EventBus::new());
        let state_machine = Arc::new(StateMachine::new(event_bus.clone()));
        let service_registry = Arc::new(ServiceRegistry::new());
        AppState {
            event_bus,
            state_machine,
            service_registry,
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
}
