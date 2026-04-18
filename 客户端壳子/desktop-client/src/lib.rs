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
        let login_scene =
            std::fs::read_to_string(manifest_dir.join("ui/screens/login_scene.slint")).unwrap();
        let app_window = std::fs::read_to_string(manifest_dir.join("ui/app-window.slint")).unwrap();
        let brand_panel =
            std::fs::read_to_string(manifest_dir.join("ui/components/brand_panel.slint")).unwrap();

        assert!(login.contains("账户登录"));
        assert!(login.contains("记住密码"));
        assert!(login.contains("免登录"));
        assert!(login.contains("text: \"登录\""));
        assert!(login_scene.contains("欢迎王者归来"));
        assert!(app_window.contains("一键开整"));
        assert!(brand_panel.contains("极简，极速，极度专注。"));
        assert!(!login.contains("登录与注册"));
        assert!(!app_window.contains("Sub2API Desktop Client"));
        assert!(!login_scene.contains("ETHEREAL"));
        assert!(!app_window.contains("Prism Desktop"));
    }

    #[test]
    fn overview_and_update_shell_copy_match_new_information_architecture() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let overview =
            std::fs::read_to_string(manifest_dir.join("ui/screens/overview.slint")).unwrap();
        let help_detail =
            std::fs::read_to_string(manifest_dir.join("ui/screens/help_detail.slint")).unwrap();
        let update_dialog =
            std::fs::read_to_string(manifest_dir.join("ui/screens/update_dialog.slint")).unwrap();

        assert!(overview.contains("欢迎回来"));
        assert!(overview.contains("启动 Codex"));
        assert!(overview.contains("计费中心"));
        assert!(overview.contains("系统公告与日志"));
        assert!(help_detail.contains("设置与帮助"));
        assert!(help_detail.contains("安全边界"));
        assert!(help_detail.contains("检查更新"));
        assert!(update_dialog.contains("发现新版本"));
        assert!(update_dialog.contains("立即更新"));
    }

    #[test]
    fn launch_and_announcement_shells_match_true_light_direction() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let app_window = std::fs::read_to_string(manifest_dir.join("ui/app-window.slint")).unwrap();
        let launch =
            std::fs::read_to_string(manifest_dir.join("ui/screens/launch_panel.slint")).unwrap();
        let help_detail =
            std::fs::read_to_string(manifest_dir.join("ui/screens/help_detail.slint")).unwrap();

        assert!(app_window.contains("公告中心"));
        assert!(app_window.contains("设置与帮助"));
        assert!(launch.contains("环境状态"));
        assert!(launch.contains("启动方式"));
        assert!(launch.contains("桌面版"));
        assert!(launch.contains("CLI"));
        assert!(launch.contains("启动分组"));
        assert!(help_detail.contains("高级设置"));
        assert!(help_detail.contains("官方模式"));
        assert!(!launch.contains("平台代理模式"));
        assert!(!launch.contains("Environment Status"));
        assert!(!launch.contains("INTERFACE MODE"));
        assert!(!launch.contains("READY"));
    }

    #[test]
    fn desktop_update_copy_guard_uses_new_dialog_labels() {
        let _type_guard = std::any::type_name::<crate::api::update::DesktopUpdateCheckResponse>();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let dialog =
            std::fs::read_to_string(manifest_dir.join("ui/screens/update_dialog.slint")).unwrap();
        let app_window = std::fs::read_to_string(manifest_dir.join("ui/app-window.slint")).unwrap();

        assert!(dialog.contains("发现新版本"));
        assert!(dialog.contains("立即更新"));
        assert!(dialog.contains("稍后"));
        assert!(app_window.contains("update-dialog-visible"));
    }

    #[test]
    fn main_window_does_not_ship_with_dev_preview_shortcuts() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let main_rs = std::fs::read_to_string(manifest_dir.join("src/main.rs")).unwrap();

        assert!(!main_rs.contains("DEV PREVIEW"));
        assert!(!main_rs.contains("preview@ethereal.dev"));
    }
}
