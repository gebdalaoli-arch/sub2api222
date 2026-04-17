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
}

#[cfg(test)]
mod tests {
    use super::{DesktopSessionCreateRequest, DesktopSessionResponse, DesktopSessionTarget};
    use serde_json::json;

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
}
