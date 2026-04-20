#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

slint::include_modules!();

use slint::{ModelRc, SharedString, VecModel};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use sub2api_desktop::{
    api::{
        account::{fetch_current_user_blocking, UserProfile},
        auth::{
            forgot_password_blocking, login_2fa_blocking, login_blocking, refresh_token_blocking,
            register_blocking, reset_password_blocking, send_verify_code_blocking, AuthResponse,
            LoginResponse, RefreshTokenRequest, RegisterRequest, ResetPasswordRequest,
            SendVerifyCodeRequest,
        },
        billing_summary::{fetch_billing_summary_blocking, BillingSummary},
        desktop_sessions::{
            create_desktop_session_blocking, refresh_desktop_session_blocking,
            revoke_desktop_session_blocking, DesktopSessionCreateRequest, DesktopSessionTarget,
        },
        groups::{fetch_available_groups_blocking, GroupPlatform, GroupSummary},
        http::ApiClient,
        keys::{create_api_key_blocking, CreateAPIKeyRequest},
        payment::{
            create_order_blocking, fetch_checkout_info_blocking, fetch_my_orders_blocking,
            fetch_order_blocking, CheckoutInfo, CreateOrderRequest, PaymentOrder, SubscriptionPlan,
        },
        redeem::{
            fetch_redeem_history_blocking, redeem_code_blocking, RedeemCodeRequest,
            RedeemHistoryItem,
        },
        subscriptions::{fetch_subscription_summary_blocking, SubscriptionSummary},
        update::{
            check_desktop_update_blocking, list_desktop_announcements_blocking,
            resolve_desktop_download_url, DesktopAnnouncementItem, DesktopUpdateCheckResponse,
        },
        usage::{fetch_usage_logs_blocking, PaginatedUsageLogs, UsageQuery},
    },
    app::{
        auth_flow::{build_login_submission, should_restore_session, LoginSubmission},
        launch_errors::describe_platform_launch_error,
        view_models::{
            billing_vm::{format_token_count, BillingViewModel},
            dashboard_vm::DashboardViewModel,
            launch_vm::LaunchViewModel,
            update_vm::{UpdateDialogState, UpdateViewModel},
            usage_vm::UsageDetailViewModel,
        },
    },
    config::{app_config, AppConfig},
    platform::{
        install_detection::{detect_installed_targets, InstalledTarget, LaunchTarget},
        launcher::{
            launch_official, launch_platform, requires_user_home_injection,
            validate_platform_launch_target, windows_store_desktop_is_running_for_launch,
        },
        managed_home::{
            backup_user_codex_config, cleanup_runtime_roots_older_than,
            inject_platform_config_into_user_home, resolve_user_codex_home,
            restore_user_codex_config, write_platform_home, write_runtime_metadata, ManagedHomePaths,
        },
        runtime_bootstrap::StartupDiagnostics,
        session_manager::{
            get_session_token_stats, list_session_groups, list_session_homes, list_trashed_sessions,
            move_sessions_to_trash, repair_session_visibility, restore_sessions_from_trash,
            sync_sessions_across_homes, SessionGroup, SessionHome, SessionTokenStats,
            TrashedSessionRecord,
        },
    },
    storage::{
        app_state::{AppStateStore, AuthPreferences},
        secure_store::{RefreshTokenStore, SystemCredentialStore},
    },
};

#[derive(Debug, Clone)]
struct AuthSession {
    access_token: String,
    user: UserProfile,
}

type SharedAuthSession = Arc<Mutex<Option<AuthSession>>>;
type SharedGroups = Arc<Mutex<Vec<GroupSummary>>>;
type SharedBillingSummary = Arc<Mutex<Option<BillingSummary>>>;
type SharedSubscriptionSummary = Arc<Mutex<Option<SubscriptionSummary>>>;
type SharedRedeemHistory = Arc<Mutex<Vec<RedeemHistoryItem>>>;
type SharedOrders = Arc<Mutex<Vec<PaymentOrder>>>;
type SharedCheckoutInfo = Arc<Mutex<Option<CheckoutInfo>>>;
type SharedPendingPayment = Arc<Mutex<Option<PendingPaymentState>>>;
type SharedUsagePage = Arc<Mutex<Option<PaginatedUsageLogs>>>;
type SharedUsageQuery = Arc<Mutex<UsageQueryState>>;
type SharedSessionManager = Arc<Mutex<SessionManagerState>>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingPaymentState {
    order_id: i64,
    open_target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UsageQueryState {
    page: i32,
    page_size: i32,
    view_mode: UsageViewMode,
}

#[derive(Debug, Clone, Default)]
struct SessionManagerState {
    homes: Vec<SessionHome>,
    groups: Vec<SessionGroup>,
    token_stats: HashMap<String, SessionTokenStats>,
    trash: Vec<TrashedSessionRecord>,
    selected_group_index: i32,
    selected_session_id: Option<String>,
    selected_trash_id: Option<String>,
    status_text: String,
}

impl Default for UsageQueryState {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 4,
            view_mode: UsageViewMode::ByTime,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UsageViewMode {
    ByTime,
    ByModel,
}

impl UsageViewMode {
    fn options() -> Vec<&'static str> {
        vec!["按时间", "按模型"]
    }

    fn from_index(index: i32) -> Self {
        match index {
            1 => Self::ByModel,
            _ => Self::ByTime,
        }
    }

    fn to_index(self) -> i32 {
        match self {
            Self::ByTime => 0,
            Self::ByModel => 1,
        }
    }

    fn sort_by(self) -> &'static str {
        match self {
            Self::ByTime => "created_at",
            Self::ByModel => "model",
        }
    }

    fn sort_order(self) -> &'static str {
        match self {
            Self::ByTime => "desc",
            Self::ByModel => "asc",
        }
    }
}

fn main() -> anyhow::Result<()> {
    let startup_diagnostics = StartupDiagnostics::initialize();
    startup_diagnostics.log("creating main window");
    let app = match AppWindow::new() {
        Ok(app) => {
            startup_diagnostics.log("main window created");
            app
        }
        Err(error) => {
            startup_diagnostics.log(format!("AppWindow::new failed: {error}"));
            return Err(error.into());
        }
    };
    let config = Arc::new(app_config());
    let app_state = AppStateStore::default_for_app().unwrap_or_else(|_| {
        AppStateStore::new(std::env::temp_dir().join("sub2api-desktop-client"))
    });
    let token_store = SystemCredentialStore::new("primary-auth");
    let targets = Rc::new(RefCell::new(detect_installed_targets()));
    let auth_session: SharedAuthSession = Arc::new(Mutex::new(None));
    let pending_totp_token = Arc::new(Mutex::new(None::<String>));
    let available_groups: SharedGroups = Arc::new(Mutex::new(Vec::new()));
    let billing_summary: SharedBillingSummary = Arc::new(Mutex::new(None));
    let subscription_summary: SharedSubscriptionSummary = Arc::new(Mutex::new(None));
    let redeem_history: SharedRedeemHistory = Arc::new(Mutex::new(Vec::new()));
    let recent_orders: SharedOrders = Arc::new(Mutex::new(Vec::new()));
    let checkout_info: SharedCheckoutInfo = Arc::new(Mutex::new(None));
    let pending_payment: SharedPendingPayment = Arc::new(Mutex::new(None));
    let usage_page: SharedUsagePage = Arc::new(Mutex::new(None));
    let usage_query: SharedUsageQuery = Arc::new(Mutex::new(UsageQueryState::default()));
    let session_manager: SharedSessionManager = Arc::new(Mutex::new(SessionManagerState {
        status_text: "本地会话会显示在这里。".to_string(),
        ..SessionManagerState::default()
    }));

    apply_launch_state(&app, &targets.borrow());
    apply_logged_out_state(&app);
    preload_local_state(&app, &app_state, &token_store);
    refresh_session_manager_state(&app_state, &session_manager);
    apply_session_manager_state(&app, &current_session_manager_vm(&session_manager));
    let _ = cleanup_runtime_roots_older_than(
        &app_state.root().join("runtime"),
        Duration::from_secs(60 * 60 * 12),
    );

    wire_launch_callbacks(&app, Rc::clone(&targets));
    wire_platform_launch_callbacks(
        &app,
        Rc::clone(&targets),
        Arc::clone(&config),
        app_state.clone(),
        Arc::clone(&auth_session),
        Arc::clone(&available_groups),
        token_store.clone(),
    );
    wire_auth_callbacks(
        &app,
        Arc::clone(&config),
        app_state.clone(),
        token_store.clone(),
        Arc::clone(&auth_session),
        Arc::clone(&pending_totp_token),
        Arc::clone(&available_groups),
        Arc::clone(&billing_summary),
        Arc::clone(&subscription_summary),
        Arc::clone(&redeem_history),
        Arc::clone(&recent_orders),
        Arc::clone(&checkout_info),
        Arc::clone(&pending_payment),
        Arc::clone(&usage_page),
        Arc::clone(&usage_query),
    );
    wire_billing_callbacks(
        &app,
        Arc::clone(&config),
        Arc::clone(&auth_session),
        Arc::clone(&available_groups),
        Arc::clone(&billing_summary),
        Arc::clone(&subscription_summary),
        Arc::clone(&redeem_history),
        Arc::clone(&recent_orders),
        Arc::clone(&checkout_info),
        Arc::clone(&pending_payment),
        Arc::clone(&usage_page),
        Arc::clone(&usage_query),
    );
    wire_usage_callbacks(
        &app,
        Arc::clone(&config),
        Arc::clone(&auth_session),
        Arc::clone(&available_groups),
        Arc::clone(&subscription_summary),
        Arc::clone(&redeem_history),
        Arc::clone(&recent_orders),
        Arc::clone(&checkout_info),
        Arc::clone(&usage_page),
        Arc::clone(&usage_query),
    );
    wire_use_key_callbacks(
        &app,
        Arc::clone(&config),
        Arc::clone(&auth_session),
        Arc::clone(&available_groups),
    );
    wire_session_manager_callbacks(&app, app_state.clone(), Arc::clone(&session_manager));
    wire_update_callbacks(&app, Arc::clone(&config));
    restore_saved_session(
        &app,
        Arc::clone(&config),
        app_state.clone(),
        token_store,
        Arc::clone(&auth_session),
        Arc::clone(&pending_totp_token),
        Arc::clone(&available_groups),
        Arc::clone(&billing_summary),
        Arc::clone(&subscription_summary),
        Arc::clone(&redeem_history),
        Arc::clone(&recent_orders),
        Arc::clone(&checkout_info),
        Arc::clone(&pending_payment),
        Arc::clone(&usage_page),
        Arc::clone(&usage_query),
    );

    startup_diagnostics.log(format!(
        "startup diagnostics log path: {}",
        startup_diagnostics.log_path().display()
    ));
    startup_diagnostics.log("entering Slint event loop");
    match app.run() {
        Ok(()) => {
            startup_diagnostics.log("Slint event loop exited normally");
            Ok(())
        }
        Err(error) => {
            startup_diagnostics.log(format!("app.run failed: {error}"));
            Err(error.into())
        }
    }
}

fn preload_local_state(
    app: &AppWindow,
    app_state: &AppStateStore,
    token_store: &SystemCredentialStore,
) {
    if let Ok(Some(email)) = app_state.load_last_email() {
        app.set_email(SharedString::from(email));
    }
    let prefs = app_state
        .load_auth_preferences()
        .ok()
        .flatten()
        .unwrap_or_default();
    app.set_remember_password(prefs.remember_password);
    app.set_auto_login(prefs.auto_login);
    if prefs.remember_password {
        if let Ok(Some(password)) = token_store.load_password() {
            app.set_password(SharedString::from(password));
        }
    }
}

fn wire_launch_callbacks(app: &AppWindow, targets: Rc<RefCell<Vec<InstalledTarget>>>) {
    let refresh_targets = Rc::clone(&targets);
    let refresh_app = app.as_weak();
    app.on_refresh_installations_requested(move || {
        let refreshed = detect_installed_targets();
        *refresh_targets.borrow_mut() = refreshed;
        if let Some(app) = refresh_app.upgrade() {
            apply_launch_state(&app, &refresh_targets.borrow());
        }
    });

    let desktop_targets = Rc::clone(&targets);
    let desktop_app = app.as_weak();
    app.on_launch_desktop_requested(move || {
        if let Some(app) = desktop_app.upgrade() {
            launch_first_target(&app, &desktop_targets.borrow(), LaunchTarget::Desktop);
        }
    });

    let cli_targets = Rc::clone(&targets);
    let cli_app = app.as_weak();
    app.on_launch_cli_requested(move || {
        if let Some(app) = cli_app.upgrade() {
            launch_first_target(&app, &cli_targets.borrow(), LaunchTarget::Cli);
        }
    });
}

fn wire_platform_launch_callbacks(
    app: &AppWindow,
    targets: Rc<RefCell<Vec<InstalledTarget>>>,
    config: Arc<AppConfig>,
    app_state: AppStateStore,
    auth_session: SharedAuthSession,
    available_groups: SharedGroups,
    token_store: SystemCredentialStore,
) {
    let platform_desktop_app = app.as_weak();
    let platform_desktop_targets = Rc::clone(&targets);
    let platform_desktop_config = Arc::clone(&config);
    let platform_desktop_session = Arc::clone(&auth_session);
    let platform_desktop_groups = Arc::clone(&available_groups);
    let platform_desktop_state = app_state.clone();
    let platform_desktop_token_store = token_store.clone();
    app.on_launch_platform_desktop_requested(move || {
        start_platform_launch(
            &platform_desktop_app,
            platform_desktop_targets.borrow().clone(),
            Arc::clone(&platform_desktop_config),
            platform_desktop_state.clone(),
            Arc::clone(&platform_desktop_session),
            Arc::clone(&platform_desktop_groups),
            platform_desktop_token_store.clone(),
            LaunchTarget::Desktop,
        );
    });

    let platform_cli_app = app.as_weak();
    let platform_cli_targets = Rc::clone(&targets);
    let platform_cli_config = Arc::clone(&config);
    let platform_cli_session = Arc::clone(&auth_session);
    let platform_cli_groups = Arc::clone(&available_groups);
    let platform_cli_token_store = token_store.clone();
    app.on_launch_platform_cli_requested(move || {
        start_platform_launch(
            &platform_cli_app,
            platform_cli_targets.borrow().clone(),
            Arc::clone(&platform_cli_config),
            app_state.clone(),
            Arc::clone(&platform_cli_session),
            Arc::clone(&platform_cli_groups),
            platform_cli_token_store.clone(),
            LaunchTarget::Cli,
        );
    });
}

fn start_platform_launch(
    app_handle: &slint::Weak<AppWindow>,
    installed_targets: Vec<InstalledTarget>,
    config: Arc<AppConfig>,
    app_state: AppStateStore,
    auth_session: SharedAuthSession,
    available_groups: SharedGroups,
    token_store: SystemCredentialStore,
    target_kind: LaunchTarget,
) {
    let Some(app) = app_handle.upgrade() else {
        return;
    };
    app.set_launch_status_text(SharedString::from("正在创建平台代理会话..."));

    let ui_handle = app_handle.clone();
    thread::spawn(move || {
        let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                app.set_launch_status_text(SharedString::from(
                    "请先登录并加载桌面客户端专用分组，再启动平台代理模式。",
                ))
            });
            return;
        };

        let groups = platform_launch_groups(&current_groups_snapshot(&available_groups));
        let Some(group) = groups.first().cloned() else {
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                app.set_launch_status_text(SharedString::from(
                    "当前没有可用于桌面客户端的 OpenAI 分组，请先检查服务端分组配置。",
                ))
            });
            return;
        };

        let Some(target) = installed_targets
            .iter()
            .find(|target| target.kind == target_kind)
            .cloned()
        else {
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                app.set_launch_status_text(SharedString::from(
                    "未检测到对应的 Codex 目标，无法创建平台代理模式。",
                ))
            });
            return;
        };

        if let Err(error) = validate_platform_launch_target(&target) {
            let message = describe_platform_launch_error(&error.to_string());
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                app.set_launch_status_text(SharedString::from(message))
            });
            return;
        }

        let api_client = ApiClient::new(config.api_base_url.clone())
            .with_access_token(Some(session.access_token));
        let session_request = DesktopSessionCreateRequest {
            target: match target_kind {
                LaunchTarget::Desktop => DesktopSessionTarget::Desktop,
                LaunchTarget::Cli => DesktopSessionTarget::Cli,
            },
            group_id: group.id,
            device_id: local_device_id(target_kind),
            device_name: local_device_name(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
        };

        match create_desktop_session_blocking(&api_client, &session_request) {
            Ok(platform_session) => {
                let Some(runtime_token) = platform_session.runtime_token.as_deref() else {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_launch_status_text(SharedString::from(
                            "平台会话缺少启动凭据，无法继续启动。",
                        ))
                    });
                    return;
                };

                let runtime_root = app_state
                    .root()
                    .join("runtime")
                    .join(&platform_session.session_id);
                let paths = ManagedHomePaths::new(runtime_root, &platform_session.profile_key);
                let gateway_url = platform_session.gateway_url(&config.api_base_url);

                let uses_user_home_injection = requires_user_home_injection(&target);
                let target_label = target.display_name.clone();
                let result = if uses_user_home_injection {
                    resolve_user_codex_home().and_then(|user_home| {
                        backup_user_codex_config(&user_home)?;
                        inject_platform_config_into_user_home(
                            &user_home,
                            &gateway_url,
                            runtime_token,
                        )?;
                        write_runtime_metadata(
                            &paths,
                            &platform_session.session_id,
                            &platform_session.profile_key,
                            "desktop",
                        )?;
                        match launch_platform(&target, &user_home) {
                            Ok(child) => Ok((child, Some(user_home))),
                            Err(error) => {
                                let _ = restore_user_codex_config(&user_home);
                                Err(error)
                            }
                        }
                    })
                } else {
                    write_platform_home(&paths, &gateway_url, runtime_token)
                        .and_then(|_| {
                            write_runtime_metadata(
                                &paths,
                                &platform_session.session_id,
                                &platform_session.profile_key,
                                match target_kind {
                                    LaunchTarget::Desktop => "desktop",
                                    LaunchTarget::Cli => "cli",
                                },
                            )
                        })
                        .and_then(|_| launch_platform(&target, &paths.codex_home))
                        .map(|child| (child, None))
                };

                match result {
                    Ok((maybe_child, injected_home)) => {
                        let message = format!(
                            "已为分组“{}”创建平台代理会话，并启动 {}。",
                            group.name, target_label
                        );
                        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
                        spawn_platform_refresh_loop(
                            ui_handle.clone(),
                            Arc::clone(&config),
                            Arc::clone(&auth_session),
                            token_store.clone(),
                            platform_session.session_id.clone(),
                            platform_session.refresh_after_duration(),
                            Arc::clone(&stop_flag),
                        );
                        if let Some(child) = maybe_child {
                            spawn_platform_exit_watcher(
                                ui_handle.clone(),
                                Arc::clone(&config),
                                Arc::clone(&auth_session),
                                token_store.clone(),
                                platform_session.session_id.clone(),
                                paths.root.clone(),
                                injected_home,
                                stop_flag,
                                child,
                            );
                        } else if let Some(user_home) = injected_home {
                            spawn_windows_store_exit_watcher(
                                ui_handle.clone(),
                                Arc::clone(&config),
                                Arc::clone(&auth_session),
                                token_store.clone(),
                                platform_session.session_id.clone(),
                                paths.root.clone(),
                                user_home,
                                stop_flag,
                            );
                        }
                        let _ = ui_handle.upgrade_in_event_loop(move |app| {
                            app.set_launch_status_text(SharedString::from(message))
                        });
                    }
                    Err(error) => {
                        let message = describe_platform_launch_error(&error.to_string());
                        let _ = ui_handle.upgrade_in_event_loop(move |app| {
                            app.set_launch_status_text(SharedString::from(message))
                        });
                    }
                }
            }
            Err(error) => {
                let message = describe_platform_launch_error(&error.to_string());
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_launch_status_text(SharedString::from(message))
                });
            }
        }
    });
}

fn wire_auth_callbacks(
    app: &AppWindow,
    config: Arc<AppConfig>,
    app_state: AppStateStore,
    token_store: SystemCredentialStore,
    auth_session: SharedAuthSession,
    pending_totp_token: Arc<Mutex<Option<String>>>,
    available_groups: SharedGroups,
    billing_summary: SharedBillingSummary,
    subscription_summary: SharedSubscriptionSummary,
    redeem_history: SharedRedeemHistory,
    recent_orders: SharedOrders,
    checkout_info: SharedCheckoutInfo,
    pending_payment: SharedPendingPayment,
    usage_page: SharedUsagePage,
    usage_query: SharedUsageQuery,
) {
    let login_app = app.as_weak();
    let login_config = Arc::clone(&config);
    let login_state = app_state.clone();
    let login_store = token_store.clone();
    let login_session = Arc::clone(&auth_session);
    let login_totp = Arc::clone(&pending_totp_token);
    let login_groups = Arc::clone(&available_groups);
    let login_billing_summary = Arc::clone(&billing_summary);
    let login_summary = Arc::clone(&subscription_summary);
    let login_history = Arc::clone(&redeem_history);
    let login_orders = Arc::clone(&recent_orders);
    let login_checkout = Arc::clone(&checkout_info);
    let login_pending_payment = Arc::clone(&pending_payment);
    let login_usage_page = Arc::clone(&usage_page);
    let login_usage_query = Arc::clone(&usage_query);
    app.on_login_requested(move || {
        let Some(app) = login_app.upgrade() else {
            return;
        };

        let email = app.get_email().to_string();
        let password = app.get_password().to_string();
        let verification_code = app.get_verification_code().to_string();
        let auth_preferences = AuthPreferences {
            remember_password: app.get_remember_password(),
            auto_login: app.get_auto_login(),
        }
        .sanitized();
        let pending_token = login_totp.lock().ok().and_then(|state| state.clone());

        let submission = match build_login_submission(
            &email,
            &password,
            &verification_code,
            pending_token.as_deref(),
        ) {
            Ok(submission) => submission,
            Err(message) => {
                app.set_auth_busy(false);
                app.set_auth_status_text(SharedString::from(message));
                return;
            }
        };
        if let Some(message) = login_config.packaged_local_debug_api_message() {
            app.set_auth_busy(false);
            app.set_auth_status_text(SharedString::from(message));
            return;
        }

        app.set_auth_busy(true);
        app.set_auth_status_text(SharedString::from("正在处理登录请求..."));
        let ui_handle = login_app.clone();
        let config = Arc::clone(&login_config);
        let app_state = login_state.clone();
        let token_store = login_store.clone();
        let auth_session = Arc::clone(&login_session);
        let pending_totp_token = Arc::clone(&login_totp);
        let available_groups = Arc::clone(&login_groups);
        let billing_summary = Arc::clone(&login_billing_summary);
        let subscription_summary = Arc::clone(&login_summary);
        let redeem_history = Arc::clone(&login_history);
        let recent_orders = Arc::clone(&login_orders);
        let checkout_info = Arc::clone(&login_checkout);
        let pending_payment = Arc::clone(&login_pending_payment);
        let usage_page = Arc::clone(&login_usage_page);
        let usage_query = Arc::clone(&login_usage_query);
        thread::spawn(move || {
            let client = ApiClient::new(config.api_base_url.clone());
            let result = match submission {
                LoginSubmission::Password(request) => login_blocking(&client, &request).and_then(
                    |response| match response {
                        LoginResponse::Authenticated(auth) => Ok(AuthFlowOutcome::Authenticated(auth)),
                        LoginResponse::TotpRequired(totp) => Ok(AuthFlowOutcome::TotpRequired {
                            temp_token: totp.temp_token,
                            masked_email: totp.user_email_masked,
                        }),
                    },
                ),
                LoginSubmission::TwoFactor(request) => {
                    login_2fa_blocking(&client, &request).map(AuthFlowOutcome::Authenticated)
                }
            };

            match result {
                Ok(AuthFlowOutcome::Authenticated(auth)) => {
                    handle_auth_success(
                        &config,
                        &app_state,
                        &token_store,
                        &auth_preferences,
                        &auth_session,
                        &pending_totp_token,
                        &available_groups,
                        &billing_summary,
                        &subscription_summary,
                        &redeem_history,
                        &recent_orders,
                        &checkout_info,
                        &pending_payment,
                        &usage_page,
                        &usage_query,
                        email,
                        Some(password.clone()),
                        auth,
                    );
                    let groups_snapshot = current_groups_snapshot(&available_groups);
                    let billing_summary_snapshot =
                        current_billing_summary_snapshot(&billing_summary);
                    let billing_vm = current_billing_vm(
                        &auth_session,
                        &available_groups,
                        &billing_summary,
                        &subscription_summary,
                        &recent_orders,
                        &redeem_history,
                    );
                    let dashboard_vm =
                        current_dashboard_vm(&auth_session, &billing_summary);
                    let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                    let usage_vm = current_usage_vm(&usage_page);
                    let group_count = Some(groups_snapshot.len());
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_busy(false);
                        if let Some(session) =
                            auth_session.lock().ok().and_then(|state| state.clone())
                        {
                            apply_authenticated_state(
                                &app,
                                &session,
                                group_count,
                                billing_summary_snapshot.as_ref(),
                            );
                            apply_dashboard_state(&app, &dashboard_vm);
                            apply_available_groups_state(&app, &groups_snapshot);
                            apply_billing_state(&app, &billing_vm);
                            apply_checkout_state(&app, checkout_snapshot.as_ref());
                            apply_usage_state(&app, &usage_vm);
                        }
                    });
                }
                Ok(AuthFlowOutcome::TotpRequired {
                    temp_token,
                    masked_email,
                }) => {
                    if let Ok(mut pending) = pending_totp_token.lock() {
                        *pending = temp_token.clone();
                    }
                    let message = match masked_email {
                        Some(masked) => format!(
                            "检测到二步验证，请在“验证码 / 2FA”输入框填写 6 位动态码后再次点击登录。目标邮箱：{masked}"
                        ),
                        None => "检测到二步验证，请在“验证码 / 2FA”输入框填写 6 位动态码后再次点击登录。".to_string(),
                    };
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_busy(false);
                        app.set_auth_status_text(SharedString::from(message));
                        app.set_show_login_totp(true);
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_busy(false);
                        app.set_auth_status_text(SharedString::from(format!("登录失败：{error}")))
                    });
                }
            }
        });
    });

    let register_app = app.as_weak();
    let register_config = Arc::clone(&config);
    let register_state = app_state.clone();
    let register_store = token_store.clone();
    let register_session = Arc::clone(&auth_session);
    let register_totp = Arc::clone(&pending_totp_token);
    let register_groups = Arc::clone(&available_groups);
    let register_billing_summary = Arc::clone(&billing_summary);
    let register_summary = Arc::clone(&subscription_summary);
    let register_history = Arc::clone(&redeem_history);
    let register_orders = Arc::clone(&recent_orders);
    let register_checkout = Arc::clone(&checkout_info);
    let register_pending_payment = Arc::clone(&pending_payment);
    let register_usage_page = Arc::clone(&usage_page);
    let register_usage_query = Arc::clone(&usage_query);
    app.on_register_requested(move || {
        let Some(app) = register_app.upgrade() else {
            return;
        };

        let email = app.get_email().to_string();
        let password = app.get_password().to_string();
        let verification_code = app.get_verification_code().to_string();
        let auth_preferences = AuthPreferences {
            remember_password: app.get_remember_password(),
            auto_login: app.get_auto_login(),
        }
        .sanitized();
        if email.trim().is_empty() || password.is_empty() {
            app.set_auth_busy(false);
            app.set_auth_status_text(SharedString::from("注册前请填写邮箱、密码和邮箱验证码。"));
            return;
        }
        if let Some(message) = register_config.packaged_local_debug_api_message() {
            app.set_auth_busy(false);
            app.set_auth_status_text(SharedString::from(message));
            return;
        }
        app.set_auth_busy(true);
        app.set_auth_status_text(SharedString::from("正在提交注册请求..."));

        let ui_handle = register_app.clone();
        let config = Arc::clone(&register_config);
        let app_state = register_state.clone();
        let token_store = register_store.clone();
        let auth_session = Arc::clone(&register_session);
        let pending_totp_token = Arc::clone(&register_totp);
        let available_groups = Arc::clone(&register_groups);
        let billing_summary = Arc::clone(&register_billing_summary);
        let subscription_summary = Arc::clone(&register_summary);
        let redeem_history = Arc::clone(&register_history);
        let recent_orders = Arc::clone(&register_orders);
        let checkout_info = Arc::clone(&register_checkout);
        let pending_payment = Arc::clone(&register_pending_payment);
        let usage_page = Arc::clone(&register_usage_page);
        let usage_query = Arc::clone(&register_usage_query);
        thread::spawn(move || {
            let client = ApiClient::new(config.api_base_url.clone());
            let request = RegisterRequest::new(email.trim(), password.clone())
                .with_verify_code(verification_code.trim());
            match register_blocking(&client, &request) {
                Ok(auth) => {
                    handle_auth_success(
                        &config,
                        &app_state,
                        &token_store,
                        &auth_preferences,
                        &auth_session,
                        &pending_totp_token,
                        &available_groups,
                        &billing_summary,
                        &subscription_summary,
                        &redeem_history,
                        &recent_orders,
                        &checkout_info,
                        &pending_payment,
                        &usage_page,
                        &usage_query,
                        email,
                        Some(password.clone()),
                        auth,
                    );
                    let groups_snapshot = current_groups_snapshot(&available_groups);
                    let billing_summary_snapshot =
                        current_billing_summary_snapshot(&billing_summary);
                    let billing_vm = current_billing_vm(
                        &auth_session,
                        &available_groups,
                        &billing_summary,
                        &subscription_summary,
                        &recent_orders,
                        &redeem_history,
                    );
                    let dashboard_vm = current_dashboard_vm(&auth_session, &billing_summary);
                    let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                    let usage_vm = current_usage_vm(&usage_page);
                    let group_count = Some(groups_snapshot.len());
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_busy(false);
                        if let Some(session) =
                            auth_session.lock().ok().and_then(|state| state.clone())
                        {
                            apply_authenticated_state(
                                &app,
                                &session,
                                group_count,
                                billing_summary_snapshot.as_ref(),
                            );
                            apply_dashboard_state(&app, &dashboard_vm);
                            apply_available_groups_state(&app, &groups_snapshot);
                            apply_billing_state(&app, &billing_vm);
                            apply_checkout_state(&app, checkout_snapshot.as_ref());
                            apply_usage_state(&app, &usage_vm);
                            app.set_auth_status_text(SharedString::from(
                                "注册成功，已自动登录当前账户。",
                            ));
                            app.set_auth_subview(0);
                        }
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_busy(false);
                        app.set_auth_status_text(SharedString::from(format!("注册失败：{error}")))
                    });
                }
            }
        });
    });

    let verify_app = app.as_weak();
    let verify_config = Arc::clone(&config);
    app.on_verify_code_requested(move || {
        let Some(app) = verify_app.upgrade() else {
            return;
        };
        let email = app.get_email().to_string();
        if email.trim().is_empty() {
            app.set_auth_busy(false);
            app.set_auth_status_text(SharedString::from("发送验证码前请先填写邮箱。"));
            return;
        }
        if let Some(message) = verify_config.packaged_local_debug_api_message() {
            app.set_auth_busy(false);
            app.set_auth_status_text(SharedString::from(message));
            return;
        }
        app.set_auth_busy(true);
        app.set_auth_status_text(SharedString::from("正在发送验证码..."));

        let ui_handle = verify_app.clone();
        let config = Arc::clone(&verify_config);
        thread::spawn(move || {
            let client = ApiClient::new(config.api_base_url.clone());
            match send_verify_code_blocking(&client, &SendVerifyCodeRequest::new(email.trim())) {
                Ok(response) => {
                    let message = format!("验证码已发送，{} 秒后可再次发送。", response.countdown);
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_busy(false);
                        app.set_auth_status_text(SharedString::from(message))
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_busy(false);
                        app.set_auth_status_text(SharedString::from(format!(
                            "发送验证码失败：{error}"
                        )))
                    });
                }
            }
        });
    });

    let forgot_app = app.as_weak();
    let forgot_config = Arc::clone(&config);
    app.on_forgot_password_requested(move || {
        let Some(app) = forgot_app.upgrade() else {
            return;
        };
        let email = app.get_email().to_string();
        if email.trim().is_empty() {
            app.set_auth_busy(false);
            app.set_reset_status_text(SharedString::from("请先填写重置邮箱。"));
            return;
        }
        if let Some(message) = forgot_config.packaged_local_debug_api_message() {
            app.set_auth_busy(false);
            app.set_reset_status_text(SharedString::from(message));
            return;
        }
        app.set_auth_busy(true);
        app.set_reset_status_text(SharedString::from("正在发送重置邮件..."));

        let ui_handle = forgot_app.clone();
        let config = Arc::clone(&forgot_config);
        thread::spawn(move || {
            let client = ApiClient::new(config.api_base_url.clone());
            match forgot_password_blocking(
                &client,
                &sub2api_desktop::api::auth::ForgotPasswordRequest::new(email.trim()),
            ) {
                Ok(response) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_busy(false);
                        app.set_reset_status_text(SharedString::from(response.message))
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_busy(false);
                        app.set_reset_status_text(SharedString::from(format!(
                            "发送重置邮件失败：{error}"
                        )))
                    });
                }
            }
        });
    });

    let reset_app = app.as_weak();
    let reset_config = Arc::clone(&config);
    app.on_reset_password_requested(move || {
        let Some(app) = reset_app.upgrade() else {
            return;
        };
        let email = app.get_email().to_string();
        let reset_token = app.get_reset_token().to_string();
        let new_password = app.get_new_password().to_string();
        if email.trim().is_empty() || reset_token.trim().is_empty() || new_password.is_empty() {
            app.set_auth_busy(false);
            app.set_reset_status_text(SharedString::from("请填写邮箱、邮件重置码和新密码。"));
            return;
        }
        if let Some(message) = reset_config.packaged_local_debug_api_message() {
            app.set_auth_busy(false);
            app.set_reset_status_text(SharedString::from(message));
            return;
        }
        app.set_auth_busy(true);
        app.set_reset_status_text(SharedString::from("正在重置密码..."));

        let ui_handle = reset_app.clone();
        let config = Arc::clone(&reset_config);
        thread::spawn(move || {
            let client = ApiClient::new(config.api_base_url.clone());
            let request =
                ResetPasswordRequest::new(email.trim(), reset_token.trim(), new_password.as_str());
            match reset_password_blocking(&client, &request) {
                Ok(response) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_busy(false);
                        app.set_reset_status_text(SharedString::from(response.message))
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_busy(false);
                        app.set_reset_status_text(SharedString::from(format!(
                            "重置密码失败：{error}"
                        )))
                    });
                }
            }
        });
    });
}

fn wire_billing_callbacks(
    app: &AppWindow,
    config: Arc<AppConfig>,
    auth_session: SharedAuthSession,
    available_groups: SharedGroups,
    billing_summary: SharedBillingSummary,
    subscription_summary: SharedSubscriptionSummary,
    redeem_history: SharedRedeemHistory,
    recent_orders: SharedOrders,
    checkout_info: SharedCheckoutInfo,
    pending_payment: SharedPendingPayment,
    usage_page: SharedUsagePage,
    usage_query: SharedUsageQuery,
) {
    let redeem_app = app.as_weak();
    let redeem_config = Arc::clone(&config);
    let redeem_session = Arc::clone(&auth_session);
    let redeem_groups = Arc::clone(&available_groups);
    let redeem_billing_summary = Arc::clone(&billing_summary);
    let redeem_summary = Arc::clone(&subscription_summary);
    let redeem_history_store = Arc::clone(&redeem_history);
    let redeem_orders = Arc::clone(&recent_orders);
    let redeem_checkout = Arc::clone(&checkout_info);
    let redeem_usage_page = Arc::clone(&usage_page);
    let redeem_usage_query = Arc::clone(&usage_query);
    app.on_redeem_requested(move || {
        let Some(app) = redeem_app.upgrade() else {
            return;
        };
        let code = app.get_redeem_code().to_string();
        if code.trim().is_empty() {
            app.set_redeem_status_text(SharedString::from("请输入要兑换的 CDK。"));
            return;
        }
        app.set_redeem_status_text(SharedString::from("正在兑换 CDK..."));

        let ui_handle = redeem_app.clone();
        let config = Arc::clone(&redeem_config);
        let auth_session = Arc::clone(&redeem_session);
        let available_groups = Arc::clone(&redeem_groups);
        let billing_summary = Arc::clone(&redeem_billing_summary);
        let subscription_summary = Arc::clone(&redeem_summary);
        let redeem_history_store = Arc::clone(&redeem_history_store);
        let recent_orders = Arc::clone(&redeem_orders);
        let checkout_info = Arc::clone(&redeem_checkout);
        let usage_page = Arc::clone(&redeem_usage_page);
        let usage_query = Arc::clone(&redeem_usage_query);
        thread::spawn(move || {
            let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_redeem_status_text(SharedString::from("请先登录后再兑换 CDK。"))
                });
                return;
            };

            let client = ApiClient::new(config.api_base_url.clone())
                .with_access_token(Some(session.access_token));
            match redeem_code_blocking(&client, &RedeemCodeRequest::new(code.trim())) {
                Ok(result) => {
                    let updated_user = fetch_current_user_blocking(&client).ok();
                    let (group_count, groups_snapshot, billing_vm) = sync_user_side_state(
                        &client,
                        &auth_session,
                        &available_groups,
                        &billing_summary,
                        &subscription_summary,
                        &recent_orders,
                        &redeem_history_store,
                        &checkout_info,
                        &usage_page,
                        &usage_query,
                    );
                    let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                    let billing_summary_snapshot =
                        current_billing_summary_snapshot(&billing_summary);
                    let dashboard_vm = current_dashboard_vm(&auth_session, &billing_summary);
                    let usage_vm = current_usage_vm(&usage_page);

                    if let Some(user) = updated_user {
                        if let Ok(mut state) = auth_session.lock() {
                            if let Some(existing) = state.as_mut() {
                                existing.user = user;
                            }
                        }
                    }

                    let status_message = format_redeem_success_message(&result);
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        if let Some(session) =
                            auth_session.lock().ok().and_then(|state| state.clone())
                        {
                            apply_authenticated_state(
                                &app,
                                &session,
                                group_count,
                                billing_summary_snapshot.as_ref(),
                            );
                            apply_dashboard_state(&app, &dashboard_vm);
                        }
                        apply_available_groups_state(&app, &groups_snapshot);
                        apply_billing_state(&app, &billing_vm);
                        apply_checkout_state(&app, checkout_snapshot.as_ref());
                        apply_usage_state(&app, &usage_vm);
                        app.set_redeem_status_text(SharedString::from(status_message));
                        app.set_redeem_code(SharedString::from(""));
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_redeem_status_text(SharedString::from(format!("兑换失败：{error}")))
                    });
                }
            }
        });
    });

    let recharge_app = app.as_weak();
    let recharge_config = Arc::clone(&config);
    let recharge_session = Arc::clone(&auth_session);
    let recharge_billing = Arc::clone(&billing_summary);
    let recharge_summary = Arc::clone(&subscription_summary);
    let recharge_history = Arc::clone(&redeem_history);
    let recharge_orders = Arc::clone(&recent_orders);
    let recharge_groups = Arc::clone(&available_groups);
    let recharge_checkout = Arc::clone(&checkout_info);
    let recharge_pending = Arc::clone(&pending_payment);
    let recharge_usage_page = Arc::clone(&usage_page);
    let recharge_usage_query = Arc::clone(&usage_query);
    app.on_billing_recharge_requested(move || {
        let Some(app) = recharge_app.upgrade() else {
            return;
        };
        let amount_text = app.get_billing_recharge_amount().to_string();
        let amount = amount_text.trim().parse::<f64>().unwrap_or(0.0);
        if amount <= 0.0 {
            app.set_billing_checkout_status_text(SharedString::from("请输入有效的充值金额。"));
            return;
        }
        app.set_billing_checkout_status_text(SharedString::from("正在创建充值订单..."));

        let ui_handle = recharge_app.clone();
        let config = Arc::clone(&recharge_config);
        let auth_session = Arc::clone(&recharge_session);
        let billing_summary = Arc::clone(&recharge_billing);
        let subscription_summary = Arc::clone(&recharge_summary);
        let redeem_history = Arc::clone(&recharge_history);
        let recent_orders = Arc::clone(&recharge_orders);
        let available_groups = Arc::clone(&recharge_groups);
        let checkout_info = Arc::clone(&recharge_checkout);
        let pending_payment = Arc::clone(&recharge_pending);
        let usage_page = Arc::clone(&recharge_usage_page);
        let usage_query = Arc::clone(&recharge_usage_query);
        let selected_method_index = app.get_billing_selected_payment_method_index() as usize;
        thread::spawn(move || {
            let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_billing_checkout_status_text(SharedString::from(
                        "请先登录后再创建充值订单。",
                    ))
                });
                return;
            };
            let checkout_snapshot = current_checkout_snapshot(&checkout_info);
            let Some(checkout) = checkout_snapshot.as_ref() else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_billing_checkout_status_text(SharedString::from(
                        "当前支付配置尚未加载，请先刷新计费数据。",
                    ))
                });
                return;
            };
            let payment_keys = ordered_payment_method_keys(checkout);
            let Some(payment_type) = payment_keys.get(selected_method_index) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_billing_checkout_status_text(SharedString::from("请选择可用支付方式。"))
                });
                return;
            };

            let client = ApiClient::new(config.api_base_url.clone())
                .with_access_token(Some(session.access_token));
            let request = CreateOrderRequest {
                amount,
                payment_type: payment_type.clone(),
                order_type: "balance".to_string(),
                plan_id: None,
            };
            match create_order_blocking(&client, &request) {
                Ok(result) => {
                    let open_target = match create_payment_open_target(&result, &request) {
                        Ok(target) => target,
                        Err(error) => {
                            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                                app.set_billing_checkout_status_text(SharedString::from(format!(
                                    "订单已创建，但生成支付入口失败：{error}"
                                )))
                            });
                            return;
                        }
                    };
                    let _ = open_external_target(&open_target);
                    if let Ok(mut state) = pending_payment.lock() {
                        *state = Some(PendingPaymentState {
                            order_id: result.order_id,
                            open_target: open_target.clone(),
                        });
                    }
                    let (_, groups_snapshot, billing_vm) = sync_user_side_state(
                        &client,
                        &auth_session,
                        &available_groups,
                        &billing_summary,
                        &subscription_summary,
                        &recent_orders,
                        &redeem_history,
                        &checkout_info,
                        &usage_page,
                        &usage_query,
                    );
                    let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                    let usage_vm = current_usage_vm(&usage_page);
                    let message = format!(
                        "充值订单 #{} 已创建，已为你打开支付入口；可在桌面端继续刷新订单状态。",
                        result.order_id
                    );
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        apply_available_groups_state(&app, &groups_snapshot);
                        apply_billing_state(&app, &billing_vm);
                        apply_checkout_state(&app, checkout_snapshot.as_ref());
                        apply_usage_state(&app, &usage_vm);
                        app.set_billing_checkout_status_text(SharedString::from(message));
                        app.set_billing_recharge_amount(SharedString::from(""));
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_billing_checkout_status_text(SharedString::from(format!(
                            "创建充值订单失败：{error}"
                        )))
                    });
                }
            }
        });
    });

    let subscribe_app = app.as_weak();
    let subscribe_config = Arc::clone(&config);
    let subscribe_session = Arc::clone(&auth_session);
    let subscribe_billing = Arc::clone(&billing_summary);
    let subscribe_summary = Arc::clone(&subscription_summary);
    let subscribe_history = Arc::clone(&redeem_history);
    let subscribe_orders = Arc::clone(&recent_orders);
    let subscribe_groups = Arc::clone(&available_groups);
    let subscribe_checkout = Arc::clone(&checkout_info);
    let subscribe_pending = Arc::clone(&pending_payment);
    let subscribe_usage_page = Arc::clone(&usage_page);
    let subscribe_usage_query = Arc::clone(&usage_query);
    app.on_billing_subscription_requested(move || {
        let Some(app) = subscribe_app.upgrade() else {
            return;
        };
        app.set_billing_checkout_status_text(SharedString::from("正在创建订阅订单..."));

        let ui_handle = subscribe_app.clone();
        let config = Arc::clone(&subscribe_config);
        let auth_session = Arc::clone(&subscribe_session);
        let billing_summary = Arc::clone(&subscribe_billing);
        let subscription_summary = Arc::clone(&subscribe_summary);
        let redeem_history = Arc::clone(&subscribe_history);
        let recent_orders = Arc::clone(&subscribe_orders);
        let available_groups = Arc::clone(&subscribe_groups);
        let checkout_info = Arc::clone(&subscribe_checkout);
        let pending_payment = Arc::clone(&subscribe_pending);
        let usage_page = Arc::clone(&subscribe_usage_page);
        let usage_query = Arc::clone(&subscribe_usage_query);
        let selected_method_index = app.get_billing_selected_payment_method_index() as usize;
        let selected_plan_index = app.get_billing_selected_subscription_plan_index() as usize;
        thread::spawn(move || {
            let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_billing_checkout_status_text(SharedString::from(
                        "请先登录后再创建订阅订单。",
                    ))
                });
                return;
            };
            let checkout_snapshot = current_checkout_snapshot(&checkout_info);
            let Some(checkout) = checkout_snapshot.as_ref() else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_billing_checkout_status_text(SharedString::from(
                        "当前支付配置尚未加载，请先刷新计费数据。",
                    ))
                });
                return;
            };
            let payment_keys = ordered_payment_method_keys(checkout);
            let Some(payment_type) = payment_keys.get(selected_method_index) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_billing_checkout_status_text(SharedString::from("请选择可用支付方式。"))
                });
                return;
            };
            let plans = ordered_subscription_plans(checkout);
            let Some(plan) = plans.get(selected_plan_index) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_billing_checkout_status_text(SharedString::from("请选择可购买套餐。"))
                });
                return;
            };

            let client = ApiClient::new(config.api_base_url.clone())
                .with_access_token(Some(session.access_token));
            let request = CreateOrderRequest {
                amount: plan.price,
                payment_type: payment_type.clone(),
                order_type: "subscription".to_string(),
                plan_id: Some(plan.id),
            };
            match create_order_blocking(&client, &request) {
                Ok(result) => {
                    let open_target = match create_payment_open_target(&result, &request) {
                        Ok(target) => target,
                        Err(error) => {
                            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                                app.set_billing_checkout_status_text(SharedString::from(format!(
                                    "订阅订单已创建，但生成支付入口失败：{error}"
                                )))
                            });
                            return;
                        }
                    };
                    let _ = open_external_target(&open_target);
                    if let Ok(mut state) = pending_payment.lock() {
                        *state = Some(PendingPaymentState {
                            order_id: result.order_id,
                            open_target: open_target.clone(),
                        });
                    }
                    let (_, groups_snapshot, billing_vm) = sync_user_side_state(
                        &client,
                        &auth_session,
                        &available_groups,
                        &billing_summary,
                        &subscription_summary,
                        &recent_orders,
                        &redeem_history,
                        &checkout_info,
                        &usage_page,
                        &usage_query,
                    );
                    let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                    let usage_vm = current_usage_vm(&usage_page);
                    let message = format!(
                        "订阅订单 #{}（{}）已创建，已打开支付入口。",
                        result.order_id, plan.name
                    );
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        apply_available_groups_state(&app, &groups_snapshot);
                        apply_billing_state(&app, &billing_vm);
                        apply_checkout_state(&app, checkout_snapshot.as_ref());
                        apply_usage_state(&app, &usage_vm);
                        app.set_billing_checkout_status_text(SharedString::from(message));
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_billing_checkout_status_text(SharedString::from(format!(
                            "创建订阅订单失败：{error}"
                        )))
                    });
                }
            }
        });
    });

    let reopen_payment_app = app.as_weak();
    let reopen_pending_payment = Arc::clone(&pending_payment);
    app.on_billing_open_last_payment_requested(move || {
        let Some(app) = reopen_payment_app.upgrade() else {
            return;
        };
        let Some(pending) = reopen_pending_payment
            .lock()
            .ok()
            .and_then(|state| state.clone())
        else {
            app.set_billing_checkout_status_text(SharedString::from(
                "当前没有可重新打开的待支付订单。",
            ));
            return;
        };
        match open_external_target(&pending.open_target) {
            Ok(()) => app.set_billing_checkout_status_text(SharedString::from(format!(
                "已重新打开订单 #{} 的支付入口。",
                pending.order_id
            ))),
            Err(error) => app.set_billing_checkout_status_text(SharedString::from(format!(
                "重新打开支付入口失败：{error}"
            ))),
        }
    });

    let refresh_billing_app = app.as_weak();
    let refresh_billing_config = Arc::clone(&config);
    let refresh_billing_session = Arc::clone(&auth_session);
    let refresh_billing_groups = Arc::clone(&available_groups);
    let refresh_billing_token_summary = Arc::clone(&billing_summary);
    let refresh_billing_summary = Arc::clone(&subscription_summary);
    let refresh_billing_history = Arc::clone(&redeem_history);
    let refresh_billing_orders = Arc::clone(&recent_orders);
    let refresh_billing_checkout = Arc::clone(&checkout_info);
    let refresh_billing_pending = Arc::clone(&pending_payment);
    let refresh_billing_usage_page = Arc::clone(&usage_page);
    let refresh_billing_usage_query = Arc::clone(&usage_query);
    app.on_billing_refresh_requested(move || {
        let Some(app) = refresh_billing_app.upgrade() else {
            return;
        };
        app.set_billing_checkout_status_text(SharedString::from("正在刷新计费数据..."));

        let ui_handle = refresh_billing_app.clone();
        let config = Arc::clone(&refresh_billing_config);
        let auth_session = Arc::clone(&refresh_billing_session);
        let available_groups = Arc::clone(&refresh_billing_groups);
        let billing_summary = Arc::clone(&refresh_billing_token_summary);
        let subscription_summary = Arc::clone(&refresh_billing_summary);
        let redeem_history = Arc::clone(&refresh_billing_history);
        let recent_orders = Arc::clone(&refresh_billing_orders);
        let checkout_info = Arc::clone(&refresh_billing_checkout);
        let pending_payment = Arc::clone(&refresh_billing_pending);
        let usage_page = Arc::clone(&refresh_billing_usage_page);
        let usage_query = Arc::clone(&refresh_billing_usage_query);
        thread::spawn(move || {
            let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    apply_checkout_state(&app, None);
                    app.set_billing_checkout_status_text(SharedString::from(
                        "请先登录后再刷新计费数据。",
                    ))
                });
                return;
            };
            let client = ApiClient::new(config.api_base_url.clone())
                .with_access_token(Some(session.access_token));

            let pending_order_status = pending_payment
                .lock()
                .ok()
                .and_then(|state| state.clone())
                .and_then(|pending| {
                    fetch_order_blocking(&client, pending.order_id)
                        .ok()
                        .map(|order| {
                            format!("最近订单 #{} 当前状态：{}", pending.order_id, order.status)
                        })
                });

            let (group_count, groups_snapshot, billing_vm) = sync_user_side_state(
                &client,
                &auth_session,
                &available_groups,
                &billing_summary,
                &subscription_summary,
                &recent_orders,
                &redeem_history,
                &checkout_info,
                &usage_page,
                &usage_query,
            );
            let checkout_snapshot = current_checkout_snapshot(&checkout_info);
            let billing_summary_snapshot = current_billing_summary_snapshot(&billing_summary);
            let dashboard_vm = current_dashboard_vm(&auth_session, &billing_summary);
            let usage_vm = current_usage_vm(&usage_page);
            let status_message = pending_order_status
                .unwrap_or_else(|| "计费数据已刷新，可继续创建或追踪订单。".to_string());
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                if let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) {
                    apply_authenticated_state(
                        &app,
                        &session,
                        group_count,
                        billing_summary_snapshot.as_ref(),
                    );
                    apply_dashboard_state(&app, &dashboard_vm);
                }
                apply_available_groups_state(&app, &groups_snapshot);
                apply_billing_state(&app, &billing_vm);
                apply_checkout_state(&app, checkout_snapshot.as_ref());
                apply_usage_state(&app, &usage_vm);
                app.set_billing_checkout_status_text(SharedString::from(status_message));
            });
        });
    });
}

fn wire_usage_callbacks(
    app: &AppWindow,
    config: Arc<AppConfig>,
    auth_session: SharedAuthSession,
    _available_groups: SharedGroups,
    _subscription_summary: SharedSubscriptionSummary,
    _redeem_history: SharedRedeemHistory,
    _recent_orders: SharedOrders,
    _checkout_info: SharedCheckoutInfo,
    usage_page: SharedUsagePage,
    usage_query: SharedUsageQuery,
) {
    let bind_usage_loader = |app_handle: slint::Weak<AppWindow>,
                             config: Arc<AppConfig>,
                             auth_session: SharedAuthSession,
                             usage_page: SharedUsagePage,
                             usage_query: SharedUsageQuery,
                             status_text: &'static str| {
        thread::spawn(move || {
            let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
                let _ = app_handle.upgrade_in_event_loop(move |app| {
                    app.set_usage_status_text(SharedString::from("请先登录后再查看消费明细。"));
                });
                return;
            };

            let client = ApiClient::new(config.api_base_url.clone())
                .with_access_token(Some(session.access_token));
            match fetch_and_store_usage_page(&client, &usage_page, &usage_query) {
                Ok(_) => {
                    let usage_vm = current_usage_vm(&usage_page);
                    let selected_index =
                        current_usage_query_state(&usage_query).view_mode.to_index();
                    let _ = app_handle.upgrade_in_event_loop(move |app| {
                        apply_usage_state(&app, &usage_vm);
                        app.set_usage_selected_view_mode_index(selected_index);
                        app.set_usage_status_text(SharedString::from(status_text));
                    });
                }
                Err(error) => {
                    let _ = app_handle.upgrade_in_event_loop(move |app| {
                        app.set_usage_status_text(SharedString::from(format!(
                            "刷新消费明细失败：{error}"
                        )));
                    });
                }
            }
        });
    };

    let usage_app = app.as_weak();
    let usage_config = Arc::clone(&config);
    let usage_session = Arc::clone(&auth_session);
    let usage_page_state = Arc::clone(&usage_page);
    let usage_query_state = Arc::clone(&usage_query);
    app.on_usage_refresh_requested(move || {
        if let Some(app) = usage_app.upgrade() {
            app.set_usage_status_text(SharedString::from("正在刷新消费明细..."));
        }
        bind_usage_loader(
            usage_app.clone(),
            Arc::clone(&usage_config),
            Arc::clone(&usage_session),
            Arc::clone(&usage_page_state),
            Arc::clone(&usage_query_state),
            "消费明细已刷新。",
        );
    });

    let prev_app = app.as_weak();
    let prev_config = Arc::clone(&config);
    let prev_session = Arc::clone(&auth_session);
    let prev_usage_page = Arc::clone(&usage_page);
    let prev_usage_query = Arc::clone(&usage_query);
    app.on_usage_prev_page_requested(move || {
        if let Ok(mut state) = prev_usage_query.lock() {
            if state.page > 1 {
                state.page -= 1;
            }
        }
        bind_usage_loader(
            prev_app.clone(),
            Arc::clone(&prev_config),
            Arc::clone(&prev_session),
            Arc::clone(&prev_usage_page),
            Arc::clone(&prev_usage_query),
            "已切换到上一页。",
        );
    });

    let next_app = app.as_weak();
    let next_config = Arc::clone(&config);
    let next_session = Arc::clone(&auth_session);
    let next_usage_page = Arc::clone(&usage_page);
    let next_usage_query = Arc::clone(&usage_query);
    app.on_usage_next_page_requested(move || {
        let can_advance = next_usage_page
            .lock()
            .ok()
            .and_then(|state| state.as_ref().map(|page| page.page < page.pages))
            .unwrap_or(false);
        if can_advance {
            if let Ok(mut state) = next_usage_query.lock() {
                state.page += 1;
            }
        }
        bind_usage_loader(
            next_app.clone(),
            Arc::clone(&next_config),
            Arc::clone(&next_session),
            Arc::clone(&next_usage_page),
            Arc::clone(&next_usage_query),
            if can_advance {
                "已切换到下一页。"
            } else {
                "已经是最后一页。"
            },
        );
    });

    let mode_app = app.as_weak();
    let mode_config = Arc::clone(&config);
    let mode_session = Arc::clone(&auth_session);
    let mode_usage_page = Arc::clone(&usage_page);
    let mode_usage_query = Arc::clone(&usage_query);
    app.on_usage_view_mode_changed(move |index| {
        if let Ok(mut state) = mode_usage_query.lock() {
            state.view_mode = UsageViewMode::from_index(index);
            state.page = 1;
        }
        bind_usage_loader(
            mode_app.clone(),
            Arc::clone(&mode_config),
            Arc::clone(&mode_session),
            Arc::clone(&mode_usage_page),
            Arc::clone(&mode_usage_query),
            "消费明细查看方式已更新。",
        );
    });

    let export_app = app.as_weak();
    let export_config = Arc::clone(&config);
    let export_session = Arc::clone(&auth_session);
    let export_usage_query = Arc::clone(&usage_query);
    app.on_usage_export_excel_requested(move || {
        if let Some(app) = export_app.upgrade() {
            app.set_usage_status_text(SharedString::from("正在导出 Excel..."));
        }
        let app_handle = export_app.clone();
        let config = Arc::clone(&export_config);
        let auth_session = Arc::clone(&export_session);
        let usage_query = Arc::clone(&export_usage_query);
        thread::spawn(move || {
            let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
                let _ = app_handle.upgrade_in_event_loop(move |app| {
                    app.set_usage_status_text(SharedString::from("请先登录后再导出 Excel。"));
                });
                return;
            };
            let client = ApiClient::new(config.api_base_url.clone())
                .with_access_token(Some(session.access_token));
            match export_usage_excel(&client, &usage_query) {
                Ok(path) => {
                    let _ = open_external_target(path.to_string_lossy().as_ref());
                    let _ = app_handle.upgrade_in_event_loop(move |app| {
                        app.set_usage_status_text(SharedString::from(format!(
                            "已导出 Excel：{}",
                            path.display()
                        )));
                    });
                }
                Err(error) => {
                    let _ = app_handle.upgrade_in_event_loop(move |app| {
                        app.set_usage_status_text(SharedString::from(format!(
                            "导出 Excel 失败：{error}"
                        )));
                    });
                }
            }
        });
    });
}

fn wire_use_key_callbacks(
    app: &AppWindow,
    config: Arc<AppConfig>,
    auth_session: SharedAuthSession,
    available_groups: SharedGroups,
) {
    let key_app = app.as_weak();
    let key_config = Arc::clone(&config);
    let key_session = Arc::clone(&auth_session);
    let key_groups = Arc::clone(&available_groups);
    app.on_view_usage_key_requested(move || {
        let Some(app) = key_app.upgrade() else {
            return;
        };
        let password = app.get_view_key_password().to_string();
        if password.trim().is_empty() {
            app.set_view_key_status_text(SharedString::from("请输入当前账户密码。"));
            return;
        }
        app.set_view_key_status_text(SharedString::from("正在校验密码并生成 7 天使用密钥..."));

        let ui_handle = key_app.clone();
        let config = Arc::clone(&key_config);
        let auth_session = Arc::clone(&key_session);
        let available_groups = Arc::clone(&key_groups);
        thread::spawn(move || {
            let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_view_key_status_text(SharedString::from("请先登录后再查看使用密钥。"));
                });
                return;
            };
            let email = session.user.email.clone();
            let verify_client = ApiClient::new(config.api_base_url.clone());
            match login_blocking(
                &verify_client,
                &sub2api_desktop::api::auth::LoginRequest::new(email.as_str(), password.as_str()),
            ) {
                Ok(LoginResponse::Authenticated(_)) => {
                    let groups_snapshot = current_groups_snapshot(&available_groups);
                    let Some(group) = first_openai_group(&groups_snapshot) else {
                        let _ = ui_handle.upgrade_in_event_loop(move |app| {
                            app.set_view_key_status_text(SharedString::from(
                                "当前账户没有可用的 OpenAI 分组，无法生成使用密钥。",
                            ));
                        });
                        return;
                    };

                    let authed_client = ApiClient::new(config.api_base_url.clone())
                        .with_access_token(Some(session.access_token));
                    let key_name = format!(
                        "desktop-view-key-{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or(Duration::ZERO)
                            .as_secs()
                    );
                    match create_api_key_blocking(
                        &authed_client,
                        &CreateAPIKeyRequest {
                            name: key_name,
                            group_id: group.id,
                            expires_in_days: 7,
                        },
                    ) {
                        Ok(api_key) => {
                            match write_use_key_guide_page(
                                &config.api_base_url,
                                api_key.key.as_str(),
                                api_key.expires_at.as_deref(),
                            ) {
                                Ok(path) => {
                                    let _ = open_external_target(path.to_string_lossy().as_ref());
                                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                                        app.set_view_key_password(SharedString::from(""));
                                        app.set_view_key_status_text(SharedString::from(format!(
                                            "已生成 7 天有效的使用密钥，并打开说明页：{}",
                                            path.display()
                                        )));
                                    });
                                }
                                Err(error) => {
                                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                                        app.set_view_key_status_text(SharedString::from(format!(
                                            "已生成使用密钥，但创建说明页失败：{error}"
                                        )));
                                    });
                                }
                            }
                        }
                        Err(error) => {
                            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                                app.set_view_key_status_text(SharedString::from(format!(
                                    "生成使用密钥失败：{error}"
                                )));
                            });
                        }
                    }
                }
                _ => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_view_key_status_text(SharedString::from(
                            "密码校验失败，无法查看使用密钥。",
                        ));
                    });
                }
            }
        });
    });
}

fn wire_session_manager_callbacks(
    app: &AppWindow,
    app_state: AppStateStore,
    session_manager: SharedSessionManager,
) {
    let refresh_app = app.as_weak();
    let refresh_state = app_state.clone();
    let refresh_manager = Arc::clone(&session_manager);
    app.on_session_refresh_requested(move || {
        let ui_handle = refresh_app.clone();
        let app_state = refresh_state.clone();
        let session_manager = Arc::clone(&refresh_manager);
        thread::spawn(move || {
            refresh_session_manager_state(&app_state, &session_manager);
            if let Ok(mut state) = session_manager.lock() {
                state.status_text = "已刷新本地会话列表。".to_string();
            }
            let vm = current_session_manager_vm(&session_manager);
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                apply_session_manager_state(&app, &vm);
            });
        });
    });

    let sync_app = app.as_weak();
    let sync_state = app_state.clone();
    let sync_manager = Arc::clone(&session_manager);
    app.on_session_sync_requested(move || {
        let ui_handle = sync_app.clone();
        let app_state = sync_state.clone();
        let session_manager = Arc::clone(&sync_manager);
        thread::spawn(move || {
            let message = match sync_sessions_across_homes(app_state.root()) {
                Ok(summary) => summary.message,
                Err(error) => format!("同步会话失败：{error}"),
            };
            refresh_session_manager_state(&app_state, &session_manager);
            if let Ok(mut state) = session_manager.lock() {
                state.status_text = message;
            }
            let vm = current_session_manager_vm(&session_manager);
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                apply_session_manager_state(&app, &vm);
            });
        });
    });

    let repair_app = app.as_weak();
    let repair_state = app_state.clone();
    let repair_manager = Arc::clone(&session_manager);
    app.on_session_repair_requested(move || {
        let ui_handle = repair_app.clone();
        let app_state = repair_state.clone();
        let session_manager = Arc::clone(&repair_manager);
        thread::spawn(move || {
            let message = match repair_session_visibility(app_state.root()) {
                Ok(summary) => summary.message,
                Err(error) => format!("修复可见性失败：{error}"),
            };
            refresh_session_manager_state(&app_state, &session_manager);
            if let Ok(mut state) = session_manager.lock() {
                state.status_text = message;
            }
            let vm = current_session_manager_vm(&session_manager);
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                apply_session_manager_state(&app, &vm);
            });
        });
    });

    let group_app = app.as_weak();
    let group_manager = Arc::clone(&session_manager);
    app.on_session_group_selected(move |index| {
        if let Ok(mut state) = group_manager.lock() {
            state.selected_group_index = index;
            state.selected_session_id = state
                .groups
                .get(index.max(0) as usize)
                .and_then(|group| group.sessions.first().map(|session| session.session_id.clone()));
        }
        if let Some(app) = group_app.upgrade() {
            apply_session_manager_state(&app, &current_session_manager_vm(&group_manager));
        }
    });

    let entry_app = app.as_weak();
    let entry_manager = Arc::clone(&session_manager);
    app.on_session_entry_selected(move |index| {
        if let Ok(mut state) = entry_manager.lock() {
            state.selected_session_id = state
                .groups
                .get(state.selected_group_index.max(0) as usize)
                .and_then(|group| group.sessions.get(index.max(0) as usize))
                .map(|session| session.session_id.clone());
        }
        if let Some(app) = entry_app.upgrade() {
            apply_session_manager_state(&app, &current_session_manager_vm(&entry_manager));
        }
    });

    let trash_app = app.as_weak();
    let trash_manager = Arc::clone(&session_manager);
    app.on_session_trash_selected(move |index| {
        if let Ok(mut state) = trash_manager.lock() {
            state.selected_trash_id = state
                .trash
                .get(index.max(0) as usize)
                .map(|item| item.session_id.clone());
        }
        if let Some(app) = trash_app.upgrade() {
            apply_session_manager_state(&app, &current_session_manager_vm(&trash_manager));
        }
    });

    let move_app = app.as_weak();
    let move_state = app_state.clone();
    let move_manager = Arc::clone(&session_manager);
    app.on_session_move_selected_requested(move || {
        let ui_handle = move_app.clone();
        let app_state = move_state.clone();
        let session_manager = Arc::clone(&move_manager);
        thread::spawn(move || {
            let selected_id = session_manager
                .lock()
                .ok()
                .and_then(|state| state.selected_session_id.clone());
            let message = if let Some(session_id) = selected_id {
                match move_sessions_to_trash(app_state.root(), &[session_id]) {
                    Ok(summary) => summary.message,
                    Err(error) => format!("移到废纸篓失败：{error}"),
                }
            } else {
                "请先选择一条会话。".to_string()
            };
            refresh_session_manager_state(&app_state, &session_manager);
            if let Ok(mut state) = session_manager.lock() {
                state.status_text = message;
            }
            let vm = current_session_manager_vm(&session_manager);
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                apply_session_manager_state(&app, &vm);
            });
        });
    });

    let restore_app = app.as_weak();
    let restore_state = app_state.clone();
    let restore_manager = Arc::clone(&session_manager);
    app.on_session_restore_selected_requested(move || {
        let ui_handle = restore_app.clone();
        let app_state = restore_state.clone();
        let session_manager = Arc::clone(&restore_manager);
        thread::spawn(move || {
            let selected_id = session_manager
                .lock()
                .ok()
                .and_then(|state| state.selected_trash_id.clone());
            let message = if let Some(session_id) = selected_id {
                match restore_sessions_from_trash(app_state.root(), &[session_id]) {
                    Ok(summary) => summary.message,
                    Err(error) => format!("恢复会话失败：{error}"),
                }
            } else {
                "请先选择一条待恢复会话。".to_string()
            };
            refresh_session_manager_state(&app_state, &session_manager);
            if let Ok(mut state) = session_manager.lock() {
                state.status_text = message;
            }
            let vm = current_session_manager_vm(&session_manager);
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                apply_session_manager_state(&app, &vm);
            });
        });
    });
}

fn restore_saved_session(
    app: &AppWindow,
    config: Arc<AppConfig>,
    app_state: AppStateStore,
    token_store: SystemCredentialStore,
    auth_session: SharedAuthSession,
    pending_totp_token: Arc<Mutex<Option<String>>>,
    available_groups: SharedGroups,
    billing_summary: SharedBillingSummary,
    subscription_summary: SharedSubscriptionSummary,
    redeem_history: SharedRedeemHistory,
    recent_orders: SharedOrders,
    checkout_info: SharedCheckoutInfo,
    pending_payment: SharedPendingPayment,
    usage_page: SharedUsagePage,
    usage_query: SharedUsageQuery,
) {
    let app_handle = app.as_weak();
    if let Some(message) = config.packaged_local_debug_api_message() {
        app.set_auth_status_text(SharedString::from(message));
        return;
    }
    let auth_preferences = app_state
        .load_auth_preferences()
        .ok()
        .flatten()
        .unwrap_or_default();
    let saved_refresh_token = token_store.load_refresh_token().ok().flatten();
    let saved_password = token_store.load_password().ok().flatten();
    let saved_email = app_state.load_last_email().ok().flatten();

    if !should_restore_session(
        &auth_preferences,
        saved_refresh_token.is_some() || saved_password.is_some(),
    ) {
        return;
    }

    match saved_refresh_token {
        Some(refresh_token) => {
            app.set_auth_status_text(SharedString::from("正在恢复上次登录状态..."));
            thread::spawn(move || {
                let client = ApiClient::new(config.api_base_url.clone());
                match refresh_token_blocking(&client, &RefreshTokenRequest::new(refresh_token)) {
                    Ok(token_pair) => {
                        let user_client = ApiClient::new(config.api_base_url.clone())
                            .with_access_token(Some(token_pair.access_token.clone()));
                        match fetch_current_user_blocking(&user_client) {
                            Ok(user) => {
                                let session = AuthSession {
                                    access_token: token_pair.access_token,
                                    user,
                                };
                                if let Ok(mut session_slot) = auth_session.lock() {
                                    *session_slot = Some(session.clone());
                                }
                                if let Ok(mut pending) = pending_totp_token.lock() {
                                    *pending = None;
                                }
                                if let Ok(mut pending) = pending_payment.lock() {
                                    *pending = None;
                                }

                                let (group_count, groups_snapshot, billing_vm) =
                                    sync_user_side_state(
                                        &user_client,
                                        &auth_session,
                                        &available_groups,
                                        &billing_summary,
                                        &subscription_summary,
                                        &recent_orders,
                                        &redeem_history,
                                        &checkout_info,
                                        &usage_page,
                                        &usage_query,
                                    );
                                let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                                let billing_summary_snapshot =
                                    current_billing_summary_snapshot(&billing_summary);
                                let dashboard_vm =
                                    current_dashboard_vm(&auth_session, &billing_summary);
                                let usage_vm = current_usage_vm(&usage_page);
                                let _ = app_handle.upgrade_in_event_loop(move |app| {
                                    apply_authenticated_state(
                                        &app,
                                        &session,
                                        group_count,
                                        billing_summary_snapshot.as_ref(),
                                    );
                                    apply_dashboard_state(&app, &dashboard_vm);
                                    apply_available_groups_state(&app, &groups_snapshot);
                                    apply_billing_state(&app, &billing_vm);
                                    apply_checkout_state(&app, checkout_snapshot.as_ref());
                                    apply_usage_state(&app, &usage_vm);
                                    app.set_auth_status_text(SharedString::from(
                                        "已恢复上次登录状态。",
                                    ));
                                    app.set_session_active(true);
                                    app.set_auth_subview(0);
                                    app.set_show_login_totp(false);
                                    app.set_current_section(0);
                                });
                            }
                            Err(error) => {
                                let _ = token_store.clear_refresh_token();
                                if let (Some(email), Some(password)) =
                                    (saved_email.clone(), saved_password.clone())
                                {
                                    let login_client = ApiClient::new(config.api_base_url.clone());
                                    match login_blocking(
                                        &login_client,
                                        &sub2api_desktop::api::auth::LoginRequest::new(
                                            email.as_str(),
                                            password.as_str(),
                                        ),
                                    ) {
                                        Ok(LoginResponse::Authenticated(auth)) => {
                                            handle_auth_success(
                                                &config,
                                                &app_state,
                                                &token_store,
                                                &auth_preferences,
                                                &auth_session,
                                                &pending_totp_token,
                                                &available_groups,
                                                &billing_summary,
                                                &subscription_summary,
                                                &redeem_history,
                                                &recent_orders,
                                                &checkout_info,
                                                &pending_payment,
                                                &usage_page,
                                                &usage_query,
                                                email,
                                                Some(password),
                                                auth,
                                            );
                                            let groups_snapshot =
                                                current_groups_snapshot(&available_groups);
                                            let billing_summary_snapshot =
                                                current_billing_summary_snapshot(&billing_summary);
                                            let billing_vm = current_billing_vm(
                                                &auth_session,
                                                &available_groups,
                                                &billing_summary,
                                                &subscription_summary,
                                                &recent_orders,
                                                &redeem_history,
                                            );
                                            let dashboard_vm = current_dashboard_vm(
                                                &auth_session,
                                                &billing_summary,
                                            );
                                            let checkout_snapshot =
                                                current_checkout_snapshot(&checkout_info);
                                            let usage_vm = current_usage_vm(&usage_page);
                                            let group_count = Some(groups_snapshot.len());
                                            let _ = app_handle.upgrade_in_event_loop(move |app| {
                                                if let Some(session) = auth_session
                                                    .lock()
                                                    .ok()
                                                    .and_then(|state| state.clone())
                                                {
                                                    apply_authenticated_state(
                                                        &app,
                                                        &session,
                                                        group_count,
                                                        billing_summary_snapshot.as_ref(),
                                                    );
                                                    apply_dashboard_state(&app, &dashboard_vm);
                                                    apply_available_groups_state(
                                                        &app,
                                                        &groups_snapshot,
                                                    );
                                                    apply_billing_state(&app, &billing_vm);
                                                    apply_checkout_state(
                                                        &app,
                                                        checkout_snapshot.as_ref(),
                                                    );
                                                    apply_usage_state(&app, &usage_vm);
                                                    app.set_auth_status_text(SharedString::from(
                                                        "已用保存的账号密码恢复登录状态。",
                                                    ));
                                                }
                                            });
                                        }
                                        Ok(LoginResponse::TotpRequired { .. }) | Err(_) => {
                                            let _ = app_handle.upgrade_in_event_loop(move |app| {
                                                apply_logged_out_state(&app);
                                                app.set_auth_status_text(SharedString::from(
                                                    format!("恢复登录失败：{error}，请重新登录。"),
                                                ));
                                            });
                                        }
                                    }
                                } else {
                                    let _ = app_handle.upgrade_in_event_loop(move |app| {
                                        apply_logged_out_state(&app);
                                        app.set_auth_status_text(SharedString::from(format!(
                                            "恢复登录失败：{error}，请重新登录。"
                                        )));
                                    });
                                }
                            }
                        }
                    }
                    Err(error) => {
                        let _ = token_store.clear_refresh_token();
                        let _ = app_handle.upgrade_in_event_loop(move |app| {
                            apply_logged_out_state(&app);
                            app.set_auth_status_text(SharedString::from(format!(
                                "会话已失效：{error}，请重新登录。"
                            )));
                        });
                    }
                }
            });
        }
        None => {
            if let (Some(email), Some(password)) = (saved_email, saved_password) {
                app.set_auth_status_text(SharedString::from(
                    "正在使用保存的账号密码恢复登录状态...",
                ));
                thread::spawn(move || {
                    let client = ApiClient::new(config.api_base_url.clone());
                    match login_blocking(
                        &client,
                        &sub2api_desktop::api::auth::LoginRequest::new(
                            email.as_str(),
                            password.as_str(),
                        ),
                    ) {
                        Ok(LoginResponse::Authenticated(auth)) => {
                            handle_auth_success(
                                &config,
                                &app_state,
                                &token_store,
                                &auth_preferences,
                                &auth_session,
                                &pending_totp_token,
                                &available_groups,
                                &billing_summary,
                                &subscription_summary,
                                &redeem_history,
                                &recent_orders,
                                &checkout_info,
                                &pending_payment,
                                &usage_page,
                                &usage_query,
                                email,
                                Some(password),
                                auth,
                            );
                            let groups_snapshot = current_groups_snapshot(&available_groups);
                            let billing_summary_snapshot =
                                current_billing_summary_snapshot(&billing_summary);
                            let billing_vm = current_billing_vm(
                                &auth_session,
                                &available_groups,
                                &billing_summary,
                                &subscription_summary,
                                &recent_orders,
                                &redeem_history,
                            );
                            let dashboard_vm =
                                current_dashboard_vm(&auth_session, &billing_summary);
                            let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                            let usage_vm = current_usage_vm(&usage_page);
                            let group_count = Some(groups_snapshot.len());
                            let _ = app_handle.upgrade_in_event_loop(move |app| {
                                if let Some(session) =
                                    auth_session.lock().ok().and_then(|state| state.clone())
                                {
                                    apply_authenticated_state(
                                        &app,
                                        &session,
                                        group_count,
                                        billing_summary_snapshot.as_ref(),
                                    );
                                    apply_dashboard_state(&app, &dashboard_vm);
                                    apply_available_groups_state(&app, &groups_snapshot);
                                    apply_billing_state(&app, &billing_vm);
                                    apply_checkout_state(&app, checkout_snapshot.as_ref());
                                    apply_usage_state(&app, &usage_vm);
                                    app.set_auth_status_text(SharedString::from(
                                        "已自动登录当前账户。",
                                    ));
                                    app.set_session_active(true);
                                    app.set_auth_subview(0);
                                    app.set_show_login_totp(false);
                                    app.set_current_section(0);
                                }
                            });
                        }
                        _ => {
                            let _ = app_handle.upgrade_in_event_loop(move |app| {
                                apply_logged_out_state(&app);
                                app.set_auth_status_text(SharedString::from(
                                    "自动登录失败，请手动重新登录。",
                                ));
                            });
                        }
                    }
                });
            }
        }
    }
}

fn handle_auth_success(
    config: &AppConfig,
    app_state: &AppStateStore,
    token_store: &SystemCredentialStore,
    auth_preferences: &AuthPreferences,
    auth_session: &SharedAuthSession,
    pending_totp_token: &Arc<Mutex<Option<String>>>,
    available_groups: &SharedGroups,
    billing_summary: &SharedBillingSummary,
    subscription_summary: &SharedSubscriptionSummary,
    redeem_history: &SharedRedeemHistory,
    recent_orders: &SharedOrders,
    checkout_info: &SharedCheckoutInfo,
    pending_payment: &SharedPendingPayment,
    usage_page: &SharedUsagePage,
    usage_query: &SharedUsageQuery,
    email: String,
    remembered_password: Option<String>,
    auth: AuthResponse,
) {
    let _ = app_state.save_last_email(&email);
    let _ = app_state.save_auth_preferences(auth_preferences);
    if auth_preferences.remember_password {
        if let Some(password) = remembered_password.as_deref() {
            let _ = token_store.save_password(password);
        }
        if let Some(refresh_token) = auth.refresh_token.as_deref() {
            let _ = token_store.save_refresh_token(refresh_token);
        }
    } else {
        let _ = token_store.clear_refresh_token();
        let _ = token_store.clear_password();
    }
    if let Ok(mut pending) = pending_totp_token.lock() {
        *pending = None;
    }

    let session = AuthSession {
        access_token: auth.access_token.clone(),
        user: auth.user,
    };

    if let Ok(mut session_slot) = auth_session.lock() {
        *session_slot = Some(session.clone());
    }

    let client =
        ApiClient::new(config.api_base_url.clone()).with_access_token(Some(session.access_token));
    if let Ok(mut state) = pending_payment.lock() {
        *state = None;
    }
    let _ = sync_user_side_state(
        &client,
        auth_session,
        available_groups,
        billing_summary,
        subscription_summary,
        recent_orders,
        redeem_history,
        checkout_info,
        usage_page,
        usage_query,
    );
}

fn spawn_platform_refresh_loop(
    app_handle: slint::Weak<AppWindow>,
    config: Arc<AppConfig>,
    auth_session: SharedAuthSession,
    token_store: SystemCredentialStore,
    session_id: String,
    interval: Duration,
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
) {
    thread::spawn(move || {
        while !stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
            thread::sleep(interval);
            if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            match refresh_platform_session(&config, &auth_session, &token_store, &session_id) {
                Ok(()) => {
                    let _ = app_handle.upgrade_in_event_loop(move |app| {
                        app.set_launch_status_text(SharedString::from(
                            "平台代理会话已续期，连接保持有效。",
                        ))
                    });
                }
                Err(error) => {
                    let _ = app_handle.upgrade_in_event_loop(move |app| {
                        app.set_launch_status_text(SharedString::from(format!(
                            "平台代理会话续期失败：{error}"
                        )))
                    });
                }
            }
        }
    });
}

fn spawn_platform_exit_watcher(
    app_handle: slint::Weak<AppWindow>,
    config: Arc<AppConfig>,
    auth_session: SharedAuthSession,
    token_store: SystemCredentialStore,
    session_id: String,
    runtime_root: std::path::PathBuf,
    injected_user_home: Option<std::path::PathBuf>,
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    mut child: std::process::Child,
) {
    thread::spawn(move || {
        let _ = child.wait();
        stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        let revoke_result =
            revoke_platform_session(&config, &auth_session, &token_store, &session_id);
        if let Some(user_home) = injected_user_home.as_ref() {
            let _ = restore_user_codex_config(user_home);
        }
        let _ = std::fs::remove_dir_all(&runtime_root);

        let _ = app_handle.upgrade_in_event_loop(move |app| match revoke_result {
            Ok(()) => app.set_launch_status_text(SharedString::from(
                "平台代理会话已正常结束，并完成回收清理。",
            )),
            Err(error) => app.set_launch_status_text(SharedString::from(format!(
                "平台代理进程已退出，但会话回收失败：{error}"
            ))),
        });
    });
}

fn spawn_windows_store_exit_watcher(
    app_handle: slint::Weak<AppWindow>,
    config: Arc<AppConfig>,
    auth_session: SharedAuthSession,
    token_store: SystemCredentialStore,
    session_id: String,
    runtime_root: std::path::PathBuf,
    user_home: std::path::PathBuf,
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
) {
    thread::spawn(move || {
        let mut seen_running = false;
        for _ in 0..60 {
            if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            if windows_store_desktop_is_running_for_launch() {
                seen_running = true;
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }

        if seen_running {
            while windows_store_desktop_is_running_for_launch() {
                if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                thread::sleep(Duration::from_secs(2));
            }
        }

        stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        let revoke_result =
            revoke_platform_session(&config, &auth_session, &token_store, &session_id);
        let restore_result = restore_user_codex_config(&user_home);
        let _ = std::fs::remove_dir_all(&runtime_root);

        let _ = app_handle.upgrade_in_event_loop(move |app| {
            let message = match (revoke_result, restore_result) {
                (Ok(()), Ok(())) => "平台代理会话已正常结束，并完成回收清理。".to_string(),
                (Err(error), Ok(())) => {
                    format!("平台代理桌面版已退出，但会话回收失败：{error}")
                }
                (Ok(()), Err(error)) => {
                    format!("平台代理桌面版已退出，但用户配置恢复失败：{error}")
                }
                (Err(revoke_error), Err(restore_error)) => format!(
                    "平台代理桌面版已退出，但会话回收失败：{revoke_error}；用户配置恢复失败：{restore_error}"
                ),
            };
            app.set_launch_status_text(SharedString::from(message));
        });
    });
}

fn refresh_platform_session(
    config: &AppConfig,
    auth_session: &SharedAuthSession,
    token_store: &SystemCredentialStore,
    session_id: &str,
) -> anyhow::Result<()> {
    let client = authenticated_client(config, auth_session, token_store)?;
    let _ = refresh_desktop_session_blocking(&client, session_id)?;
    Ok(())
}

fn revoke_platform_session(
    config: &AppConfig,
    auth_session: &SharedAuthSession,
    token_store: &SystemCredentialStore,
    session_id: &str,
) -> anyhow::Result<()> {
    let client = authenticated_client(config, auth_session, token_store)?;
    let _ = revoke_desktop_session_blocking(&client, session_id)?;
    Ok(())
}

fn authenticated_client(
    config: &AppConfig,
    auth_session: &SharedAuthSession,
    token_store: &SystemCredentialStore,
) -> anyhow::Result<ApiClient> {
    if let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) {
        return Ok(ApiClient::new(config.api_base_url.clone())
            .with_access_token(Some(session.access_token)));
    }

    let refresh_token = token_store
        .load_refresh_token()?
        .ok_or_else(|| anyhow::anyhow!("缺少 refresh token，无法恢复认证"))?;
    let fresh_pair = refresh_token_blocking(
        &ApiClient::new(config.api_base_url.clone()),
        &RefreshTokenRequest::new(refresh_token),
    )?;
    let user_client = ApiClient::new(config.api_base_url.clone())
        .with_access_token(Some(fresh_pair.access_token.clone()));
    let user = fetch_current_user_blocking(&user_client)?;
    if let Ok(mut state) = auth_session.lock() {
        *state = Some(AuthSession {
            access_token: fresh_pair.access_token.clone(),
            user,
        });
    }
    let _ = token_store.save_refresh_token(&fresh_pair.refresh_token);
    Ok(user_client)
}

fn sync_user_side_state(
    client: &ApiClient,
    auth_session: &SharedAuthSession,
    available_groups: &SharedGroups,
    billing_summary: &SharedBillingSummary,
    subscription_summary: &SharedSubscriptionSummary,
    recent_orders: &SharedOrders,
    redeem_history: &SharedRedeemHistory,
    checkout_info: &SharedCheckoutInfo,
    usage_page: &SharedUsagePage,
    usage_query: &SharedUsageQuery,
) -> (Option<usize>, Vec<GroupSummary>, BillingViewModel) {
    let group_count = refresh_available_groups_state(client, available_groups);
    refresh_billing_state(
        client,
        billing_summary,
        subscription_summary,
        recent_orders,
        redeem_history,
    );
    refresh_checkout_info_state(client, checkout_info);
    refresh_usage_page_state(client, usage_page, usage_query);
    let groups_snapshot = current_groups_snapshot(available_groups);
    let billing_vm = current_billing_vm(
        auth_session,
        available_groups,
        billing_summary,
        subscription_summary,
        recent_orders,
        redeem_history,
    );
    (group_count, groups_snapshot, billing_vm)
}

fn refresh_available_groups_state(
    client: &ApiClient,
    available_groups: &SharedGroups,
) -> Option<usize> {
    match fetch_available_groups_blocking(client) {
        Ok(groups) => {
            let count = groups.len();
            if let Ok(mut state) = available_groups.lock() {
                *state = groups;
            }
            Some(count)
        }
        Err(_) => None,
    }
}

fn refresh_billing_state(
    client: &ApiClient,
    billing_summary: &SharedBillingSummary,
    subscription_summary: &SharedSubscriptionSummary,
    recent_orders: &SharedOrders,
    redeem_history: &SharedRedeemHistory,
) {
    if let Ok(summary) = fetch_billing_summary_blocking(client) {
        if let Ok(mut state) = billing_summary.lock() {
            *state = Some(summary);
        }
    }
    if let Ok(summary) = fetch_subscription_summary_blocking(client) {
        if let Ok(mut state) = subscription_summary.lock() {
            *state = Some(summary);
        }
    }
    if let Ok(history) = fetch_redeem_history_blocking(client) {
        if let Ok(mut state) = redeem_history.lock() {
            *state = history;
        }
    }
    if let Ok(page) = fetch_my_orders_blocking(client) {
        if let Ok(mut state) = recent_orders.lock() {
            *state = page.items;
        }
    }
}

fn refresh_checkout_info_state(client: &ApiClient, checkout_info: &SharedCheckoutInfo) {
    if let Ok(checkout) = fetch_checkout_info_blocking(client) {
        if let Ok(mut state) = checkout_info.lock() {
            *state = Some(checkout);
        }
    }
}

fn refresh_usage_page_state(
    client: &ApiClient,
    usage_page: &SharedUsagePage,
    usage_query: &SharedUsageQuery,
) {
    let query = build_usage_query(usage_query);
    if let Ok(page) = fetch_usage_logs_blocking(client, &query) {
        if let Ok(mut state) = usage_page.lock() {
            *state = Some(page);
        }
    }
}

fn build_usage_query(usage_query: &SharedUsageQuery) -> UsageQuery {
    usage_query
        .lock()
        .ok()
        .map(|state| UsageQuery {
            page: state.page,
            page_size: state.page_size,
            sort_by: state.view_mode.sort_by().to_string(),
            sort_order: state.view_mode.sort_order().to_string(),
        })
        .unwrap_or_default()
}

fn current_usage_query_state(usage_query: &SharedUsageQuery) -> UsageQueryState {
    usage_query
        .lock()
        .ok()
        .map(|state| state.clone())
        .unwrap_or_default()
}

fn fetch_and_store_usage_page(
    client: &ApiClient,
    usage_page: &SharedUsagePage,
    usage_query: &SharedUsageQuery,
) -> anyhow::Result<PaginatedUsageLogs> {
    let query = build_usage_query(usage_query);
    let page = fetch_usage_logs_blocking(client, &query)?;
    if let Ok(mut state) = usage_page.lock() {
        *state = Some(page.clone());
    }
    Ok(page)
}

fn export_usage_excel(
    client: &ApiClient,
    usage_query: &SharedUsageQuery,
) -> anyhow::Result<PathBuf> {
    let current = current_usage_query_state(usage_query);
    let first_page = fetch_usage_logs_blocking(
        client,
        &UsageQuery {
            page: 1,
            page_size: current.page_size,
            sort_by: current.view_mode.sort_by().to_string(),
            sort_order: current.view_mode.sort_order().to_string(),
        },
    )?;

    let mut all_items = first_page.items.clone();
    for page_number in 2..=first_page.pages {
        let next_page = fetch_usage_logs_blocking(
            client,
            &UsageQuery {
                page: page_number,
                page_size: current.page_size,
                sort_by: current.view_mode.sort_by().to_string(),
                sort_order: current.view_mode.sort_order().to_string(),
            },
        )?;
        all_items.extend(next_page.items);
    }

    let output_dir = std::env::temp_dir().join("sub2api-desktop-exports");
    std::fs::create_dir_all(&output_dir)?;
    let filename = format!(
        "usage-details-{}.xlsx",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs()
    );
    let path = output_dir.join(filename);

    let mut workbook = rust_xlsxwriter::Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.write_string(0, 0, "模型")?;
    worksheet.write_string(0, 1, "时间")?;
    worksheet.write_string(0, 2, "输入（含缓存输入）")?;
    worksheet.write_string(0, 3, "输出（含缓存输出）")?;
    worksheet.write_string(0, 4, "费用")?;

    for (index, item) in all_items.iter().enumerate() {
        let row = (index + 1) as u32;
        worksheet.write_string(row, 0, item.model.as_str())?;
        worksheet.write_string(row, 1, item.created_at.as_str())?;
        worksheet.write_number(
            row,
            2,
            (item.input_tokens + item.cache_creation_tokens + item.cache_read_tokens) as f64,
        )?;
        worksheet.write_string(
            row,
            3,
            if item.image_count > 0 && item.output_tokens == 0 {
                format!("图片 {}", item.image_count)
            } else {
                item.output_tokens.to_string()
            }
            .as_str(),
        )?;
        worksheet.write_number(row, 4, item.actual_cost)?;
    }

    workbook.save(&path)?;
    Ok(path)
}

fn current_groups_snapshot(available_groups: &SharedGroups) -> Vec<GroupSummary> {
    available_groups
        .lock()
        .ok()
        .map(|state| state.clone())
        .unwrap_or_default()
}

fn platform_launch_groups(groups: &[GroupSummary]) -> Vec<GroupSummary> {
    groups
        .iter()
        .filter(|group| {
            group.platform == GroupPlatform::OpenAI
                && group.status.eq_ignore_ascii_case("active")
                && !group.claude_code_only
        })
        .cloned()
        .collect()
}

fn current_checkout_snapshot(checkout_info: &SharedCheckoutInfo) -> Option<CheckoutInfo> {
    checkout_info.lock().ok().and_then(|state| state.clone())
}

fn current_usage_vm(usage_page: &SharedUsagePage) -> UsageDetailViewModel {
    let page = usage_page.lock().ok().and_then(|state| state.clone());
    UsageDetailViewModel::from_page(page.as_ref())
}

fn current_billing_summary_snapshot(
    billing_summary: &SharedBillingSummary,
) -> Option<BillingSummary> {
    billing_summary.lock().ok().and_then(|state| state.clone())
}

fn current_dashboard_vm(
    auth_session: &SharedAuthSession,
    billing_summary: &SharedBillingSummary,
) -> DashboardViewModel {
    let user = auth_session
        .lock()
        .ok()
        .and_then(|state| state.as_ref().map(|session| session.user.clone()));
    let summary = current_billing_summary_snapshot(billing_summary);
    user.map(|user| DashboardViewModel::from_user_and_billing(&user, summary.as_ref()))
        .unwrap_or_else(DashboardViewModel::empty)
}

fn current_billing_vm(
    auth_session: &SharedAuthSession,
    available_groups: &SharedGroups,
    billing_summary: &SharedBillingSummary,
    subscription_summary: &SharedSubscriptionSummary,
    recent_orders: &SharedOrders,
    redeem_history: &SharedRedeemHistory,
) -> BillingViewModel {
    let user = auth_session
        .lock()
        .ok()
        .and_then(|state| state.as_ref().map(|session| session.user.clone()));
    let token_summary = current_billing_summary_snapshot(billing_summary);
    let summary = subscription_summary
        .lock()
        .ok()
        .and_then(|state| state.clone());
    let orders = recent_orders
        .lock()
        .ok()
        .map(|state| state.clone())
        .unwrap_or_default();
    let history = redeem_history
        .lock()
        .ok()
        .map(|state| state.clone())
        .unwrap_or_default();
    let groups = available_groups
        .lock()
        .ok()
        .map(|state| state.clone())
        .unwrap_or_default();
    let openai_groups = groups
        .iter()
        .filter(|group| group.platform == GroupPlatform::OpenAI)
        .cloned()
        .collect::<Vec<_>>();
    let openai_group_ids = openai_groups
        .iter()
        .map(|group| group.id)
        .collect::<Vec<_>>();
    let filtered_summary = summary.map(|current| SubscriptionSummary {
        active_count: current
            .subscriptions
            .iter()
            .filter(|item| openai_group_ids.contains(&item.group_id))
            .count() as i32,
        total_used_usd: current
            .subscriptions
            .iter()
            .filter(|item| openai_group_ids.contains(&item.group_id))
            .map(|item| item.monthly_used_usd)
            .sum(),
        subscriptions: current
            .subscriptions
            .iter()
            .filter(|item| openai_group_ids.contains(&item.group_id))
            .cloned()
            .collect(),
    });
    let active_openai_group = filtered_summary
        .as_ref()
        .and_then(|current| current.subscriptions.first())
        .and_then(|subscription| {
            openai_groups
                .iter()
                .find(|group| group.id == subscription.group_id)
        })
        .or_else(|| openai_groups.first());
    BillingViewModel::from_account_state(
        user.as_ref(),
        token_summary.as_ref(),
        filtered_summary.as_ref(),
        &orders,
        &history,
        active_openai_group,
    )
}

#[derive(Debug, Clone)]
struct SessionManagerViewModel {
    home_count_text: String,
    session_count_text: String,
    trash_count_text: String,
    status_text: String,
    group_lines: Vec<String>,
    selected_group_index: i32,
    session_lines: Vec<String>,
    selected_session_index: i32,
    trash_lines: Vec<String>,
    selected_trash_index: i32,
}

fn refresh_session_manager_state(app_state: &AppStateStore, session_manager: &SharedSessionManager) {
    let homes = list_session_homes(app_state.root()).unwrap_or_default();
    let groups = list_session_groups(app_state.root()).unwrap_or_default();
    let session_ids = groups
        .iter()
        .flat_map(|group| group.sessions.iter().map(|session| session.session_id.clone()))
        .collect::<Vec<_>>();
    let token_stats = get_session_token_stats(app_state.root(), &session_ids)
        .unwrap_or_default()
        .into_iter()
        .map(|item| (item.session_id.clone(), item))
        .collect::<HashMap<_, _>>();
    let trash = list_trashed_sessions(app_state.root()).unwrap_or_default();

    if let Ok(mut state) = session_manager.lock() {
        let previous_group_cwd = state
            .groups
            .get(state.selected_group_index.max(0) as usize)
            .map(|group| group.cwd.clone());
        let previous_session_id = state.selected_session_id.clone();
        let previous_trash_id = state.selected_trash_id.clone();

        state.homes = homes;
        state.groups = groups;
        state.token_stats = token_stats;
        state.trash = trash;

        state.selected_group_index = previous_group_cwd
            .and_then(|cwd| {
                state
                    .groups
                    .iter()
                    .position(|group| group.cwd == cwd)
                    .map(|index| index as i32)
            })
            .unwrap_or_else(|| if state.groups.is_empty() { -1 } else { 0 });

        state.selected_session_id = if let Some(session_id) = previous_session_id {
            if state
                .groups
                .iter()
                .flat_map(|group| group.sessions.iter())
                .any(|session| session.session_id == session_id)
            {
                Some(session_id)
            } else {
                state
                    .groups
                    .get(state.selected_group_index.max(0) as usize)
                    .and_then(|group| group.sessions.first().map(|session| session.session_id.clone()))
            }
        } else {
            state
                .groups
                .get(state.selected_group_index.max(0) as usize)
                .and_then(|group| group.sessions.first().map(|session| session.session_id.clone()))
        };

        state.selected_trash_id = if let Some(trash_id) = previous_trash_id {
            if state.trash.iter().any(|item| item.session_id == trash_id) {
                Some(trash_id)
            } else {
                state.trash.first().map(|item| item.session_id.clone())
            }
        } else {
            state.trash.first().map(|item| item.session_id.clone())
        };
    }
}

fn current_session_manager_vm(session_manager: &SharedSessionManager) -> SessionManagerViewModel {
    let state = session_manager.lock().ok().map(|item| item.clone()).unwrap_or_default();
    let selected_group = state
        .groups
        .get(state.selected_group_index.max(0) as usize)
        .cloned();
    let selected_session_index = selected_group
        .as_ref()
        .and_then(|group| {
            state
                .selected_session_id
                .as_ref()
                .and_then(|selected| {
                    group
                        .sessions
                        .iter()
                        .position(|session| session.session_id == *selected)
                })
        })
        .map(|index| index as i32)
        .unwrap_or_else(|| if selected_group.as_ref().is_some_and(|group| !group.sessions.is_empty()) { 0 } else { -1 });
    let selected_trash_index = state
        .selected_trash_id
        .as_ref()
        .and_then(|selected| state.trash.iter().position(|item| item.session_id == *selected))
        .map(|index| index as i32)
        .unwrap_or_else(|| if state.trash.is_empty() { -1 } else { 0 });

    SessionManagerViewModel {
        home_count_text: state.homes.len().to_string(),
        session_count_text: state
            .groups
            .iter()
            .map(|group| group.sessions.len())
            .sum::<usize>()
            .to_string(),
        trash_count_text: state.trash.len().to_string(),
        status_text: if state.status_text.trim().is_empty() {
            "本地会话会显示在这里。".to_string()
        } else {
            state.status_text.clone()
        },
        group_lines: if state.groups.is_empty() {
            vec!["暂无工作区".to_string()]
        } else {
            state.groups.iter().map(format_session_group_line).collect()
        },
        selected_group_index: if state.groups.is_empty() {
            0
        } else {
            state.selected_group_index.max(0)
        },
        session_lines: selected_group
            .map(|group| {
                if group.sessions.is_empty() {
                    vec!["暂无会话".to_string()]
                } else {
                    group
                        .sessions
                        .iter()
                        .map(|session| format_session_entry_line(session, state.token_stats.get(&session.session_id)))
                        .collect()
                }
            })
            .unwrap_or_else(|| vec!["暂无会话".to_string()]),
        selected_session_index: selected_session_index.max(0),
        trash_lines: if state.trash.is_empty() {
            vec!["废纸篓为空".to_string()]
        } else {
            state.trash.iter().map(format_trashed_session_line).collect()
        },
        selected_trash_index: selected_trash_index.max(0),
    }
}

fn apply_session_manager_state(app: &AppWindow, vm: &SessionManagerViewModel) {
    app.set_session_home_count_text(SharedString::from(vm.home_count_text.clone()));
    app.set_session_count_text(SharedString::from(vm.session_count_text.clone()));
    app.set_session_trash_count_text(SharedString::from(vm.trash_count_text.clone()));
    app.set_session_status_text(SharedString::from(vm.status_text.clone()));
    app.set_session_group_lines(string_model(
        vm.group_lines.iter().cloned().map(SharedString::from).collect(),
    ));
    app.set_session_selected_group_index(vm.selected_group_index);
    app.set_session_entry_lines(string_model(
        vm.session_lines.iter().cloned().map(SharedString::from).collect(),
    ));
    app.set_session_selected_entry_index(vm.selected_session_index);
    app.set_session_trash_lines(string_model(
        vm.trash_lines.iter().cloned().map(SharedString::from).collect(),
    ));
    app.set_session_selected_trash_index(vm.selected_trash_index);
}

fn format_session_group_line(group: &SessionGroup) -> String {
    format!(
        "{}\n{} 条会话 · 最近 {}",
        resolve_session_group_label(&group.cwd),
        group.sessions.len(),
        format_session_timestamp(group.latest_updated_at)
    )
}

fn format_session_entry_line(
    session: &sub2api_desktop::platform::session_manager::SessionRecord,
    stats: Option<&SessionTokenStats>,
) -> String {
    let locations = session
        .locations
        .iter()
        .map(|location| location.home_label.clone())
        .collect::<Vec<_>>()
        .join(" / ");
    let token_text = stats
        .map(|item| {
            format!(
                "输入 {} / 输出 {} / 总计 {}",
                format_large_count(item.input_tokens),
                format_large_count(item.output_tokens),
                format_large_count(item.total_tokens)
            )
        })
        .unwrap_or_else(|| "Token 暂无统计".to_string());
    format!(
        "{}\n{} · {}\n{}",
        session.title,
        format_session_id_short(&session.session_id),
        format_session_timestamp(session.updated_at),
        if locations.is_empty() {
            token_text
        } else {
            format!("{token_text}\n{locations}")
        }
    )
}

fn format_trashed_session_line(session: &TrashedSessionRecord) -> String {
    let locations = session
        .locations
        .iter()
        .map(|location| location.home_label.clone())
        .collect::<Vec<_>>()
        .join(" / ");
    format!(
        "{}\n{} · 删除于 {}\n{}",
        session.title,
        format_session_id_short(&session.session_id),
        format_session_timestamp(session.deleted_at),
        if locations.is_empty() { session.cwd.clone() } else { locations }
    )
}

fn resolve_session_group_label(cwd: &str) -> String {
    cwd.replace('\\', "/")
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(cwd)
        .to_string()
}

fn format_session_id_short(session_id: &str) -> String {
    if session_id.len() <= 18 {
        session_id.to_string()
    } else {
        format!("{}...{}", &session_id[..8], &session_id[session_id.len() - 6..])
    }
}

fn format_large_count(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn format_session_timestamp(value: Option<i64>) -> String {
    let Some(value) = value else {
        return "时间未知".to_string();
    };
    let seconds = if value > 1_000_000_000_000 { value / 1000 } else { value };
    let now = chrono::Utc::now().timestamp();
    let diff = (now - seconds).max(0);
    if diff < 3600 {
        format!("{} 分钟前", (diff / 60).max(1))
    } else if diff < 86_400 {
        format!("{} 小时前", diff / 3600)
    } else if diff < 604_800 {
        format!("{} 天前", diff / 86_400)
    } else {
        chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0)
            .map(|value| value.format("%m-%d %H:%M").to_string())
            .unwrap_or_else(|| seconds.to_string())
    }
}

fn apply_launch_state(app: &AppWindow, targets: &[InstalledTarget]) {
    let vm = LaunchViewModel::from_targets(targets);
    app.set_desktop_available(vm.desktop_available);
    app.set_cli_available(vm.cli_available);
    app.set_launch_status_text(SharedString::from(vm.status_text));
}

fn apply_logged_out_state(app: &AppWindow) {
    app.set_session_active(false);
    app.set_auth_busy(false);
    app.set_auth_subview(0);
    app.set_show_login_totp(false);
    app.set_current_section(0);
    app.set_brand_status_copy(SharedString::from("你的电子牛马已就位。"));
    app.set_dashboard_user_label(SharedString::from("当前账号：未登录"));
    app.set_dashboard_balance_text(SharedString::from("--"));
    app.set_dashboard_usage_text(SharedString::from("--"));
    app.set_dashboard_account_status_text(SharedString::from("账户状态：待登录"));
    app.set_dashboard_notice_text(SharedString::from(
        "登录后可直接查看余额、套餐、订单与兑换记录，并在需要时切换到官方模式。",
    ));
    app.set_launch_group_options(single_option_model("登录后加载桌面客户端专用分组"));
    app.set_launch_selected_group_index(0);
    apply_billing_state(app, &BillingViewModel::empty());
    apply_checkout_state(app, None);
    app.set_billing_checkout_status_text(SharedString::from("登录后可创建充值或订阅订单。"));
    apply_usage_state(app, &UsageDetailViewModel::empty());
    app.set_usage_status_text(SharedString::from("登录后可查看消费明细。"));
}

fn apply_authenticated_state(
    app: &AppWindow,
    session: &AuthSession,
    group_count: Option<usize>,
    billing_summary: Option<&BillingSummary>,
) {
    app.set_session_active(true);
    app.set_auth_busy(false);
    app.set_auth_subview(0);
    app.set_show_login_totp(false);
    app.set_current_section(0);
    app.set_brand_status_copy(SharedString::from("电子牛马已经把你的工作台准备好了。"));
    let dashboard = DashboardViewModel::from_user_and_billing(&session.user, billing_summary);
    app.set_dashboard_user_label(SharedString::from(format!(
        "当前账号：{}",
        session.user.display_name()
    )));
    app.set_dashboard_balance_text(SharedString::from(dashboard.balance_text));
    app.set_dashboard_usage_text(SharedString::from(dashboard.usage_text));
    app.set_dashboard_account_status_text(SharedString::from(format!(
        "账户状态：{}{}",
        session.user.status,
        session
            .user
            .run_mode
            .as_deref()
            .map(|mode| format!(" / 运行模式：{mode}"))
            .unwrap_or_default()
    )));
    app.set_dashboard_notice_text(SharedString::from(match group_count {
        Some(count) => format!(
            "已登录，当前服务端已为桌面客户端准备 {count} 个可用分组；客户端会固定使用首个桌面专用分组启动。"
        ),
        None => "已登录，但暂未拉到桌面客户端分组；可稍后重试或继续使用官方模式。".to_string(),
    }));
    app.set_auth_status_text(SharedString::from("登录成功，可继续进入启动中心。"));
}

fn apply_dashboard_state(app: &AppWindow, dashboard: &DashboardViewModel) {
    app.set_dashboard_balance_text(SharedString::from(dashboard.balance_text.clone()));
    app.set_dashboard_usage_text(SharedString::from(dashboard.usage_text.clone()));
}

fn wire_update_callbacks(app: &AppWindow, config: Arc<AppConfig>) {
    start_desktop_update_check(app.as_weak(), Arc::clone(&config), false);
    start_desktop_announcement_refresh(app.as_weak(), Arc::clone(&config));

    let manual_update_app = app.as_weak();
    let manual_update_config = Arc::clone(&config);
    app.on_manual_update_check_requested(move || {
        start_desktop_update_check(
            manual_update_app.clone(),
            Arc::clone(&manual_update_config),
            true,
        );
    });

    let secondary_update_app = app.as_weak();
    app.on_update_secondary_requested(move || {
        if let Some(app) = secondary_update_app.upgrade() {
            app.set_update_dialog_visible(false);
        }
    });

    let primary_update_app = app.as_weak();
    let primary_update_config = Arc::clone(&config);
    app.on_update_primary_requested(move || {
        if let Some(app) = primary_update_app.upgrade() {
            let download_url = app.get_update_download_url().to_string();
            match resolve_desktop_download_url(&primary_update_config.api_base_url, &download_url)
                .and_then(|resolved| {
                    open_update_download_url(&resolved)?;
                    Ok(resolved)
                }) {
                Ok(resolved) => {
                    app.set_update_summary(SharedString::from(format!(
                        "已打开更新下载链接，请完成安装后重启客户端：{resolved}"
                    )));
                    if !app.get_update_force() {
                        app.set_update_dialog_visible(false);
                    }
                }
                Err(error) => {
                    app.set_update_dialog_visible(true);
                    app.set_update_summary(SharedString::from(format!(
                        "打开更新下载失败：{error}"
                    )));
                }
            }
        }
    });
}

fn start_desktop_update_check(
    app_handle: slint::Weak<AppWindow>,
    config: Arc<AppConfig>,
    show_dialog_on_no_update: bool,
) {
    thread::spawn(move || {
        let client = ApiClient::new(config.api_base_url.clone());
        let current_version = env!("CARGO_PKG_VERSION").to_string();

        match check_desktop_update_blocking(&client, &current_version) {
            Ok(check) if check.has_update => {
                let vm = build_update_view_model(&check);
                let _ = app_handle.upgrade_in_event_loop(move |app| {
                    apply_update_check_state(&app, &check, &vm);
                });
            }
            Ok(check) if show_dialog_on_no_update => {
                let _ = app_handle.upgrade_in_event_loop(move |app| {
                    app.set_update_dialog_visible(true);
                    app.set_update_force(false);
                    app.set_update_current_version(SharedString::from(current_version.clone()));
                    app.set_update_latest_version(SharedString::from(check.latest_version.clone()));
                    app.set_update_download_url(SharedString::from(check.download_url.clone()));
                    app.set_update_summary(SharedString::from("当前版本已是最新，可稍后再检查。"));
                    apply_announcement_highlight(
                        &app,
                        &check.latest_version,
                        &check.title,
                        if check.release_notes.trim().is_empty() {
                            &check.summary
                        } else {
                            &check.release_notes
                        },
                    );
                });
            }
            Ok(_) => {}
            Err(error) if show_dialog_on_no_update => {
                let message = format!("检查更新失败：{error}");
                let _ = app_handle.upgrade_in_event_loop(move |app| {
                    app.set_update_dialog_visible(true);
                    app.set_update_force(false);
                    app.set_update_current_version(SharedString::from(current_version.clone()));
                    app.set_update_latest_version(SharedString::from(current_version.clone()));
                    app.set_update_summary(SharedString::from(message.clone()));
                });
            }
            Err(_) => {}
        }
    });
}

fn start_desktop_announcement_refresh(app_handle: slint::Weak<AppWindow>, config: Arc<AppConfig>) {
    thread::spawn(move || {
        let client = ApiClient::new(config.api_base_url.clone());
        if let Ok(items) = list_desktop_announcements_blocking(&client) {
            let _ = app_handle.upgrade_in_event_loop(move |app| {
                apply_desktop_announcements(&app, &items);
            });
        }
    });
}

fn build_update_view_model(check: &DesktopUpdateCheckResponse) -> UpdateViewModel {
    let summary = if check.release_notes.trim().is_empty() {
        check.summary.clone()
    } else {
        check.release_notes.clone()
    };
    UpdateViewModel::available(
        check.current_version.clone(),
        check.latest_version.clone(),
        check.force_update,
        check.title.clone(),
        summary,
    )
}

fn apply_update_check_state(
    app: &AppWindow,
    check: &DesktopUpdateCheckResponse,
    vm: &UpdateViewModel,
) {
    apply_update_view_model(app, vm);
    app.set_update_download_url(SharedString::from(check.download_url.clone()));
    apply_announcement_highlight(
        app,
        &check.latest_version,
        &check.title,
        if check.release_notes.trim().is_empty() {
            &check.summary
        } else {
            &check.release_notes
        },
    );
}

fn apply_update_view_model(app: &AppWindow, vm: &UpdateViewModel) {
    app.set_update_dialog_visible(!matches!(vm.state, UpdateDialogState::Hidden));
    app.set_update_force(vm.force_update);
    app.set_update_current_version(SharedString::from(vm.current_version.clone()));
    app.set_update_latest_version(SharedString::from(vm.latest_version.clone()));
    app.set_update_summary(SharedString::from(vm.summary.clone()));
}

fn apply_announcement_highlight(app: &AppWindow, latest_version: &str, title: &str, summary: &str) {
    app.set_announcement_hero_version(SharedString::from(latest_version.to_string()));
    app.set_announcement_hero_title(SharedString::from(title.to_string()));
    app.set_announcement_hero_summary(SharedString::from(summary.to_string()));
}

fn apply_desktop_announcements(app: &AppWindow, items: &[DesktopAnnouncementItem]) {
    if items.is_empty() {
        app.set_announcement_version_lines(single_option_model("暂无版本动态"));
        app.set_announcement_feed_lines(single_option_model("暂无系统公告"));
        return;
    }

    if let Some(first) = items.first() {
        app.set_announcement_hero_title(SharedString::from(first.title.clone()));
        app.set_announcement_hero_summary(SharedString::from(first.content.clone()));
    }

    let version_lines = items
        .iter()
        .take(4)
        .map(|item| {
            SharedString::from(format!(
                "{} · {}",
                announcement_kind_label(&item.kind),
                item.title
            ))
        })
        .collect::<Vec<_>>();
    app.set_announcement_version_lines(string_model(version_lines));

    let feed_lines = items
        .iter()
        .take(6)
        .map(|item| SharedString::from(format!("{}\n{}", item.title, item.content.trim())))
        .collect::<Vec<_>>();
    app.set_announcement_feed_lines(string_model(feed_lines));
}

fn announcement_kind_label(kind: &str) -> &'static str {
    let normalized = kind.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "release" => "版本更新",
        "maintenance" => "维护通知",
        "notice" => "使用提醒",
        _ => "系统公告",
    }
}

fn open_update_download_url(url: &str) -> anyhow::Result<()> {
    open_external_target(url)
}

fn apply_available_groups_state(app: &AppWindow, groups: &[GroupSummary]) {
    let launchable_groups = platform_launch_groups(groups);
    if launchable_groups.is_empty() {
        app.set_launch_group_options(single_option_model("当前没有可用于桌面客户端的 OpenAI 分组"));
        app.set_launch_selected_group_index(0);
        return;
    }

    let labels = launchable_groups
        .iter()
        .map(|group| SharedString::from(format!("{} · {}", group.name, group.status)))
        .collect::<Vec<_>>();
    app.set_launch_group_options(string_model(labels));
    app.set_launch_selected_group_index(0);
}

fn first_openai_group(groups: &[GroupSummary]) -> Option<&GroupSummary> {
    groups
        .iter()
        .find(|group| group.platform == GroupPlatform::OpenAI)
}

fn apply_billing_state(app: &AppWindow, billing: &BillingViewModel) {
    app.set_billing_plan_title(SharedString::from(billing.plan_title.clone()));
    app.set_billing_balance_headline(SharedString::from(billing.balance_headline.clone()));
    app.set_billing_usage_caption(SharedString::from(billing.usage_caption.clone()));
    app.set_subscription_summary_text(SharedString::from(
        billing.subscription_summary_text.clone(),
    ));
    app.set_subscription_lines(string_model(
        billing
            .subscription_lines
            .iter()
            .cloned()
            .map(SharedString::from)
            .collect(),
    ));
    app.set_subscription_detail_lines(string_model(
        billing
            .subscription_detail_lines
            .iter()
            .cloned()
            .map(SharedString::from)
            .collect(),
    ));
    app.set_order_lines(string_model(
        billing
            .order_lines
            .iter()
            .cloned()
            .map(SharedString::from)
            .collect(),
    ));
    app.set_order_detail_lines(string_model(
        billing
            .order_detail_lines
            .iter()
            .cloned()
            .map(SharedString::from)
            .collect(),
    ));
    app.set_redeem_history_lines(string_model(
        billing
            .redeem_history_lines
            .iter()
            .cloned()
            .map(SharedString::from)
            .collect(),
    ));
}

fn apply_usage_state(app: &AppWindow, usage: &UsageDetailViewModel) {
    app.set_usage_summary_title(SharedString::from(usage.summary_title.clone()));
    app.set_usage_total_requests_text(SharedString::from(usage.total_requests_text.clone()));
    app.set_usage_total_tokens_text(SharedString::from(usage.total_tokens_text.clone()));
    app.set_usage_total_actual_cost_text(SharedString::from(usage.total_actual_cost_text.clone()));
    app.set_usage_model_lines(string_model(
        usage
            .model_lines
            .iter()
            .cloned()
            .map(SharedString::from)
            .collect(),
    ));
    app.set_usage_time_lines(string_model(
        usage
            .time_lines
            .iter()
            .cloned()
            .map(SharedString::from)
            .collect(),
    ));
    app.set_usage_input_lines(string_model(
        usage
            .input_lines
            .iter()
            .cloned()
            .map(SharedString::from)
            .collect(),
    ));
    app.set_usage_output_lines(string_model(
        usage
            .output_lines
            .iter()
            .cloned()
            .map(SharedString::from)
            .collect(),
    ));
    app.set_usage_page_meta_text(SharedString::from(usage.page_meta_text.clone()));
    app.set_usage_view_mode_options(string_model(
        UsageViewMode::options()
            .into_iter()
            .map(SharedString::from)
            .collect(),
    ));
}

fn apply_checkout_state(app: &AppWindow, checkout: Option<&CheckoutInfo>) {
    let payment_methods = checkout
        .map(ordered_payment_method_labels)
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| vec!["暂无可用支付方式".to_string()]);
    app.set_billing_payment_method_options(string_model(
        payment_methods
            .into_iter()
            .map(SharedString::from)
            .collect(),
    ));
    app.set_billing_selected_payment_method_index(0);

    let subscription_plans = checkout
        .map(ordered_subscription_plan_labels)
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| vec!["暂无可用 OpenAI 套餐".to_string()]);
    app.set_billing_subscription_plan_options(string_model(
        subscription_plans
            .into_iter()
            .map(SharedString::from)
            .collect(),
    ));
    app.set_billing_selected_subscription_plan_index(0);
}

fn ordered_payment_method_keys(checkout: &CheckoutInfo) -> Vec<String> {
    const PREFERRED_ORDER: [&str; 6] = [
        "alipay_direct",
        "alipay",
        "wxpay_direct",
        "wxpay",
        "stripe",
        "easypay",
    ];
    let mut keys = checkout
        .methods
        .iter()
        .filter(|(_, limit)| limit.available)
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    keys.sort_by_key(|key| {
        PREFERRED_ORDER
            .iter()
            .position(|candidate| *candidate == key)
            .unwrap_or(PREFERRED_ORDER.len())
    });
    keys
}

fn ordered_payment_method_labels(checkout: &CheckoutInfo) -> Vec<String> {
    ordered_payment_method_keys(checkout)
        .into_iter()
        .map(|key| {
            let limit = checkout.methods.get(&key);
            let fee = limit.map(|item| item.fee_rate).unwrap_or_default();
            let title = match key.as_str() {
                "alipay" => "支付宝",
                "alipay_direct" => "支付宝直连",
                "wxpay" => "微信支付",
                "wxpay_direct" => "微信直连",
                "stripe" => "Stripe",
                "easypay" => "易支付",
                _ => key.as_str(),
            };
            if fee > 0.0 {
                format!("{title} · 手续费 {fee:.2}%")
            } else {
                title.to_string()
            }
        })
        .collect()
}

fn ordered_subscription_plans(checkout: &CheckoutInfo) -> Vec<SubscriptionPlan> {
    let mut plans = checkout
        .plans
        .iter()
        .filter(|plan| plan.for_sale)
        .filter(|plan| {
            plan.group_platform
                .as_deref()
                .map(|platform| platform.eq_ignore_ascii_case("openai"))
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    plans.sort_by(|left, right| {
        left.sort_order.cmp(&right.sort_order).then_with(|| {
            left.price
                .partial_cmp(&right.price)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    plans
}

fn ordered_subscription_plan_labels(checkout: &CheckoutInfo) -> Vec<String> {
    ordered_subscription_plans(checkout)
        .into_iter()
        .map(|plan| {
            format!(
                "{} · ￥{:.2} / {}{}",
                plan.name, plan.price, plan.validity_days, plan.validity_unit
            )
        })
        .collect()
}

fn format_redeem_success_message(result: &sub2api_desktop::api::redeem::RedeemResult) -> String {
    match result.r#type.as_str() {
        "token" => format!(
            "兑换成功：已入账 {}",
            format_token_count(result.token_amount.unwrap_or(result.value))
        ),
        "balance" => format!("兑换成功：已入账 ¥{:.2}", result.value),
        "concurrency" => format!("兑换成功：已增加 {:.0} 路并发", result.value),
        "subscription" => match result.validity_days {
            Some(days) if days > 0 => format!("兑换成功：订阅已延长 {days} 天"),
            Some(days) if days < 0 => format!("兑换成功：订阅已调整 {} 天", days.abs()),
            _ => "兑换成功：订阅已更新".to_string(),
        },
        other => format!("兑换成功：{other} 类型 CDK 已处理"),
    }
}

fn create_payment_open_target(
    result: &sub2api_desktop::api::payment::CreateOrderResult,
    request: &CreateOrderRequest,
) -> anyhow::Result<String> {
    if let Some(pay_url) = result
        .pay_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
    {
        return Ok(pay_url.to_string());
    }

    if let Some(qr_code) = result
        .qr_code
        .as_deref()
        .filter(|code| !code.trim().is_empty())
    {
        let path = write_payment_qr_page(
            result.order_id,
            qr_code,
            &request.payment_type,
            request.order_type.as_str(),
            result.expires_at.as_deref(),
        )?;
        return Ok(path.to_string_lossy().into_owned());
    }

    anyhow::bail!("后端没有返回可用的支付入口");
}

fn write_payment_qr_page(
    order_id: i64,
    qr_code: &str,
    payment_type: &str,
    order_type: &str,
    expires_at: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let temp_dir = std::env::temp_dir().join("sub2api-desktop-payments");
    std::fs::create_dir_all(&temp_dir)?;
    let file_path = temp_dir.join(format!("order-{order_id}.html"));
    let expires_text = expires_at.unwrap_or("未返回截止时间");
    let payment_label = match payment_type {
        "alipay" | "alipay_direct" => "支付宝",
        "wxpay" | "wxpay_direct" => "微信支付",
        "stripe" => "Stripe",
        "easypay" => "易支付",
        _ => payment_type,
    };
    let title = if order_type == "subscription" {
        "订阅支付"
    } else {
        "余额充值"
    };
    let escaped_qr = escape_html(qr_code);
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>{title} - 订单 #{order_id}</title>
  <script src="https://cdn.jsdelivr.net/npm/qrcode@1.5.4/build/qrcode.min.js"></script>
  <style>
    body {{
      font-family: "Inter", "Microsoft YaHei UI", sans-serif;
      background: #f7f9fb;
      color: #2c3437;
      display: flex;
      align-items: center;
      justify-content: center;
      min-height: 100vh;
      margin: 0;
    }}
    .card {{
      width: min(92vw, 540px);
      background: #ffffff;
      border-radius: 20px;
      box-shadow: 0 20px 60px rgba(44, 52, 55, 0.08);
      padding: 32px;
    }}
    h1 {{ margin: 0 0 8px; font-size: 28px; }}
    p {{ margin: 0 0 10px; color: #596064; }}
    #qr {{
      display: flex;
      justify-content: center;
      align-items: center;
      padding: 24px 0;
    }}
    code {{
      display: block;
      background: #f0f4f7;
      border-radius: 12px;
      padding: 12px;
      word-break: break-all;
      white-space: pre-wrap;
      color: #51616b;
    }}
  </style>
</head>
<body>
  <div class="card">
    <h1>{title}</h1>
    <p>订单 #{order_id} · 支付方式：{payment_label}</p>
    <p>请使用手机扫码完成支付，完成后回到桌面客户端点击“刷新订单”。</p>
    <p>订单有效期：{expires_text}</p>
    <div id="qr"></div>
    <p>如果二维码无法显示，可使用下面的原始内容手动处理：</p>
    <code>{escaped_qr}</code>
  </div>
  <script>
    QRCode.toCanvas(document.getElementById('qr'), {qr_json}, {{
      width: 280,
      margin: 2,
      errorCorrectionLevel: 'M'
    }});
  </script>
</body>
</html>"#,
        qr_json = serde_json::to_string(qr_code)?,
    );
    std::fs::write(&file_path, html)?;
    Ok(file_path)
}

fn write_use_key_guide_page(
    api_base_url: &str,
    api_key: &str,
    expires_at: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let temp_dir = std::env::temp_dir().join("sub2api-desktop-use-key");
    std::fs::create_dir_all(&temp_dir)?;
    let file_path = temp_dir.join("use-key-guide.html");
    let expires_text = expires_at.unwrap_or("7 天后自动过期");
    let config_toml = format!(
        r#"model_provider = "OpenAI"
model = "gpt-5.4"
review_model = "gpt-5.4"
model_reasoning_effort = "xhigh"
disable_response_storage = true
network_access = "enabled"
windows_wsl_setup_acknowledged = true
model_context_window = 1000000
model_auto_compact_token_limit = 900000

[model_providers.OpenAI]
name = "OpenAI"
base_url = "{api_base_url}"
wire_api = "responses"
requires_openai_auth = true"#,
    );
    let auth_json = format!(
        r#"{{
  "OPENAI_API_KEY": "{api_key}"
}}"#
    );
    let generic_example = format!(
        r#"curl {api_base_url}/chat/completions \
  -H "Authorization: Bearer {api_key}" \
  -H "Content-Type: application/json" \
  -d "{{\"model\":\"gpt-5.4\",\"messages\":[{{\"role\":\"user\",\"content\":\"hello\"}}]}}"#
    );

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>使用密钥说明</title>
  <style>
    body {{ font-family: "Inter", "Microsoft YaHei UI", sans-serif; background: #f7f9fb; color: #2c3437; margin: 0; padding: 32px; }}
    .wrap {{ max-width: 960px; margin: 0 auto; }}
    .card {{ background: #fff; border-radius: 18px; padding: 24px; margin-bottom: 20px; box-shadow: 0 20px 60px rgba(44,52,55,0.08); }}
    pre {{ background: #0f172a; color: #e2e8f0; border-radius: 14px; padding: 18px; overflow-x: auto; white-space: pre-wrap; word-break: break-all; }}
    h1,h2 {{ margin-top: 0; }}
    .tip {{ color: #596064; line-height: 1.7; }}
    .tag {{ display:inline-block; padding:6px 12px; border-radius:999px; background:#eef8ff; color:#006499; font-weight:700; font-size:12px; }}
  </style>
</head>
<body>
  <div class="wrap">
    <div class="card">
      <span class="tag">7 天有效</span>
      <h1>查看使用密钥</h1>
      <p class="tip">这是一个专门给第三方应用使用的短期 OpenAI 兼容 API Key。过期时间：{expires_text}</p>
      <p class="tip">你可以把它用于 Codex、OpenCode，或任何支持 OpenAI 协议的第三方应用。为了安全，建议只在当前 7 天窗口内使用。</p>
    </div>
    <div class="card">
      <h2>config.toml</h2>
      <pre>{config_toml}</pre>
    </div>
    <div class="card">
      <h2>auth.json</h2>
      <pre>{auth_json}</pre>
    </div>
    <div class="card">
      <h2>第三方应用直连示例</h2>
      <pre>{generic_example}</pre>
    </div>
  </div>
</body>
</html>"#,
        config_toml = escape_html(&config_toml),
        auth_json = escape_html(&auth_json),
        generic_example = escape_html(&generic_example),
    );
    std::fs::write(&file_path, html)?;
    Ok(file_path)
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn open_external_target(target: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(target)
            .spawn()?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = target;
        Err(anyhow::anyhow!("当前平台暂未实现外部链接跳转"))
    }
}

fn launch_first_target(app: &AppWindow, targets: &[InstalledTarget], kind: LaunchTarget) {
    let Some(target) = targets.iter().find(|target| target.kind == kind) else {
        app.set_launch_status_text(SharedString::from("未检测到可启动的 Codex 目标"));
        return;
    };

    match launch_official(target) {
        Ok(()) => {
            app.set_launch_status_text(SharedString::from(format!(
                "已请求启动 {}",
                target.display_name
            )));
        }
        Err(error) => {
            app.set_launch_status_text(SharedString::from(format!(
                "启动 {} 失败：{}",
                target.display_name, error
            )));
        }
    }
}

enum AuthFlowOutcome {
    Authenticated(AuthResponse),
    TotpRequired {
        temp_token: Option<String>,
        masked_email: Option<String>,
    },
}

fn single_option_model(text: &str) -> ModelRc<SharedString> {
    string_model(vec![SharedString::from(text)])
}

fn string_model(values: Vec<SharedString>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(values))
}

fn local_device_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "desktop-client".to_string())
}

fn local_device_id(target_kind: LaunchTarget) -> String {
    let host = local_device_name();
    let suffix = match target_kind {
        LaunchTarget::Desktop => "desktop",
        LaunchTarget::Cli => "cli",
    };
    format!("{host}-{suffix}").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{format_redeem_success_message, platform_launch_groups};
    use sub2api_desktop::api::{
        groups::{GroupPlatform, GroupSummary, SubscriptionType},
        redeem::{RedeemHistoryGroup, RedeemResult},
    };

    fn test_group(id: i64, name: &str, platform: GroupPlatform) -> GroupSummary {
        GroupSummary {
            id,
            name: name.to_string(),
            description: None,
            platform,
            rate_multiplier: 1.0,
            is_exclusive: false,
            status: "active".to_string(),
            subscription_type: SubscriptionType::Standard,
            daily_limit_usd: None,
            weekly_limit_usd: None,
            monthly_limit_usd: None,
            image_price_1k: None,
            image_price_2k: None,
            image_price_4k: None,
            input_price_per_million_tokens: Some(1.5),
            claude_code_only: false,
            fallback_group_id: None,
            fallback_group_id_on_invalid_request: None,
            require_oauth_only: false,
            require_privacy_set: false,
            created_at: "2025-01-02T15:04:05Z".to_string(),
            updated_at: "2025-01-02T15:04:05Z".to_string(),
        }
    }

    #[test]
    fn platform_launch_groups_keep_only_active_openai_groups() {
        let groups = vec![
            test_group(5, "Anthropic", GroupPlatform::Anthropic),
            test_group(6, "OpenAI", GroupPlatform::OpenAI),
            test_group(7, "GPT", GroupPlatform::OpenAI),
        ];

        let launchable = platform_launch_groups(&groups);

        assert_eq!(launchable.len(), 2);
        assert_eq!(launchable[0].id, 6);
        assert_eq!(launchable[1].id, 7);
    }

    #[test]
    fn platform_launch_groups_skip_inactive_or_claude_only_entries() {
        let mut inactive = test_group(8, "Inactive", GroupPlatform::OpenAI);
        inactive.status = "disabled".to_string();
        let mut claude_only = test_group(9, "ClaudeOnly", GroupPlatform::OpenAI);
        claude_only.claude_code_only = true;

        let launchable = platform_launch_groups(&[inactive, claude_only]);

        assert!(launchable.is_empty());
    }

    #[test]
    fn redeem_success_message_prefers_token_amount_for_token_codes() {
        let result = RedeemResult {
            id: 1,
            code: "CDK-123".to_string(),
            r#type: "token".to_string(),
            value: 100_000_000.0,
            token_amount: Some(100_000_000.0),
            status: "used".to_string(),
            used_at: Some("2025-01-02T15:04:05Z".to_string()),
            created_at: "2025-01-01T15:04:05Z".to_string(),
            group_id: Some(9),
            validity_days: Some(0),
            group: Some(RedeemHistoryGroup {
                id: 9,
                name: "desktop-openai".to_string(),
            }),
        };

        assert_eq!(format_redeem_success_message(&result), "兑换成功：已入账 1亿 Token");
    }
}
