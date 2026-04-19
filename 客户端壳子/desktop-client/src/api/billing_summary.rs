use crate::api::http::ApiClient;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BillingSummary {
    pub remaining_milli_tokens: i64,
    pub recharged_milli_tokens: i64,
    pub consumed_milli_tokens: i64,
    pub remaining_tokens: f64,
    pub recharged_tokens: f64,
    pub consumed_tokens: f64,
    pub token_unit: String,
}

pub fn fetch_billing_summary_blocking(client: &ApiClient) -> anyhow::Result<BillingSummary> {
    client.get_json_blocking("/client/billing-summary")
}

#[cfg(test)]
mod tests {
    use super::fetch_billing_summary_blocking;
    use crate::api::http::ApiClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn fetch_billing_summary_blocking_hits_client_summary_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"remaining_milli_tokens\":1234500,\"recharged_milli_tokens\":2000000,\"consumed_milli_tokens\":765500,\"remaining_tokens\":1234.5,\"recharged_tokens\":2000,\"consumed_tokens\":765.5,\"token_unit\":\"token\"}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let summary = fetch_billing_summary_blocking(&client).unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/client/billing-summary");
        assert_eq!(summary.remaining_tokens, 1234.5);
        assert_eq!(summary.token_unit, "token");
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
