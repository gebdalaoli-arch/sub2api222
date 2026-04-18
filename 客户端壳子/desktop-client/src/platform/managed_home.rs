use anyhow::Result;
use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        cleanup_runtime_roots_older_than, write_platform_home, write_runtime_metadata,
        ManagedHomePaths, RuntimeSessionMetadata,
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
}
