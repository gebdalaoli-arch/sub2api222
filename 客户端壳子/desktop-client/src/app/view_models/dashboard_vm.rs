use super::billing_vm::format_token_count;
use crate::api::account::UserProfile;
use crate::api::billing_summary::BillingSummary;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardViewModel {
    pub balance_text: String,
    pub usage_text: String,
    pub recharge_notice: String,
}

impl DashboardViewModel {
    pub fn empty() -> Self {
        Self {
            balance_text: "余额：--".to_string(),
            usage_text: "今日用量：--".to_string(),
            recharge_notice: "计费中心已支持余额、套餐、订单与兑换记录的统一查看。".to_string(),
        }
    }

    pub fn from_user_and_billing(user: &UserProfile, summary: Option<&BillingSummary>) -> Self {
        let balance_text = summary
            .map(|item| format!("剩余 Token：{}", format_token_count(item.remaining_tokens)))
            .unwrap_or_else(|| "剩余 Token：--".to_string());
        let usage_text = summary
            .map(|item| format!("累计消费：{}", format_token_count(item.consumed_tokens)))
            .unwrap_or_else(|| format!("并发额度：{} 路", user.concurrency));
        Self {
            balance_text,
            usage_text,
            recharge_notice: "可在计费中心兑换 Token CDK，并查看消费明细与 Token 账本。"
                .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DashboardViewModel;
    use crate::api::{account::UserProfile, billing_summary::BillingSummary};

    #[test]
    fn dashboard_view_model_formats_user_account_without_api_details() {
        let user = UserProfile {
            id: 7,
            email: "alice@example.com".to_string(),
            username: "alice".to_string(),
            role: "user".to_string(),
            balance: 20.0,
            concurrency: 3,
            status: "active".to_string(),
            allowed_groups: Some(vec![1]),
            run_mode: Some("simple".to_string()),
        };

        let summary = BillingSummary {
            remaining_milli_tokens: 100_000_000_000,
            recharged_milli_tokens: 120_000_000_000,
            consumed_milli_tokens: 20_000_000_000,
            remaining_tokens: 100_000_000.0,
            recharged_tokens: 120_000_000.0,
            consumed_tokens: 20_000_000.0,
            token_unit: "token".to_string(),
        };
        let vm = DashboardViewModel::from_user_and_billing(&user, Some(&summary));

        assert_eq!(vm.balance_text, "剩余 Token：1亿 Token");
        assert_eq!(vm.usage_text, "累计消费：2000万 Token");
        assert!(vm.recharge_notice.contains("Token CDK"));
        assert!(!vm.recharge_notice.contains("API Key"));
    }
}
