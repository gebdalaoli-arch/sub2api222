#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Login,
    ForgotPassword,
    Dashboard,
    Launch,
    Redeem,
    RechargeNotice,
    About,
}

impl Route {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Login => "登录",
            Self::ForgotPassword => "找回密码",
            Self::Dashboard => "账户总览",
            Self::Launch => "启动 Codex",
            Self::Redeem => "兑换 CDK",
            Self::RechargeNotice => "充值说明",
            Self::About => "关于",
        }
    }
}
