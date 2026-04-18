#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Login,
    ForgotPassword,
    Overview,
    Launch,
    Billing,
    Announcements,
    Settings,
    About,
}

impl Route {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Login => "登录",
            Self::ForgotPassword => "找回密码",
            Self::Overview => "账户总览",
            Self::Launch => "启动 Codex",
            Self::Billing => "计费中心",
            Self::Announcements => "公告中心",
            Self::Settings => "设置与帮助",
            Self::About => "关于",
        }
    }
}
