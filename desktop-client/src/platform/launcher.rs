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
}

impl LaunchCommandSpec {
    pub fn direct(executable: PathBuf) -> Self {
        Self {
            program: executable.into_os_string(),
            args: Vec::new(),
            envs: Vec::new(),
        }
    }
}

pub fn official_launch_command(target: &InstalledTarget) -> LaunchCommandSpec {
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
        },
        _ => LaunchCommandSpec::direct(executable),
    }
}

pub fn launch_official(target: &InstalledTarget) -> Result<()> {
    let spec = official_launch_command(target);
    spawn_command(spec).map(|_| ())
}

pub fn platform_launch_command(target: &InstalledTarget, codex_home: &Path) -> LaunchCommandSpec {
    let mut spec = official_launch_command(target);
    spec.envs.push((
        OsString::from("CODEX_HOME"),
        codex_home.as_os_str().to_os_string(),
    ));
    spec
}

pub fn launch_platform(target: &InstalledTarget, codex_home: &Path) -> Result<Child> {
    let spec = platform_launch_command(target, codex_home);
    spawn_command(spec)
}

fn spawn_command(spec: LaunchCommandSpec) -> Result<Child> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    for (key, value) in spec.envs {
        command.env(key, value);
    }
    Ok(command.spawn()?)
}

#[cfg(test)]
mod tests {
    use super::{official_launch_command, platform_launch_command, LaunchCommandSpec};
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
        assert!(!spec
            .args
            .iter()
            .any(|arg| arg.to_string_lossy().contains("CODEX_HOME")));
    }

    #[test]
    fn official_launch_for_desktop_runs_executable_directly() {
        let target = InstalledTarget {
            kind: LaunchTarget::Desktop,
            executable: PathBuf::from(
                r"C:\Program Files\WindowsApps\OpenAI.Codex_1.0.0.0_x64__2p2nqsd0c76g0\codex.exe",
            ),
            display_name: "Codex Desktop".to_string(),
        };

        let spec = official_launch_command(&target);

        assert_eq!(
            spec.program.to_string_lossy(),
            target.executable.to_string_lossy()
        );
        assert_eq!(spec, LaunchCommandSpec::direct(target.executable));
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
}
