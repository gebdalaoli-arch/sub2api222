use anyhow::Result;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use super::install_detection::InstalledTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub envs: Vec<(OsString, OsString)>,
    pub current_dir: Option<PathBuf>,
    pub tracks_child_lifecycle: bool,
    pub windows_create_no_window: bool,
    pub windows_detach_process: bool,
    pub null_stdio: bool,
}

pub const WINDOWS_STORE_DESKTOP_PLATFORM_UNSUPPORTED: &str =
    "WINDOWS_STORE_DESKTOP_PLATFORM_UNSUPPORTED";
pub const WINDOWS_STORE_DESKTOP_ALREADY_RUNNING: &str = "WINDOWS_STORE_DESKTOP_ALREADY_RUNNING";

impl LaunchCommandSpec {
    pub fn direct(executable: PathBuf) -> Self {
        Self {
            program: executable.into_os_string(),
            args: Vec::new(),
            envs: Vec::new(),
            current_dir: None,
            tracks_child_lifecycle: true,
            windows_create_no_window: false,
            windows_detach_process: false,
            null_stdio: false,
        }
    }

    pub fn detached_gui(executable: PathBuf) -> Self {
        let current_dir = executable.parent().map(PathBuf::from);
        Self {
            program: executable.into_os_string(),
            args: Vec::new(),
            envs: Vec::new(),
            current_dir,
            tracks_child_lifecycle: true,
            windows_create_no_window: false,
            windows_detach_process: true,
            null_stdio: true,
        }
    }

    pub fn windows_store_entry(app_id: String) -> Self {
        let script = format!(
            "$target='shell:AppsFolder\\{app_id}'; Start-Process -FilePath $target -ErrorAction Stop | Out-Null"
        );
        Self {
            program: OsString::from("powershell"),
            args: vec![
                OsString::from("-WindowStyle"),
                OsString::from("Hidden"),
                OsString::from("-NonInteractive"),
                OsString::from("-NoProfile"),
                OsString::from("-Command"),
                OsString::from(script),
            ],
            envs: Vec::new(),
            current_dir: None,
            tracks_child_lifecycle: false,
            windows_create_no_window: true,
            windows_detach_process: false,
            null_stdio: true,
        }
    }
}

pub fn validate_platform_launch_target(target: &InstalledTarget) -> Result<()> {
    if is_windows_store_desktop_target(target) && !cfg!(test) && windows_store_desktop_is_running()
    {
        anyhow::bail!(WINDOWS_STORE_DESKTOP_ALREADY_RUNNING);
    }
    Ok(())
}

pub fn official_launch_command(target: &InstalledTarget) -> LaunchCommandSpec {
    if let Some(app_id) = windows_store_app_id(target) {
        return LaunchCommandSpec::windows_store_entry(app_id);
    }

    let executable = target.executable.clone();
    let extension = executable
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());

    match extension.as_deref() {
        Some("cmd" | "bat") => LaunchCommandSpec {
            program: OsString::from("cmd"),
            args: vec![
                OsString::from("/C"),
                OsString::from("start"),
                OsString::from(""),
                executable.into_os_string(),
            ],
            envs: Vec::new(),
            current_dir: None,
            tracks_child_lifecycle: false,
            windows_create_no_window: true,
            windows_detach_process: false,
            null_stdio: true,
        },
        Some("ps1") => LaunchCommandSpec {
            program: OsString::from("powershell"),
            args: vec![
                OsString::from("-NoProfile"),
                OsString::from("-ExecutionPolicy"),
                OsString::from("Bypass"),
                OsString::from("-File"),
                executable.into_os_string(),
            ],
            envs: Vec::new(),
            current_dir: None,
            tracks_child_lifecycle: false,
            windows_create_no_window: true,
            windows_detach_process: false,
            null_stdio: true,
        },
        _ => LaunchCommandSpec::direct(executable),
    }
}

pub fn launch_official(target: &InstalledTarget) -> Result<()> {
    let spec = official_launch_command(target);
    let _ = spawn_command(spec)?;
    Ok(())
}

pub fn platform_launch_command(target: &InstalledTarget, codex_home: &Path) -> LaunchCommandSpec {
    let mut spec = official_launch_command(target);
    if !requires_user_home_injection(target) {
        spec.envs.push((
            OsString::from("CODEX_HOME"),
            codex_home.as_os_str().to_os_string(),
        ));
    }
    spec
}

pub fn launch_platform(target: &InstalledTarget, codex_home: &Path) -> Result<Option<Child>> {
    validate_platform_launch_target(target)?;
    let spec = platform_launch_command(target, codex_home);
    spawn_command(spec)
}

pub fn requires_user_home_injection(target: &InstalledTarget) -> bool {
    is_windows_store_desktop_target(target)
}

pub fn windows_store_desktop_is_running_for_launch() -> bool {
    windows_store_desktop_is_running()
}

fn spawn_command(spec: LaunchCommandSpec) -> Result<Option<Child>> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    for (key, value) in spec.envs {
        command.env(key, value);
    }
    if let Some(current_dir) = spec.current_dir {
        command.current_dir(current_dir);
    }
    if spec.null_stdio {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let mut creation_flags = 0;
        if spec.windows_create_no_window {
            creation_flags |= CREATE_NO_WINDOW;
        }
        if spec.windows_detach_process {
            creation_flags |= CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS;
        }
        if creation_flags != 0 {
            command.creation_flags(creation_flags);
        }
    }
    let child = command.spawn()?;
    if spec.tracks_child_lifecycle {
        Ok(Some(child))
    } else {
        Ok(None)
    }
}

fn windows_store_app_id(target: &InstalledTarget) -> Option<String> {
    if target.kind != crate::platform::install_detection::LaunchTarget::Desktop {
        return None;
    }

    let normalized = target.executable.to_string_lossy().replace('/', "\\");
    let normalized_lower = normalized.to_ascii_lowercase();
    let shell_prefix = r"shell:AppsFolder\";
    let shell_prefix_lower = shell_prefix.to_ascii_lowercase();
    if normalized_lower.starts_with(&shell_prefix_lower) {
        let app_id = normalized[shell_prefix.len()..].trim();
        if app_id.is_empty() {
            return None;
        }
        return Some(app_id.to_string());
    }
    if !normalized_lower.contains("windowsapps\\openai.codex_") {
        return None;
    }

    let package_full_name = target.executable.ancestors().find_map(|ancestor| {
        ancestor
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| {
                let normalized = name.to_ascii_lowercase();
                normalized.starts_with("openai.codex_") && normalized.contains("__")
            })
    })?;

    let (prefix, publisher_id) = package_full_name.split_once("__")?;
    let app_name = prefix.split('_').next()?;
    Some(format!("{app_name}_{publisher_id}!App"))
}

fn is_windows_store_desktop_target(target: &InstalledTarget) -> bool {
    windows_store_app_id(target).is_some()
}

#[cfg(windows)]
fn windows_store_desktop_is_running() -> bool {
    let Ok(output) = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq Codex.exe", "/FO", "CSV", "/NH"])
        .output()
    else {
        return false;
    };

    windows_tasklist_reports_codex_running(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(windows))]
fn windows_store_desktop_is_running() -> bool {
    false
}

fn windows_tasklist_reports_codex_running(tasklist_stdout: &str) -> bool {
    tasklist_stdout.lines().any(|line| {
        let Some(first_column) = line.split(',').next() else {
            return false;
        };
        first_column
            .trim()
            .trim_matches('"')
            .eq_ignore_ascii_case("Codex.exe")
    })
}

#[cfg(test)]
mod tests {
    use super::{
        official_launch_command, platform_launch_command, requires_user_home_injection,
        validate_platform_launch_target,
        windows_tasklist_reports_codex_running,
    };
    use crate::platform::install_detection::{InstalledTarget, LaunchTarget};
    use std::path::PathBuf;

    #[test]
    fn official_launch_for_cmd_uses_shell_without_platform_environment() {
        let target = InstalledTarget {
            kind: LaunchTarget::Cli,
            executable: PathBuf::from(r"C:\Users\tester\AppData\Roaming\npm\codex.cmd"),
            display_name: "Codex CLI".to_string(),
        };

        let spec = official_launch_command(&target);

        assert_eq!(spec.program.to_string_lossy(), "cmd");
        assert_eq!(spec.args[0].to_string_lossy(), "/C");
        assert!(spec.envs.is_empty());
        assert!(!spec.tracks_child_lifecycle);
        assert!(!spec
            .args
            .iter()
            .any(|arg| arg.to_string_lossy().contains("CODEX_HOME")));
    }

    #[test]
    fn official_launch_for_windows_store_desktop_uses_shell_app_launcher() {
        let target = InstalledTarget {
            kind: LaunchTarget::Desktop,
            executable: PathBuf::from(
                r"C:\Program Files\WindowsApps\OpenAI.Codex_1.0.0.0_x64__2p2nqsd0c76g0\app\Codex.exe",
            ),
            display_name: "Codex Desktop".to_string(),
        };

        let spec = official_launch_command(&target);

        assert_eq!(spec.program.to_string_lossy(), "powershell");
        assert!(!spec.tracks_child_lifecycle);
        assert!(spec.windows_create_no_window);
        assert!(spec.null_stdio);
        assert!(spec.args.iter().any(|arg| {
            arg.to_string_lossy()
                .contains(r"shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App")
        }));
    }

    #[test]
    fn official_launch_for_shell_appsfolder_target_uses_hidden_powershell() {
        let target = InstalledTarget {
            kind: LaunchTarget::Desktop,
            executable: PathBuf::from(r"shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App"),
            display_name: "Codex Desktop".to_string(),
        };

        let spec = official_launch_command(&target);

        assert_eq!(spec.program.to_string_lossy(), "powershell");
        assert!(spec.windows_create_no_window);
        assert!(spec.null_stdio);
    }

    #[test]
    fn platform_launch_for_windows_store_desktop_uses_shell_launcher_with_isolated_home() {
        let target = InstalledTarget {
            kind: LaunchTarget::Desktop,
            executable: PathBuf::from(r"shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App"),
            display_name: "Codex Desktop".to_string(),
        };

        let runtime_home = PathBuf::from(r"D:\TokenClient\runtime\platform-desktop");
        let spec = platform_launch_command(&target, runtime_home.as_path());

        assert_eq!(spec.program.to_string_lossy(), "powershell");
        assert!(!spec.tracks_child_lifecycle);
        assert_eq!(spec.current_dir, None);
        assert!(spec.envs.is_empty());
        assert!(spec.args.iter().any(|arg| {
            arg.to_string_lossy()
                .contains(r"shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App")
        }));
    }

    #[test]
    fn platform_launch_for_windows_store_desktop_bundle_prefers_gui_executable() {
        let target = InstalledTarget {
            kind: LaunchTarget::Desktop,
            executable: PathBuf::from(
                r"C:\Program Files\WindowsApps\OpenAI.Codex_26.415.4716.0_x64__2p2nqsd0c76g0\app\Codex.exe",
            ),
            display_name: "Codex Desktop".to_string(),
        };

        let runtime_home = PathBuf::from(r"D:\TokenClient\runtime\platform-desktop");
        let spec = platform_launch_command(&target, runtime_home.as_path());

        assert_eq!(spec.program.to_string_lossy(), "powershell");
        assert!(!spec.tracks_child_lifecycle);
        assert!(spec.windows_create_no_window);
        assert!(spec.null_stdio);
    }

    #[test]
    fn platform_launch_injects_isolated_codex_home() {
        let target = InstalledTarget {
            kind: LaunchTarget::Cli,
            executable: PathBuf::from(r"C:\Users\tester\AppData\Roaming\npm\codex.cmd"),
            display_name: "Codex CLI".to_string(),
        };

        let runtime_home = PathBuf::from(r"D:\TokenClient\runtime\platform-cli");
        let spec = platform_launch_command(&target, runtime_home.as_path());

        let env_pairs: Vec<(String, String)> = spec
            .envs
            .iter()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.to_string_lossy().into_owned(),
                )
            })
            .collect();

        assert!(env_pairs.iter().any(|(key, value)| {
            key == "CODEX_HOME" && value == r"D:\TokenClient\runtime\platform-cli"
        }));
    }

    #[test]
    fn direct_launches_keep_child_lifecycle_tracking() {
        let target = InstalledTarget {
            kind: LaunchTarget::Cli,
            executable: PathBuf::from(r"C:\Tools\codex.exe"),
            display_name: "Codex CLI".to_string(),
        };

        let spec = official_launch_command(&target);

        assert!(spec.tracks_child_lifecycle);
    }

    #[test]
    fn platform_launch_validation_allows_windows_store_desktop() {
        let target = InstalledTarget {
            kind: LaunchTarget::Desktop,
            executable: PathBuf::from(r"shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App"),
            display_name: "Codex Desktop".to_string(),
        };

        assert!(validate_platform_launch_target(&target).is_ok());
    }

    #[test]
    fn windows_store_desktop_requires_user_home_injection() {
        let target = InstalledTarget {
            kind: LaunchTarget::Desktop,
            executable: PathBuf::from(r"shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App"),
            display_name: "Codex Desktop".to_string(),
        };

        assert!(requires_user_home_injection(&target));
    }

    #[test]
    fn platform_launch_validation_allows_cli_targets() {
        let target = InstalledTarget {
            kind: LaunchTarget::Cli,
            executable: PathBuf::from(r"C:\Users\tester\AppData\Roaming\npm\codex.cmd"),
            display_name: "Codex CLI".to_string(),
        };

        assert!(validate_platform_launch_target(&target).is_ok());
    }

    #[test]
    fn windows_tasklist_reports_codex_running_detects_desktop_process() {
        let output = "\"Codex.exe\",\"1234\",\"Console\",\"1\",\"120,000 K\"";

        assert!(windows_tasklist_reports_codex_running(output));
    }

    #[test]
    fn windows_tasklist_reports_codex_running_ignores_empty_tasklist() {
        let output = "INFO: No tasks are running which match the specified criteria.";

        assert!(!windows_tasklist_reports_codex_running(output));
    }
}
