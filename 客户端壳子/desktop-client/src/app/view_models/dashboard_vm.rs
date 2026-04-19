use crate::api::account::UserProfile;

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

    pub fn from_user(user: &UserProfile) -> Self {
        Self {
            balance_text: format!("余额：¥{:.2}", user.balance),
            usage_text: format!("并发额度：{} 路", user.concurrency),
            recharge_notice:
                "可在计费中心兑换 CDK，并在需要时查看消费明细、套餐与订单状态。"
                    .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DashboardViewModel;
    use crate::api::account::UserProfile;

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

        let vm = DashboardViewModel::from_user(&user);

        assert_eq!(vm.balance_text, "余额：¥20.00");
        assert_eq!(vm.usage_text, "并发额度：3 路");
        assert!(vm.recharge_notice.contains("兑换 CDK"));
        assert!(!vm.recharge_notice.contains("API Key"));
    }
}
