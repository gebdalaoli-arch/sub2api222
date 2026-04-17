use crate::platform::install_detection::{InstalledTarget, LaunchTarget};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchViewModel {
    pub desktop_available: bool,
    pub cli_available: bool,
    pub status_text: String,
}

impl LaunchViewModel {
    pub fn empty() -> Self {
        Self {
            desktop_available: false,
            cli_available: false,
            status_text: "尚未检测 Codex 安装".to_string(),
        }
    }

    pub fn from_targets(targets: &[InstalledTarget]) -> Self {
        let desktop_available = targets
            .iter()
            .any(|target| target.kind == LaunchTarget::Desktop);
        let cli_available = targets
            .iter()
            .any(|target| target.kind == LaunchTarget::Cli);

        Self {
            desktop_available,
            cli_available,
            status_text: launch_status_text(desktop_available, cli_available).to_string(),
        }
    }
}

fn launch_status_text(desktop_available: bool, cli_available: bool) -> &'static str {
    match (desktop_available, cli_available) {
        (true, true) => "已检测到 Codex Desktop 和 Codex CLI",
        (true, false) => "已检测到 Codex Desktop",
        (false, true) => "已检测到 Codex CLI",
        (false, false) => "未检测到 Codex 安装",
    }
}

#[cfg(test)]
mod tests {
    use super::LaunchViewModel;
    use crate::platform::install_detection::{InstalledTarget, LaunchTarget};
    use std::path::PathBuf;

    #[test]
    fn launch_view_model_marks_available_targets() {
        let vm = LaunchViewModel::from_targets(&[
            InstalledTarget {
                kind: LaunchTarget::Desktop,
                executable: PathBuf::from("codex.exe"),
                display_name: "Codex Desktop".to_string(),
            },
            InstalledTarget {
                kind: LaunchTarget::Cli,
                executable: PathBuf::from("codex.cmd"),
                display_name: "Codex CLI".to_string(),
            },
        ]);

        assert!(vm.desktop_available);
        assert!(vm.cli_available);
        assert_eq!(vm.status_text, "已检测到 Codex Desktop 和 Codex CLI");
    }

    #[test]
    fn launch_view_model_explains_missing_targets() {
        let vm = LaunchViewModel::from_targets(&[]);

        assert!(!vm.desktop_available);
        assert!(!vm.cli_available);
        assert_eq!(vm.status_text, "未检测到 Codex 安装");
    }
}
