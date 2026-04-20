use crate::api::http::ApiClient;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RedeemCodeRequest {
    pub code: String,
}

impl RedeemCodeRequest {
    pub fn new(code: impl Into<String>) -> Self {
        Self { code: code.into() }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RedeemResult {
    pub id: i64,
    pub code: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub value: f64,
    pub token_amount: Option<f64>,
    pub status: String,
    pub used_at: Option<String>,
    pub created_at: String,
    pub group_id: Option<i64>,
    pub validity_days: Option<i32>,
    pub group: Option<RedeemHistoryGroup>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RedeemHistoryGroup {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RedeemHistoryItem {
    pub id: i64,
    pub code: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub value: f64,
    pub token_amount: Option<f64>,
    pub status: String,
    pub used_at: String,
    pub created_at: String,
    pub notes: Option<String>,
    pub group_id: Option<i64>,
    pub validity_days: Option<i32>,
    pub group: Option<RedeemHistoryGroup>,
}

pub fn redeem_code_blocking(
    client: &ApiClient,
    request: &RedeemCodeRequest,
) -> anyhow::Result<RedeemResult> {
    client.post_json_blocking("/redeem", request)
}

pub fn fetch_redeem_history_blocking(client: &ApiClient) -> anyhow::Result<Vec<RedeemHistoryItem>> {
    client.get_json_blocking("/redeem/history")
}

#[cfg(test)]
mod tests {
    use super::{fetch_redeem_history_blocking, redeem_code_blocking, RedeemCodeRequest};
    use crate::api::http::ApiClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn redeem_code_blocking_hits_redeem_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"id\":1,\"code\":\"CDK-123\",\"type\":\"token\",\"value\":100000000,\"token_amount\":100000000,\"status\":\"used\",\"used_at\":\"2025-01-02T15:04:05Z\",\"created_at\":\"2025-01-01T15:04:05Z\",\"group_id\":9,\"validity_days\":0,\"group\":{\"id\":9,\"name\":\"desktop-openai\"}}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let response = redeem_code_blocking(&client, &RedeemCodeRequest::new("CDK-123")).unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/redeem");
        assert_eq!(response.r#type, "token");
        assert_eq!(response.token_amount, Some(100000000.0));
        assert_eq!(
            response.group.as_ref().map(|group| group.name.as_str()),
            Some("desktop-openai")
        );
    }

    #[test]
    fn fetch_redeem_history_blocking_hits_history_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":[{\"id\":1,\"code\":\"CDK-123\",\"type\":\"token\",\"value\":100000000,\"token_amount\":100000000,\"status\":\"used\",\"used_at\":\"2025-01-02T15:04:05Z\",\"created_at\":\"2025-01-01T15:04:05Z\",\"group_id\":9,\"validity_days\":30,\"group\":{\"id\":9,\"name\":\"OpenAI Pro\"}}]}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let history = fetch_redeem_history_blocking(&client).unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/redeem/history");
        assert_eq!(history[0].token_amount, Some(100000000.0));
        assert_eq!(
            history[0].group.as_ref().map(|group| group.name.as_str()),
            Some("OpenAI Pro")
        );
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
