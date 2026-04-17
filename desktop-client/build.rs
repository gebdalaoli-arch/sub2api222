fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("ProductName", "Sub2API Desktop Client");
        res.set(
            "FileDescription",
            "Bright Windows desktop client for Sub2API users",
        );
        res.set("CompanyName", "Sub2API");
        res.set("LegalCopyright", "Copyright (C) Sub2API");
        res.compile().expect("failed to compile windows resources");
    }
    slint_build::compile("ui/app-window.slint").expect("failed to compile slint ui");
}
