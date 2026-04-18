use anyhow::{anyhow, Result};
use directories::BaseDirs;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use toml::Value as TomlValue;

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

    pub fn metadata_path(&self) -> PathBuf {
        self.root.join("runtime-session.json")
    }
}

fn user_config_path(home_dir: &Path) -> PathBuf {
    home_dir.join("config.toml")
}

fn user_auth_path(home_dir: &Path) -> PathBuf {
    home_dir.join("auth.json")
}

fn backup_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.platform-backup", path.to_string_lossy()))
}

fn absent_marker_path(path: &Path) -> PathBuf {
    PathBuf::from(format!(
        "{}.platform-backup.absent",
        path.to_string_lossy()
    ))
}

pub fn resolve_user_codex_home() -> Result<PathBuf> {
    if let Some(explicit) = env::var_os("CODEX_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(explicit));
    }

    let base_dirs =
        BaseDirs::new().ok_or_else(|| anyhow!("无法解析当前用户的 home 目录"))?;
    Ok(base_dirs.home_dir().join(".codex"))
}

pub fn backup_user_codex_config(home_dir: &Path) -> Result<()> {
    fs::create_dir_all(home_dir)?;

    for file_path in [user_config_path(home_dir), user_auth_path(home_dir)] {
        let backup = backup_path(&file_path);
        let absent_marker = absent_marker_path(&file_path);
        if file_path.exists() {
            fs::copy(&file_path, &backup)?;
            if absent_marker.exists() {
                fs::remove_file(absent_marker)?;
            }
        } else {
            if backup.exists() {
                fs::remove_file(&backup)?;
            }
            fs::write(absent_marker, b"absent")?;
        }
    }

    Ok(())
}

pub fn inject_platform_config_into_user_home(
    home_dir: &Path,
    gateway_base_url: &str,
    runtime_token: &str,
) -> Result<()> {
    fs::create_dir_all(home_dir)?;

    let config_path = user_config_path(home_dir);
    let mut config = if config_path.exists() {
        std::fs::read_to_string(&config_path)?
            .parse::<TomlValue>()
            .unwrap_or_else(|_| TomlValue::Table(Default::default()))
    } else {
        TomlValue::Table(Default::default())
    };

    let root = config
        .as_table_mut()
        .ok_or_else(|| anyhow!("config.toml 不是有效的 table"))?;
    root.insert(
        "model_provider".to_string(),
        TomlValue::String("OpenAI".to_string()),
    );

    let model_providers = root
        .entry("model_providers")
        .or_insert_with(|| TomlValue::Table(Default::default()));
    let model_providers = model_providers
        .as_table_mut()
        .ok_or_else(|| anyhow!("model_providers 不是有效的 table"))?;
    let openai = model_providers
        .entry("OpenAI")
        .or_insert_with(|| TomlValue::Table(Default::default()));
    let openai = openai
        .as_table_mut()
        .ok_or_else(|| anyhow!("model_providers.OpenAI 不是有效的 table"))?;
    openai.insert("name".to_string(), TomlValue::String("OpenAI".to_string()));
    openai.insert(
        "base_url".to_string(),
        TomlValue::String(gateway_base_url.to_string()),
    );
    openai.insert(
        "wire_api".to_string(),
        TomlValue::String("responses".to_string()),
    );
    openai.insert(
        "requires_openai_auth".to_string(),
        TomlValue::Boolean(true),
    );

    fs::write(&config_path, toml::to_string_pretty(&config)?)?;

    let auth_path = user_auth_path(home_dir);
    let mut auth = if auth_path.exists() {
        serde_json::from_slice::<JsonValue>(&std::fs::read(&auth_path)?)
            .unwrap_or_else(|_| JsonValue::Object(Map::new()))
    } else {
        JsonValue::Object(Map::new())
    };

    let auth_map = auth
        .as_object_mut()
        .ok_or_else(|| anyhow!("auth.json 不是有效的 object"))?;
    auth_map.insert(
        "OPENAI_API_KEY".to_string(),
        JsonValue::String(runtime_token.to_string()),
    );

    fs::write(&auth_path, serde_json::to_vec_pretty(&auth)?)?;
    Ok(())
}

pub fn restore_user_codex_config(home_dir: &Path) -> Result<()> {
    for file_path in [user_config_path(home_dir), user_auth_path(home_dir)] {
        let backup = backup_path(&file_path);
        let absent_marker = absent_marker_path(&file_path);

        if backup.exists() {
            fs::copy(&backup, &file_path)?;
            fs::remove_file(&backup)?;
        } else if absent_marker.exists() {
            if file_path.exists() {
                fs::remove_file(&file_path)?;
            }
            fs::remove_file(&absent_marker)?;
        }
    }
    Ok(())
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSessionMetadata {
    pub session_id: String,
    pub profile_key: String,
    pub target: String,
    pub created_at_epoch_secs: u64,
}

pub fn write_runtime_metadata(
    paths: &ManagedHomePaths,
    session_id: &str,
    profile_key: &str,
    target: &str,
) -> Result<()> {
    fs::create_dir_all(&paths.root)?;
    let metadata = RuntimeSessionMetadata {
        session_id: session_id.to_string(),
        profile_key: profile_key.to_string(),
        target: target.to_string(),
        created_at_epoch_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs(),
    };
    fs::write(paths.metadata_path(), serde_json::to_vec_pretty(&metadata)?)?;
    Ok(())
}

pub fn cleanup_runtime_roots_older_than(
    runtime_root: &std::path::Path,
    max_age: Duration,
) -> Result<usize> {
    if !runtime_root.exists() {
        return Ok(0);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    let mut removed = 0;

    for entry in fs::read_dir(runtime_root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let metadata_path = path.join("runtime-session.json");
        let Ok(bytes) = fs::read(&metadata_path) else {
            continue;
        };
        let Ok(metadata) = serde_json::from_slice::<RuntimeSessionMetadata>(&bytes) else {
            continue;
        };

        if now.saturating_sub(metadata.created_at_epoch_secs) >= max_age.as_secs() {
            fs::remove_dir_all(&path)?;
            removed += 1;
        }
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::{
        backup_user_codex_config, cleanup_runtime_roots_older_than,
        inject_platform_config_into_user_home, resolve_user_codex_home, restore_user_codex_config,
        write_platform_home, write_runtime_metadata, ManagedHomePaths, RuntimeSessionMetadata,
    };
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

    #[test]
    fn runtime_metadata_round_trips_session_identity() {
        let temp = tempdir().unwrap();
        let paths = ManagedHomePaths::new(
            temp.path().join("runtime").join("sess-1"),
            "platform-desktop",
        );

        write_runtime_metadata(&paths, "sess-1", "platform-desktop", "desktop").unwrap();

        let metadata: RuntimeSessionMetadata =
            serde_json::from_slice(&std::fs::read(paths.metadata_path()).unwrap()).unwrap();
        assert_eq!(metadata.session_id, "sess-1");
        assert_eq!(metadata.profile_key, "platform-desktop");
    }

    #[test]
    fn cleanup_runtime_roots_removes_old_sessions_only() {
        let temp = tempdir().unwrap();
        let runtime_root = temp.path().join("runtime");
        let stale = runtime_root.join("stale");
        let fresh = runtime_root.join("fresh");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::create_dir_all(&fresh).unwrap();

        let stale_metadata = RuntimeSessionMetadata {
            session_id: "old".to_string(),
            profile_key: "platform-desktop".to_string(),
            target: "desktop".to_string(),
            created_at_epoch_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(60 * 60 * 24),
        };
        let fresh_metadata = RuntimeSessionMetadata {
            session_id: "new".to_string(),
            profile_key: "platform-cli".to_string(),
            target: "cli".to_string(),
            created_at_epoch_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        std::fs::write(
            stale.join("runtime-session.json"),
            serde_json::to_vec(&stale_metadata).unwrap(),
        )
        .unwrap();
        std::fs::write(
            fresh.join("runtime-session.json"),
            serde_json::to_vec(&fresh_metadata).unwrap(),
        )
        .unwrap();

        let removed =
            cleanup_runtime_roots_older_than(&runtime_root, Duration::from_secs(60 * 60)).unwrap();

        assert_eq!(removed, 1);
        assert!(!stale.exists());
        assert!(fresh.exists());
    }

    #[test]
    fn resolve_user_codex_home_prefers_explicit_code_home() {
        let temp = tempdir().unwrap();
        let explicit = temp.path().join("explicit-codex-home");
        std::fs::create_dir_all(&explicit).unwrap();
        std::env::set_var("CODEX_HOME", &explicit);

        let resolved = resolve_user_codex_home().unwrap();

        std::env::remove_var("CODEX_HOME");
        assert_eq!(resolved, explicit);
    }

    #[test]
    fn inject_and_restore_user_home_preserves_existing_files() {
        let temp = tempdir().unwrap();
        let user_home = temp.path().join(".codex");
        std::fs::create_dir_all(&user_home).unwrap();
        std::fs::write(
            user_home.join("config.toml"),
            r#"model = "gpt-5.4"
[plugins]
enabled = true
[model_providers.OpenAI]
base_url = "https://old.example.com"
"#,
        )
        .unwrap();
        std::fs::write(
            user_home.join("auth.json"),
            r#"{"OPENAI_API_KEY":"old-token","other":"keep-me"}"#,
        )
        .unwrap();

        backup_user_codex_config(&user_home).unwrap();
        inject_platform_config_into_user_home(
            &user_home,
            "http://127.0.0.1:8080/api/desktop/v1",
            "runtime-token-abc",
        )
        .unwrap();

        let injected_config = std::fs::read_to_string(user_home.join("config.toml")).unwrap();
        let injected_auth = std::fs::read_to_string(user_home.join("auth.json")).unwrap();
        assert!(injected_config.contains("model = \"gpt-5.4\""));
        assert!(injected_config.contains("enabled = true"));
        assert!(injected_config.contains("base_url = \"http://127.0.0.1:8080/api/desktop/v1\""));
        assert!(injected_auth.contains("runtime-token-abc"));
        assert!(injected_auth.contains("\"other\": \"keep-me\""));

        restore_user_codex_config(&user_home).unwrap();

        let restored_config = std::fs::read_to_string(user_home.join("config.toml")).unwrap();
        let restored_auth = std::fs::read_to_string(user_home.join("auth.json")).unwrap();
        assert!(restored_config.contains("https://old.example.com"));
        assert!(restored_auth.contains("old-token"));
        assert!(!user_home.join("config.toml.platform-backup").exists());
        assert!(!user_home.join("auth.json.platform-backup").exists());
    }

    #[test]
    fn restore_user_home_removes_injected_files_when_originals_were_missing() {
        let temp = tempdir().unwrap();
        let user_home = temp.path().join(".codex");
        std::fs::create_dir_all(&user_home).unwrap();

        backup_user_codex_config(&user_home).unwrap();
        inject_platform_config_into_user_home(
            &user_home,
            "http://127.0.0.1:8080/api/desktop/v1",
            "runtime-token-xyz",
        )
        .unwrap();
        restore_user_codex_config(&user_home).unwrap();

        assert!(!user_home.join("config.toml").exists());
        assert!(!user_home.join("auth.json").exists());
        assert!(!user_home.join("config.toml.platform-backup.absent").exists());
        assert!(!user_home.join("auth.json.platform-backup.absent").exists());
    }
}
