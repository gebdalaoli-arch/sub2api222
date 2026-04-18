use anyhow::Result;
use keyring::Entry;

const SERVICE_NAME: &str = "sub2api-desktop";

pub trait RefreshTokenStore {
    fn save_refresh_token(&self, token: &str) -> Result<()>;
    fn load_refresh_token(&self) -> Result<Option<String>>;
    fn clear_refresh_token(&self) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct SystemCredentialStore {
    service_name: String,
    account_name: String,
}

impl SystemCredentialStore {
    pub fn new(device_id: impl Into<String>) -> Self {
        let device_id = device_id.into();
        Self {
            service_name: SERVICE_NAME.to_string(),
            account_name: format!("refresh-token:{device_id}"),
        }
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    pub fn account_name(&self) -> &str {
        &self.account_name
    }

    fn entry(&self) -> Result<Entry> {
        Ok(Entry::new(self.service_name(), self.account_name())?)
    }
}

impl RefreshTokenStore for SystemCredentialStore {
    fn save_refresh_token(&self, token: &str) -> Result<()> {
        Ok(self.entry()?.set_password(token)?)
    }

    fn load_refresh_token(&self) -> Result<Option<String>> {
        match self.entry()?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn clear_refresh_token(&self) -> Result<()> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct FileCredentialStore {
    root: std::path::PathBuf,
}

#[cfg(test)]
impl FileCredentialStore {
    pub fn new(root: std::path::PathBuf) -> Self {
        Self { root }
    }

    fn refresh_token_path(&self) -> std::path::PathBuf {
        self.root.join("refresh_token")
    }
}

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::SystemCredentialStore;
    use super::{FileCredentialStore, RefreshTokenStore};

    #[test]
    fn system_credential_store_uses_stable_service_and_account_names() {
        let store = SystemCredentialStore::new("device-1");

        assert_eq!(store.service_name(), "sub2api-desktop");
        assert_eq!(store.account_name(), "refresh-token:device-1");
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
}
