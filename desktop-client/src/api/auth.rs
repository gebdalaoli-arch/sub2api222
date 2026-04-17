use crate::api::{account::UserProfile, http::ApiClient};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turnstile_token: Option<String>,
}

impl LoginRequest {
    pub fn new(email: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            password: password.into(),
            turnstile_token: None,
        }
    }

    pub fn with_turnstile_token(mut self, token: impl Into<String>) -> Self {
        self.turnstile_token = Some(token.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Login2FARequest {
    pub temp_token: String,
    pub totp_code: String,
}

impl Login2FARequest {
    pub fn new(temp_token: impl Into<String>, totp_code: impl Into<String>) -> Self {
        Self {
            temp_token: temp_token.into(),
            totp_code: totp_code.into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TotpLoginResponse {
    pub requires_2fa: bool,
    pub temp_token: Option<String>,
    pub user_email_masked: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turnstile_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promo_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitation_code: Option<String>,
}

impl RegisterRequest {
    pub fn new(email: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            password: password.into(),
            verify_code: None,
            turnstile_token: None,
            promo_code: None,
            invitation_code: None,
        }
    }

    pub fn with_verify_code(mut self, code: impl Into<String>) -> Self {
        self.verify_code = Some(code.into());
        self
    }

    pub fn with_invitation_code(mut self, code: impl Into<String>) -> Self {
        self.invitation_code = Some(code.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SendVerifyCodeRequest {
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turnstile_token: Option<String>,
}

impl SendVerifyCodeRequest {
    pub fn new(email: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            turnstile_token: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SendVerifyCodeResponse {
    pub message: String,
    pub countdown: i32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ForgotPasswordRequest {
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turnstile_token: Option<String>,
}

impl ForgotPasswordRequest {
    pub fn new(email: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            turnstile_token: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResetPasswordRequest {
    pub email: String,
    pub token: String,
    pub new_password: String,
}

impl ResetPasswordRequest {
    pub fn new(
        email: impl Into<String>,
        token: impl Into<String>,
        new_password: impl Into<String>,
    ) -> Self {
        Self {
            email: email.into(),
            token: token.into(),
            new_password: new_password.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

impl RefreshTokenRequest {
    pub fn new(refresh_token: impl Into<String>) -> Self {
        Self {
            refresh_token: refresh_token.into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub token_type: String,
    pub user: UserProfile,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum LoginResponse {
    Authenticated(AuthResponse),
    TotpRequired(TotpLoginResponse),
}

pub fn login_blocking(client: &ApiClient, request: &LoginRequest) -> anyhow::Result<LoginResponse> {
    client.post_json_blocking("/auth/login", request)
}

pub fn login_2fa_blocking(
    client: &ApiClient,
    request: &Login2FARequest,
) -> anyhow::Result<AuthResponse> {
    client.post_json_blocking("/auth/login/2fa", request)
}

pub fn register_blocking(
    client: &ApiClient,
    request: &RegisterRequest,
) -> anyhow::Result<AuthResponse> {
    client.post_json_blocking("/auth/register", request)
}

pub fn send_verify_code_blocking(
    client: &ApiClient,
    request: &SendVerifyCodeRequest,
) -> anyhow::Result<SendVerifyCodeResponse> {
    client.post_json_blocking("/auth/send-verify-code", request)
}

pub fn forgot_password_blocking(
    client: &ApiClient,
    request: &ForgotPasswordRequest,
) -> anyhow::Result<StatusMessageResponse> {
    client.post_json_blocking("/auth/forgot-password", request)
}

pub fn reset_password_blocking(
    client: &ApiClient,
    request: &ResetPasswordRequest,
) -> anyhow::Result<StatusMessageResponse> {
    client.post_json_blocking("/auth/reset-password", request)
}

pub fn refresh_token_blocking(
    client: &ApiClient,
    request: &RefreshTokenRequest,
) -> anyhow::Result<AuthTokenPairResponse> {
    client.post_json_blocking("/auth/refresh", request)
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct StatusMessageResponse {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AuthTokenPairResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub token_type: String,
}

#[cfg(test)]
mod tests {
    use super::{
        login_blocking, send_verify_code_blocking, AuthResponse, ForgotPasswordRequest,
        Login2FARequest, LoginRequest, LoginResponse, RefreshTokenRequest, RegisterRequest,
        ResetPasswordRequest, SendVerifyCodeRequest,
    };
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn auth_requests_serialize_to_backend_contract() {
        assert_eq!(
            serde_json::to_value(LoginRequest::new("a@example.com", "secret")).unwrap(),
            json!({"email":"a@example.com","password":"secret"})
        );
        assert_eq!(
            serde_json::to_value(
                RegisterRequest::new("a@example.com", "secret").with_verify_code("123456")
            )
            .unwrap(),
            json!({"email":"a@example.com","password":"secret","verify_code":"123456"})
        );
        assert_eq!(
            serde_json::to_value(SendVerifyCodeRequest::new("a@example.com")).unwrap(),
            json!({"email":"a@example.com"})
        );
        assert_eq!(
            serde_json::to_value(ForgotPasswordRequest::new("a@example.com")).unwrap(),
            json!({"email":"a@example.com"})
        );
        assert_eq!(
            serde_json::to_value(ResetPasswordRequest::new(
                "a@example.com",
                "reset-token",
                "new-secret"
            ))
            .unwrap(),
            json!({"email":"a@example.com","token":"reset-token","new_password":"new-secret"})
        );
        assert_eq!(
            serde_json::to_value(RefreshTokenRequest::new("refresh-123")).unwrap(),
            json!({"refresh_token":"refresh-123"})
        );
    }

    #[test]
    fn auth_response_deserializes_standard_backend_fields() {
        let response: AuthResponse = serde_json::from_value(json!({
            "access_token": "access-123",
            "refresh_token": "refresh-123",
            "expires_in": 3600,
            "token_type": "Bearer",
            "user": {
                "id": 42,
                "username": "alice",
                "email": "alice@example.com",
                "role": "user",
                "balance": 2000,
                "concurrency": 3,
                "status": "active",
                "allowed_groups": [1, 3],
                "balance_notify_enabled": false,
                "balance_notify_threshold": null,
                "balance_notify_extra_emails": [],
                "created_at": "2026-04-18T00:00:00Z",
                "updated_at": "2026-04-18T00:00:00Z",
                "run_mode": "simple"
            }
        }))
        .unwrap();

        assert_eq!(response.access_token, "access-123");
        assert_eq!(response.refresh_token.as_deref(), Some("refresh-123"));
        assert_eq!(response.user.email, "alice@example.com");
        assert_eq!(response.user.run_mode.as_deref(), Some("simple"));
    }

    #[test]
    fn login_response_deserializes_totp_2fa_branch() {
        let response: LoginResponse = serde_json::from_value(json!({
            "requires_2fa": true,
            "temp_token": "temp-login-token",
            "user_email_masked": "a***@example.com"
        }))
        .unwrap();

        match response {
            LoginResponse::TotpRequired(totp) => {
                assert!(totp.requires_2fa);
                assert_eq!(totp.temp_token.as_deref(), Some("temp-login-token"));
                assert_eq!(totp.user_email_masked.as_deref(), Some("a***@example.com"));
            }
            LoginResponse::Authenticated(_) => panic!("expected 2FA branch"),
        }

        assert_eq!(
            serde_json::to_value(Login2FARequest::new("temp-login-token", "123456")).unwrap(),
            json!({"temp_token":"temp-login-token","totp_code":"123456"})
        );
    }

    #[test]
    fn login_blocking_hits_auth_login_and_parses_authenticated_branch() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"access_token\":\"access-123\",\"refresh_token\":\"refresh-123\",\"token_type\":\"Bearer\",\"user\":{\"id\":1,\"email\":\"alice@example.com\",\"username\":\"alice\",\"role\":\"user\",\"balance\":20,\"concurrency\":3,\"status\":\"active\",\"allowed_groups\":null}}}",
        );

        let client = crate::api::http::ApiClient::new(format!("{base_url}/api/v1"));
        let response =
            login_blocking(&client, &LoginRequest::new("alice@example.com", "secret")).unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/auth/login");
        match response {
            LoginResponse::Authenticated(auth) => {
                assert_eq!(auth.access_token, "access-123");
            }
            LoginResponse::TotpRequired(_) => panic!("expected authenticated branch"),
        }
    }

    #[test]
    fn send_verify_code_blocking_hits_verify_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"message\":\"Verification code sent\",\"countdown\":60}}",
        );

        let client = crate::api::http::ApiClient::new(format!("{base_url}/api/v1"));
        let response =
            send_verify_code_blocking(&client, &SendVerifyCodeRequest::new("alice@example.com"))
                .unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/auth/send-verify-code");
        assert_eq!(response.countdown, 60);
    }

    fn spawn_api_server(
        status_line: &'static str,
        body: &'static str,
    ) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (path_tx, path_rx) = mpsc::channel();

        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 2048];
                let size = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..size]).to_string();
                if let Some(line) = request.lines().next() {
                    let mut parts = line.split_whitespace();
                    let _method = parts.next();
                    if let Some(path) = parts.next() {
                        let _ = path_tx.send(path.to_string());
                    }
                }

                let response = format!(
                    "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        (format!("http://{}", address), path_rx)
    }
}
