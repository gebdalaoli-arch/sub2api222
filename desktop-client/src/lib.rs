pub mod api;
pub mod app;
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
}
