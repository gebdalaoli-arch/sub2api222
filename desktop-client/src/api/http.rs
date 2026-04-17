use anyhow::{anyhow, Result};
use reqwest::{Client, RequestBuilder};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
    access_token: Option<String>,
    request_timeout: Duration,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: Client::builder()
                .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
                .timeout(DEFAULT_REQUEST_TIMEOUT)
                .build()
                .expect("http client"),
            base_url: base_url.into(),
            access_token: None,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }

    pub fn with_access_token(mut self, access_token: Option<String>) -> Self {
        self.access_token = access_token;
        self
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        self.authorize(
            self.client
                .post(self.endpoint(path))
                .timeout(self.request_timeout),
        )
    }

    pub fn get(&self, path: &str) -> RequestBuilder {
        self.authorize(
            self.client
                .get(self.endpoint(path))
                .timeout(self.request_timeout),
        )
    }

    fn endpoint(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    fn authorize(&self, request: RequestBuilder) -> RequestBuilder {
        match self.access_token.as_deref() {
            Some(token) if !token.trim().is_empty() => request.bearer_auth(token),
            _ => request,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiEnvelope<T> {
    pub code: i32,
    pub message: String,
    pub reason: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
    pub data: Option<T>,
}

impl<T> ApiEnvelope<T> {
    pub fn is_success(&self) -> bool {
        self.code == 0
    }

    pub fn into_data(self) -> Result<T> {
        if !self.is_success() {
            return Err(anyhow!(self.message));
        }
        self.data
            .ok_or_else(|| anyhow!("missing data in successful API response"))
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiClient, ApiEnvelope};
    use crate::api::auth::AuthResponse;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn post_joins_base_url_and_adds_bearer_token() {
        let request = ApiClient::new("https://sub2api.example/api/v1/")
            .with_access_token(Some("access-123".to_string()))
            .post("/auth/login")
            .build()
            .unwrap();

        assert_eq!(
            request.url().as_str(),
            "https://sub2api.example/api/v1/auth/login"
        );
        assert_eq!(
            request
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer access-123")
        );
    }

    #[test]
    fn requests_have_a_total_timeout() {
        let request = ApiClient::new("https://sub2api.example/api/v1/")
            .post("/auth/login")
            .build()
            .unwrap();

        assert_eq!(request.timeout(), Some(&Duration::from_secs(30)));
    }

    #[test]
    fn api_envelope_unwraps_backend_success_response() {
        let envelope: ApiEnvelope<AuthResponse> = serde_json::from_value(json!({
            "code": 0,
            "message": "success",
            "data": {
                "access_token": "access-123",
                "refresh_token": "refresh-123",
                "expires_in": 3600,
                "token_type": "Bearer",
                "user": {
                    "id": 42,
                    "username": "alice",
                    "email": "alice@example.com",
                    "role": "user",
                    "balance": 20,
                    "concurrency": 3,
                    "status": "active",
                    "allowed_groups": null
                }
            }
        }))
        .unwrap();

        assert!(envelope.is_success());
        assert_eq!(envelope.into_data().unwrap().access_token, "access-123");
    }
}
