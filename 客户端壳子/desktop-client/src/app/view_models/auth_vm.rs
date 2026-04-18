use crate::{
    app::brand::{login_button_text, login_title},
    storage::app_state::AuthPreferences,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthViewModel {
    pub remember_password: bool,
    pub auto_login: bool,
    pub show_totp_field: bool,
}

impl AuthViewModel {
    pub fn for_login(prefs: AuthPreferences, show_totp_field: bool) -> Self {
        let prefs = prefs.sanitized();
        Self {
            remember_password: prefs.remember_password,
            auto_login: prefs.auto_login,
            show_totp_field,
        }
    }

    pub fn title(&self) -> &'static str {
        login_title()
    }

    pub fn primary_action_text(&self) -> &'static str {
        login_button_text()
    }

    pub fn remember_password_label(&self) -> &'static str {
        "记住密码"
    }

    pub fn auto_login_label(&self) -> &'static str {
        "免登录"
    }

    pub fn auto_login_enabled(&self) -> bool {
        self.remember_password
    }

    pub fn auto_login_checked(&self) -> bool {
        self.remember_password && self.auto_login
    }

    pub fn show_password_fields(&self) -> bool {
        true
    }

    pub fn show_totp_field(&self) -> bool {
        self.show_totp_field
    }
}

#[cfg(test)]
mod tests {
    use super::AuthViewModel;
    use crate::storage::app_state::AuthPreferences;

    #[test]
    fn login_surface_state_matches_approved_copy_and_toggle_rules() {
        let state = AuthViewModel::for_login(
            AuthPreferences {
                remember_password: true,
                auto_login: true,
            },
            false,
        );

        assert_eq!(state.title(), "欢迎王者归来");
        assert_eq!(state.primary_action_text(), "登录");
        assert_eq!(state.remember_password_label(), "记住密码");
        assert_eq!(state.auto_login_label(), "免登录");
        assert!(state.show_password_fields());
        assert!(!state.show_totp_field());
    }

    #[test]
    fn auto_login_is_disabled_when_remember_password_is_off() {
        let state = AuthViewModel::for_login(
            AuthPreferences {
                remember_password: false,
                auto_login: true,
            },
            false,
        );

        assert!(!state.auto_login_enabled());
        assert!(!state.auto_login_checked());
    }
}
