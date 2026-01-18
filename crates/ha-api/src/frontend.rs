//! Frontend serving module
//!
//! Serves the Home Assistant frontend static files and handles
//! template processing for index.html.

use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::path::{Path, PathBuf};
use tower_http::services::ServeDir;
use tracing::debug;

/// Frontend configuration
#[derive(Clone)]
pub struct FrontendConfig {
    /// Path to the frontend files (hass_frontend directory)
    pub frontend_path: PathBuf,
    /// Theme color for the frontend
    pub theme_color: String,
}

impl Default for FrontendConfig {
    fn default() -> Self {
        Self {
            frontend_path: PathBuf::from("/usr/share/hass_frontend"),
            theme_color: "#18BCF2".to_string(),
        }
    }
}

/// Shared state for frontend routes
#[derive(Clone)]
pub struct FrontendState {
    pub config: FrontendConfig,
}

/// Create frontend router
pub fn create_frontend_router(config: FrontendConfig) -> Router {
    let frontend_path = config.frontend_path.clone();
    let state = FrontendState { config };

    // Create service for static files
    let serve_dir = ServeDir::new(&frontend_path);

    Router::new()
        // Index route - serves processed index.html
        .route("/", get(serve_index))
        // Lovelace route
        .route("/lovelace", get(serve_index))
        .route("/lovelace/*path", get(serve_index))
        // Config route
        .route("/config", get(serve_index))
        .route("/config/*path", get(serve_index))
        // Developer tools
        .route("/developer-tools", get(serve_index))
        .route("/developer-tools/*path", get(serve_index))
        // History
        .route("/history", get(serve_index))
        // Logbook
        .route("/logbook", get(serve_index))
        // Map
        .route("/map", get(serve_index))
        // Media browser
        .route("/media-browser", get(serve_index))
        .route("/media-browser/*path", get(serve_index))
        // Profile
        .route("/profile", get(serve_index))
        // Settings pages
        .route("/settings", get(serve_index))
        .route("/settings/*path", get(serve_index))
        // Energy
        .route("/energy", get(serve_index))
        // Todo
        .route("/todo", get(serve_index))
        // Calendar
        .route("/calendar", get(serve_index))
        // Onboarding
        .route("/onboarding", get(serve_onboarding))
        // Auth routes
        .route("/auth/authorize", get(serve_authorize))
        // Manifest
        .route("/manifest.json", get(serve_manifest))
        // Static files - these are served directly
        .nest_service(
            "/frontend_latest",
            ServeDir::new(frontend_path.join("frontend_latest")),
        )
        .nest_service(
            "/frontend_es5",
            ServeDir::new(frontend_path.join("frontend_es5")),
        )
        .nest_service("/static", ServeDir::new(frontend_path.join("static")))
        // Service worker
        .route("/service_worker.js", get(serve_service_worker))
        // Fallback for other static files
        .fallback_service(serve_dir)
        .with_state(state)
}

/// Serve the main index.html with template processing
async fn serve_index(State(state): State<FrontendState>) -> impl IntoResponse {
    serve_html_template(&state.config.frontend_path, "index.html", &state.config).await
}

/// Serve the onboarding page
async fn serve_onboarding(State(state): State<FrontendState>) -> impl IntoResponse {
    serve_html_template(
        &state.config.frontend_path,
        "onboarding.html",
        &state.config,
    )
    .await
}

/// Serve the authorize page
async fn serve_authorize(State(state): State<FrontendState>) -> impl IntoResponse {
    serve_html_template(&state.config.frontend_path, "authorize.html", &state.config).await
}

/// Serve the manifest.json
async fn serve_manifest(State(state): State<FrontendState>) -> impl IntoResponse {
    let manifest_path = state.config.frontend_path.join("manifest.json");

    // Read and return manifest, or generate a default one
    match tokio::fs::read_to_string(&manifest_path).await {
        Ok(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(content))
            .unwrap(),
        Err(_) => {
            // Generate default manifest
            let manifest = serde_json::json!({
                "name": "Home Assistant",
                "short_name": "HA",
                "start_url": "/",
                "display": "standalone",
                "theme_color": state.config.theme_color,
                "background_color": "#FFFFFF",
                "icons": [
                    {
                        "src": "/static/icons/favicon-192x192.png",
                        "sizes": "192x192",
                        "type": "image/png"
                    }
                ]
            });
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(manifest.to_string()))
                .unwrap()
        }
    }
}

/// Serve the service worker
async fn serve_service_worker(State(state): State<FrontendState>) -> impl IntoResponse {
    let sw_path = state.config.frontend_path.join("service_worker.js");

    match tokio::fs::read_to_string(&sw_path).await {
        Ok(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/javascript")
            .header("Service-Worker-Allowed", "/")
            .body(Body::from(content))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Service worker not found"))
            .unwrap(),
    }
}

/// Serve an HTML template with variable substitution
async fn serve_html_template(
    frontend_path: &Path,
    filename: &str,
    config: &FrontendConfig,
) -> Response {
    let file_path = frontend_path.join(filename);

    debug!("Serving HTML template: {:?}", file_path);

    match tokio::fs::read_to_string(&file_path).await {
        Ok(content) => {
            // Process template variables
            let processed = process_template(&content, config);

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(processed))
                .unwrap()
        }
        Err(e) => {
            debug!("Failed to read {}: {}", filename, e);
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(format!(
                    "<html><body><h1>404 Not Found</h1><p>Frontend file not found: {}</p></body></html>",
                    filename
                )))
                .unwrap()
        }
    }
}

/// Process Jinja2-style template variables in HTML content
fn process_template(content: &str, config: &FrontendConfig) -> String {
    content
        // Replace theme color
        .replace("{{ theme_color }}", &config.theme_color)
        // Remove extra_modules loop (we don't have any)
        .replace("{%- for extra_module in extra_modules -%}\n        import(\"{{ extra_module }}\");\n        {%- endfor -%}", "")
        // Remove extra_js_es5 loop
        .replace("{%- for extra_script in extra_js_es5 -%}\n          _ls(\"{{ extra_script }}\");\n          {%- endfor -%}", "")
        // Clean up any remaining Jinja2 template tags we don't handle
        .replace("{%- for extra_module in extra_modules -%}", "")
        .replace("{%- endfor -%}", "")
        .replace("{{ extra_module }}", "")
        .replace("{%- for extra_script in extra_js_es5 -%}", "")
        .replace("{{ extra_script }}", "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::fs;
    use tempfile::TempDir;
    use tower::ServiceExt;

    #[test]
    fn test_process_template_theme_color() {
        let config = FrontendConfig {
            frontend_path: PathBuf::from("/test"),
            theme_color: "#FF0000".to_string(),
        };

        let content = r##"<meta name="theme-color" content="{{ theme_color }}">"##;
        let result = process_template(content, &config);

        assert_eq!(result, r##"<meta name="theme-color" content="#FF0000">"##);
    }

    #[test]
    fn test_process_template_removes_loops() {
        let config = FrontendConfig::default();

        let content = r#"<script>{%- for extra_module in extra_modules -%}
        import("{{ extra_module }}");
        {%- endfor -%}</script>"#;
        let result = process_template(content, &config);

        assert_eq!(result, "<script></script>");
    }

    /// Create a mock frontend directory with test files
    fn create_mock_frontend() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Create index.html with template variable
        fs::write(
            path.join("index.html"),
            r##"<!DOCTYPE html><html><head><meta name="theme-color" content="{{ theme_color }}"></head><body>Test</body></html>"##,
        )
        .unwrap();

        // Create frontend_latest directory with a JS file
        fs::create_dir_all(path.join("frontend_latest")).unwrap();
        fs::write(
            path.join("frontend_latest/app.js"),
            "console.log('frontend');",
        )
        .unwrap();

        // Create static directory with an icon
        fs::create_dir_all(path.join("static/icons")).unwrap();
        fs::write(path.join("static/icons/favicon.ico"), "fake-icon-data").unwrap();

        // Create service_worker.js
        fs::write(path.join("service_worker.js"), "// service worker").unwrap();

        temp_dir
    }

    #[tokio::test]
    async fn test_frontend_serves_index_html() {
        let temp_dir = create_mock_frontend();
        let config = FrontendConfig {
            frontend_path: temp_dir.path().to_path_buf(),
            theme_color: "#18BCF2".to_string(),
        };

        let app = create_frontend_router(config);

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Verify template variable was replaced
        assert!(body_str.contains(r##"content="#18BCF2""##));
        assert!(!body_str.contains("{{ theme_color }}"));
    }

    #[tokio::test]
    async fn test_frontend_serves_static_js() {
        let temp_dir = create_mock_frontend();
        let config = FrontendConfig {
            frontend_path: temp_dir.path().to_path_buf(),
            theme_color: "#18BCF2".to_string(),
        };

        let app = create_frontend_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/frontend_latest/app.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"console.log('frontend');");
    }

    #[tokio::test]
    async fn test_frontend_serves_static_icons() {
        let temp_dir = create_mock_frontend();
        let config = FrontendConfig {
            frontend_path: temp_dir.path().to_path_buf(),
            theme_color: "#18BCF2".to_string(),
        };

        let app = create_frontend_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/static/icons/favicon.ico")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_frontend_serves_service_worker() {
        let temp_dir = create_mock_frontend();
        let config = FrontendConfig {
            frontend_path: temp_dir.path().to_path_buf(),
            theme_color: "#18BCF2".to_string(),
        };

        let app = create_frontend_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/service_worker.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"// service worker");
    }

    #[tokio::test]
    async fn test_frontend_lovelace_routes_serve_index() {
        let temp_dir = create_mock_frontend();
        let config = FrontendConfig {
            frontend_path: temp_dir.path().to_path_buf(),
            theme_color: "#18BCF2".to_string(),
        };

        let app = create_frontend_router(config);

        // Test /lovelace serves index.html
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/lovelace")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.contains("<!DOCTYPE html>"));
    }
}
