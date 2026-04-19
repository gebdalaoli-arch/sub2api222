slint::include_modules!();

use slint::{ModelRc, SharedString, VecModel};
use std::{
    cell::RefCell,
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
        desktop_sessions::{
            create_desktop_session_blocking, refresh_desktop_session_blocking,
            revoke_desktop_session_blocking, DesktopSessionCreateRequest, DesktopSessionTarget,
        },
        groups::{fetch_available_groups_blocking, GroupPlatform, GroupSummary},
        http::ApiClient,
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
        usage::{fetch_usage_logs_blocking, PaginatedUsageLogs},
    },
    app::{
        auth_flow::{build_login_submission, should_restore_session, LoginSubmission},
        launch_errors::describe_platform_launch_error,
        view_models::{
            billing_vm::BillingViewModel,
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
            restore_user_codex_config, write_platform_home, write_runtime_metadata,
            ManagedHomePaths,
        },
        runtime_bootstrap::StartupDiagnostics,
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
type SharedSubscriptionSummary = Arc<Mutex<Option<SubscriptionSummary>>>;
type SharedRedeemHistory = Arc<Mutex<Vec<RedeemHistoryItem>>>;
type SharedOrders = Arc<Mutex<Vec<PaymentOrder>>>;
type SharedCheckoutInfo = Arc<Mutex<Option<CheckoutInfo>>>;
type SharedPendingPayment = Arc<Mutex<Option<PendingPaymentState>>>;
type SharedUsagePage = Arc<Mutex<Option<PaginatedUsageLogs>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingPaymentState {
    order_id: i64,
    open_target: String,
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
    let subscription_summary: SharedSubscriptionSummary = Arc::new(Mutex::new(None));
    let redeem_history: SharedRedeemHistory = Arc::new(Mutex::new(Vec::new()));
    let recent_orders: SharedOrders = Arc::new(Mutex::new(Vec::new()));
    let checkout_info: SharedCheckoutInfo = Arc::new(Mutex::new(None));
    let pending_payment: SharedPendingPayment = Arc::new(Mutex::new(None));
    let usage_page: SharedUsagePage = Arc::new(Mutex::new(None));

    apply_launch_state(&app, &targets.borrow());
    apply_logged_out_state(&app);
    preload_local_state(&app, &app_state, &token_store);
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
        Arc::clone(&subscription_summary),
        Arc::clone(&redeem_history),
        Arc::clone(&recent_orders),
        Arc::clone(&checkout_info),
        Arc::clone(&pending_payment),
        Arc::clone(&usage_page),
    );
    wire_billing_callbacks(
        &app,
        Arc::clone(&config),
        Arc::clone(&auth_session),
        Arc::clone(&available_groups),
        Arc::clone(&subscription_summary),
        Arc::clone(&redeem_history),
        Arc::clone(&recent_orders),
        Arc::clone(&checkout_info),
        Arc::clone(&pending_payment),
        Arc::clone(&usage_page),
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
    );
    wire_update_callbacks(&app, Arc::clone(&config));
    restore_saved_session(
        &app,
        Arc::clone(&config),
        app_state.clone(),
        token_store,
        Arc::clone(&auth_session),
        Arc::clone(&pending_totp_token),
        Arc::clone(&available_groups),
        Arc::clone(&subscription_summary),
        Arc::clone(&redeem_history),
        Arc::clone(&recent_orders),
        Arc::clone(&checkout_info),
        Arc::clone(&pending_payment),
        Arc::clone(&usage_page),
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
    let selected_group_index = app.get_launch_selected_group_index() as usize;
    app.set_launch_status_text(SharedString::from("正在创建平台代理会话..."));

    let ui_handle = app_handle.clone();
    thread::spawn(move || {
        let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                app.set_launch_status_text(SharedString::from(
                    "请先登录并加载可用分组，再启动平台代理模式。",
                ))
            });
            return;
        };

        let groups = platform_launch_groups(&current_groups_snapshot(&available_groups));
        let Some(group) = groups.get(selected_group_index).cloned() else {
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                app.set_launch_status_text(SharedString::from(
                    "当前没有可用于 Codex 的 OpenAI 分组，请先检查套餐或切换分组。",
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
    subscription_summary: SharedSubscriptionSummary,
    redeem_history: SharedRedeemHistory,
    recent_orders: SharedOrders,
    checkout_info: SharedCheckoutInfo,
    pending_payment: SharedPendingPayment,
    usage_page: SharedUsagePage,
) {
    let login_app = app.as_weak();
    let login_config = Arc::clone(&config);
    let login_state = app_state.clone();
    let login_store = token_store.clone();
    let login_session = Arc::clone(&auth_session);
    let login_totp = Arc::clone(&pending_totp_token);
    let login_groups = Arc::clone(&available_groups);
    let login_summary = Arc::clone(&subscription_summary);
    let login_history = Arc::clone(&redeem_history);
    let login_orders = Arc::clone(&recent_orders);
    let login_checkout = Arc::clone(&checkout_info);
    let login_pending_payment = Arc::clone(&pending_payment);
    let login_usage_page = Arc::clone(&usage_page);
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
                app.set_auth_status_text(SharedString::from(message));
                return;
            }
        };
        if let Some(message) = login_config.packaged_local_debug_api_message() {
            app.set_auth_status_text(SharedString::from(message));
            return;
        }

        app.set_auth_status_text(SharedString::from("正在处理登录请求..."));
        let ui_handle = login_app.clone();
        let config = Arc::clone(&login_config);
        let app_state = login_state.clone();
        let token_store = login_store.clone();
        let auth_session = Arc::clone(&login_session);
        let pending_totp_token = Arc::clone(&login_totp);
        let available_groups = Arc::clone(&login_groups);
        let subscription_summary = Arc::clone(&login_summary);
        let redeem_history = Arc::clone(&login_history);
        let recent_orders = Arc::clone(&login_orders);
        let checkout_info = Arc::clone(&login_checkout);
        let pending_payment = Arc::clone(&login_pending_payment);
        let usage_page = Arc::clone(&login_usage_page);
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
                        &subscription_summary,
                        &redeem_history,
                        &recent_orders,
                        &checkout_info,
                        &pending_payment,
                        &usage_page,
                        email,
                        Some(password.clone()),
                        auth,
                    );
                    let groups_snapshot = current_groups_snapshot(&available_groups);
                    let billing_vm = current_billing_vm(
                        &auth_session,
                        &available_groups,
                        &subscription_summary,
                        &recent_orders,
                        &redeem_history,
                    );
                    let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                    let usage_vm = current_usage_vm(&usage_page);
                    let group_count = Some(groups_snapshot.len());
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        if let Some(session) =
                            auth_session.lock().ok().and_then(|state| state.clone())
                        {
                            apply_authenticated_state(&app, &session, group_count);
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
                        app.set_auth_status_text(SharedString::from(message));
                        app.set_show_login_totp(true);
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
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
    let register_summary = Arc::clone(&subscription_summary);
    let register_history = Arc::clone(&redeem_history);
    let register_orders = Arc::clone(&recent_orders);
    let register_checkout = Arc::clone(&checkout_info);
    let register_pending_payment = Arc::clone(&pending_payment);
    let register_usage_page = Arc::clone(&usage_page);
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
            app.set_auth_status_text(SharedString::from("注册前请填写邮箱、密码和邮箱验证码。"));
            return;
        }
        if let Some(message) = register_config.packaged_local_debug_api_message() {
            app.set_auth_status_text(SharedString::from(message));
            return;
        }
        app.set_auth_status_text(SharedString::from("正在提交注册请求..."));

        let ui_handle = register_app.clone();
        let config = Arc::clone(&register_config);
        let app_state = register_state.clone();
        let token_store = register_store.clone();
        let auth_session = Arc::clone(&register_session);
        let pending_totp_token = Arc::clone(&register_totp);
        let available_groups = Arc::clone(&register_groups);
        let subscription_summary = Arc::clone(&register_summary);
        let redeem_history = Arc::clone(&register_history);
        let recent_orders = Arc::clone(&register_orders);
        let checkout_info = Arc::clone(&register_checkout);
        let pending_payment = Arc::clone(&register_pending_payment);
        let usage_page = Arc::clone(&register_usage_page);
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
                        &subscription_summary,
                        &redeem_history,
                        &recent_orders,
                        &checkout_info,
                        &pending_payment,
                        &usage_page,
                        email,
                        Some(password.clone()),
                        auth,
                    );
                    let groups_snapshot = current_groups_snapshot(&available_groups);
                    let billing_vm = current_billing_vm(
                        &auth_session,
                        &available_groups,
                        &subscription_summary,
                        &recent_orders,
                        &redeem_history,
                    );
                    let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                    let usage_vm = current_usage_vm(&usage_page);
                    let group_count = Some(groups_snapshot.len());
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        if let Some(session) =
                            auth_session.lock().ok().and_then(|state| state.clone())
                        {
                            apply_authenticated_state(&app, &session, group_count);
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
            app.set_auth_status_text(SharedString::from("发送验证码前请先填写邮箱。"));
            return;
        }
        if let Some(message) = verify_config.packaged_local_debug_api_message() {
            app.set_auth_status_text(SharedString::from(message));
            return;
        }
        app.set_auth_status_text(SharedString::from("正在发送验证码..."));

        let ui_handle = verify_app.clone();
        let config = Arc::clone(&verify_config);
        thread::spawn(move || {
            let client = ApiClient::new(config.api_base_url.clone());
            match send_verify_code_blocking(&client, &SendVerifyCodeRequest::new(email.trim())) {
                Ok(response) => {
                    let message = format!("验证码已发送，{} 秒后可再次发送。", response.countdown);
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        app.set_auth_status_text(SharedString::from(message))
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
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
            app.set_reset_status_text(SharedString::from("请先填写重置邮箱。"));
            return;
        }
        if let Some(message) = forgot_config.packaged_local_debug_api_message() {
            app.set_reset_status_text(SharedString::from(message));
            return;
        }
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
                        app.set_reset_status_text(SharedString::from(response.message))
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
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
            app.set_reset_status_text(SharedString::from("请填写邮箱、邮件重置码和新密码。"));
            return;
        }
        if let Some(message) = reset_config.packaged_local_debug_api_message() {
            app.set_reset_status_text(SharedString::from(message));
            return;
        }
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
                        app.set_reset_status_text(SharedString::from(response.message))
                    });
                }
                Err(error) => {
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
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
    subscription_summary: SharedSubscriptionSummary,
    redeem_history: SharedRedeemHistory,
    recent_orders: SharedOrders,
    checkout_info: SharedCheckoutInfo,
    pending_payment: SharedPendingPayment,
    usage_page: SharedUsagePage,
) {
    let redeem_app = app.as_weak();
    let redeem_config = Arc::clone(&config);
    let redeem_session = Arc::clone(&auth_session);
    let redeem_groups = Arc::clone(&available_groups);
    let redeem_summary = Arc::clone(&subscription_summary);
    let redeem_history_store = Arc::clone(&redeem_history);
    let redeem_orders = Arc::clone(&recent_orders);
    let redeem_checkout = Arc::clone(&checkout_info);
    let redeem_usage_page = Arc::clone(&usage_page);
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
        let subscription_summary = Arc::clone(&redeem_summary);
        let redeem_history_store = Arc::clone(&redeem_history_store);
        let recent_orders = Arc::clone(&redeem_orders);
        let checkout_info = Arc::clone(&redeem_checkout);
        let usage_page = Arc::clone(&redeem_usage_page);
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
                        &subscription_summary,
                        &recent_orders,
                        &redeem_history_store,
                        &checkout_info,
                        &usage_page,
                    );
                    let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                    let usage_vm = current_usage_vm(&usage_page);

                    if let Some(user) = updated_user {
                        if let Ok(mut state) = auth_session.lock() {
                            if let Some(existing) = state.as_mut() {
                                existing.user = user;
                            }
                        }
                    }

                    let status_message =
                        format!("兑换成功：{}（{}）", result.message, result.r#type);
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        if let Some(session) =
                            auth_session.lock().ok().and_then(|state| state.clone())
                        {
                            apply_authenticated_state(&app, &session, group_count);
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
    let recharge_summary = Arc::clone(&subscription_summary);
    let recharge_history = Arc::clone(&redeem_history);
    let recharge_orders = Arc::clone(&recent_orders);
    let recharge_groups = Arc::clone(&available_groups);
    let recharge_checkout = Arc::clone(&checkout_info);
    let recharge_pending = Arc::clone(&pending_payment);
    let recharge_usage_page = Arc::clone(&usage_page);
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
        let subscription_summary = Arc::clone(&recharge_summary);
        let redeem_history = Arc::clone(&recharge_history);
        let recent_orders = Arc::clone(&recharge_orders);
        let available_groups = Arc::clone(&recharge_groups);
        let checkout_info = Arc::clone(&recharge_checkout);
        let pending_payment = Arc::clone(&recharge_pending);
        let usage_page = Arc::clone(&recharge_usage_page);
        let selected_method_index = app.get_billing_selected_payment_method_index() as usize;
        thread::spawn(move || {
            let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_billing_checkout_status_text(SharedString::from("请先登录后再创建充值订单。"))
                });
                return;
            };
            let checkout_snapshot = current_checkout_snapshot(&checkout_info);
            let Some(checkout) = checkout_snapshot.as_ref() else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_billing_checkout_status_text(SharedString::from("当前支付配置尚未加载，请先刷新计费数据。"))
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
                        &subscription_summary,
                        &recent_orders,
                        &redeem_history,
                        &checkout_info,
                        &usage_page,
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
    let subscribe_summary = Arc::clone(&subscription_summary);
    let subscribe_history = Arc::clone(&redeem_history);
    let subscribe_orders = Arc::clone(&recent_orders);
    let subscribe_groups = Arc::clone(&available_groups);
    let subscribe_checkout = Arc::clone(&checkout_info);
    let subscribe_pending = Arc::clone(&pending_payment);
    let subscribe_usage_page = Arc::clone(&usage_page);
    app.on_billing_subscription_requested(move || {
        let Some(app) = subscribe_app.upgrade() else {
            return;
        };
        app.set_billing_checkout_status_text(SharedString::from("正在创建订阅订单..."));

        let ui_handle = subscribe_app.clone();
        let config = Arc::clone(&subscribe_config);
        let auth_session = Arc::clone(&subscribe_session);
        let subscription_summary = Arc::clone(&subscribe_summary);
        let redeem_history = Arc::clone(&subscribe_history);
        let recent_orders = Arc::clone(&subscribe_orders);
        let available_groups = Arc::clone(&subscribe_groups);
        let checkout_info = Arc::clone(&subscribe_checkout);
        let pending_payment = Arc::clone(&subscribe_pending);
        let usage_page = Arc::clone(&subscribe_usage_page);
        let selected_method_index = app.get_billing_selected_payment_method_index() as usize;
        let selected_plan_index = app.get_billing_selected_subscription_plan_index() as usize;
        thread::spawn(move || {
            let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_billing_checkout_status_text(SharedString::from("请先登录后再创建订阅订单。"))
                });
                return;
            };
            let checkout_snapshot = current_checkout_snapshot(&checkout_info);
            let Some(checkout) = checkout_snapshot.as_ref() else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_billing_checkout_status_text(SharedString::from("当前支付配置尚未加载，请先刷新计费数据。"))
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
                        &subscription_summary,
                        &recent_orders,
                        &redeem_history,
                        &checkout_info,
                        &usage_page,
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
        let Some(pending) = reopen_pending_payment.lock().ok().and_then(|state| state.clone()) else {
            app.set_billing_checkout_status_text(SharedString::from("当前没有可重新打开的待支付订单。"));
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
    let refresh_billing_summary = Arc::clone(&subscription_summary);
    let refresh_billing_history = Arc::clone(&redeem_history);
    let refresh_billing_orders = Arc::clone(&recent_orders);
    let refresh_billing_checkout = Arc::clone(&checkout_info);
    let refresh_billing_pending = Arc::clone(&pending_payment);
    let refresh_billing_usage_page = Arc::clone(&usage_page);
    app.on_billing_refresh_requested(move || {
        let Some(app) = refresh_billing_app.upgrade() else {
            return;
        };
        app.set_billing_checkout_status_text(SharedString::from("正在刷新计费数据..."));

        let ui_handle = refresh_billing_app.clone();
        let config = Arc::clone(&refresh_billing_config);
        let auth_session = Arc::clone(&refresh_billing_session);
        let available_groups = Arc::clone(&refresh_billing_groups);
        let subscription_summary = Arc::clone(&refresh_billing_summary);
        let redeem_history = Arc::clone(&refresh_billing_history);
        let recent_orders = Arc::clone(&refresh_billing_orders);
        let checkout_info = Arc::clone(&refresh_billing_checkout);
        let pending_payment = Arc::clone(&refresh_billing_pending);
        let usage_page = Arc::clone(&refresh_billing_usage_page);
        thread::spawn(move || {
            let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    apply_checkout_state(&app, None);
                    app.set_billing_checkout_status_text(SharedString::from("请先登录后再刷新计费数据。"))
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
                        .map(|order| format!("最近订单 #{} 当前状态：{}", pending.order_id, order.status))
                });

            let (group_count, groups_snapshot, billing_vm) = sync_user_side_state(
                &client,
                &auth_session,
                &available_groups,
                &subscription_summary,
                &recent_orders,
                &redeem_history,
                &checkout_info,
                &usage_page,
            );
            let checkout_snapshot = current_checkout_snapshot(&checkout_info);
            let usage_vm = current_usage_vm(&usage_page);
            let status_message = pending_order_status
                .unwrap_or_else(|| "计费数据已刷新，可继续创建或追踪订单。".to_string());
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                if let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) {
                    apply_authenticated_state(&app, &session, group_count);
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
    available_groups: SharedGroups,
    subscription_summary: SharedSubscriptionSummary,
    redeem_history: SharedRedeemHistory,
    recent_orders: SharedOrders,
    checkout_info: SharedCheckoutInfo,
    usage_page: SharedUsagePage,
) {
    let usage_app = app.as_weak();
    let usage_config = Arc::clone(&config);
    let usage_session = Arc::clone(&auth_session);
    let usage_groups = Arc::clone(&available_groups);
    let usage_summary = Arc::clone(&subscription_summary);
    let usage_history = Arc::clone(&redeem_history);
    let usage_orders = Arc::clone(&recent_orders);
    let usage_checkout = Arc::clone(&checkout_info);
    let usage_page_state = Arc::clone(&usage_page);
    app.on_usage_refresh_requested(move || {
        let Some(app) = usage_app.upgrade() else {
            return;
        };
        app.set_usage_status_text(SharedString::from("正在刷新消费明细..."));

        let ui_handle = usage_app.clone();
        let config = Arc::clone(&usage_config);
        let auth_session = Arc::clone(&usage_session);
        let available_groups = Arc::clone(&usage_groups);
        let subscription_summary = Arc::clone(&usage_summary);
        let redeem_history = Arc::clone(&usage_history);
        let recent_orders = Arc::clone(&usage_orders);
        let checkout_info = Arc::clone(&usage_checkout);
        let usage_page = Arc::clone(&usage_page_state);
        thread::spawn(move || {
            let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) else {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_usage_status_text(SharedString::from("请先登录后再查看消费明细。"));
                });
                return;
            };

            let client = ApiClient::new(config.api_base_url.clone())
                .with_access_token(Some(session.access_token));
            let (group_count, groups_snapshot, billing_vm) = sync_user_side_state(
                &client,
                &auth_session,
                &available_groups,
                &subscription_summary,
                &recent_orders,
                &redeem_history,
                &checkout_info,
                &usage_page,
            );
            let usage_vm = current_usage_vm(&usage_page);
            let checkout_snapshot = current_checkout_snapshot(&checkout_info);
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                if let Some(session) = auth_session.lock().ok().and_then(|state| state.clone()) {
                    apply_authenticated_state(&app, &session, group_count);
                }
                apply_available_groups_state(&app, &groups_snapshot);
                apply_billing_state(&app, &billing_vm);
                apply_checkout_state(&app, checkout_snapshot.as_ref());
                apply_usage_state(&app, &usage_vm);
                app.set_usage_status_text(SharedString::from("消费明细已刷新。"));
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
    subscription_summary: SharedSubscriptionSummary,
    redeem_history: SharedRedeemHistory,
    recent_orders: SharedOrders,
    checkout_info: SharedCheckoutInfo,
    pending_payment: SharedPendingPayment,
    usage_page: SharedUsagePage,
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
                                        &subscription_summary,
                                        &recent_orders,
                                        &redeem_history,
                                        &checkout_info,
                                        &usage_page,
                                    );
                                let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                                let usage_vm = current_usage_vm(&usage_page);
                                let _ = app_handle.upgrade_in_event_loop(move |app| {
                                    apply_authenticated_state(&app, &session, group_count);
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
                                                &subscription_summary,
                                                &redeem_history,
                                                &recent_orders,
                                                &checkout_info,
                                                &pending_payment,
                                                &usage_page,
                                                email,
                                                Some(password),
                                                auth,
                                            );
                                            let groups_snapshot = current_groups_snapshot(&available_groups);
                                            let billing_vm = current_billing_vm(
                                                &auth_session,
                                                &available_groups,
                                                &subscription_summary,
                                                &recent_orders,
                                                &redeem_history,
                                            );
                                            let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                                            let usage_vm = current_usage_vm(&usage_page);
                                            let group_count = Some(groups_snapshot.len());
                                            let _ = app_handle.upgrade_in_event_loop(move |app| {
                                                if let Some(session) = auth_session
                                                    .lock()
                                                    .ok()
                                                    .and_then(|state| state.clone())
                                                {
                                                    apply_authenticated_state(&app, &session, group_count);
                                                    apply_available_groups_state(&app, &groups_snapshot);
                                                    apply_billing_state(&app, &billing_vm);
                                                    apply_checkout_state(&app, checkout_snapshot.as_ref());
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
                                                app.set_auth_status_text(SharedString::from(format!(
                                                    "恢复登录失败：{error}，请重新登录。"
                                                )));
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
                app.set_auth_status_text(SharedString::from("正在使用保存的账号密码恢复登录状态..."));
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
                                &subscription_summary,
                                &redeem_history,
                                &recent_orders,
                                &checkout_info,
                                &pending_payment,
                                &usage_page,
                                email,
                                Some(password),
                                auth,
                            );
                            let groups_snapshot = current_groups_snapshot(&available_groups);
                            let billing_vm = current_billing_vm(
                                &auth_session,
                                &available_groups,
                                &subscription_summary,
                                &recent_orders,
                                &redeem_history,
                            );
                            let checkout_snapshot = current_checkout_snapshot(&checkout_info);
                            let usage_vm = current_usage_vm(&usage_page);
                            let group_count = Some(groups_snapshot.len());
                            let _ = app_handle.upgrade_in_event_loop(move |app| {
                                if let Some(session) =
                                    auth_session.lock().ok().and_then(|state| state.clone())
                                {
                                    apply_authenticated_state(&app, &session, group_count);
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
    subscription_summary: &SharedSubscriptionSummary,
    redeem_history: &SharedRedeemHistory,
    recent_orders: &SharedOrders,
    checkout_info: &SharedCheckoutInfo,
    pending_payment: &SharedPendingPayment,
    usage_page: &SharedUsagePage,
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
        subscription_summary,
        recent_orders,
        redeem_history,
        checkout_info,
        usage_page,
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
    subscription_summary: &SharedSubscriptionSummary,
    recent_orders: &SharedOrders,
    redeem_history: &SharedRedeemHistory,
    checkout_info: &SharedCheckoutInfo,
    usage_page: &SharedUsagePage,
) -> (Option<usize>, Vec<GroupSummary>, BillingViewModel) {
    let group_count = refresh_available_groups_state(client, available_groups);
    refresh_billing_state(client, subscription_summary, recent_orders, redeem_history);
    refresh_checkout_info_state(client, checkout_info);
    refresh_usage_page_state(client, usage_page);
    let groups_snapshot = current_groups_snapshot(available_groups);
    let billing_vm = current_billing_vm(
        auth_session,
        available_groups,
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
    subscription_summary: &SharedSubscriptionSummary,
    recent_orders: &SharedOrders,
    redeem_history: &SharedRedeemHistory,
) {
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

fn refresh_usage_page_state(client: &ApiClient, usage_page: &SharedUsagePage) {
    if let Ok(page) = fetch_usage_logs_blocking(client, 1, 20) {
        if let Ok(mut state) = usage_page.lock() {
            *state = Some(page);
        }
    }
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
    checkout_info
        .lock()
        .ok()
        .and_then(|state| state.clone())
}

fn current_usage_vm(usage_page: &SharedUsagePage) -> UsageDetailViewModel {
    let page = usage_page
        .lock()
        .ok()
        .and_then(|state| state.clone());
    UsageDetailViewModel::from_page(page.as_ref())
}

fn current_billing_vm(
    auth_session: &SharedAuthSession,
    available_groups: &SharedGroups,
    subscription_summary: &SharedSubscriptionSummary,
    recent_orders: &SharedOrders,
    redeem_history: &SharedRedeemHistory,
) -> BillingViewModel {
    let user = auth_session
        .lock()
        .ok()
        .and_then(|state| state.as_ref().map(|session| session.user.clone()));
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
    let openai_group_ids = openai_groups.iter().map(|group| group.id).collect::<Vec<_>>();
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
        .and_then(|subscription| openai_groups.iter().find(|group| group.id == subscription.group_id))
        .or_else(|| openai_groups.first());
    BillingViewModel::from_account_state(
        user.as_ref(),
        filtered_summary.as_ref(),
        &orders,
        &history,
        active_openai_group,
    )
}

fn apply_launch_state(app: &AppWindow, targets: &[InstalledTarget]) {
    let vm = LaunchViewModel::from_targets(targets);
    app.set_desktop_available(vm.desktop_available);
    app.set_cli_available(vm.cli_available);
    app.set_launch_status_text(SharedString::from(vm.status_text));
}

fn apply_logged_out_state(app: &AppWindow) {
    app.set_session_active(false);
    app.set_auth_subview(0);
    app.set_show_login_totp(false);
    app.set_current_section(0);
    app.set_brand_status_copy(SharedString::from("你的电子牛马已就位。"));
    app.set_dashboard_user_label(SharedString::from("当前账号：未登录"));
    app.set_dashboard_balance_text(SharedString::from("余额：--"));
    app.set_dashboard_usage_text(SharedString::from("并发额度：--"));
    app.set_dashboard_account_status_text(SharedString::from("账户状态：待登录"));
    app.set_dashboard_notice_text(SharedString::from(
        "登录后可直接查看余额、套餐、订单与兑换记录，并在需要时切换到官方模式。",
    ));
    app.set_launch_group_options(single_option_model("登录后加载可用分组"));
    app.set_launch_selected_group_index(0);
    apply_billing_state(app, &BillingViewModel::empty());
    apply_checkout_state(app, None);
    app.set_billing_checkout_status_text(SharedString::from("登录后可创建充值或订阅订单。"));
    apply_usage_state(app, &UsageDetailViewModel::empty());
    app.set_usage_status_text(SharedString::from("登录后可查看消费明细。"));
}

fn apply_authenticated_state(app: &AppWindow, session: &AuthSession, group_count: Option<usize>) {
    app.set_session_active(true);
    app.set_auth_subview(0);
    app.set_show_login_totp(false);
    app.set_current_section(0);
    app.set_brand_status_copy(SharedString::from("电子牛马已经把你的工作台准备好了。"));
    let dashboard = DashboardViewModel::from_user(&session.user);
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
            "已登录，可用分组 {count} 个。平台模式将基于分组、订阅和桌面会话创建独立受管启动环境。"
        ),
        None => "已登录，但暂未拉到可用分组列表；可稍后重试或继续使用官方模式。".to_string(),
    }));
    app.set_auth_status_text(SharedString::from("登录成功，可继续进入启动中心。"));
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

fn apply_announcement_highlight(
    app: &AppWindow,
    latest_version: &str,
    title: &str,
    summary: &str,
) {
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
        .map(|item| {
            SharedString::from(format!(
                "{}\n{}",
                item.title,
                item.content.trim()
            ))
        })
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
        app.set_launch_group_options(single_option_model("当前没有可用于 Codex 的 OpenAI 分组"));
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
    app.set_usage_lines(string_model(
        usage
            .lines
            .iter()
            .cloned()
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
        left.sort_order
            .cmp(&right.sort_order)
            .then_with(|| left.price.partial_cmp(&right.price).unwrap_or(std::cmp::Ordering::Equal))
    });
    plans
}

fn ordered_subscription_plan_labels(checkout: &CheckoutInfo) -> Vec<String> {
    ordered_subscription_plans(checkout)
        .into_iter()
        .map(|plan| format!("{} · ￥{:.2} / {}{}", plan.name, plan.price, plan.validity_days, plan.validity_unit))
        .collect()
}

fn create_payment_open_target(
    result: &sub2api_desktop::api::payment::CreateOrderResult,
    request: &CreateOrderRequest,
) -> anyhow::Result<String> {
    if let Some(pay_url) = result.pay_url.as_deref().filter(|url| !url.trim().is_empty()) {
        return Ok(pay_url.to_string());
    }

    if let Some(qr_code) = result.qr_code.as_deref().filter(|code| !code.trim().is_empty()) {
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
        std::process::Command::new("explorer.exe").arg(target).spawn()?;
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
    use super::platform_launch_groups;
    use sub2api_desktop::api::groups::{GroupPlatform, GroupSummary, SubscriptionType};

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
}
