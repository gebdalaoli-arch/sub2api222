pub fn describe_platform_launch_error(error_text: &str) -> String {
    let normalized = error_text.to_ascii_lowercase();

    if normalized.contains("404") || normalized.contains("page not found") {
        return "当前服务端未部署平台代理接口，请先升级 sub2api 服务端；官方模式仍可继续使用。"
            .to_string();
    }
    if normalized.contains("subscription_required") || normalized.contains("subscription required")
    {
        return "当前分组需要有效订阅才能启动平台代理模式。".to_string();
    }
    if normalized.contains("service temporarily unavailable")
        || normalized.contains("no available accounts")
    {
        return "当前分组暂无可用上游账号，请切换其他分组后重试。".to_string();
    }
    if normalized.contains("only allows codex official clients") {
        return "当前分组仅允许 Codex 官方客户端，请切换官方模式或更换分组。"
            .to_string();
    }
    if normalized.contains("desktop_session_group_forbidden") {
        return "当前账号无权绑定所选分组，请更换分组或检查套餐权限。".to_string();
    }
    if normalized.contains("desktop_session_group_required") {
        return "请先选择一个可用分组，再启动平台代理模式。".to_string();
    }

    format!("平台代理模式启动失败：{error_text}")
}

#[cfg(test)]
mod tests {
    use super::describe_platform_launch_error;

    #[test]
    fn describe_platform_launch_error_handles_missing_server_route() {
        let message =
            describe_platform_launch_error("request failed with status 404: 404 page not found");

        assert!(message.contains("未部署平台代理接口"));
    }

    #[test]
    fn describe_platform_launch_error_handles_subscription_requirement() {
        let message =
            describe_platform_launch_error("subscription required (SUBSCRIPTION_REQUIRED)");

        assert!(message.contains("需要有效订阅"));
    }

    #[test]
    fn describe_platform_launch_error_handles_unavailable_group_capacity() {
        let message = describe_platform_launch_error("Service temporarily unavailable");

        assert!(message.contains("暂无可用上游账号"));
    }

    #[test]
    fn describe_platform_launch_error_handles_official_client_only_group() {
        let message =
            describe_platform_launch_error("This account only allows Codex official clients");

        assert!(message.contains("仅允许 Codex 官方客户端"));
    }
}
