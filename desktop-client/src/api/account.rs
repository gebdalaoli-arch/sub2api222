use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UserProfile {
    pub id: i64,
    pub email: String,
    pub username: String,
    pub role: String,
    pub balance: f64,
    pub concurrency: i32,
    pub status: String,
    pub allowed_groups: Option<Vec<i64>>,
    pub run_mode: Option<String>,
}

impl UserProfile {
    pub fn display_name(&self) -> &str {
        if self.username.trim().is_empty() {
            &self.email
        } else {
            &self.username
        }
    }
}
