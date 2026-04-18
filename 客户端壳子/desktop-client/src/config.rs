pub const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:8080/api/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub api_base_url: String,
    pub uses_fallback_api_base_url: bool,
}

pub fn build_app_config(api_base_url: Option<&str>) -> AppConfig {
    AppConfig {
        api_base_url: api_base_url.unwrap_or(DEFAULT_API_BASE_URL).to_string(),
        uses_fallback_api_base_url: api_base_url.is_none(),
    }
}

pub fn app_config() -> AppConfig {
    build_app_config(option_env!("SUB2API_DESKTOP_API_BASE_URL"))
}

pub fn is_local_debug_api_base_url(api_base_url: &str) -> bool {
    let normalized = api_base_url
        .trim()
        .trim_end_matches('/')
        .to_ascii_lowercase();
    normalized == DEFAULT_API_BASE_URL || normalized == "http://localhost:8080/api/v1"
}

pub fn packaged_local_debug_api_message(
    api_base_url: &str,
    uses_fallback_api_base_url: bool,
    is_debug_build: bool,
) -> Option<String> {
    if is_debug_build || !uses_fallback_api_base_url || !is_local_debug_api_base_url(api_base_url) {
        return None;
    }

    Some(
        "当前安装包仍在使用本机调试地址 127.0.0.1:8080，无法连接线上服务。请重新执行 `powershell -NoProfile -ExecutionPolicy Bypass -File .\\build-desktop-installer.ps1 -ApiBaseUrl \"https://你的服务端地址\"` 生成安装包。".to_string(),
    )
}

impl AppConfig {
    pub fn has_packaged_local_debug_api(&self) -> bool {
        packaged_local_debug_api_message(
            &self.api_base_url,
            self.uses_fallback_api_base_url,
            cfg!(debug_assertions),
        )
        .is_some()
    }

    pub fn packaged_local_debug_api_message(&self) -> Option<String> {
        packaged_local_debug_api_message(
            &self.api_base_url,
            self.uses_fallback_api_base_url,
            cfg!(debug_assertions),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_app_config, is_local_debug_api_base_url, packaged_local_debug_api_message,
        DEFAULT_API_BASE_URL,
    };

    #[test]
    fn app_config_uses_fixed_default_backend_when_build_value_missing() {
        let config = build_app_config(None);

        assert_eq!(config.api_base_url, DEFAULT_API_BASE_URL);
        assert!(config.uses_fallback_api_base_url);
    }

    #[test]
    fn app_config_prefers_explicit_build_backend() {
        let config = build_app_config(Some("https://api.example.com/api/v1"));

        assert_eq!(config.api_base_url, "https://api.example.com/api/v1");
        assert!(!config.uses_fallback_api_base_url);
    }

    #[test]
    fn explicit_localhost_build_value_is_not_marked_as_fallback() {
        let config = build_app_config(Some(DEFAULT_API_BASE_URL));

        assert_eq!(config.api_base_url, DEFAULT_API_BASE_URL);
        assert!(!config.uses_fallback_api_base_url);
    }

    #[test]
    fn localhost_detection_accepts_known_dev_origins() {
        assert!(is_local_debug_api_base_url(DEFAULT_API_BASE_URL));
        assert!(is_local_debug_api_base_url("http://localhost:8080/api/v1"));
        assert!(!is_local_debug_api_base_url(
            "https://api.example.com/api/v1"
        ));
    }

    #[test]
    fn packaged_localhost_fallback_surfaces_actionable_message() {
        let message = packaged_local_debug_api_message(DEFAULT_API_BASE_URL, true, false);

        assert!(message
            .unwrap()
            .contains("build-desktop-installer.ps1 -ApiBaseUrl"));
    }

    #[test]
    fn debug_build_or_explicit_backend_does_not_surface_packaging_message() {
        assert!(packaged_local_debug_api_message(DEFAULT_API_BASE_URL, true, true).is_none());
        assert!(packaged_local_debug_api_message(DEFAULT_API_BASE_URL, false, false).is_none());
        assert!(
            packaged_local_debug_api_message("https://api.example.com/api/v1", true, false)
                .is_none()
        );
    }
}
