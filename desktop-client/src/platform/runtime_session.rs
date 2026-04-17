use anyhow::Result;
use std::time::Duration;

use crate::api::desktop_sessions::DesktopSessionResponse;

pub fn refresh_interval(session: &DesktopSessionResponse) -> Duration {
    session.refresh_after_duration()
}

pub async fn refresh_loop<F, Fut>(
    session: DesktopSessionResponse,
    mut refresh_fn: F,
) -> Result<DesktopSessionResponse>
where
    F: FnMut(&str) -> Fut,
    Fut: std::future::Future<Output = Result<DesktopSessionResponse>>,
{
    tokio::time::sleep(refresh_interval(&session)).await;
    refresh_fn(&session.session_id).await
}

#[cfg(test)]
mod tests {
    use super::refresh_interval;
    use crate::api::desktop_sessions::DesktopSessionResponse;

    #[test]
    fn refresh_interval_uses_backend_nanoseconds_contract() {
        let session = DesktopSessionResponse {
            session_id: "desktop-session-1".to_string(),
            user_id: 1,
            runtime_token: Some("runtime-token-1".to_string()),
            profile_key: "platform-desktop".to_string(),
            refresh_after_nanos: 1_800_000_000_000,
            expires_at: "2025-01-02T15:04:05Z".to_string(),
            gateway_base_url: "/api/desktop/v1".to_string(),
        };

        assert_eq!(refresh_interval(&session).as_secs(), 1800);
    }
}
