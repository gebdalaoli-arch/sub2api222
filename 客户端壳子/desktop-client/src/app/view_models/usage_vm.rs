use crate::api::usage::{PaginatedUsageLogs, UsageLog};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageDetailViewModel {
    pub summary_title: String,
    pub total_requests_text: String,
    pub total_tokens_text: String,
    pub total_actual_cost_text: String,
    pub model_lines: Vec<String>,
    pub time_lines: Vec<String>,
    pub input_lines: Vec<String>,
    pub output_lines: Vec<String>,
    pub page_meta_text: String,
}

impl UsageDetailViewModel {
    pub fn empty() -> Self {
        Self {
            summary_title: "最近 0 条消费明细".to_string(),
            total_requests_text: "0".to_string(),
            total_tokens_text: "0".to_string(),
            total_actual_cost_text: "¥0.000000".to_string(),
            model_lines: vec!["暂无数据".to_string()],
            time_lines: vec!["暂无数据".to_string()],
            input_lines: vec!["0".to_string()],
            output_lines: vec!["0".to_string()],
            page_meta_text: "第 0 / 0 页".to_string(),
        }
    }

    pub fn from_page(page: Option<&PaginatedUsageLogs>) -> Self {
        let Some(page) = page else {
            return Self::empty();
        };

        if page.items.is_empty() {
            return Self {
                page_meta_text: format!("第 {} / {} 页", page.page, page.pages.max(1)),
                ..Self::empty()
            };
        }

        let total_tokens = page
            .items
            .iter()
            .map(total_tokens_for_log)
            .sum::<i64>();
        let total_actual_cost = page.items.iter().map(|item| item.actual_cost).sum::<f64>();

        Self {
            summary_title: format!("最近 {} 条消费明细", page.items.len()),
            total_requests_text: page.items.len().to_string(),
            total_tokens_text: total_tokens.to_string(),
            total_actual_cost_text: format!("¥{total_actual_cost:.6}"),
            model_lines: page.items.iter().map(|item| item.model.clone()).collect(),
            time_lines: page.items.iter().map(format_time_line).collect(),
            input_lines: page.items.iter().map(format_input_line).collect(),
            output_lines: page.items.iter().map(format_output_line).collect(),
            page_meta_text: format!("第 {} / {} 页", page.page, page.pages.max(1)),
        }
    }
}

fn total_tokens_for_log(log: &UsageLog) -> i64 {
    merged_input_tokens(log) + merged_output_tokens(log)
}

fn merged_input_tokens(log: &UsageLog) -> i64 {
    log.input_tokens + log.cache_creation_tokens + log.cache_read_tokens
}

fn merged_output_tokens(log: &UsageLog) -> i64 {
    if log.image_count > 0 && log.output_tokens == 0 {
        log.image_count
    } else {
        log.output_tokens
    }
}

fn format_time_line(log: &UsageLog) -> String {
    log.created_at.replace('T', " ").trim_end_matches('Z').to_string()
}

fn format_input_line(log: &UsageLog) -> String {
    merged_input_tokens(log).to_string()
}

fn format_output_line(log: &UsageLog) -> String {
    if log.image_count > 0 && log.output_tokens == 0 {
        format!("图片 {}", log.image_count)
    } else {
        merged_output_tokens(log).to_string()
    }
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
        assert_eq!(vm.model_lines[0], "gpt-5.4");
        assert_eq!(vm.time_lines[0], "2025-01-02 15:04:05");
        assert_eq!(vm.input_lines[0], "153");
        assert_eq!(vm.output_lines[0], "456");
        assert_eq!(vm.page_meta_text, "第 1 / 1 页");
    }

    #[test]
    fn usage_detail_view_model_uses_safe_empty_state() {
        let vm = UsageDetailViewModel::from_page(None);

        assert_eq!(vm.summary_title, "最近 0 条消费明细");
        assert_eq!(vm.total_requests_text, "0");
        assert_eq!(vm.total_tokens_text, "0");
        assert_eq!(vm.total_actual_cost_text, "¥0.000000");
        assert_eq!(vm.model_lines, vec!["暂无数据".to_string()]);
        assert_eq!(vm.page_meta_text, "第 0 / 0 页");
    }
}
