use crate::api::http::ApiClient;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UsageAPIKey {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UsageLog {
    pub id: i64,
    pub user_id: i64,
    pub api_key_id: i64,
    pub account_id: i64,
    pub request_id: String,
    pub model: String,
    pub service_tier: Option<String>,
    pub reasoning_effort: Option<String>,
    pub inbound_endpoint: Option<String>,
    pub upstream_endpoint: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_5m_tokens: i64,
    pub cache_creation_1h_tokens: i64,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_creation_cost: f64,
    pub cache_read_cost: f64,
    pub total_cost: f64,
    pub actual_cost: f64,
    pub rate_multiplier: f64,
    pub billing_type: i8,
    pub request_type: String,
    pub stream: bool,
    pub openai_ws_mode: bool,
    pub duration_ms: Option<i64>,
    pub first_token_ms: Option<i64>,
    pub image_count: i64,
    pub image_size: Option<String>,
    pub user_agent: Option<String>,
    pub cache_ttl_overridden: bool,
    pub billing_mode: Option<String>,
    pub created_at: String,
    pub api_key: Option<UsageAPIKey>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PaginatedUsageLogs {
    pub items: Vec<UsageLog>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
    pub pages: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageQuery {
    pub page: i32,
    pub page_size: i32,
    pub sort_by: String,
    pub sort_order: String,
}

impl Default for UsageQuery {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 20,
            sort_by: "created_at".to_string(),
            sort_order: "desc".to_string(),
        }
    }
}

pub fn fetch_usage_logs_blocking(
    client: &ApiClient,
    query: &UsageQuery,
) -> anyhow::Result<PaginatedUsageLogs> {
    client.get_json_with_query_blocking(
        "/usage",
        &[
            ("page", query.page.to_string()),
            ("page_size", query.page_size.to_string()),
            ("sort_by", query.sort_by.clone()),
            ("sort_order", query.sort_order.clone()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::{fetch_usage_logs_blocking, UsageQuery};
    use crate::api::http::ApiClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn fetch_usage_logs_blocking_hits_usage_endpoint_and_parses_cost_fields() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"items\":[{\"id\":1,\"user_id\":1,\"api_key_id\":2,\"account_id\":3,\"request_id\":\"req_1\",\"model\":\"gpt-5.4\",\"service_tier\":\"priority\",\"reasoning_effort\":\"high\",\"inbound_endpoint\":\"/v1/responses\",\"upstream_endpoint\":\"/v1/responses\",\"input_tokens\":123,\"output_tokens\":456,\"cache_creation_tokens\":0,\"cache_read_tokens\":0,\"cache_creation_5m_tokens\":0,\"cache_creation_1h_tokens\":0,\"input_cost\":0.001,\"output_cost\":0.002,\"cache_creation_cost\":0,\"cache_read_cost\":0,\"total_cost\":0.003,\"actual_cost\":0.0045,\"rate_multiplier\":1.5,\"billing_type\":1,\"request_type\":\"sync\",\"stream\":false,\"openai_ws_mode\":false,\"duration_ms\":1200,\"first_token_ms\":300,\"image_count\":0,\"image_size\":null,\"user_agent\":\"Codex/1.0\",\"cache_ttl_overridden\":false,\"billing_mode\":\"token\",\"created_at\":\"2025-01-02T15:04:05Z\",\"api_key\":{\"id\":2,\"name\":\"主力 Key\"}}],\"total\":1,\"page\":1,\"page_size\":20,\"pages\":1}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let page = fetch_usage_logs_blocking(
            &client,
            &UsageQuery {
                page: 1,
                page_size: 20,
                sort_by: "created_at".to_string(),
                sort_order: "desc".to_string(),
            },
        )
        .unwrap();

        assert_eq!(
            path_rx.recv().unwrap(),
            "/api/v1/usage?page=1&page_size=20&sort_by=created_at&sort_order=desc"
        );
        assert_eq!(page.items[0].model, "gpt-5.4");
        assert_eq!(page.items[0].input_tokens, 123);
        assert_eq!(page.items[0].output_tokens, 456);
        assert!((page.items[0].actual_cost - 0.0045).abs() < 1e-9);
        assert_eq!(page.items[0].api_key.as_ref().map(|item| item.name.as_str()), Some("主力 Key"));
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
