use crate::api::http::ApiClient;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum GroupPlatform {
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "openai")]
    OpenAI,
    #[serde(rename = "gemini")]
    Gemini,
    #[serde(rename = "antigravity")]
    Antigravity,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum SubscriptionType {
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "subscription")]
    Subscription,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GroupSummary {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub platform: GroupPlatform,
    pub rate_multiplier: f64,
    pub is_exclusive: bool,
    pub status: String,
    pub subscription_type: SubscriptionType,
    pub daily_limit_usd: Option<f64>,
    pub weekly_limit_usd: Option<f64>,
    pub monthly_limit_usd: Option<f64>,
    pub image_price_1k: Option<f64>,
    pub image_price_2k: Option<f64>,
    pub image_price_4k: Option<f64>,
    pub input_price_per_million_tokens: Option<f64>,
    pub claude_code_only: bool,
    pub fallback_group_id: Option<i64>,
    pub fallback_group_id_on_invalid_request: Option<i64>,
    pub require_oauth_only: bool,
    pub require_privacy_set: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub fn fetch_available_groups_blocking(client: &ApiClient) -> anyhow::Result<Vec<GroupSummary>> {
    client.get_json_blocking("/groups/available")
}

#[cfg(test)]
mod tests {
    use super::{fetch_available_groups_blocking, GroupPlatform, GroupSummary, SubscriptionType};
    use crate::api::http::ApiClient;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn available_groups_deserialize_selection_fields() {
        let groups: Vec<GroupSummary> = serde_json::from_value(json!([
            {
                "id": 9,
                "name": "OpenAI Pro",
                "description": "desktop-capable",
                "platform": "openai",
                "rate_multiplier": 1.0,
                "is_exclusive": false,
                "status": "active",
                "subscription_type": "subscription",
                "daily_limit_usd": null,
                "weekly_limit_usd": null,
                "monthly_limit_usd": null,
                "image_price_1k": null,
                "image_price_2k": null,
                "image_price_4k": null,
                "input_price_per_million_tokens": 1.5,
                "claude_code_only": false,
                "fallback_group_id": null,
                "fallback_group_id_on_invalid_request": null,
                "require_oauth_only": false,
                "require_privacy_set": false,
                "created_at": "2025-01-02T15:04:05Z",
                "updated_at": "2025-01-02T15:04:05Z"
            }
        ]))
        .unwrap();

        assert_eq!(groups[0].id, 9);
        assert_eq!(groups[0].platform, GroupPlatform::OpenAI);
        assert_eq!(groups[0].subscription_type, SubscriptionType::Subscription);
        assert_eq!(groups[0].input_price_per_million_tokens, Some(1.5));
    }

    #[test]
    fn fetch_available_groups_blocking_hits_groups_available() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":[{\"id\":9,\"name\":\"OpenAI Pro\",\"description\":null,\"platform\":\"openai\",\"rate_multiplier\":1.0,\"is_exclusive\":false,\"status\":\"active\",\"subscription_type\":\"subscription\",\"daily_limit_usd\":null,\"weekly_limit_usd\":null,\"monthly_limit_usd\":null,\"image_price_1k\":null,\"image_price_2k\":null,\"image_price_4k\":null,\"input_price_per_million_tokens\":1.5,\"claude_code_only\":false,\"fallback_group_id\":null,\"fallback_group_id_on_invalid_request\":null,\"require_oauth_only\":false,\"require_privacy_set\":false,\"created_at\":\"2025-01-02T15:04:05Z\",\"updated_at\":\"2025-01-02T15:04:05Z\"}]}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let groups = fetch_available_groups_blocking(&client).unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/groups/available");
        assert_eq!(groups.len(), 1);
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
