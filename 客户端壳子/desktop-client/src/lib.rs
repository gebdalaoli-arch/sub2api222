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
        let main_rs = std::fs::read_to_string(manifest_dir.join("src/main.rs")).unwrap();

        assert!(login.contains("账户登录"));
        assert!(login.contains("记住密码"));
        assert!(login.contains("自动登录"));
        assert!(login.contains("root.status-text"));
        assert!(login.contains("登录中..."));
        assert!(login_scene.contains("欢迎王者归来"));
        assert!(login_scene.contains("auth-busy"));
        assert!(app_window.contains("一键开整"));
        assert!(app_window.contains("in-out property <bool> auth-busy"));
        assert!(brand_panel.contains("极简，极速，极度专注。"));
        assert!(main_rs.contains("windows_subsystem = \"windows\""));
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
        let billing_detail =
            std::fs::read_to_string(manifest_dir.join("ui/screens/billing_detail.slint")).unwrap();
        let help_detail =
            std::fs::read_to_string(manifest_dir.join("ui/screens/help_detail.slint")).unwrap();
        let usage_detail =
            std::fs::read_to_string(manifest_dir.join("ui/screens/usage_detail.slint")).unwrap();
        let update_dialog =
            std::fs::read_to_string(manifest_dir.join("ui/screens/update_dialog.slint")).unwrap();

        assert!(overview.contains("欢迎回来"));
        assert!(overview.contains("启动 Codex"));
        assert!(overview.contains("计费中心"));
        assert!(overview.contains("系统公告与日志"));
        assert!(billing_detail.contains("兑换 CDK"));
        assert!(billing_detail.contains("暂未开放"));
        assert!(billing_detail.contains("余额充值"));
        assert!(billing_detail.contains("套餐购买"));
        assert!(billing_detail.contains("订阅明细"));
        assert!(billing_detail.contains("订单明细"));
        assert!(help_detail.contains("设置与帮助"));
        assert!(help_detail.contains("查看使用密钥"));
        assert!(help_detail.contains("输入账户密码"));
        assert!(help_detail.contains("检查更新"));
        assert!(usage_detail.contains("消费明细"));
        assert!(usage_detail.contains("按模型、时间、输入（含缓存输入）和输出（含缓存输出）查看消费记录"));
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
        assert!(app_window.contains("消费明细"));
        assert!(launch.contains("环境状态"));
        assert!(launch.contains("启动方式"));
        assert!(launch.contains("桌面版"));
        assert!(launch.contains("CLI"));
        assert!(launch.contains("客户端专用分组"));
        assert!(launch.contains("桌面客户端当前固定走服务端唯一分组"));
        assert!(help_detail.contains("高级设置"));
        assert!(help_detail.contains("官方模式"));
        assert!(!launch.contains("平台代理模式"));
        assert!(!launch.contains("Environment Status"));
        assert!(!launch.contains("INTERFACE MODE"));
        assert!(!launch.contains("READY"));
        assert!(!launch.contains("ComboBox"));
    }

    #[test]
    fn desktop_shell_routes_help_correctly_and_avoids_duplicate_window_controls() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let app_window = std::fs::read_to_string(manifest_dir.join("ui/app-window.slint")).unwrap();
        let login_scene =
            std::fs::read_to_string(manifest_dir.join("ui/screens/login_scene.slint")).unwrap();
        let billing_detail =
            std::fs::read_to_string(manifest_dir.join("ui/screens/billing_detail.slint")).unwrap();
        let usage_detail =
            std::fs::read_to_string(manifest_dir.join("ui/screens/usage_detail.slint")).unwrap();

        assert!(app_window.contains("open-help-requested => { root.current-section = 4; }"));
        assert!(app_window.contains("announcement-touch"));
        assert!(login_scene.contains("announcement-touch"));
        assert!(!login_scene.contains("// Window: minimize"));
        assert!(!login_scene.contains("// Window: close"));
        assert!(!app_window.contains("if !root.session-active: BrandPanel {"));
        assert!(!app_window.contains("if !root.session-active: LoginScreen {"));
        assert!(!app_window.contains("if !root.session-active && root.auth-subview == 1: RegisterPanel {"));
        assert!(!app_window.contains("if !root.session-active && root.auth-subview == 2: ForgotPasswordScreen {"));
        assert!(billing_detail.contains("future-touch"));
        assert!(usage_detail.contains("property <length> list-height"));
        assert!(!usage_detail.contains("height: parent.height - 384px"));
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
    fn announcement_center_reads_runtime_announcement_state() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let announcements =
            std::fs::read_to_string(manifest_dir.join("ui/screens/announcements.slint")).unwrap();

        assert!(announcements.contains("in property <string> hero-title"));
        assert!(announcements.contains("in property <string> hero-summary"));
        assert!(announcements.contains("in property <[string]> announcement-feed-lines"));
        assert!(announcements.contains("for line[index] in root.announcement-feed-lines"));
    }

    #[test]
    fn main_window_wires_update_download_and_announcement_refresh() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let main_rs = std::fs::read_to_string(manifest_dir.join("src/main.rs")).unwrap();

        assert!(main_rs.contains("list_desktop_announcements_blocking"));
        assert!(main_rs.contains("resolve_desktop_download_url"));
        assert!(main_rs.contains("start_desktop_announcement_refresh"));
    }

    #[test]
    fn main_window_does_not_ship_with_dev_preview_shortcuts() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let main_rs = std::fs::read_to_string(manifest_dir.join("src/main.rs")).unwrap();

        assert!(!main_rs.contains("DEV PREVIEW"));
        assert!(!main_rs.contains("preview@ethereal.dev"));
    }
}
