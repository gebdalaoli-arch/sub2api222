use crate::api::{auth::StatusMessageResponse, http::ApiClient};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum DesktopSessionTarget {
    #[serde(rename = "desktop")]
    Desktop,
    #[serde(rename = "cli")]
    Cli,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DesktopSessionCreateRequest {
    pub target: DesktopSessionTarget,
    pub group_id: i64,
    pub device_id: String,
    pub device_name: String,
    pub client_version: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct DesktopSessionResponse {
    pub session_id: String,
    pub user_id: i64,
    pub runtime_token: Option<String>,
    pub profile_key: String,
    #[serde(rename = "refresh_after")]
    pub refresh_after_nanos: u64,
    pub expires_at: String,
    pub gateway_base_url: String,
}

impl DesktopSessionResponse {
    pub fn refresh_after_duration(&self) -> Duration {
        Duration::from_nanos(self.refresh_after_nanos)
    }

    pub fn gateway_url(&self, api_base_url: &str) -> String {
        if self.gateway_base_url.starts_with("http://")
            || self.gateway_base_url.starts_with("https://")
        {
            return self.gateway_base_url.clone();
        }

        let origin = api_base_url
            .split("/api/")
            .next()
            .unwrap_or(api_base_url)
            .trim_end_matches('/');
        format!("{origin}{}", self.gateway_base_url)
    }
}

pub fn create_desktop_session_blocking(
    client: &ApiClient,
    request: &DesktopSessionCreateRequest,
) -> anyhow::Result<DesktopSessionResponse> {
    client.post_json_blocking("/desktop/sessions", request)
}

pub fn refresh_desktop_session_blocking(
    client: &ApiClient,
    session_id: &str,
) -> anyhow::Result<DesktopSessionResponse> {
    client.post_empty_blocking(&format!("/desktop/sessions/{session_id}/refresh"))
}

pub fn revoke_desktop_session_blocking(
    client: &ApiClient,
    session_id: &str,
) -> anyhow::Result<StatusMessageResponse> {
    client.delete_json_blocking(&format!("/desktop/sessions/{session_id}"))
}

#[cfg(test)]
mod tests {
    use super::{
        create_desktop_session_blocking, refresh_desktop_session_blocking,
        revoke_desktop_session_blocking, DesktopSessionCreateRequest, DesktopSessionResponse,
        DesktopSessionTarget,
    };
    use crate::api::http::ApiClient;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn desktop_session_create_request_matches_backend_contract() {
        assert_eq!(
            serde_json::to_value(DesktopSessionCreateRequest {
                target: DesktopSessionTarget::Desktop,
                group_id: 9,
                device_id: "desktop-1".to_string(),
                device_name: "mbp".to_string(),
                client_version: "0.1.0".to_string(),
            })
            .unwrap(),
            json!({
                "target": "desktop",
                "group_id": 9,
                "device_id": "desktop-1",
                "device_name": "mbp",
                "client_version": "0.1.0"
            })
        );
    }

    #[test]
    fn desktop_session_response_deserializes_backend_duration_contract() {
        let response: DesktopSessionResponse = serde_json::from_value(json!({
            "session_id": "desktop-session-1",
            "user_id": 1,
            "runtime_token": "runtime-token-1",
            "profile_key": "platform-desktop",
            "refresh_after": 1800000000000u64,
            "expires_at": "2025-01-02T15:04:05Z",
            "gateway_base_url": "/api/desktop/v1"
        }))
        .unwrap();

        assert_eq!(response.session_id, "desktop-session-1");
        assert_eq!(response.refresh_after_nanos, 1_800_000_000_000);
    }

    #[test]
    fn gateway_base_url_normalizes_relative_contract_to_absolute_origin() {
        let response = DesktopSessionResponse {
            session_id: "desktop-session-1".to_string(),
            user_id: 1,
            runtime_token: Some("runtime-token-1".to_string()),
            profile_key: "platform-desktop".to_string(),
            refresh_after_nanos: 1_800_000_000_000,
            expires_at: "2025-01-02T15:04:05Z".to_string(),
            gateway_base_url: "/api/desktop/v1".to_string(),
        };

        assert_eq!(
            response.gateway_url("https://sub2api.example.com/api/v1"),
            "https://sub2api.example.com/api/desktop/v1"
        );
    }

    #[test]
    fn create_desktop_session_blocking_hits_desktop_session_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"session_id\":\"desktop-session-1\",\"user_id\":1,\"runtime_token\":\"runtime-token-1\",\"profile_key\":\"platform-desktop\",\"refresh_after\":1800000000000,\"expires_at\":\"2025-01-02T15:04:05Z\",\"gateway_base_url\":\"/api/desktop/v1\"}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));
        let response = create_desktop_session_blocking(
            &client,
            &DesktopSessionCreateRequest {
                target: DesktopSessionTarget::Desktop,
                group_id: 9,
                device_id: "desktop-1".to_string(),
                device_name: "mbp".to_string(),
                client_version: "0.1.0".to_string(),
            },
        )
        .unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/desktop/sessions");
        assert_eq!(response.profile_key, "platform-desktop");
    }

    #[test]
    fn refresh_desktop_session_blocking_hits_refresh_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"session_id\":\"desktop-session-1\",\"user_id\":1,\"profile_key\":\"platform-desktop\",\"refresh_after\":1800000000000,\"expires_at\":\"2025-01-02T15:04:05Z\",\"gateway_base_url\":\"/api/desktop/v1\"}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let response = refresh_desktop_session_blocking(&client, "desktop-session-1").unwrap();

        assert_eq!(
            path_rx.recv().unwrap(),
            "/api/v1/desktop/sessions/desktop-session-1/refresh"
        );
        assert_eq!(response.runtime_token, None);
    }

    #[test]
    fn revoke_desktop_session_blocking_hits_delete_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"message\":\"desktop session revoked\"}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let response = revoke_desktop_session_blocking(&client, "desktop-session-1").unwrap();

        assert_eq!(
            path_rx.recv().unwrap(),
            "/api/v1/desktop/sessions/desktop-session-1"
        );
        assert_eq!(response.message, "desktop session revoked");
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
                let mut buffer = [0_u8; 2048];
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
