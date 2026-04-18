use crate::api::{
    redeem::RedeemHistoryItem,
    subscriptions::{SubscriptionSummary, SubscriptionSummaryItem},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingViewModel {
    pub subscription_summary_text: String,
    pub subscription_lines: Vec<String>,
    pub redeem_history_lines: Vec<String>,
}

impl BillingViewModel {
    pub fn empty() -> Self {
        Self {
            subscription_summary_text: "暂无订阅摘要".to_string(),
            subscription_lines: vec!["登录后可查看当前订阅额度和到期时间。".to_string()],
            redeem_history_lines: vec!["最近兑换记录会显示在这里。".to_string()],
        }
    }

    pub fn from_summary_and_history(
        summary: Option<&SubscriptionSummary>,
        history: &[RedeemHistoryItem],
    ) -> Self {
        let mut model = Self::empty();

        if let Some(summary) = summary {
            model.subscription_summary_text = format!(
                "活跃订阅 {} 个，当前累计使用 ${:.2}",
                summary.active_count, summary.total_used_usd
            );
            model.subscription_lines = if summary.subscriptions.is_empty() {
                vec!["当前没有活跃订阅。".to_string()]
            } else {
                summary
                    .subscriptions
                    .iter()
                    .take(3)
                    .map(subscription_line)
                    .collect()
            };
        }

        if !history.is_empty() {
            model.redeem_history_lines = history
                .iter()
                .take(4)
                .map(|item| format!("{} · {} · {}", item.code, item.r#type, item.status))
                .collect();
        }

        model
    }
}

fn subscription_line(item: &SubscriptionSummaryItem) -> String {
    match item.expires_at.as_deref() {
        Some(expires_at) => format!(
            "{} · {} · 月用量 ${:.2}/${:.2} · 到期 {}",
            item.group_name, item.status, item.monthly_used_usd, item.monthly_limit_usd, expires_at
        ),
        None => format!(
            "{} · {} · 月用量 ${:.2}/${:.2}",
            item.group_name, item.status, item.monthly_used_usd, item.monthly_limit_usd
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::BillingViewModel;
    use crate::api::{
        redeem::RedeemHistoryItem,
        subscriptions::{SubscriptionSummary, SubscriptionSummaryItem},
    };

    #[test]
    fn billing_view_model_formats_summary_and_history() {
        let summary = SubscriptionSummary {
            active_count: 1,
            total_used_usd: 12.5,
            subscriptions: vec![SubscriptionSummaryItem {
                id: 1,
                group_id: 9,
                group_name: "OpenAI Pro".to_string(),
                status: "active".to_string(),
                daily_used_usd: 2.0,
                daily_limit_usd: 10.0,
                weekly_used_usd: 4.0,
                weekly_limit_usd: 30.0,
                monthly_used_usd: 12.5,
                monthly_limit_usd: 100.0,
                expires_at: Some("2025-01-02T15:04:05Z".to_string()),
            }],
        };
        let history = vec![RedeemHistoryItem {
            id: 1,
            code: "CDK-123".to_string(),
            r#type: "subscription".to_string(),
            value: 30.0,
            status: "used".to_string(),
            used_at: "2025-01-02T15:04:05Z".to_string(),
            created_at: "2025-01-01T15:04:05Z".to_string(),
            notes: None,
            group_id: Some(9),
            validity_days: Some(30),
            group: None,
        }];

        let vm = BillingViewModel::from_summary_and_history(Some(&summary), &history);

        assert!(vm.subscription_summary_text.contains("活跃订阅 1 个"));
        assert!(vm.subscription_lines[0].contains("OpenAI Pro"));
        assert!(vm.redeem_history_lines[0].contains("CDK-123"));
    }
}
