use crate::api::http::ApiClient;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PaymentOrder {
    pub id: i64,
    pub user_id: i64,
    pub amount: f64,
    pub pay_amount: f64,
    pub fee_rate: f64,
    pub payment_type: String,
    pub out_trade_no: String,
    pub status: String,
    pub order_type: String,
    pub created_at: String,
    pub expires_at: String,
    pub paid_at: Option<String>,
    pub completed_at: Option<String>,
    pub refund_amount: f64,
    pub refund_reason: Option<String>,
    pub refund_requested_at: Option<String>,
    pub refund_requested_by: Option<i64>,
    pub refund_request_reason: Option<String>,
    pub plan_id: Option<i64>,
    pub provider_instance_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PaginatedPaymentOrders {
    pub items: Vec<PaymentOrder>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
    pub pages: i32,
}

pub fn fetch_my_orders_blocking(client: &ApiClient) -> anyhow::Result<PaginatedPaymentOrders> {
    client.get_json_blocking("/payment/orders/my")
}

#[cfg(test)]
mod tests {
    use super::fetch_my_orders_blocking;
    use crate::api::http::ApiClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn fetch_my_orders_blocking_hits_orders_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"items\":[{\"id\":1,\"user_id\":1,\"amount\":20,\"pay_amount\":20,\"fee_rate\":0,\"payment_type\":\"alipay\",\"out_trade_no\":\"ORD-1\",\"status\":\"COMPLETED\",\"order_type\":\"balance\",\"created_at\":\"2025-01-01T15:04:05Z\",\"expires_at\":\"2025-01-01T16:04:05Z\",\"refund_amount\":0}],\"total\":1,\"page\":1,\"page_size\":20,\"pages\":1}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let orders = fetch_my_orders_blocking(&client).unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/payment/orders/my");
        assert_eq!(orders.items[0].out_trade_no, "ORD-1");
        assert_eq!(orders.total, 1);
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
