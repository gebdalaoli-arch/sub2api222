use crate::api::http::ApiClient;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UserProfile {
    pub id: i64,
    pub email: String,
    pub username: String,
    pub role: String,
    pub balance: f64,
    pub concurrency: i32,
    pub status: String,
    pub allowed_groups: Option<Vec<i64>>,
    pub run_mode: Option<String>,
}

impl UserProfile {
    pub fn display_name(&self) -> &str {
        if self.username.trim().is_empty() {
            &self.email
        } else {
            &self.username
        }
    }
}

pub type CurrentUserResponse = UserProfile;

pub fn fetch_current_user_blocking(client: &ApiClient) -> anyhow::Result<CurrentUserResponse> {
    client.get_json_blocking("/auth/me")
}

#[cfg(test)]
mod tests {
    use super::fetch_current_user_blocking;
    use crate::api::http::ApiClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn fetch_current_user_blocking_hits_auth_me() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"id\":1,\"email\":\"alice@example.com\",\"username\":\"alice\",\"role\":\"user\",\"balance\":20,\"concurrency\":3,\"status\":\"active\",\"allowed_groups\":null,\"run_mode\":\"simple\"}}",
        );

        let client = ApiClient::new(format!("{base_url}/api/v1"));
        let user = fetch_current_user_blocking(&client).unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/auth/me");
        assert_eq!(user.run_mode.as_deref(), Some("simple"));
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
