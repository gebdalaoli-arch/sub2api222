use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaunchTarget {
    Desktop,
    Cli,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledTarget {
    pub kind: LaunchTarget,
    pub executable: PathBuf,
    pub display_name: String,
}

pub fn detect_targets_from_paths(paths: &[PathBuf]) -> Vec<InstalledTarget> {
    let mut seen = HashSet::new();
    let mut targets = Vec::new();

    for path in paths {
        let Some(kind) = classify_codex_path(path) else {
            continue;
        };
        let key = (kind, normalize_path_key(path));
        if !seen.insert(key) {
            continue;
        }
        targets.push(InstalledTarget {
            kind,
            executable: path.clone(),
            display_name: target_display_name(kind).to_string(),
        });
    }

    targets.sort_by_key(|target| match target.kind {
        LaunchTarget::Desktop => 0,
        LaunchTarget::Cli => 1,
    });
    targets
}

pub fn detect_installed_targets() -> Vec<InstalledTarget> {
    detect_targets_from_paths(&candidate_codex_paths())
}

fn candidate_codex_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(path_var) = env::var_os("PATH") {
        for dir in env::split_paths(&path_var) {
            for name in ["codex.exe", "codex.cmd", "codex.ps1", "codex"] {
                let candidate = dir.join(name);
                if candidate.exists() {
                    paths.push(candidate);
                }
            }
        }
    }

    if cfg!(target_os = "windows") {
        paths.extend(windows_codex_desktop_candidates());
    }

    paths
}

fn windows_codex_desktop_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let windows_apps = PathBuf::from(r"C:\Program Files\WindowsApps");
    if let Ok(entries) = fs::read_dir(windows_apps) {
        for entry in entries.flatten() {
            let path = entry.path();
            let text = normalize_path_key(&path);
            if text.contains("openai.codex_") {
                for candidate in [
                    path.join("app").join("Codex.exe"),
                    path.join("app").join("codex.exe"),
                    path.join("codex.exe"),
                ] {
                    if candidate.exists() {
                        paths.push(candidate);
                    }
                }
            }
        }
    }

    if paths.is_empty() {
        paths.extend(windows_codex_desktop_shell_candidates());
    }
    paths
}

fn windows_codex_desktop_shell_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let Some(local_app_data) = env::var_os("LOCALAPPDATA") else {
        return paths;
    };

    let packages_root = PathBuf::from(local_app_data).join("Packages");
    let Ok(entries) = fs::read_dir(packages_root) else {
        return paths;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(package_family_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !package_family_name
            .to_ascii_lowercase()
            .starts_with("openai.codex_")
        {
            continue;
        }
        paths.push(PathBuf::from(format!(
            r"shell:AppsFolder\{package_family_name}!App"
        )));
    }

    paths
}

fn classify_codex_path(path: &Path) -> Option<LaunchTarget> {
    let file_name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    let text = normalize_path_key(path);

    if text.starts_with(r"shell:appsfolder\openai.codex_") {
        return Some(LaunchTarget::Desktop);
    }
    if matches!(file_name.as_str(), "codex.cmd" | "codex.ps1" | "codex") {
        if text.contains("windowsapps") && text.contains("openai.codex") {
            return None;
        }
        return Some(LaunchTarget::Cli);
    }
    if file_name == "codex.exe" && text.contains("windowsapps") && text.contains("openai.codex") {
        return Some(LaunchTarget::Desktop);
    }
    if file_name == "codex.exe" {
        return Some(LaunchTarget::Cli);
    }
    None
}

fn normalize_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
}

fn target_display_name(kind: LaunchTarget) -> &'static str {
    match kind {
        LaunchTarget::Desktop => "Codex Desktop",
        LaunchTarget::Cli => "Codex CLI",
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_codex_path, detect_targets_from_paths, LaunchTarget};
    use std::path::Path;

    #[test]
    fn detects_cli_and_desktop_from_known_windows_paths() {
        let targets = detect_targets_from_paths(&[
            r"C:\Users\tester\AppData\Roaming\npm\codex.cmd".into(),
            r"C:\Program Files\WindowsApps\OpenAI.Codex_26.409.7971.0_x64__2p2nqsd0c76g0\app\resources\codex.exe".into(),
        ]);

        assert!(targets
            .iter()
            .any(|target| target.kind == LaunchTarget::Cli));
        assert!(targets
            .iter()
            .any(|target| target.kind == LaunchTarget::Desktop));
    }

    #[test]
    fn detection_is_stable_and_deduplicated() {
        let targets = detect_targets_from_paths(&[
            r"C:\Users\tester\AppData\Roaming\npm\codex.cmd".into(),
            r"C:\Users\tester\AppData\Roaming\npm\codex.cmd".into(),
            r"C:\ignored\not-codex.exe".into(),
        ]);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].display_name, "Codex CLI");
    }

    #[test]
    fn classifies_windowsapps_app_codex_exe_as_desktop() {
        let kind = classify_codex_path(Path::new(
            r"C:\Program Files\WindowsApps\OpenAI.Codex_26.415.1938.0_x64__2p2nqsd0c76g0\app\Codex.exe",
        ));

        assert_eq!(kind, Some(LaunchTarget::Desktop));
    }

    #[test]
    fn ignores_windowsapps_resources_codex_helper_as_cli() {
        let kind = classify_codex_path(Path::new(
            r"C:\Program Files\WindowsApps\OpenAI.Codex_26.415.1938.0_x64__2p2nqsd0c76g0\app\resources\codex",
        ));

        assert_eq!(kind, None);
    }

    #[test]
    fn windowsapps_desktop_bundle_does_not_report_fake_cli() {
        let targets = detect_targets_from_paths(&[
            r"C:\Program Files\WindowsApps\OpenAI.Codex_26.415.1938.0_x64__2p2nqsd0c76g0\app\Codex.exe"
                .into(),
            r"C:\Program Files\WindowsApps\OpenAI.Codex_26.415.1938.0_x64__2p2nqsd0c76g0\app\resources\codex"
                .into(),
        ]);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].kind, LaunchTarget::Desktop);
    }

    #[test]
    fn classifies_shell_appsfolder_store_target_as_desktop() {
        let kind = classify_codex_path(Path::new(
            r"shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App",
        ));

        assert_eq!(kind, Some(LaunchTarget::Desktop));
    }
}
