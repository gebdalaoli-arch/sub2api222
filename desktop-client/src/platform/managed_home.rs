use anyhow::Result;
use std::{fs, path::PathBuf};

#[derive(Debug, Clone)]
pub struct ManagedHomePaths {
    pub root: PathBuf,
    pub codex_home: PathBuf,
}

impl ManagedHomePaths {
    pub fn new(root: PathBuf, profile_name: &str) -> Self {
        let codex_home = root.join(profile_name);
        Self { root, codex_home }
    }
}

pub fn write_platform_home(
    paths: &ManagedHomePaths,
    gateway_base_url: &str,
    runtime_token: &str,
) -> Result<()> {
    fs::create_dir_all(&paths.codex_home)?;
    fs::write(
        paths.codex_home.join("config.toml"),
        format!(
            "model_provider = \"OpenAI\"\n[model_providers.OpenAI]\nname = \"OpenAI\"\nbase_url = \"{}\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n",
            gateway_base_url
        ),
    )?;
    fs::write(
        paths.codex_home.join("auth.json"),
        format!("{{\"OPENAI_API_KEY\":\"{}\"}}\n", runtime_token),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{write_platform_home, ManagedHomePaths};
    use tempfile::tempdir;

    #[test]
    fn platform_home_writes_config_and_auth_without_touching_official_home() {
        let temp = tempdir().unwrap();
        let paths = ManagedHomePaths::new(temp.path().to_path_buf(), "platform-desktop");

        write_platform_home(
            &paths,
            "http://127.0.0.1:8080/api/desktop/v1",
            "runtime-token-abc",
        )
        .unwrap();

        let config = std::fs::read_to_string(paths.codex_home.join("config.toml")).unwrap();
        let auth = std::fs::read_to_string(paths.codex_home.join("auth.json")).unwrap();
        assert!(config.contains("model_provider = \"OpenAI\""));
        assert!(config.contains("base_url = \"http://127.0.0.1:8080/api/desktop/v1\""));
        assert!(auth.contains("runtime-token-abc"));
        assert!(!temp.path().join(".codex").exists());
    }
}
