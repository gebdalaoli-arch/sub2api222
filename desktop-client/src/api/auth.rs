use crate::api::account::UserProfile;
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
    pub temp_token: String,
    pub user_email_masked: String,
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

#[cfg(test)]
mod tests {
    use super::{
        AuthResponse, ForgotPasswordRequest, Login2FARequest, LoginRequest, LoginResponse,
        RegisterRequest, ResetPasswordRequest, SendVerifyCodeRequest,
    };
    use serde_json::json;

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
                assert_eq!(totp.temp_token, "temp-login-token");
                assert_eq!(totp.user_email_masked, "a***@example.com");
            }
            LoginResponse::Authenticated(_) => panic!("expected 2FA branch"),
        }

        assert_eq!(
            serde_json::to_value(Login2FARequest::new("temp-login-token", "123456")).unwrap(),
            json!({"temp_token":"temp-login-token","totp_code":"123456"})
        );
    }
}
