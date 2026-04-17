pub const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:8080/api/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub api_base_url: String,
}

pub fn build_app_config(api_base_url: Option<&str>) -> AppConfig {
    AppConfig {
        api_base_url: api_base_url.unwrap_or(DEFAULT_API_BASE_URL).to_string(),
    }
}

pub fn app_config() -> AppConfig {
    build_app_config(option_env!("SUB2API_DESKTOP_API_BASE_URL"))
}

#[cfg(test)]
mod tests {
    use super::{build_app_config, DEFAULT_API_BASE_URL};

    #[test]
    fn app_config_uses_fixed_default_backend_when_build_value_missing() {
        let config = build_app_config(None);

        assert_eq!(config.api_base_url, DEFAULT_API_BASE_URL);
    }

    #[test]
    fn app_config_prefers_explicit_build_backend() {
        let config = build_app_config(Some("https://api.example.com/api/v1"));

        assert_eq!(config.api_base_url, "https://api.example.com/api/v1");
    }
}
