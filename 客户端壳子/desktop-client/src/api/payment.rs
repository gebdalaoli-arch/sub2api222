use crate::api::http::ApiClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PaymentMethodLimit {
    pub daily_limit: f64,
    pub daily_used: f64,
    pub daily_remaining: f64,
    pub single_min: f64,
    pub single_max: f64,
    pub fee_rate: f64,
    pub available: bool,
}

fn deserialize_plan_features<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Features {
        Text(String),
        List(Vec<String>),
        Empty(()),
    }

    match Features::deserialize(deserializer)? {
        Features::Text(text) => Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect()),
        Features::List(list) => Ok(list),
        Features::Empty(_) => Ok(Vec::new()),
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SubscriptionPlan {
    pub id: i64,
    pub group_id: i64,
    pub group_platform: Option<String>,
    pub group_name: Option<String>,
    pub rate_multiplier: Option<f64>,
    pub daily_limit_usd: Option<f64>,
    pub weekly_limit_usd: Option<f64>,
    pub monthly_limit_usd: Option<f64>,
    pub supported_model_scopes: Option<Vec<String>>,
    pub name: String,
    pub description: String,
    pub price: f64,
    pub original_price: Option<f64>,
    pub validity_days: i32,
    pub validity_unit: String,
    #[serde(default, deserialize_with = "deserialize_plan_features")]
    pub features: Vec<String>,
    pub for_sale: bool,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CheckoutInfo {
    pub methods: HashMap<String, PaymentMethodLimit>,
    pub global_min: f64,
    pub global_max: f64,
    pub plans: Vec<SubscriptionPlan>,
    pub balance_disabled: bool,
    pub balance_recharge_multiplier: f64,
    pub recharge_fee_rate: f64,
    pub help_text: String,
    pub help_image_url: String,
    pub stripe_publishable_key: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CreateOrderRequest {
    pub amount: f64,
    pub payment_type: String,
    pub order_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreateOrderResult {
    pub order_id: i64,
    pub amount: f64,
    pub pay_url: Option<String>,
    pub qr_code: Option<String>,
    pub client_secret: Option<String>,
    pub pay_amount: f64,
    pub fee_rate: f64,
    pub expires_at: Option<String>,
    pub payment_mode: Option<String>,
}

pub fn fetch_checkout_info_blocking(client: &ApiClient) -> anyhow::Result<CheckoutInfo> {
    client.get_json_blocking("/payment/checkout-info")
}

pub fn create_order_blocking(
    client: &ApiClient,
    request: &CreateOrderRequest,
) -> anyhow::Result<CreateOrderResult> {
    client.post_json_blocking("/payment/orders", request)
}

pub fn fetch_my_orders_blocking(client: &ApiClient) -> anyhow::Result<PaginatedPaymentOrders> {
    client.get_json_blocking("/payment/orders/my")
}

pub fn fetch_order_blocking(client: &ApiClient, order_id: i64) -> anyhow::Result<PaymentOrder> {
    client.get_json_blocking(&format!("/payment/orders/{order_id}"))
}

pub fn cancel_order_blocking(client: &ApiClient, order_id: i64) -> anyhow::Result<PaymentOrder> {
    client.post_empty_blocking(&format!("/payment/orders/{order_id}/cancel"))
}

#[cfg(test)]
mod tests {
    use super::{
        cancel_order_blocking, create_order_blocking, fetch_checkout_info_blocking,
        fetch_my_orders_blocking, fetch_order_blocking, CreateOrderRequest,
    };
    use crate::api::http::ApiClient;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn fetch_checkout_info_blocking_hits_checkout_info_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"methods\":{\"alipay\":{\"daily_limit\":1000,\"daily_used\":0,\"daily_remaining\":1000,\"single_min\":10,\"single_max\":500,\"fee_rate\":1.5,\"available\":true}},\"global_min\":10,\"global_max\":5000,\"plans\":[{\"id\":1,\"group_id\":9,\"group_platform\":\"openai\",\"group_name\":\"OpenAI Pro\",\"rate_multiplier\":1.5,\"daily_limit_usd\":10,\"weekly_limit_usd\":30,\"monthly_limit_usd\":100,\"supported_model_scopes\":[\"gpt-5.4\"],\"name\":\"Pro 30 天\",\"description\":\"企业版套餐\",\"price\":99,\"original_price\":129,\"validity_days\":30,\"validity_unit\":\"day\",\"features\":[\"不限速\",\"优先队列\"],\"for_sale\":true,\"sort_order\":10}],\"balance_disabled\":false,\"balance_recharge_multiplier\":1,\"recharge_fee_rate\":1.5,\"help_text\":\"如需发票请联系客服\",\"help_image_url\":\"\",\"stripe_publishable_key\":\"\"}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let checkout = fetch_checkout_info_blocking(&client).unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/payment/checkout-info");
        assert_eq!(checkout.methods["alipay"].fee_rate, 1.5);
        assert_eq!(checkout.plans[0].name, "Pro 30 天");
    }

    #[test]
    fn create_order_blocking_posts_order_payload() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"order_id\":77,\"amount\":99,\"pay_url\":\"https://pay.example.com/77\",\"qr_code\":null,\"client_secret\":null,\"pay_amount\":100.49,\"fee_rate\":1.5,\"expires_at\":\"2025-01-02T15:04:05Z\",\"payment_mode\":\"redirect\"}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let result = create_order_blocking(
            &client,
            &CreateOrderRequest {
                amount: 99.0,
                payment_type: "alipay".to_string(),
                order_type: "subscription".to_string(),
                plan_id: Some(1),
            },
        )
        .unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/payment/orders");
        assert_eq!(result.order_id, 77);
        assert_eq!(result.pay_url.as_deref(), Some("https://pay.example.com/77"));
    }

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

    #[test]
    fn fetch_order_blocking_hits_order_detail_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"id\":88,\"user_id\":1,\"amount\":50,\"pay_amount\":50.5,\"fee_rate\":1,\"payment_type\":\"alipay\",\"out_trade_no\":\"ORD-88\",\"status\":\"PENDING\",\"order_type\":\"balance\",\"created_at\":\"2025-01-01T15:04:05Z\",\"expires_at\":\"2025-01-01T16:04:05Z\",\"refund_amount\":0}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let order = fetch_order_blocking(&client, 88).unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/payment/orders/88");
        assert_eq!(order.status, "PENDING");
    }

    #[test]
    fn cancel_order_blocking_hits_cancel_endpoint() {
        let (base_url, path_rx) = spawn_api_server(
            "HTTP/1.1 200 OK",
            "{\"code\":0,\"message\":\"success\",\"data\":{\"id\":88,\"user_id\":1,\"amount\":50,\"pay_amount\":50.5,\"fee_rate\":1,\"payment_type\":\"alipay\",\"out_trade_no\":\"ORD-88\",\"status\":\"CANCELLED\",\"order_type\":\"balance\",\"created_at\":\"2025-01-01T15:04:05Z\",\"expires_at\":\"2025-01-01T16:04:05Z\",\"refund_amount\":0}}",
        );
        let client = ApiClient::new(format!("{base_url}/api/v1"));

        let order = cancel_order_blocking(&client, 88).unwrap();

        assert_eq!(path_rx.recv().unwrap(), "/api/v1/payment/orders/88/cancel");
        assert_eq!(order.status, "CANCELLED");
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
