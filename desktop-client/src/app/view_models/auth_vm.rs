#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    #[default]
    Login,
    Register,
    ForgotPassword,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AuthViewModel {
    pub mode: AuthMode,
    pub email: String,
    pub password: String,
    pub verification_code: String,
    pub status_text: String,
}

impl AuthViewModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_mode(mode: AuthMode) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }

    pub fn title(&self) -> &'static str {
        match self.mode {
            AuthMode::Login => "登录到客户端",
            AuthMode::Register => "创建账号",
            AuthMode::ForgotPassword => "找回密码",
        }
    }

    pub fn primary_action_text(&self) -> &'static str {
        match self.mode {
            AuthMode::Login => "登录",
            AuthMode::Register => "注册",
            AuthMode::ForgotPassword => "发送重置邮件",
        }
    }

    pub fn needs_password(&self) -> bool {
        !matches!(self.mode, AuthMode::ForgotPassword)
    }

    pub fn needs_verification_code(&self) -> bool {
        matches!(self.mode, AuthMode::Register)
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthMode, AuthViewModel};

    #[test]
    fn auth_view_model_exposes_copy_for_login_register_and_forgot_password() {
        let login = AuthViewModel::for_mode(AuthMode::Login);
        assert_eq!(login.title(), "登录到客户端");
        assert_eq!(login.primary_action_text(), "登录");
        assert!(login.needs_password());
        assert!(!login.needs_verification_code());

        let register = AuthViewModel::for_mode(AuthMode::Register);
        assert_eq!(register.title(), "创建账号");
        assert_eq!(register.primary_action_text(), "注册");
        assert!(register.needs_password());
        assert!(register.needs_verification_code());

        let forgot = AuthViewModel::for_mode(AuthMode::ForgotPassword);
        assert_eq!(forgot.title(), "找回密码");
        assert_eq!(forgot.primary_action_text(), "发送重置邮件");
        assert!(!forgot.needs_password());
        assert!(!forgot.needs_verification_code());
    }
}
