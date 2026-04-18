use anyhow::Result;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Child, Command},
};

use super::install_detection::InstalledTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub envs: Vec<(OsString, OsString)>,
    pub tracks_child_lifecycle: bool,
}

impl LaunchCommandSpec {
    pub fn direct(executable: PathBuf) -> Self {
        Self {
            program: executable.into_os_string(),
            args: Vec::new(),
            envs: Vec::new(),
            tracks_child_lifecycle: true,
        }
    }
}

pub fn official_launch_command(target: &InstalledTarget) -> LaunchCommandSpec {
    if let Some(app_id) = windows_store_app_id(target) {
        return LaunchCommandSpec {
            program: OsString::from("explorer.exe"),
            args: vec![OsString::from(format!(r"shell:AppsFolder\{app_id}"))],
            envs: Vec::new(),
            tracks_child_lifecycle: false,
        };
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
            tracks_child_lifecycle: false,
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
            tracks_child_lifecycle: false,
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
    spec.envs.push((
        OsString::from("CODEX_HOME"),
        codex_home.as_os_str().to_os_string(),
    ));
    spec
}

pub fn launch_platform(target: &InstalledTarget, codex_home: &Path) -> Result<Option<Child>> {
    let spec = platform_launch_command(target, codex_home);
    spawn_command(spec)
}

fn spawn_command(spec: LaunchCommandSpec) -> Result<Option<Child>> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    for (key, value) in spec.envs {
        command.env(key, value);
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
    if !normalized.to_ascii_lowercase().contains("windowsapps\\openai.codex_") {
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

#[cfg(test)]
mod tests {
    use super::{official_launch_command, platform_launch_command};
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

        assert_eq!(spec.program.to_string_lossy(), "explorer.exe");
        assert_eq!(
            spec.args[0].to_string_lossy(),
            r"shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App"
        );
        assert!(!spec.tracks_child_lifecycle);
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
}
