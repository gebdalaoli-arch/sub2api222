use anyhow::{anyhow, Result};
use reqwest::{Client, RequestBuilder};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    blocking_client: reqwest::blocking::Client,
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
            blocking_client: reqwest::blocking::Client::builder()
                .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
                .timeout(DEFAULT_REQUEST_TIMEOUT)
                .build()
                .expect("blocking http client"),
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

    pub fn post_json_blocking<TBody, TResponse>(
        &self,
        path: &str,
        body: &TBody,
    ) -> Result<TResponse>
    where
        TBody: Serialize + ?Sized,
        TResponse: DeserializeOwned,
    {
        let response = self
            .blocking_authorize(self.blocking_client.post(self.endpoint(path)))
            .json(body)
            .send()?;
        decode_envelope_response(response)
    }

    pub fn get_json_blocking<TResponse>(&self, path: &str) -> Result<TResponse>
    where
        TResponse: DeserializeOwned,
    {
        let response = self
            .blocking_authorize(self.blocking_client.get(self.endpoint(path)))
            .send()?;
        decode_envelope_response(response)
    }

    pub fn post_empty_blocking<TResponse>(&self, path: &str) -> Result<TResponse>
    where
        TResponse: DeserializeOwned,
    {
        let response = self
            .blocking_authorize(self.blocking_client.post(self.endpoint(path)))
            .send()?;
        decode_envelope_response(response)
    }

    pub fn delete_json_blocking<TResponse>(&self, path: &str) -> Result<TResponse>
    where
        TResponse: DeserializeOwned,
    {
        let response = self
            .blocking_authorize(self.blocking_client.delete(self.endpoint(path)))
            .send()?;
        decode_envelope_response(response)
    }

    fn blocking_authorize(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub code: i32,
    pub message: String,
    pub reason: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.reason {
            Some(reason) => write!(f, "{} ({reason})", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ApiError {}

impl<T> ApiEnvelope<T> {
    pub fn is_success(&self) -> bool {
        self.code == 0
    }

    pub fn into_data(self) -> Result<T> {
        if !self.is_success() {
            return Err(ApiError {
                code: self.code,
                message: self.message,
                reason: self.reason,
                metadata: self.metadata,
            }
            .into());
        }
        self.data
            .ok_or_else(|| anyhow!("missing data in successful API response"))
    }
}

fn decode_envelope_response<T>(response: reqwest::blocking::Response) -> Result<T>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let body = response.text()?;

    match serde_json::from_str::<ApiEnvelope<T>>(&body) {
        Ok(envelope) => envelope.into_data(),
        Err(_) if !status.is_success() => {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                Err(anyhow!("request failed with status {}", status))
            } else {
                Err(anyhow!(
                    "request failed with status {}: {}",
                    status,
                    trimmed
                ))
            }
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiClient, ApiEnvelope, ApiError};
    use crate::api::auth::AuthResponse;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
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

    #[test]
    fn post_json_blocking_unwraps_success_envelope() {
        let base_url = spawn_test_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"message\":\"Verification code sent\",\"countdown\":60}}",
        );
        let client = ApiClient::new(base_url);

        let response: crate::api::auth::SendVerifyCodeResponse = client
            .post_json_blocking(
                "/auth/send-verify-code",
                &json!({ "email": "alice@example.com" }),
            )
            .unwrap();

        assert_eq!(response.message, "Verification code sent");
        assert_eq!(response.countdown, 60);
    }

    #[test]
    fn post_json_blocking_surfaces_backend_error_message() {
        let base_url = spawn_test_server(
            "HTTP/1.1 400 Bad Request",
            "{\"code\":400,\"message\":\"invalid verification code\",\"reason\":\"BAD_CODE\",\"data\":null}",
        );
        let client = ApiClient::new(base_url);

        let error = client
            .post_json_blocking::<_, crate::api::auth::AuthResponse>(
                "/auth/register",
                &json!({ "email": "alice@example.com" }),
            )
            .unwrap_err();

        assert!(error.to_string().contains("invalid verification code"));
    }

    #[test]
    fn post_json_blocking_preserves_reason_and_metadata() {
        let base_url = spawn_test_server(
            "HTTP/1.1 403 Forbidden",
            "{\"code\":403,\"message\":\"subscription required\",\"reason\":\"SUBSCRIPTION_REQUIRED\",\"metadata\":{\"group_id\":\"9\"},\"data\":null}",
        );
        let client = ApiClient::new(base_url);

        let error = client
            .post_json_blocking::<_, crate::api::auth::AuthResponse>(
                "/desktop/sessions",
                &json!({ "group_id": 9 }),
            )
            .unwrap_err();
        let api_error = error.downcast_ref::<ApiError>().unwrap();

        assert_eq!(api_error.reason.as_deref(), Some("SUBSCRIPTION_REQUIRED"));
        assert_eq!(
            api_error
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("group_id"))
                .map(String::as_str),
            Some("9")
        );
    }

    #[test]
    fn post_json_blocking_keeps_plaintext_error_body_for_non_json_failures() {
        let base_url = spawn_test_server("HTTP/1.1 404 Not Found", "404 page not found");
        let client = ApiClient::new(base_url);

        let error = client
            .post_json_blocking::<_, crate::api::auth::AuthResponse>(
                "/desktop/sessions",
                &json!({ "group_id": 9 }),
            )
            .unwrap_err();

        assert!(error.to_string().contains("404 page not found"));
    }

    fn spawn_test_server(status_line: &'static str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer);
                let response = format!(
                    "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{}", address)
    }
}
