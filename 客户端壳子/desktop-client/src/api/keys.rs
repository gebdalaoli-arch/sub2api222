use crate::api::http::ApiClient;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CreateAPIKeyRequest {
    pub name: String,
    pub group_id: i64,
    pub expires_in_days: i32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct APIKey {
    pub id: i64,
    pub user_id: i64,
    pub key: String,
    pub name: String,
    pub group_id: Option<i64>,
    pub expires_at: Option<String>,
}

pub fn create_api_key_blocking(
    client: &ApiClient,
    request: &CreateAPIKeyRequest,
) -> anyhow::Result<APIKey> {
    client.post_json_blocking("/keys", request)
}

#[cfg(test)]
mod tests {
    use super::{create_api_key_blocking, CreateAPIKeyRequest};
    use crate::api::http::ApiClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn create_api_key_blocking_posts_short_lived_key_request() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"id\":7,\"user_id\":1,\"key\":\"sk-temp-123\",\"name\":\"desktop-view-key\",\"group_id\":9,\"expires_at\":\"2025-01-09T15:04:05Z\"}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let key = create_api_key_blocking(
            &client,
            &CreateAPIKeyRequest {
                name: "desktop-view-key".to_string(),
                group_id: 9,
                expires_in_days: 7,
            },
        )
        .unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/keys");
        assert_eq!(key.key, "sk-temp-123");
        assert_eq!(key.group_id, Some(9));
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
