slint::include_modules!();

use slint::{ModelRc, SharedString, VecModel};
use std::{
    cell::RefCell,
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
        groups::{fetch_available_groups_blocking, GroupSummary},
        http::ApiClient,
        redeem::{
            fetch_redeem_history_blocking, redeem_code_blocking, RedeemCodeRequest,
            RedeemHistoryItem,
        },
        subscriptions::{fetch_subscription_summary_blocking, SubscriptionSummary},
    },
    app::{
        auth_flow::{build_login_submission, LoginSubmission},
        view_models::{
            billing_vm::BillingViewModel, dashboard_vm::DashboardViewModel,
            launch_vm::LaunchViewModel,
        },
    },
    config::{app_config, AppConfig},
    platform::{
        install_detection::{detect_installed_targets, InstalledTarget, LaunchTarget},
        launcher::{launch_official, launch_platform},
        managed_home::{
            cleanup_runtime_roots_older_than, write_platform_home, write_runtime_metadata,
            ManagedHomePaths,
        },
    },
    storage::{
        app_state::AppStateStore,
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

fn main() -> anyhow::Result<()> {
    let app = AppWindow::new()?;
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

    apply_launch_state(&app, &targets.borrow());
    apply_logged_out_state(&app);
    preload_local_state(&app, &app_state);
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
    );
    wire_billing_callbacks(
        &app,
        Arc::clone(&config),
        Arc::clone(&auth_session),
        Arc::clone(&available_groups),
        Arc::clone(&subscription_summary),
        Arc::clone(&redeem_history),
    );
    restore_saved_session(
        &app,
        Arc::clone(&config),
        token_store,
        Arc::clone(&auth_session),
        Arc::clone(&pending_totp_token),
        Arc::clone(&available_groups),
        Arc::clone(&subscription_summary),
        Arc::clone(&redeem_history),
    );

    app.run()?;
    Ok(())
}

fn preload_local_state(app: &AppWindow, app_state: &AppStateStore) {
    if let Ok(Some(email)) = app_state.load_last_email() {
        app.set_email(SharedString::from(email));
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

        let groups = current_groups_snapshot(&available_groups);
        let Some(group) = groups.get(selected_group_index).cloned() else {
            let _ = ui_handle.upgrade_in_event_loop(move |app| {
                app.set_launch_status_text(SharedString::from(
                    "当前没有可用分组，请先登录并确认订阅状态。",
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

                let result = write_platform_home(&paths, &gateway_url, runtime_token)
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
                    .and_then(|_| launch_platform(&target, &paths.codex_home));

                match result {
                    Ok(child) => {
                        let message = format!(
                            "已为分组“{}”创建平台代理会话，并启动 {}。",
                            group.name, target.display_name
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
                        spawn_platform_exit_watcher(
                            ui_handle.clone(),
                            Arc::clone(&config),
                            Arc::clone(&auth_session),
                            token_store.clone(),
                            platform_session.session_id.clone(),
                            paths.root.clone(),
                            stop_flag,
                            child,
                        );
                        let _ = ui_handle.upgrade_in_event_loop(move |app| {
                            app.set_launch_status_text(SharedString::from(message))
                        });
                    }
                    Err(error) => {
                        let _ = ui_handle.upgrade_in_event_loop(move |app| {
                            app.set_launch_status_text(SharedString::from(format!(
                                "平台代理模式启动失败：{error}"
                            )))
                        });
                    }
                }
            }
            Err(error) => {
                let _ = ui_handle.upgrade_in_event_loop(move |app| {
                    app.set_launch_status_text(SharedString::from(format!(
                        "创建平台代理会话失败：{error}"
                    )))
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
    app.on_login_requested(move || {
        let Some(app) = login_app.upgrade() else {
            return;
        };

        let email = app.get_email().to_string();
        let password = app.get_password().to_string();
        let verification_code = app.get_verification_code().to_string();
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
                        &auth_session,
                        &pending_totp_token,
                        &available_groups,
                        &subscription_summary,
                        &redeem_history,
                        email,
                        auth,
                    );
                    let groups_snapshot = current_groups_snapshot(&available_groups);
                    let billing_vm = current_billing_vm(&subscription_summary, &redeem_history);
                    let group_count = Some(groups_snapshot.len());
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        if let Some(session) =
                            auth_session.lock().ok().and_then(|state| state.clone())
                        {
                            apply_authenticated_state(&app, &session, group_count);
                            apply_available_groups_state(&app, &groups_snapshot);
                            apply_billing_state(&app, &billing_vm);
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
                        app.set_auth_status_text(SharedString::from(message))
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
    app.on_register_requested(move || {
        let Some(app) = register_app.upgrade() else {
            return;
        };

        let email = app.get_email().to_string();
        let password = app.get_password().to_string();
        let verification_code = app.get_verification_code().to_string();
        if email.trim().is_empty() || password.is_empty() {
            app.set_auth_status_text(SharedString::from("注册前请填写邮箱、密码和邮箱验证码。"));
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
        thread::spawn(move || {
            let client = ApiClient::new(config.api_base_url.clone());
            let request = RegisterRequest::new(email.trim(), password)
                .with_verify_code(verification_code.trim());
            match register_blocking(&client, &request) {
                Ok(auth) => {
                    handle_auth_success(
                        &config,
                        &app_state,
                        &token_store,
                        &auth_session,
                        &pending_totp_token,
                        &available_groups,
                        &subscription_summary,
                        &redeem_history,
                        email,
                        auth,
                    );
                    let groups_snapshot = current_groups_snapshot(&available_groups);
                    let billing_vm = current_billing_vm(&subscription_summary, &redeem_history);
                    let group_count = Some(groups_snapshot.len());
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        if let Some(session) =
                            auth_session.lock().ok().and_then(|state| state.clone())
                        {
                            apply_authenticated_state(&app, &session, group_count);
                            apply_available_groups_state(&app, &groups_snapshot);
                            apply_billing_state(&app, &billing_vm);
                            app.set_auth_status_text(SharedString::from(
                                "注册成功，已自动登录当前账户。",
                            ));
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
) {
    let redeem_app = app.as_weak();
    let redeem_config = Arc::clone(&config);
    let redeem_session = Arc::clone(&auth_session);
    let redeem_groups = Arc::clone(&available_groups);
    let redeem_summary = Arc::clone(&subscription_summary);
    let redeem_history_store = Arc::clone(&redeem_history);
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
                        &available_groups,
                        &subscription_summary,
                        &redeem_history_store,
                    );

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
}

fn restore_saved_session(
    app: &AppWindow,
    config: Arc<AppConfig>,
    token_store: SystemCredentialStore,
    auth_session: SharedAuthSession,
    pending_totp_token: Arc<Mutex<Option<String>>>,
    available_groups: SharedGroups,
    subscription_summary: SharedSubscriptionSummary,
    redeem_history: SharedRedeemHistory,
) {
    let app_handle = app.as_weak();
    match token_store.load_refresh_token() {
        Ok(Some(refresh_token)) => {
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

                                let (group_count, groups_snapshot, billing_vm) =
                                    sync_user_side_state(
                                        &user_client,
                                        &available_groups,
                                        &subscription_summary,
                                        &redeem_history,
                                    );
                                let _ = app_handle.upgrade_in_event_loop(move |app| {
                                    apply_authenticated_state(&app, &session, group_count);
                                    apply_available_groups_state(&app, &groups_snapshot);
                                    apply_billing_state(&app, &billing_vm);
                                    app.set_auth_status_text(SharedString::from(
                                        "已恢复上次登录状态。",
                                    ));
                                });
                            }
                            Err(error) => {
                                let _ = token_store.clear_refresh_token();
                                let _ = app_handle.upgrade_in_event_loop(move |app| {
                                    apply_logged_out_state(&app);
                                    app.set_auth_status_text(SharedString::from(format!(
                                        "恢复登录失败：{error}，请重新登录。"
                                    )));
                                });
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
        Ok(None) | Err(_) => {}
    }
}

fn handle_auth_success(
    config: &AppConfig,
    app_state: &AppStateStore,
    token_store: &SystemCredentialStore,
    auth_session: &SharedAuthSession,
    pending_totp_token: &Arc<Mutex<Option<String>>>,
    available_groups: &SharedGroups,
    subscription_summary: &SharedSubscriptionSummary,
    redeem_history: &SharedRedeemHistory,
    email: String,
    auth: AuthResponse,
) {
    let _ = app_state.save_last_email(&email);
    if let Some(refresh_token) = auth.refresh_token.as_deref() {
        let _ = token_store.save_refresh_token(refresh_token);
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
    let _ = sync_user_side_state(
        &client,
        available_groups,
        subscription_summary,
        redeem_history,
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
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    mut child: std::process::Child,
) {
    thread::spawn(move || {
        let _ = child.wait();
        stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        let revoke_result =
            revoke_platform_session(&config, &auth_session, &token_store, &session_id);
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
    available_groups: &SharedGroups,
    subscription_summary: &SharedSubscriptionSummary,
    redeem_history: &SharedRedeemHistory,
) -> (Option<usize>, Vec<GroupSummary>, BillingViewModel) {
    let group_count = refresh_available_groups_state(client, available_groups);
    refresh_billing_state(client, subscription_summary, redeem_history);
    let groups_snapshot = current_groups_snapshot(available_groups);
    let billing_vm = current_billing_vm(subscription_summary, redeem_history);
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
}

fn current_groups_snapshot(available_groups: &SharedGroups) -> Vec<GroupSummary> {
    available_groups
        .lock()
        .ok()
        .map(|state| state.clone())
        .unwrap_or_default()
}

fn current_billing_vm(
    subscription_summary: &SharedSubscriptionSummary,
    redeem_history: &SharedRedeemHistory,
) -> BillingViewModel {
    let summary = subscription_summary
        .lock()
        .ok()
        .and_then(|state| state.clone());
    let history = redeem_history
        .lock()
        .ok()
        .map(|state| state.clone())
        .unwrap_or_default();
    BillingViewModel::from_summary_and_history(summary.as_ref(), &history)
}

fn apply_launch_state(app: &AppWindow, targets: &[InstalledTarget]) {
    let vm = LaunchViewModel::from_targets(targets);
    app.set_desktop_available(vm.desktop_available);
    app.set_cli_available(vm.cli_available);
    app.set_launch_status_text(SharedString::from(vm.status_text));
}

fn apply_logged_out_state(app: &AppWindow) {
    app.set_dashboard_user_label(SharedString::from("当前账号：未登录"));
    app.set_dashboard_balance_text(SharedString::from("余额：--"));
    app.set_dashboard_usage_text(SharedString::from("并发额度：--"));
    app.set_dashboard_account_status_text(SharedString::from("账户状态：待登录"));
    app.set_launch_group_options(single_option_model("登录后加载可用分组"));
    app.set_launch_selected_group_index(0);
    apply_billing_state(app, &BillingViewModel::empty());
}

fn apply_authenticated_state(app: &AppWindow, session: &AuthSession, group_count: Option<usize>) {
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

fn apply_available_groups_state(app: &AppWindow, groups: &[GroupSummary]) {
    if groups.is_empty() {
        app.set_launch_group_options(single_option_model("当前没有可用分组"));
        app.set_launch_selected_group_index(0);
        return;
    }

    let labels = groups
        .iter()
        .map(|group| SharedString::from(format!("{} · {}", group.name, group.status)))
        .collect::<Vec<_>>();
    app.set_launch_group_options(string_model(labels));
    app.set_launch_selected_group_index(0);
}

fn apply_billing_state(app: &AppWindow, billing: &BillingViewModel) {
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
    app.set_redeem_history_lines(string_model(
        billing
            .redeem_history_lines
            .iter()
            .cloned()
            .map(SharedString::from)
            .collect(),
    ));
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
