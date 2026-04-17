use crate::api::auth::{Login2FARequest, LoginRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginSubmission {
    Password(LoginRequest),
    TwoFactor(Login2FARequest),
}

pub fn build_login_submission(
    email: &str,
    password: &str,
    verification_code: &str,
    pending_totp_token: Option<&str>,
) -> Result<LoginSubmission, &'static str> {
    if let Some(temp_token) = pending_totp_token.filter(|token| !token.trim().is_empty()) {
        if verification_code.trim().is_empty() {
            return Err("请输入 6 位二步验证码后再继续登录。");
        }
        return Ok(LoginSubmission::TwoFactor(Login2FARequest::new(
            temp_token,
            verification_code.trim(),
        )));
    }

    if email.trim().is_empty() {
        return Err("请输入邮箱地址。");
    }
    if password.is_empty() {
        return Err("请输入密码。");
    }

    Ok(LoginSubmission::Password(LoginRequest::new(
        email.trim(),
        password,
    )))
}

#[cfg(test)]
mod tests {
    use super::{build_login_submission, LoginSubmission};

    #[test]
    fn login_submission_uses_password_login_when_no_pending_totp() {
        let submission = build_login_submission("alice@example.com", "secret", "", None).unwrap();

        match submission {
            LoginSubmission::Password(request) => {
                assert_eq!(request.email, "alice@example.com");
                assert_eq!(request.password, "secret");
            }
            LoginSubmission::TwoFactor(_) => panic!("expected password login"),
        }
    }

    #[test]
    fn login_submission_uses_totp_when_pending_token_exists() {
        let submission =
            build_login_submission("alice@example.com", "secret", "123456", Some("temp-token"))
                .unwrap();

        match submission {
            LoginSubmission::TwoFactor(request) => {
                assert_eq!(request.temp_token, "temp-token");
                assert_eq!(request.totp_code, "123456");
            }
            LoginSubmission::Password(_) => panic!("expected totp submission"),
        }
    }
}
