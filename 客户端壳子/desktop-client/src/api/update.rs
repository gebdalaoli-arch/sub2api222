use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::borrow::Cow;

use crate::api::http::ApiClient;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct DesktopAnnouncementItem {
    pub title: String,
    pub content: String,
    pub kind: String,
    #[serde(default)]
    pub pinned: bool,
}

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
    #[serde(default)]
    pub announcement_items: Vec<DesktopAnnouncementItem>,
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

pub fn list_desktop_announcements_blocking(
    client: &ApiClient,
) -> Result<Vec<DesktopAnnouncementItem>> {
    client.get_json_with_query_blocking(
        "/desktop/updates/announcements",
        &[("platform", "windows"), ("arch", "x64")],
    )
}

pub fn resolve_desktop_download_url(api_base_url: &str, download_url: &str) -> Result<String> {
    let download_url = download_url.trim();
    if download_url.is_empty() {
        return Err(anyhow!("desktop update download url is empty"));
    }

    if download_url.starts_with("http://") || download_url.starts_with("https://") {
        return Ok(download_url.to_string());
    }

    let base = reqwest::Url::parse(api_base_url)?;
    let host = base
        .host_str()
        .ok_or_else(|| anyhow!("desktop update api base url is missing host"))?;

    let mut resolved = format!("{}://{}", base.scheme(), host);
    if let Some(port) = base.port() {
        resolved.push(':');
        resolved.push_str(&port.to_string());
    }

    let normalized_path: Cow<'_, str> = if download_url.starts_with('/') {
        Cow::Borrowed(download_url)
    } else {
        Cow::Owned(format!("/{download_url}"))
    };
    resolved.push_str(&normalized_path);
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::{
        check_desktop_update_blocking, list_desktop_announcements_blocking,
        resolve_desktop_download_url,
    };
    use crate::api::http::ApiClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn check_desktop_update_blocking_deserializes_announcements_and_download_url() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"has_update\":true,\"current_version\":\"0.1.0\",\"latest_version\":\"0.2.0\",\"release_id\":2,\"force_update\":false,\"title\":\"发现新版本\",\"summary\":\"新版本已可用\",\"sha256\":\"abc123\",\"file_size\":4096,\"download_url\":\"/api/v1/desktop/updates/releases/2/package\",\"release_notes\":\"## 更新\\n- 更稳定\",\"announcement_items\":[{\"title\":\"维护提醒\",\"content\":\"今晚进行更新维护\",\"kind\":\"maintenance\",\"pinned\":true}]}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let response = check_desktop_update_blocking(&client, "0.1.0").unwrap();

        assert_eq!(
            path_rx.recv().unwrap(),
            "/api/v1/desktop/updates/check?platform=windows&arch=x64&current_version=0.1.0"
        );
        assert_eq!(response.latest_version, "0.2.0");
        assert_eq!(response.download_url, "/api/v1/desktop/updates/releases/2/package");
        assert_eq!(response.announcement_items.len(), 1);
        assert_eq!(response.announcement_items[0].title, "维护提醒");
        assert!(response.announcement_items[0].pinned);
    }

    #[test]
    fn list_desktop_announcements_blocking_hits_public_announcements_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":[{\"title\":\"版本更新\",\"content\":\"V2.4.1 版本已发布\",\"kind\":\"release\",\"pinned\":true},{\"title\":\"使用提醒\",\"content\":\"推荐默认使用桌面版启动\",\"kind\":\"notice\",\"pinned\":false}]}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let items = list_desktop_announcements_blocking(&client).unwrap();

        assert_eq!(
            path_rx.recv().unwrap(),
            "/api/v1/desktop/updates/announcements?platform=windows&arch=x64"
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "release");
        assert_eq!(items[1].title, "使用提醒");
    }

    #[test]
    fn resolve_desktop_download_url_joins_relative_path_to_api_origin() {
        let resolved = resolve_desktop_download_url(
            "https://desktop.example.com/api/v1",
            "/api/v1/desktop/updates/releases/9/package",
        )
        .unwrap();

        assert_eq!(
            resolved,
            "https://desktop.example.com/api/v1/desktop/updates/releases/9/package"
        );
    }

    #[test]
    fn resolve_desktop_download_url_keeps_absolute_url() {
        let resolved = resolve_desktop_download_url(
            "https://desktop.example.com/api/v1",
            "https://cdn.example.com/desktop/setup.exe",
        )
        .unwrap();

        assert_eq!(resolved, "https://cdn.example.com/desktop/setup.exe");
    }

    fn spawn_api_server(
        status_line: &'static str,
        body: &'static str,
    ) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (path_tx, path_rx) = mpsc::channel();

        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 4096];
                let size = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..size]).to_string();
                if let Some(line) = request.lines().next() {
                    let mut parts = line.split_whitespace();
                    let _method = parts.next();
                    if let Some(path) = parts.next() {
                        let _ = path_tx.send(path.to_string());
                    }
                }

                let response = format!(
                    "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        (format!("http://{}", address), path_rx)
    }
}
