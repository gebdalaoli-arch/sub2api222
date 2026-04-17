use anyhow::{anyhow, Result};
use directories::ProjectDirs;
use std::{
    fs,
    path::{Path, PathBuf},
};

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

    fn last_email_path(&self) -> PathBuf {
        self.root.join("last_email")
    }
}

#[cfg(test)]
mod tests {
    use super::AppStateStore;

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
}
