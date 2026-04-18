use crate::api::{
    account::UserProfile,
    payment::PaymentOrder,
    redeem::RedeemHistoryItem,
    subscriptions::{SubscriptionSummary, SubscriptionSummaryItem},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingViewModel {
    pub plan_title: String,
    pub balance_headline: String,
    pub usage_caption: String,
    pub subscription_summary_text: String,
    pub subscription_lines: Vec<String>,
    pub subscription_detail_lines: Vec<String>,
    pub order_lines: Vec<String>,
    pub order_detail_lines: Vec<String>,
    pub redeem_history_lines: Vec<String>,
}

impl BillingViewModel {
    pub fn empty() -> Self {
        Self {
            plan_title: "暂未开通订阅".to_string(),
            balance_headline: "¥--".to_string(),
            usage_caption: "登录后可查看余额、套餐与订单明细。".to_string(),
            subscription_summary_text: "暂无订阅摘要".to_string(),
            subscription_lines: vec!["登录后可查看当前订阅额度和到期时间。".to_string()],
            subscription_detail_lines: vec!["暂无订阅明细。".to_string()],
            order_lines: vec!["最近订单会显示在这里。".to_string()],
            order_detail_lines: vec!["暂无订单明细。".to_string()],
            redeem_history_lines: vec!["最近兑换记录会显示在这里。".to_string()],
        }
    }

    pub fn from_account_state(
        user: Option<&UserProfile>,
        summary: Option<&SubscriptionSummary>,
        orders: &[PaymentOrder],
        history: &[RedeemHistoryItem],
    ) -> Self {
        let mut model = Self::empty();

        if let Some(user) = user {
            model.balance_headline = format!("¥{:.2}", user.balance);
            model.usage_caption = format!("账户状态：{} · 并发额度 {} 路", user.status, user.concurrency);
        }

        if let Some(summary) = summary {
            model.plan_title = summary
                .subscriptions
                .first()
                .map(|item| item.group_name.clone())
                .unwrap_or_else(|| "按量钱包".to_string());
            model.subscription_summary_text = format!(
                "活跃订阅 {} 个，累计标准计费 ${:.2}",
                summary.active_count, summary.total_used_usd
            );
            model.subscription_lines = if summary.subscriptions.is_empty() {
                vec!["当前没有活跃订阅。".to_string()]
            } else {
                summary
                    .subscriptions
                    .iter()
                    .take(3)
                    .map(subscription_summary_line)
                    .collect()
            };
            model.subscription_detail_lines = if summary.subscriptions.is_empty() {
                vec!["暂无订阅明细。".to_string()]
            } else {
                summary
                    .subscriptions
                    .iter()
                    .take(6)
                    .map(subscription_detail_line)
                    .collect()
            };
        }

        if !orders.is_empty() {
            model.order_lines = orders
                .iter()
                .take(4)
                .map(order_summary_line)
                .collect();
            model.order_detail_lines = orders
                .iter()
                .take(6)
                .map(order_detail_line)
                .collect();
        }

        if !history.is_empty() {
            model.redeem_history_lines = history
                .iter()
                .take(6)
                .map(history_detail_line)
                .collect();
        }

        model
    }
}

fn subscription_summary_line(item: &SubscriptionSummaryItem) -> String {
    match item.expires_at.as_deref() {
        Some(expires_at) => format!("{} · {} · 到期 {}", item.group_name, item.status, expires_at),
        None => format!("{} · {}", item.group_name, item.status),
    }
}

fn subscription_detail_line(item: &SubscriptionSummaryItem) -> String {
    match item.expires_at.as_deref() {
        Some(expires_at) => format!(
            "{} · 日 ${:.2}/${:.2} · 周 ${:.2}/${:.2} · 月 ${:.2}/${:.2} · 到期 {}",
            item.group_name,
            item.daily_used_usd,
            item.daily_limit_usd,
            item.weekly_used_usd,
            item.weekly_limit_usd,
            item.monthly_used_usd,
            item.monthly_limit_usd,
            expires_at
        ),
        None => format!(
            "{} · 日 ${:.2}/${:.2} · 周 ${:.2}/${:.2} · 月 ${:.2}/${:.2}",
            item.group_name,
            item.daily_used_usd,
            item.daily_limit_usd,
            item.weekly_used_usd,
            item.weekly_limit_usd,
            item.monthly_used_usd,
            item.monthly_limit_usd,
        ),
    }
}

fn order_summary_line(order: &PaymentOrder) -> String {
    format!(
        "{} · {} · ￥{:.2}",
        order.out_trade_no, order.status, order.pay_amount
    )
}

fn order_detail_line(order: &PaymentOrder) -> String {
    format!(
        "{} · {} · {} · 实付 ￥{:.2} · 创建 {}",
        order.out_trade_no,
        order.order_type,
        order.payment_type,
        order.pay_amount,
        order.created_at
    )
}

fn history_detail_line(item: &RedeemHistoryItem) -> String {
    let group_text = item
        .group
        .as_ref()
        .map(|group| group.name.as_str())
        .or_else(|| item.notes.as_deref())
        .unwrap_or("未绑定分组");
    format!(
        "{} · {} · {} · {}",
        item.code, item.r#type, item.status, group_text
    )
}

#[cfg(test)]
mod tests {
    use super::BillingViewModel;
    use crate::api::{
        account::UserProfile,
        payment::PaymentOrder,
        redeem::RedeemHistoryItem,
        subscriptions::{SubscriptionSummary, SubscriptionSummaryItem},
    };

    #[test]
    fn billing_view_model_formats_summary_history_and_user_state() {
        let user = UserProfile {
            id: 1,
            email: "alice@example.com".to_string(),
            username: "alice".to_string(),
            role: "user".to_string(),
            balance: 188.0,
            concurrency: 6,
            status: "active".to_string(),
            allowed_groups: Some(vec![9]),
            run_mode: Some("full".to_string()),
        };
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
            group: Some(crate::api::redeem::RedeemHistoryGroup {
                id: 9,
                name: "OpenAI Pro".to_string(),
            }),
        }];
        let orders = vec![PaymentOrder {
            id: 1,
            user_id: 1,
            amount: 20.0,
            pay_amount: 20.0,
            fee_rate: 0.0,
            payment_type: "alipay".to_string(),
            out_trade_no: "ORD-1".to_string(),
            status: "COMPLETED".to_string(),
            order_type: "balance".to_string(),
            created_at: "2025-01-01T15:04:05Z".to_string(),
            expires_at: "2025-01-01T16:04:05Z".to_string(),
            paid_at: None,
            completed_at: None,
            refund_amount: 0.0,
            refund_reason: None,
            refund_requested_at: None,
            refund_requested_by: None,
            refund_request_reason: None,
            plan_id: None,
            provider_instance_id: None,
        }];

        let vm = BillingViewModel::from_account_state(Some(&user), Some(&summary), &orders, &history);

        assert_eq!(vm.plan_title, "OpenAI Pro");
        assert_eq!(vm.balance_headline, "¥188.00");
        assert!(vm.usage_caption.contains("并发额度 6 路"));
        assert!(vm.subscription_summary_text.contains("活跃订阅 1 个"));
        assert!(vm.subscription_detail_lines[0].contains("日 $2.00/$10.00"));
        assert!(vm.subscription_detail_lines[0].contains("周 $4.00/$30.00"));
        assert!(vm.order_lines[0].contains("ORD-1"));
        assert!(vm.order_detail_lines[0].contains("balance"));
        assert!(vm.redeem_history_lines[0].contains("CDK-123"));
        assert!(vm.redeem_history_lines[0].contains("OpenAI Pro"));
    }

    #[test]
    fn billing_view_model_uses_safe_defaults_without_data() {
        let vm = BillingViewModel::from_account_state(None, None, &[], &[]);

        assert_eq!(vm.plan_title, "暂未开通订阅");
        assert_eq!(vm.balance_headline, "¥--");
        assert_eq!(vm.subscription_detail_lines, vec!["暂无订阅明细。".to_string()]);
        assert_eq!(vm.order_detail_lines, vec!["暂无订单明细。".to_string()]);
    }
}
