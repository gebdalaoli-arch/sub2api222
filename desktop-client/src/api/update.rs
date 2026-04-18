use anyhow::Result;
use serde::Deserialize;

use crate::api::http::ApiClient;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct DesktopUpdateCheckResponse {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_id: i64,
    pub force_update: bool,
    pub title: String,
    pub summary: String,
    pub sha256: String,
    pub file_size: i64,
    pub download_url: String,
    pub release_notes: String,
}

pub fn check_desktop_update_blocking(
    client: &ApiClient,
    current_version: &str,
) -> Result<DesktopUpdateCheckResponse> {
    client.get_json_with_query_blocking(
        "/desktop/updates/check",
        &[
            ("platform", "windows"),
            ("arch", "x64"),
            ("current_version", current_version),
        ],
    )
}
