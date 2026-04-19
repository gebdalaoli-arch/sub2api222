use crate::api::usage::{PaginatedUsageLogs, UsageLog};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageDetailViewModel {
    pub summary_title: String,
    pub total_requests_text: String,
    pub total_tokens_text: String,
    pub total_actual_cost_text: String,
    pub lines: Vec<String>,
}

impl UsageDetailViewModel {
    pub fn empty() -> Self {
        Self {
            summary_title: "最近 0 条消费明细".to_string(),
            total_requests_text: "0".to_string(),
            total_tokens_text: "0".to_string(),
            total_actual_cost_text: "¥0.000000".to_string(),
            lines: vec!["登录后可查看模型、Token、时间与费用明细。".to_string()],
        }
    }

    pub fn from_page(page: Option<&PaginatedUsageLogs>) -> Self {
        let Some(page) = page else {
            return Self::empty();
        };

        if page.items.is_empty() {
            return Self::empty();
        }

        let total_tokens = page
            .items
            .iter()
            .map(total_tokens_for_log)
            .sum::<i64>();
        let total_actual_cost = page.items.iter().map(|item| item.actual_cost).sum::<f64>();
        let lines = page.items.iter().take(12).map(format_usage_line).collect();

        Self {
            summary_title: format!("最近 {} 条消费明细", page.items.len()),
            total_requests_text: page.items.len().to_string(),
            total_tokens_text: total_tokens.to_string(),
            total_actual_cost_text: format!("¥{total_actual_cost:.6}"),
            lines,
        }
    }
}

fn total_tokens_for_log(log: &UsageLog) -> i64 {
    log.input_tokens + log.output_tokens + log.cache_creation_tokens + log.cache_read_tokens
}

fn format_usage_line(log: &UsageLog) -> String {
    let time = log.created_at.replace('T', " ").trim_end_matches('Z').to_string();
    let api_key_name = log
        .api_key
        .as_ref()
        .map(|item| item.name.as_str())
        .unwrap_or("未命名 Key");
    let endpoint = log
        .inbound_endpoint
        .as_deref()
        .unwrap_or("/v1/unknown");
    format!(
        "{} · {}\n{} · {} · 输入 {} · 输出 {} · 缓存 {} / {} · ¥{:.6}",
        log.model,
        time,
        api_key_name,
        endpoint,
        log.input_tokens,
        log.output_tokens,
        log.cache_creation_tokens,
        log.cache_read_tokens,
        log.actual_cost
    )
}

#[cfg(test)]
mod tests {
    use super::UsageDetailViewModel;
    use crate::api::usage::{PaginatedUsageLogs, UsageAPIKey, UsageLog};

    #[test]
    fn usage_detail_view_model_formats_model_tokens_cost_and_time() {
        let page = PaginatedUsageLogs {
            items: vec![UsageLog {
                id: 1,
                user_id: 1,
                api_key_id: 2,
                account_id: 3,
                request_id: "req_1".to_string(),
                model: "gpt-5.4".to_string(),
                service_tier: Some("priority".to_string()),
                reasoning_effort: Some("high".to_string()),
                inbound_endpoint: Some("/v1/responses".to_string()),
                upstream_endpoint: Some("/v1/responses".to_string()),
                input_tokens: 123,
                output_tokens: 456,
                cache_creation_tokens: 10,
                cache_read_tokens: 20,
                cache_creation_5m_tokens: 0,
                cache_creation_1h_tokens: 0,
                input_cost: 0.001,
                output_cost: 0.002,
                cache_creation_cost: 0.0003,
                cache_read_cost: 0.0002,
                total_cost: 0.0035,
                actual_cost: 0.0045,
                rate_multiplier: 1.5,
                billing_type: 1,
                request_type: "sync".to_string(),
                stream: false,
                openai_ws_mode: false,
                duration_ms: Some(1200),
                first_token_ms: Some(300),
                image_count: 0,
                image_size: None,
                user_agent: Some("Codex/1.0".to_string()),
                cache_ttl_overridden: false,
                billing_mode: Some("token".to_string()),
                created_at: "2025-01-02T15:04:05Z".to_string(),
                api_key: Some(UsageAPIKey {
                    id: 2,
                    name: "主力 Key".to_string(),
                }),
            }],
            total: 1,
            page: 1,
            page_size: 20,
            pages: 1,
        };

        let vm = UsageDetailViewModel::from_page(Some(&page));

        assert_eq!(vm.summary_title, "最近 1 条消费明细");
        assert_eq!(vm.total_requests_text, "1");
        assert_eq!(vm.total_tokens_text, "609");
        assert_eq!(vm.total_actual_cost_text, "¥0.004500");
        assert!(vm.lines[0].contains("gpt-5.4"));
        assert!(vm.lines[0].contains("2025-01-02 15:04:05"));
        assert!(vm.lines[0].contains("输入 123"));
        assert!(vm.lines[0].contains("输出 456"));
        assert!(vm.lines[0].contains("¥0.004500"));
    }

    #[test]
    fn usage_detail_view_model_uses_safe_empty_state() {
        let vm = UsageDetailViewModel::from_page(None);

        assert_eq!(vm.summary_title, "最近 0 条消费明细");
        assert_eq!(vm.total_requests_text, "0");
        assert_eq!(vm.total_tokens_text, "0");
        assert_eq!(vm.total_actual_cost_text, "¥0.000000");
        assert_eq!(vm.lines, vec!["登录后可查看模型、Token、时间与费用明细。".to_string()]);
    }
}
