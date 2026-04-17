slint::include_modules!();

use slint::SharedString;
use std::{cell::RefCell, rc::Rc};

use sub2api_desktop::{
    app::view_models::launch_vm::LaunchViewModel,
    platform::{
        install_detection::{detect_installed_targets, InstalledTarget, LaunchTarget},
        launcher::launch_official,
    },
};

fn main() -> anyhow::Result<()> {
    let app = AppWindow::new()?;
    let targets = Rc::new(RefCell::new(detect_installed_targets()));

    apply_launch_state(&app, &targets.borrow());
    wire_launch_callbacks(&app, targets);

    app.run()?;
    Ok(())
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

fn apply_launch_state(app: &AppWindow, targets: &[InstalledTarget]) {
    let vm = LaunchViewModel::from_targets(targets);
    app.set_desktop_available(vm.desktop_available);
    app.set_cli_available(vm.cli_available);
    app.set_launch_status_text(SharedString::from(vm.status_text));
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
