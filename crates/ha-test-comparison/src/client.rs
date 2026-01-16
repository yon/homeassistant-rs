//! HTTP client for API comparison tests

use reqwest::{header, Client, Response, StatusCode};
use serde_json::Value;
use std::time::Duration;

/// API client that can talk to either Python HA or Rust HA
#[derive(Clone)]
pub struct HaClient {
    client: Client,
    base_url: String,
    token: Option<String>,
    name: String,
}

/// Response from an API call, capturing everything we need to compare
#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub status: StatusCode,
    pub headers: Vec<(String, String)>,
    pub body: Option<Value>,
    pub raw_body: String,
}

impl HaClient {
    /// Create a new client for Python HA
    pub fn python_ha(base_url: &str, token: &str) -> Self {
        Self::new("Python HA", base_url, Some(token.to_string()))
    }

    /// Create a new client for Rust HA
    pub fn rust_ha(base_url: &str, token: Option<&str>) -> Self {
        Self::new("Rust HA", base_url, token.map(String::from))
    }

    fn new(name: &str, base_url: &str, token: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            name: name.to_string(),
        }
    }

    /// Get the client name (for logging)
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Make a GET request
    pub async fn get(&self, path: &str) -> Result<ApiResponse, reqwest::Error> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.get(&url);

        if let Some(ref token) = self.token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", token));
        }

        let response = request.send().await?;
        Self::parse_response(response).await
    }

    /// Make a POST request with JSON body
    pub async fn post(
        &self,
        path: &str,
        body: Option<Value>,
    ) -> Result<ApiResponse, reqwest::Error> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.post(&url);

        if let Some(ref token) = self.token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", token));
        }

        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request.send().await?;
        Self::parse_response(response).await
    }

    /// Make a DELETE request
    pub async fn delete(&self, path: &str) -> Result<ApiResponse, reqwest::Error> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.delete(&url);

        if let Some(ref token) = self.token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", token));
        }

        let response = request.send().await?;
        Self::parse_response(response).await
    }

    async fn parse_response(response: Response) -> Result<ApiResponse, reqwest::Error> {
        let status = response.status();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        let raw_body = response.text().await?;
        let body = serde_json::from_str(&raw_body).ok();

        Ok(ApiResponse {
            status,
            headers,
            body,
            raw_body,
        })
    }

    /// Check if the server is healthy
    pub async fn is_healthy(&self) -> bool {
        match self.get("/api/").await {
            Ok(response) => response.status.is_success(),
            Err(_) => false,
        }
    }

    /// Wait for the server to become healthy
    pub async fn wait_for_healthy(&self, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        let check_interval = Duration::from_secs(2);

        while start.elapsed() < timeout {
            if self.is_healthy().await {
                return true;
            }
            tokio::time::sleep(check_interval).await;
        }

        false
    }
}

impl ApiResponse {
    /// Check if the response indicates success
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Get the body as JSON, panicking if not valid JSON
    pub fn json(&self) -> &Value {
        self.body.as_ref().expect("Response body is not valid JSON")
    }

    /// Get a header value
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}
