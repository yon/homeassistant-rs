//! Home Assistant Authentication API
//!
//! Implements authentication endpoints for the HA frontend.
//! Based on: https://developers.home-assistant.io/docs/auth_api
//!
//! Auth flow:
//! 1. GET /auth/providers - List available auth providers
//! 2. POST /auth/login_flow - Start login flow
//! 3. POST /auth/login_flow/{flow_id} - Submit credentials, get auth code
//! 4. POST /auth/token - Exchange auth code for tokens

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use ulid::Ulid;

/// Default access token expiration (30 minutes)
const ACCESS_TOKEN_EXPIRATION_SECS: u64 = 1800;

/// Auth state shared across all auth endpoints
#[derive(Clone)]
pub struct AuthState {
    inner: Arc<AuthStateInner>,
}

struct AuthStateInner {
    /// Active login flows (flow_id -> LoginFlow)
    login_flows: RwLock<HashMap<String, LoginFlow>>,
    /// Authorization codes awaiting exchange (code -> AuthCode)
    auth_codes: RwLock<HashMap<String, AuthCode>>,
    /// Active refresh tokens (token -> RefreshToken)
    refresh_tokens: RwLock<HashMap<String, RefreshToken>>,
    /// Users in the system
    users: RwLock<HashMap<String, User>>,
    /// Whether onboarding is complete
    onboarded: RwLock<bool>,
}

impl AuthState {
    /// Create a new auth state
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AuthStateInner {
                login_flows: RwLock::new(HashMap::new()),
                auth_codes: RwLock::new(HashMap::new()),
                refresh_tokens: RwLock::new(HashMap::new()),
                users: RwLock::new(HashMap::new()),
                onboarded: RwLock::new(false),
            }),
        }
    }

    /// Create auth state with onboarding already complete and a default user
    pub fn new_onboarded() -> Self {
        // Create default owner user
        let user = User {
            id: Ulid::new().to_string(),
            name: "Owner".to_string(),
            is_owner: true,
            is_active: true,
            credentials: vec![Credential {
                auth_provider_type: "homeassistant".to_string(),
                auth_provider_id: None,
            }],
        };

        // Initialize synchronously via blocking (only during setup)
        let users = HashMap::from([(user.id.clone(), user)]);

        Self {
            inner: Arc::new(AuthStateInner {
                login_flows: RwLock::new(HashMap::new()),
                auth_codes: RwLock::new(HashMap::new()),
                refresh_tokens: RwLock::new(HashMap::new()),
                users: RwLock::new(users),
                onboarded: RwLock::new(true),
            }),
        }
    }

    /// Check if onboarding is complete
    pub async fn is_onboarded(&self) -> bool {
        *self.inner.onboarded.read().await
    }

    /// Set onboarding status
    pub async fn set_onboarded(&self, onboarded: bool) {
        *self.inner.onboarded.write().await = onboarded;
    }

    /// Create a new login flow
    async fn create_login_flow(&self, client_id: String, redirect_uri: String) -> LoginFlow {
        let flow = LoginFlow {
            flow_id: Ulid::new().to_string(),
            handler: ("homeassistant".to_string(), None),
            step_id: "init".to_string(),
            client_id,
            redirect_uri,
        };

        self.inner
            .login_flows
            .write()
            .await
            .insert(flow.flow_id.clone(), flow.clone());

        flow
    }

    /// Get a login flow by ID
    async fn get_login_flow(&self, flow_id: &str) -> Option<LoginFlow> {
        self.inner.login_flows.read().await.get(flow_id).cloned()
    }

    /// Complete a login flow and generate an auth code
    async fn complete_login_flow(
        &self,
        flow_id: &str,
        _username: &str,
        _password: &str,
    ) -> Option<String> {
        // Remove the flow
        let flow = self.inner.login_flows.write().await.remove(flow_id)?;

        // In a real implementation, we'd validate credentials here
        // For now, accept any credentials for development

        // Get or create a user
        let user_id = {
            let users = self.inner.users.read().await;
            if let Some(user) = users.values().next() {
                user.id.clone()
            } else {
                drop(users);
                // Create a default user
                let user = User {
                    id: Ulid::new().to_string(),
                    name: "User".to_string(),
                    is_owner: true,
                    is_active: true,
                    credentials: vec![Credential {
                        auth_provider_type: "homeassistant".to_string(),
                        auth_provider_id: None,
                    }],
                };
                let id = user.id.clone();
                self.inner.users.write().await.insert(id.clone(), user);
                id
            }
        };

        // Generate auth code
        let code = Ulid::new().to_string().to_lowercase();
        tracing::info!(
            "complete_login_flow: generated code={}, client_id={}",
            code,
            flow.client_id
        );

        let auth_code = AuthCode {
            code: code.clone(),
            client_id: flow.client_id,
            user_id,
            created_at: SystemTime::now(),
        };

        self.inner
            .auth_codes
            .write()
            .await
            .insert(code.clone(), auth_code);

        Some(code)
    }

    /// Exchange an auth code for tokens
    async fn exchange_auth_code(&self, code: &str, client_id: &str) -> Option<TokenResponse> {
        tracing::info!("exchange_auth_code: code={}, client_id={}", code, client_id);

        // Remove and validate auth code
        let auth_code = match self.inner.auth_codes.write().await.remove(code) {
            Some(c) => c,
            None => {
                tracing::warn!("exchange_auth_code: code not found");
                // List all codes for debugging
                let codes = self.inner.auth_codes.read().await;
                for (k, _) in codes.iter() {
                    tracing::info!("  stored code: {}", k);
                }
                return None;
            }
        };

        tracing::info!(
            "exchange_auth_code: found code, stored_client_id={}",
            auth_code.client_id
        );

        // Verify client_id matches
        if auth_code.client_id != client_id {
            tracing::warn!(
                "exchange_auth_code: client_id mismatch: stored={}, provided={}",
                auth_code.client_id,
                client_id
            );
            return None;
        }

        // Check code hasn't expired (10 minute lifetime)
        if let Ok(elapsed) = auth_code.created_at.elapsed() {
            if elapsed > Duration::from_secs(600) {
                return None;
            }
        }

        // Create refresh token
        let refresh_token_value = generate_token();
        let jwt_key = generate_token(); // Used to sign access tokens

        let refresh_token = RefreshToken {
            id: Ulid::new().to_string(),
            token: refresh_token_value.clone(),
            user_id: auth_code.user_id,
            client_id: client_id.to_string(),
            jwt_key,
            created_at: SystemTime::now(),
        };

        // Create access token
        let access_token = self.create_access_token(&refresh_token);

        self.inner
            .refresh_tokens
            .write()
            .await
            .insert(refresh_token_value.clone(), refresh_token);

        Some(TokenResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: ACCESS_TOKEN_EXPIRATION_SECS,
            refresh_token: Some(refresh_token_value),
            ha_auth_provider: Some("homeassistant".to_string()),
        })
    }

    /// Refresh an access token
    async fn refresh_access_token(
        &self,
        refresh_token_value: &str,
        _client_id: &str,
    ) -> Option<TokenResponse> {
        let refresh_tokens = self.inner.refresh_tokens.read().await;
        let refresh_token = refresh_tokens.get(refresh_token_value)?;

        let access_token = self.create_access_token(refresh_token);

        Some(TokenResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: ACCESS_TOKEN_EXPIRATION_SECS,
            refresh_token: None, // Don't return refresh token on refresh
            ha_auth_provider: None,
        })
    }

    /// Create an access token from a refresh token
    fn create_access_token(&self, refresh_token: &RefreshToken) -> String {
        // In a real implementation, this would be a JWT signed with jwt_key
        // For now, use a simple format: refresh_token_id.timestamp.signature
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let exp = now + ACCESS_TOKEN_EXPIRATION_SECS;

        // Simple token format for development
        // A real implementation would use jwt crate
        format!(
            "{}:{}:{}",
            refresh_token.id,
            exp,
            &refresh_token.jwt_key[..16]
        )
    }

    /// Validate an access token and return the user ID
    pub async fn validate_access_token(&self, token: &str) -> Option<String> {
        // Parse the simple token format
        let parts: Vec<&str> = token.split(':').collect();
        if parts.len() != 3 {
            return None;
        }

        let refresh_token_id = parts[0];
        let exp: u64 = parts[1].parse().ok()?;

        // Check expiration
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if now > exp {
            return None;
        }

        // Find the refresh token by ID
        let refresh_tokens = self.inner.refresh_tokens.read().await;
        for rt in refresh_tokens.values() {
            if rt.id == refresh_token_id {
                // Verify signature matches
                if parts[2] == &rt.jwt_key[..16] {
                    return Some(rt.user_id.clone());
                }
            }
        }

        None
    }
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

/// A login flow in progress
#[derive(Clone)]
struct LoginFlow {
    flow_id: String,
    handler: (String, Option<String>),
    step_id: String,
    client_id: String,
    #[allow(dead_code)]
    redirect_uri: String,
}

/// An authorization code waiting to be exchanged
struct AuthCode {
    #[allow(dead_code)]
    code: String,
    client_id: String,
    user_id: String,
    created_at: SystemTime,
}

/// A refresh token
struct RefreshToken {
    id: String,
    #[allow(dead_code)]
    token: String,
    user_id: String,
    #[allow(dead_code)]
    client_id: String,
    jwt_key: String,
    #[allow(dead_code)]
    created_at: SystemTime,
}

/// A user in the auth system
#[derive(Clone)]
pub struct User {
    pub id: String,
    pub name: String,
    pub is_owner: bool,
    pub is_active: bool,
    pub credentials: Vec<Credential>,
}

/// A credential linked to a user
#[derive(Clone, Serialize)]
pub struct Credential {
    pub auth_provider_type: String,
    pub auth_provider_id: Option<String>,
}

/// Parse multipart form data into a TokenRequest
fn parse_multipart_form(body: &str) -> Result<TokenRequest, String> {
    let mut grant_type = None;
    let mut client_id = None;
    let mut code = None;
    let mut refresh_token = None;

    // Find the boundary (first line)
    let first_line = body.lines().next().ok_or("Empty body")?;
    let boundary = first_line.trim();

    // Split by boundary
    for part in body.split(boundary) {
        let part = part.trim();
        if part.is_empty() || part == "--" {
            continue;
        }

        // Find the field name
        if let Some(name_start) = part.find("name=\"") {
            let name_start = name_start + 6;
            if let Some(name_end) = part[name_start..].find('"') {
                let name = &part[name_start..name_start + name_end];

                // Find the value (after double newline)
                if let Some(value_start) = part.find("\r\n\r\n") {
                    let value = part[value_start + 4..].trim().trim_end_matches("--");
                    let value = value.trim();

                    match name {
                        "grant_type" => grant_type = Some(value.to_string()),
                        "client_id" => client_id = Some(value.to_string()),
                        "code" => code = Some(value.to_string()),
                        "refresh_token" => refresh_token = Some(value.to_string()),
                        _ => {}
                    }
                } else if let Some(value_start) = part.find("\n\n") {
                    let value = part[value_start + 2..].trim().trim_end_matches("--");
                    let value = value.trim();

                    match name {
                        "grant_type" => grant_type = Some(value.to_string()),
                        "client_id" => client_id = Some(value.to_string()),
                        "code" => code = Some(value.to_string()),
                        "refresh_token" => refresh_token = Some(value.to_string()),
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(TokenRequest {
        grant_type: grant_type.ok_or("missing grant_type")?,
        client_id: client_id.ok_or("missing client_id")?,
        code,
        refresh_token,
    })
}

/// Generate a random token
fn generate_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let hasher = RandomState::new();
    let mut h = hasher.build_hasher();
    h.write_u128(Ulid::new().0);
    h.write_u64(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64,
    );
    format!("{:032x}{:032x}", h.finish(), Ulid::new().0)
}

// =============================================================================
// Request/Response Types
// =============================================================================

/// Auth provider info
#[derive(Serialize)]
pub struct AuthProvider {
    pub name: String,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub provider_type: String,
}

/// Response for GET /auth/providers
#[derive(Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<AuthProvider>,
    pub preselect_remember_me: bool,
}

/// Request for POST /auth/login_flow
#[derive(Deserialize)]
pub struct LoginFlowInitRequest {
    pub client_id: String,
    pub handler: (String, Option<String>),
    pub redirect_uri: String,
    #[serde(rename = "type", default = "default_flow_type")]
    pub flow_type: String,
}

fn default_flow_type() -> String {
    "authorize".to_string()
}

/// Login flow step response
#[derive(Serialize)]
pub struct LoginFlowResponse {
    pub flow_id: String,
    pub handler: (String, Option<String>),
    pub step_id: String,
    #[serde(rename = "type")]
    pub result_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_schema: Option<Vec<DataSchemaItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

/// Data schema item for form fields
#[derive(Serialize)]
pub struct DataSchemaItem {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
}

/// Request for POST /auth/login_flow/{flow_id}
#[derive(Deserialize)]
pub struct LoginFlowStepRequest {
    pub client_id: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Request for POST /auth/token
#[derive(Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub client_id: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

/// Token response
#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ha_auth_provider: Option<String>,
}

/// Error response
#[derive(Serialize)]
pub struct AuthErrorResponse {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_code: Option<String>,
}

// =============================================================================
// Handlers
// =============================================================================

/// GET /auth/providers - List available auth providers
pub async fn get_providers(State(auth): State<AuthState>) -> impl IntoResponse {
    // Check onboarding status
    if !auth.is_onboarded().await {
        return (
            StatusCode::BAD_REQUEST,
            Json(AuthErrorResponse {
                message: "Onboarding not finished".to_string(),
                message_code: Some("onboarding_required".to_string()),
            }),
        )
            .into_response();
    }

    let response = ProvidersResponse {
        providers: vec![AuthProvider {
            name: "Home Assistant Local".to_string(),
            id: None,
            provider_type: "homeassistant".to_string(),
        }],
        preselect_remember_me: true,
    };

    Json(response).into_response()
}

/// POST /auth/login_flow - Start a new login flow
pub async fn create_login_flow(State(auth): State<AuthState>, body: Bytes) -> impl IntoResponse {
    // Parse JSON from body without strict Content-Type checking
    let request: LoginFlowInitRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuthErrorResponse {
                    message: format!("Invalid JSON: {}", e),
                    message_code: Some("invalid_json".to_string()),
                }),
            )
                .into_response();
        }
    };

    let flow = auth
        .create_login_flow(request.client_id, request.redirect_uri)
        .await;

    let response = LoginFlowResponse {
        flow_id: flow.flow_id,
        handler: flow.handler,
        step_id: flow.step_id,
        result_type: "form".to_string(),
        data_schema: Some(vec![
            DataSchemaItem {
                name: "username".to_string(),
                field_type: "string".to_string(),
            },
            DataSchemaItem {
                name: "password".to_string(),
                field_type: "string".to_string(),
            },
        ]),
        errors: Some(HashMap::new()),
        result: None,
    };

    Json(response).into_response()
}

/// POST /auth/login_flow/{flow_id} - Submit login credentials
pub async fn submit_login_flow(
    State(auth): State<AuthState>,
    Path(flow_id): Path<String>,
    body: Bytes,
) -> impl IntoResponse {
    // Parse JSON from body without strict Content-Type checking
    let request: LoginFlowStepRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuthErrorResponse {
                    message: format!("Invalid JSON: {}", e),
                    message_code: Some("invalid_json".to_string()),
                }),
            )
                .into_response();
        }
    };

    // Verify flow exists
    let flow = match auth.get_login_flow(&flow_id).await {
        Some(f) => f,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(AuthErrorResponse {
                    message: "Invalid flow specified".to_string(),
                    message_code: None,
                }),
            )
                .into_response();
        }
    };

    let username = request.username.unwrap_or_default();
    let password = request.password.unwrap_or_default();

    // Complete the login flow
    match auth
        .complete_login_flow(&flow_id, &username, &password)
        .await
    {
        Some(auth_code) => {
            let response = LoginFlowResponse {
                flow_id,
                handler: flow.handler,
                step_id: "init".to_string(),
                result_type: "create_entry".to_string(),
                data_schema: None,
                errors: None,
                result: Some(auth_code),
            };
            Json(response).into_response()
        }
        None => (
            StatusCode::BAD_REQUEST,
            Json(AuthErrorResponse {
                message: "Invalid credentials".to_string(),
                message_code: Some("invalid_auth".to_string()),
            }),
        )
            .into_response(),
    }
}

/// POST /auth/token - Exchange auth code or refresh token for access token
pub async fn get_token(State(auth): State<AuthState>, body: Bytes) -> impl IntoResponse {
    tracing::info!("get_token: received {} bytes", body.len());

    // Parse form data from body without strict Content-Type checking
    let body_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("get_token: invalid UTF-8");
            return (
                StatusCode::BAD_REQUEST,
                Json(AuthErrorResponse {
                    message: "Invalid UTF-8 in request body".to_string(),
                    message_code: None,
                }),
            )
                .into_response();
        }
    };

    // Try to parse as URL-encoded first, then as multipart form data
    let request: TokenRequest = if body_str.starts_with("------") || body_str.starts_with("--") {
        // Multipart form data - parse manually
        match parse_multipart_form(body_str) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("get_token: multipart parse error: {}", e);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(AuthErrorResponse {
                        message: format!("Invalid multipart form data: {}", e),
                        message_code: None,
                    }),
                )
                    .into_response();
            }
        }
    } else {
        // URL-encoded form data
        match serde_urlencoded::from_str(body_str) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("get_token: urlencoded parse error: {}", e);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(AuthErrorResponse {
                        message: format!("Invalid form data: {}", e),
                        message_code: None,
                    }),
                )
                    .into_response();
            }
        }
    };

    tracing::info!(
        "get_token: grant_type={}, client_id={}, code={:?}",
        request.grant_type,
        request.client_id,
        request.code
    );

    match request.grant_type.as_str() {
        "authorization_code" => {
            let code = match request.code {
                Some(c) => c,
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(AuthErrorResponse {
                            message: "Code required for authorization_code grant".to_string(),
                            message_code: None,
                        }),
                    )
                        .into_response();
                }
            };

            match auth.exchange_auth_code(&code, &request.client_id).await {
                Some(tokens) => Json(tokens).into_response(),
                None => (
                    StatusCode::BAD_REQUEST,
                    Json(AuthErrorResponse {
                        message: "Invalid authorization code".to_string(),
                        message_code: None,
                    }),
                )
                    .into_response(),
            }
        }
        "refresh_token" => {
            let refresh_token = match request.refresh_token {
                Some(t) => t,
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(AuthErrorResponse {
                            message: "Refresh token required for refresh_token grant".to_string(),
                            message_code: None,
                        }),
                    )
                        .into_response();
                }
            };

            match auth
                .refresh_access_token(&refresh_token, &request.client_id)
                .await
            {
                Some(tokens) => Json(tokens).into_response(),
                None => (
                    StatusCode::BAD_REQUEST,
                    Json(AuthErrorResponse {
                        message: "Invalid refresh token".to_string(),
                        message_code: None,
                    }),
                )
                    .into_response(),
            }
        }
        _ => (
            StatusCode::BAD_REQUEST,
            Json(AuthErrorResponse {
                message: format!("Unsupported grant type: {}", request.grant_type),
                message_code: None,
            }),
        )
            .into_response(),
    }
}

/// GET /.well-known/oauth-authorization-server - OAuth2 metadata
pub async fn oauth_metadata() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "authorization_endpoint": "/auth/authorize",
        "token_endpoint": "/auth/token",
        "revocation_endpoint": "/auth/revoke",
        "response_types_supported": ["code"],
        "service_documentation": "https://developers.home-assistant.io/docs/auth_api"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auth_state_creation() {
        let state = AuthState::new();
        assert!(!state.is_onboarded().await);

        state.set_onboarded(true).await;
        assert!(state.is_onboarded().await);
    }

    #[tokio::test]
    async fn test_onboarded_state() {
        let state = AuthState::new_onboarded();
        assert!(state.is_onboarded().await);
    }

    #[tokio::test]
    async fn test_login_flow() {
        let state = AuthState::new_onboarded();

        // Create a login flow
        let flow = state
            .create_login_flow("http://localhost:8123/".to_string(), "/".to_string())
            .await;

        assert!(!flow.flow_id.is_empty());
        assert_eq!(flow.step_id, "init");

        // Get the flow
        let retrieved = state.get_login_flow(&flow.flow_id).await;
        assert!(retrieved.is_some());

        // Complete the flow
        let code = state
            .complete_login_flow(&flow.flow_id, "user", "password")
            .await;
        assert!(code.is_some());

        // Flow should be removed
        let retrieved = state.get_login_flow(&flow.flow_id).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_token_exchange() {
        let state = AuthState::new_onboarded();
        let client_id = "http://localhost:8123/";

        // Create and complete a login flow
        let flow = state
            .create_login_flow(client_id.to_string(), "/".to_string())
            .await;

        let code = state
            .complete_login_flow(&flow.flow_id, "user", "password")
            .await
            .unwrap();

        // Exchange code for tokens
        let tokens = state.exchange_auth_code(&code, client_id).await;
        assert!(tokens.is_some());

        let tokens = tokens.unwrap();
        assert!(!tokens.access_token.is_empty());
        assert!(tokens.refresh_token.is_some());
        assert_eq!(tokens.token_type, "Bearer");
        assert_eq!(tokens.expires_in, ACCESS_TOKEN_EXPIRATION_SECS);
    }

    #[tokio::test]
    async fn test_token_refresh() {
        let state = AuthState::new_onboarded();
        let client_id = "http://localhost:8123/";

        // Get initial tokens
        let flow = state
            .create_login_flow(client_id.to_string(), "/".to_string())
            .await;
        let code = state
            .complete_login_flow(&flow.flow_id, "user", "password")
            .await
            .unwrap();
        let tokens = state.exchange_auth_code(&code, client_id).await.unwrap();

        // Refresh the token
        let refresh_token = tokens.refresh_token.unwrap();
        let new_tokens = state.refresh_access_token(&refresh_token, client_id).await;
        assert!(new_tokens.is_some());

        let new_tokens = new_tokens.unwrap();
        assert!(!new_tokens.access_token.is_empty());
        assert!(new_tokens.refresh_token.is_none()); // Should not return new refresh token
    }

    #[tokio::test]
    async fn test_access_token_validation() {
        let state = AuthState::new_onboarded();
        let client_id = "http://localhost:8123/";

        // Get tokens
        let flow = state
            .create_login_flow(client_id.to_string(), "/".to_string())
            .await;
        let code = state
            .complete_login_flow(&flow.flow_id, "user", "password")
            .await
            .unwrap();
        let tokens = state.exchange_auth_code(&code, client_id).await.unwrap();

        // Validate access token
        let user_id = state.validate_access_token(&tokens.access_token).await;
        assert!(user_id.is_some());

        // Invalid token should fail
        let invalid = state.validate_access_token("invalid-token").await;
        assert!(invalid.is_none());
    }
}
