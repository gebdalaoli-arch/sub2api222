#[cfg(test)]
mod tests {
    #[test]
    fn app_bootstrap_exposes_router_module() {
        let router_name = std::any::type_name::<crate::app::router::Route>();
        assert!(router_name.contains("Route"));
    }
}

pub mod app;
