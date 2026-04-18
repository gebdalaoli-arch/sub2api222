pub mod api;
pub mod app;
pub mod config;
pub mod platform;
pub mod storage;

#[cfg(test)]
mod tests {
    #[test]
    fn app_bootstrap_exposes_router_module() {
        let router_name = std::any::type_name::<crate::app::router::Route>();
        assert!(router_name.contains("Route"));
    }

    #[test]
    fn slint_copy_does_not_expose_backend_connection_terms() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let ui_files = [
            manifest_dir.join("ui/app-window.slint"),
            manifest_dir.join("ui/components/brand_panel.slint"),
            manifest_dir.join("ui/screens/about.slint"),
            manifest_dir.join("ui/screens/dashboard.slint"),
            manifest_dir.join("ui/screens/forgot_password.slint"),
            manifest_dir.join("ui/screens/launch_panel.slint"),
            manifest_dir.join("ui/screens/login.slint"),
        ];
        let forbidden_terms = ["API Key", "Base URL", "runtime token", "重置 token"];

        for file in ui_files {
            let content = std::fs::read_to_string(&file).unwrap();
            for term in forbidden_terms {
                assert!(
                    !content.contains(term),
                    "{} should not contain user-visible backend term {term}",
                    file.display()
                );
            }
        }
    }

    #[test]
    fn login_shell_copy_matches_approved_brand_language() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let login = std::fs::read_to_string(manifest_dir.join("ui/screens/login.slint")).unwrap();
        let app_window = std::fs::read_to_string(manifest_dir.join("ui/app-window.slint")).unwrap();
        let brand_panel =
            std::fs::read_to_string(manifest_dir.join("ui/components/brand_panel.slint")).unwrap();

        assert!(login.contains("欢迎王者归来"));
        assert!(login.contains("记住密码"));
        assert!(login.contains("免登录"));
        assert!(login.contains("text: \"登录\""));
        assert!(app_window.contains("一键开整"));
        assert!(brand_panel.contains("少折腾，直接开工。"));
        assert!(!login.contains("登录与注册"));
        assert!(!app_window.contains("Sub2API Desktop Client"));
    }
}
