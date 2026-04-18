use anyhow::{anyhow, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthPreferences {
    pub remember_password: bool,
    pub auto_login: bool,
}

impl Default for AuthPreferences {
    fn default() -> Self {
        Self {
            remember_password: true,
            auto_login: false,
        }
    }
}

impl AuthPreferences {
    pub fn sanitized(&self) -> Self {
        Self {
            remember_password: self.remember_password,
            auto_login: self.remember_password && self.auto_login,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppStateStore {
    root: PathBuf,
}

impl AppStateStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_for_app() -> Result<Self> {
        let dirs = ProjectDirs::from("com", "sub2api", "TokenClient")
            .ok_or_else(|| anyhow!("failed to locate application data directory"))?;
        Ok(Self::new(dirs.data_local_dir().to_path_buf()))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn save_last_email(&self, email: &str) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        fs::write(self.last_email_path(), email)?;
        Ok(())
    }

    pub fn load_last_email(&self) -> Result<Option<String>> {
        let path = self.last_email_path();
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(fs::read_to_string(path)?))
    }

    pub fn clear_last_email(&self) -> Result<()> {
        let path = self.last_email_path();
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn save_auth_preferences(&self, prefs: &AuthPreferences) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        fs::write(
            self.auth_preferences_path(),
            serde_json::to_vec_pretty(&prefs.sanitized())?,
        )?;
        Ok(())
    }

    pub fn load_auth_preferences(&self) -> Result<Option<AuthPreferences>> {
        let path = self.auth_preferences_path();
        if !path.exists() {
            return Ok(None);
        }
        let prefs: AuthPreferences = serde_json::from_slice(&fs::read(path)?)?;
        Ok(Some(prefs.sanitized()))
    }

    fn last_email_path(&self) -> PathBuf {
        self.root.join("last_email")
    }

    fn auth_preferences_path(&self) -> PathBuf {
        self.root.join("auth_preferences.json")
    }
}

#[cfg(test)]
mod tests {
    use super::{AppStateStore, AuthPreferences};

    #[test]
    fn app_state_round_trips_last_email_without_secret_material() {
        let dir = tempfile::tempdir().unwrap();
        let store = AppStateStore::new(dir.path().to_path_buf());

        store.save_last_email("alice@example.com").unwrap();

        assert_eq!(
            store.load_last_email().unwrap().as_deref(),
            Some("alice@example.com")
        );
    }

    #[test]
    fn default_app_state_store_uses_project_directory() {
        let store = AppStateStore::default_for_app().unwrap();

        assert!(store.root().to_string_lossy().contains("TokenClient"));
    }

    #[test]
    fn app_state_round_trips_auth_preferences_and_sanitizes_auto_login() {
        let dir = tempfile::tempdir().unwrap();
        let store = AppStateStore::new(dir.path().to_path_buf());

        let prefs = AuthPreferences {
            remember_password: false,
            auto_login: true,
        };
        store.save_auth_preferences(&prefs).unwrap();

        let loaded = store.load_auth_preferences().unwrap().unwrap();
        assert!(!loaded.remember_password);
        assert!(!loaded.auto_login);
    }
}
