use anyhow::Result;
use directories::ProjectDirs;
use keyring::Entry;
use std::path::PathBuf;

const SERVICE_NAME: &str = "sub2api-desktop";

pub trait RefreshTokenStore {
    fn save_refresh_token(&self, token: &str) -> Result<()>;
    fn load_refresh_token(&self) -> Result<Option<String>>;
    fn clear_refresh_token(&self) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct SystemCredentialStore {
    service_name: String,
    refresh_token_account_name: String,
    password_account_name: String,
    fallback_root: PathBuf,
}

impl SystemCredentialStore {
    pub fn new(device_id: impl Into<String>) -> Self {
        let device_id = device_id.into();
        Self {
            service_name: SERVICE_NAME.to_string(),
            refresh_token_account_name: format!("refresh-token:{device_id}"),
            password_account_name: format!("password:{device_id}"),
            fallback_root: fallback_root_for_device(&device_id),
        }
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    pub fn account_name(&self) -> &str {
        &self.refresh_token_account_name
    }

    pub fn password_account_name(&self) -> &str {
        &self.password_account_name
    }

    fn refresh_token_entry(&self) -> Result<Entry> {
        Ok(Entry::new(self.service_name(), self.account_name())?)
    }

    fn password_entry(&self) -> Result<Entry> {
        Ok(Entry::new(
            self.service_name(),
            self.password_account_name(),
        )?)
    }

    fn fallback_refresh_token_store(&self) -> FileCredentialStore {
        FileCredentialStore::new(self.fallback_root.clone())
    }

    fn fallback_password_store(&self) -> FileCredentialStore {
        FileCredentialStore::new(self.fallback_root.clone())
    }

    pub fn save_password(&self, password: &str) -> Result<()> {
        match self.password_entry()?.set_password(password) {
            Ok(()) => Ok(()),
            Err(_) => self.fallback_password_store().save_password(password),
        }
    }

    pub fn load_password(&self) -> Result<Option<String>> {
        match self.password_entry()?.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => self.fallback_password_store().load_password(),
            Err(_) => self.fallback_password_store().load_password(),
        }
    }

    pub fn clear_password(&self) -> Result<()> {
        let keyring_result: Result<()> = match self.password_entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        };
        let file_result = self.fallback_password_store().clear_password();
        keyring_result.or(file_result)
    }
}

impl RefreshTokenStore for SystemCredentialStore {
    fn save_refresh_token(&self, token: &str) -> Result<()> {
        match self.refresh_token_entry()?.set_password(token) {
            Ok(()) => Ok(()),
            Err(_) => self
                .fallback_refresh_token_store()
                .save_refresh_token(token),
        }
    }

    fn load_refresh_token(&self) -> Result<Option<String>> {
        match self.refresh_token_entry()?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => {
                self.fallback_refresh_token_store().load_refresh_token()
            }
            Err(_) => self.fallback_refresh_token_store().load_refresh_token(),
        }
    }

    fn clear_refresh_token(&self) -> Result<()> {
        let keyring_result: Result<()> = match self.refresh_token_entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        };
        let file_result = self.fallback_refresh_token_store().clear_refresh_token();
        keyring_result.or(file_result)
    }
}

#[derive(Debug, Clone)]
pub struct FileCredentialStore {
    root: std::path::PathBuf,
}

impl FileCredentialStore {
    pub fn new(root: std::path::PathBuf) -> Self {
        Self { root }
    }

    fn refresh_token_path(&self) -> std::path::PathBuf {
        self.root.join("refresh_token")
    }

    fn password_path(&self) -> std::path::PathBuf {
        self.root.join("password")
    }

    pub fn save_password(&self, password: &str) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::write(self.password_path(), password)?;
        Ok(())
    }

    pub fn load_password(&self) -> Result<Option<String>> {
        let path = self.password_path();
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(std::fs::read_to_string(path)?))
    }

    pub fn clear_password(&self) -> Result<()> {
        let path = self.password_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

impl RefreshTokenStore for FileCredentialStore {
    fn save_refresh_token(&self, token: &str) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::write(self.refresh_token_path(), token)?;
        Ok(())
    }

    fn load_refresh_token(&self) -> Result<Option<String>> {
        let path = self.refresh_token_path();
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(std::fs::read_to_string(path)?))
    }

    fn clear_refresh_token(&self) -> Result<()> {
        let path = self.refresh_token_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

fn fallback_root_for_device(device_id: &str) -> PathBuf {
    if let Some(dirs) = ProjectDirs::from("com", "sub2api", "TokenClient") {
        return dirs
            .data_local_dir()
            .join("credentials")
            .join(device_id.replace(':', "_"));
    }
    std::env::temp_dir()
        .join("sub2api-desktop-client")
        .join("credentials")
        .join(device_id.replace(':', "_"))
}

#[cfg(test)]
mod tests {
    use super::SystemCredentialStore;
    use super::{FileCredentialStore, RefreshTokenStore};

    #[test]
    fn system_credential_store_uses_stable_service_and_account_names() {
        let store = SystemCredentialStore::new("device-1");

        assert_eq!(store.service_name(), "sub2api-desktop");
        assert_eq!(store.account_name(), "refresh-token:device-1");
        assert_eq!(store.password_account_name(), "password:device-1");
    }

    #[test]
    fn file_credential_store_can_clear_refresh_token() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileCredentialStore::new(dir.path().to_path_buf());

        store.save_refresh_token("refresh-token-123").unwrap();
        assert_eq!(
            store.load_refresh_token().unwrap().as_deref(),
            Some("refresh-token-123")
        );

        store.clear_refresh_token().unwrap();

        assert_eq!(store.load_refresh_token().unwrap(), None);
    }

    #[test]
    fn file_credential_store_can_round_trip_password() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileCredentialStore::new(dir.path().to_path_buf());

        store.save_password("secret-123").unwrap();
        assert_eq!(
            store.load_password().unwrap().as_deref(),
            Some("secret-123")
        );

        store.clear_password().unwrap();
        assert_eq!(store.load_password().unwrap(), None);
    }
}
