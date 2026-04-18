use crate::api::http::ApiClient;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SubscriptionSummaryItem {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub status: String,
    pub daily_used_usd: f64,
    pub daily_limit_usd: f64,
    pub weekly_used_usd: f64,
    pub weekly_limit_usd: f64,
    pub monthly_used_usd: f64,
    pub monthly_limit_usd: f64,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SubscriptionSummary {
    pub active_count: i32,
    pub total_used_usd: f64,
    pub subscriptions: Vec<SubscriptionSummaryItem>,
}

pub fn fetch_subscription_summary_blocking(
    client: &ApiClient,
) -> anyhow::Result<SubscriptionSummary> {
    client.get_json_blocking("/subscriptions/summary")
}

#[cfg(test)]
mod tests {
    use super::fetch_subscription_summary_blocking;
    use crate::api::http::ApiClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn fetch_subscription_summary_blocking_hits_summary_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"active_count\":1,\"total_used_usd\":12.5,\"subscriptions\":[{\"id\":1,\"group_id\":9,\"group_name\":\"OpenAI Pro\",\"status\":\"active\",\"daily_used_usd\":2,\"daily_limit_usd\":10,\"weekly_used_usd\":4,\"weekly_limit_usd\":30,\"monthly_used_usd\":12.5,\"monthly_limit_usd\":100,\"expires_at\":\"2025-01-02T15:04:05Z\"}]}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let summary = fetch_subscription_summary_blocking(&client).unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/subscriptions/summary");
        assert_eq!(summary.active_count, 1);
        assert_eq!(summary.subscriptions[0].group_name, "OpenAI Pro");
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
