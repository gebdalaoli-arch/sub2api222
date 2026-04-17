slint::include_modules!();

use slint::{ModelRc, SharedString, VecModel};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
};

use sub2api_desktop::{
    api::{
        account::{fetch_current_user_blocking, UserProfile},
        auth::{
            forgot_password_blocking, login_2fa_blocking, login_blocking, refresh_token_blocking,
            register_blocking, reset_password_blocking, send_verify_code_blocking, AuthResponse,
            LoginResponse, RefreshTokenRequest, RegisterRequest, ResetPasswordRequest,
        },
        desktop_sessions::{
            create_desktop_session_blocking, DesktopSessionCreateRequest, DesktopSessionTarget,
        },
        groups::{fetch_available_groups_blocking, GroupSummary},
        http::ApiClient,
    },
    app::{
        auth_flow::{build_login_submission, LoginSubmission},
        view_models::{dashboard_vm::DashboardViewModel, launch_vm::LaunchViewModel},
    },
    config::{app_config, AppConfig},
    platform::{
        install_detection::{detect_installed_targets, InstalledTarget, LaunchTarget},
        launcher::{launch_official, launch_platform},
        managed_home::{write_platform_home, ManagedHomePaths},
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

fn main() -> anyhow::Result<()> {
    let app = AppWindow::new()?;
    let config = Arc::new(app_config());
    let app_state = AppStateStore::default_for_app().unwrap_or_else(|_| {
        AppStateStore::new(std::env::temp_dir().join("sub2api-desktop-client"))
    });
    let token_store = SystemCredentialStore::new("primary-auth");
    let targets = Rc::new(RefCell::new(detect_installed_targets()));
    let auth_session = Arc::new(Mutex::new(None::<AuthSession>));
    let pending_totp_token = Arc::new(Mutex::new(None::<String>));
    let available_groups = Arc::new(Mutex::new(Vec::<GroupSummary>::new()));

    apply_launch_state(&app, &targets.borrow());
    apply_logged_out_state(&app);
    preload_local_state(&app, &app_state);

    wire_launch_callbacks(&app, Rc::clone(&targets));
    wire_platform_launch_callbacks(
        &app,
        Rc::clone(&targets),
        Arc::clone(&config),
        app_state.clone(),
        Arc::clone(&auth_session),
        Arc::clone(&available_groups),
    );
    wire_auth_callbacks(
        &app,
        Arc::clone(&config),
        app_state.clone(),
        token_store.clone(),
        Arc::clone(&auth_session),
        Arc::clone(&pending_totp_token),
        Arc::clone(&available_groups),
    );
    restore_saved_session(
        &app,
        Arc::clone(&config),
        token_store,
        Arc::clone(&auth_session),
        Arc::clone(&pending_totp_token),
        Arc::clone(&available_groups),
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
    auth_session: Arc<Mutex<Option<AuthSession>>>,
    available_groups: Arc<Mutex<Vec<GroupSummary>>>,
) {
    let platform_desktop_app = app.as_weak();
    let platform_desktop_targets = Rc::clone(&targets);
    let platform_desktop_config = Arc::clone(&config);
    let platform_desktop_session = Arc::clone(&auth_session);
    let platform_desktop_groups = Arc::clone(&available_groups);
    let platform_desktop_state = app_state.clone();
    app.on_launch_platform_desktop_requested(move || {
        start_platform_launch(
            &platform_desktop_app,
            platform_desktop_targets.borrow().clone(),
            Arc::clone(&platform_desktop_config),
            platform_desktop_state.clone(),
            Arc::clone(&platform_desktop_session),
            Arc::clone(&platform_desktop_groups),
            LaunchTarget::Desktop,
        );
    });

    let platform_cli_app = app.as_weak();
    let platform_cli_targets = Rc::clone(&targets);
    let platform_cli_config = Arc::clone(&config);
    let platform_cli_session = Arc::clone(&auth_session);
    let platform_cli_groups = Arc::clone(&available_groups);
    app.on_launch_platform_cli_requested(move || {
        start_platform_launch(
            &platform_cli_app,
            platform_cli_targets.borrow().clone(),
            Arc::clone(&platform_cli_config),
            app_state.clone(),
            Arc::clone(&platform_cli_session),
            Arc::clone(&platform_cli_groups),
            LaunchTarget::Cli,
        );
    });
}

fn start_platform_launch(
    app_handle: &slint::Weak<AppWindow>,
    installed_targets: Vec<InstalledTarget>,
    config: Arc<AppConfig>,
    app_state: AppStateStore,
    auth_session: Arc<Mutex<Option<AuthSession>>>,
    available_groups: Arc<Mutex<Vec<GroupSummary>>>,
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

        let groups = available_groups
            .lock()
            .ok()
            .map(|state| state.clone())
            .unwrap_or_default();
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
            .with_access_token(Some(session.access_token.clone()));
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
                            "平台会话缺少 runtime token，无法继续启动。",
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
                    .and_then(|_| launch_platform(&target, &paths.codex_home));

                match result {
                    Ok(()) => {
                        let message = format!(
                            "已为分组“{}”创建平台代理会话，并启动 {}。",
                            group.name, target.display_name
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
    auth_session: Arc<Mutex<Option<AuthSession>>>,
    pending_totp_token: Arc<Mutex<Option<String>>>,
    available_groups: Arc<Mutex<Vec<GroupSummary>>>,
) {
    let login_app = app.as_weak();
    let login_config = Arc::clone(&config);
    let login_state = app_state.clone();
    let login_store = token_store.clone();
    let login_session = Arc::clone(&auth_session);
    let login_totp = Arc::clone(&pending_totp_token);
    let login_groups = Arc::clone(&available_groups);
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
        thread::spawn(move || {
            let client = ApiClient::new(config.api_base_url.clone());
            let result = match submission {
                LoginSubmission::Password(request) => login_blocking(&client, &request)
                    .and_then(|response| match response {
                        LoginResponse::Authenticated(auth) => Ok(AuthFlowOutcome::Authenticated(auth)),
                        LoginResponse::TotpRequired(totp) => Ok(AuthFlowOutcome::TotpRequired {
                            temp_token: totp.temp_token,
                            masked_email: totp.user_email_masked,
                        }),
                    }),
                LoginSubmission::TwoFactor(request) => {
                    login_2fa_blocking(&client, &request).map(AuthFlowOutcome::Authenticated)
                }
            };

            match result {
                Ok(AuthFlowOutcome::Authenticated(auth)) => {
                    let group_count = handle_auth_success(
                        &config,
                        &app_state,
                        &token_store,
                        &auth_session,
                        &pending_totp_token,
                        &available_groups,
                        email,
                        auth,
                    );
                    let groups_snapshot = available_groups
                        .lock()
                        .ok()
                        .map(|groups| groups.clone())
                        .unwrap_or_default();
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        if let Ok(session_guard) = auth_session.lock() {
                            if let Some(session) = session_guard.as_ref() {
                                apply_authenticated_state(&app, session, group_count);
                                apply_available_groups_state(&app, &groups_snapshot);
                            }
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
                    let _ = ui_handle
                        .upgrade_in_event_loop(move |app| app.set_auth_status_text(SharedString::from(message)));
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
        thread::spawn(move || {
            let client = ApiClient::new(config.api_base_url.clone());
            let request = RegisterRequest::new(email.trim(), password)
                .with_verify_code(verification_code.trim());
            match register_blocking(&client, &request) {
                Ok(auth) => {
                    let group_count = handle_auth_success(
                        &config,
                        &app_state,
                        &token_store,
                        &auth_session,
                        &pending_totp_token,
                        &available_groups,
                        email,
                        auth,
                    );
                    let groups_snapshot = available_groups
                        .lock()
                        .ok()
                        .map(|groups| groups.clone())
                        .unwrap_or_default();
                    let _ = ui_handle.upgrade_in_event_loop(move |app| {
                        if let Ok(session_guard) = auth_session.lock() {
                            if let Some(session) = session_guard.as_ref() {
                                apply_authenticated_state(&app, session, group_count);
                                apply_available_groups_state(&app, &groups_snapshot);
                                app.set_auth_status_text(SharedString::from(
                                    "注册成功，已自动登录当前账户。",
                                ));
                            }
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
            match send_verify_code_blocking(
                &client,
                &sub2api_desktop::api::auth::SendVerifyCodeRequest::new(email.trim()),
            ) {
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

fn restore_saved_session(
    app: &AppWindow,
    config: Arc<AppConfig>,
    token_store: SystemCredentialStore,
    auth_session: Arc<Mutex<Option<AuthSession>>>,
    pending_totp_token: Arc<Mutex<Option<String>>>,
    available_groups: Arc<Mutex<Vec<GroupSummary>>>,
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
                                    access_token: token_pair.access_token.clone(),
                                    user,
                                };
                                if let Ok(groups) = fetch_available_groups_blocking(&user_client) {
                                    if let Ok(mut cached_groups) = available_groups.lock() {
                                        *cached_groups = groups;
                                    }
                                }
                                if let Ok(mut session_slot) = auth_session.lock() {
                                    *session_slot = Some(session.clone());
                                }
                                if let Ok(mut pending) = pending_totp_token.lock() {
                                    *pending = None;
                                }
                                let group_count =
                                    available_groups.lock().ok().map(|groups| groups.len());
                                let groups_snapshot = available_groups
                                    .lock()
                                    .ok()
                                    .map(|groups| groups.clone())
                                    .unwrap_or_default();
                                let _ = app_handle.upgrade_in_event_loop(move |app| {
                                    apply_authenticated_state(&app, &session, group_count);
                                    apply_available_groups_state(&app, &groups_snapshot);
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
    auth_session: &Arc<Mutex<Option<AuthSession>>>,
    pending_totp_token: &Arc<Mutex<Option<String>>>,
    available_groups: &Arc<Mutex<Vec<GroupSummary>>>,
    email: String,
    auth: AuthResponse,
) -> Option<usize> {
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

    let group_count = {
        let user_client = ApiClient::new(config.api_base_url.clone())
            .with_access_token(Some(session.access_token.clone()));
        match fetch_available_groups_blocking(&user_client) {
            Ok(groups) => {
                let count = groups.len();
                if let Ok(mut cached_groups) = available_groups.lock() {
                    *cached_groups = groups;
                }
                Some(count)
            }
            Err(_) => None,
        }
    };

    if let Ok(mut session_slot) = auth_session.lock() {
        *session_slot = Some(session);
    }

    group_count
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
        Some(count) => {
            format!("已登录，可用分组 {count} 个。后续平台模式将基于分组与订阅创建独立桌面会话。")
        }
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
    app.set_launch_group_options(ModelRc::new(VecModel::from(labels)));
    app.set_launch_selected_group_index(0);
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
    ModelRc::new(VecModel::from(vec![SharedString::from(text)]))
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
