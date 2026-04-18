use directories::ProjectDirs;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemFingerprint {
    pub manufacturer: String,
    pub product_name: String,
    pub baseboard_manufacturer: String,
    pub baseboard_product: String,
}

impl SystemFingerprint {
    fn summary(&self) -> String {
        format!(
            "manufacturer='{}', product='{}', baseboard_manufacturer='{}', baseboard_product='{}'",
            self.manufacturer,
            self.product_name,
            self.baseboard_manufacturer,
            self.baseboard_product
        )
    }

    fn normalized_blob(&self) -> String {
        [
            self.manufacturer.as_str(),
            self.product_name.as_str(),
            self.baseboard_manufacturer.as_str(),
            self.baseboard_product.as_str(),
        ]
        .join(" ")
        .to_ascii_lowercase()
    }
}

#[derive(Debug, Clone)]
pub struct StartupDiagnostics {
    log_path: Arc<PathBuf>,
}

impl StartupDiagnostics {
    pub fn initialize() -> Self {
        let diagnostics = Self {
            log_path: Arc::new(default_startup_log_path()),
        };
        diagnostics.install_panic_hook();
        diagnostics.log("startup bootstrap initialized");

        let current_backend = current_slint_backend();
        match detect_system_fingerprint() {
            Some(fingerprint) => {
                diagnostics.log(format!(
                    "system fingerprint: {}",
                    fingerprint.summary()
                ));
                if should_force_software_renderer(current_backend.as_deref(), &fingerprint) {
                    // This runs before the UI/event loop and worker threads exist, so mutating
                    // the process environment here is still within the documented safety boundary.
                    unsafe {
                        std::env::set_var("SLINT_BACKEND", "winit-software");
                    }
                    diagnostics.log(
                        "detected a virtual machine graphics profile; forcing SLINT_BACKEND=winit-software",
                    );
                } else if let Some(backend) = current_backend {
                    diagnostics.log(format!(
                        "keeping user supplied SLINT_BACKEND={backend}"
                    ));
                } else {
                    diagnostics.log("keeping default Slint renderer selection");
                }
            }
            None => diagnostics.log("system fingerprint unavailable; keeping default Slint renderer selection"),
        }

        diagnostics.log(format!(
            "effective SLINT_BACKEND={}",
            current_slint_backend().unwrap_or_else(|| "<default>".to_string())
        ));
        diagnostics
    }

    pub fn log(&self, message: impl AsRef<str>) {
        let _ = append_log(&self.log_path, message.as_ref());
    }

    pub fn log_path(&self) -> &std::path::Path {
        &self.log_path
    }

    fn install_panic_hook(&self) {
        let log_path = Arc::clone(&self.log_path);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let location = panic_info
                .location()
                .map(|location| format!("{}:{}:{}", location.file(), location.line(), location.column()))
                .unwrap_or_else(|| "<unknown>".to_string());
            let payload = panic_info
                .payload()
                .downcast_ref::<&str>()
                .map(|message| (*message).to_string())
                .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            let _ = append_log(
                &log_path,
                &format!("panic at {location}: {payload}"),
            );
            previous(panic_info);
        }));
    }
}

pub fn should_force_software_renderer(
    explicit_backend: Option<&str>,
    fingerprint: &SystemFingerprint,
) -> bool {
    if explicit_backend
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return false;
    }

    let normalized = fingerprint.normalized_blob();
    const VM_MARKERS: [&str; 8] = [
        "vmware",
        "virtual machine",
        "virtualbox",
        "virtual platform",
        "hyper-v",
        "hyperv",
        "kvm",
        "qemu",
    ];

    VM_MARKERS
        .iter()
        .any(|marker| normalized.contains(marker))
}

fn current_slint_backend() -> Option<String> {
    std::env::var("SLINT_BACKEND")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_startup_log_path() -> PathBuf {
    ProjectDirs::from("com", "sub2api", "TokenClient")
        .map(|dirs| dirs.data_local_dir().join("logs").join("startup.log"))
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .join("sub2api-desktop-client")
                .join("logs")
                .join("startup.log")
        })
}

fn append_log(path: &PathBuf, message: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    writeln!(file, "[{timestamp}] {message}")?;
    Ok(())
}

#[cfg(windows)]
fn detect_system_fingerprint() -> Option<SystemFingerprint> {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let bios = hklm
        .open_subkey("HARDWARE\\DESCRIPTION\\System\\BIOS")
        .ok()?;

    Some(SystemFingerprint {
        manufacturer: bios.get_value("SystemManufacturer").unwrap_or_default(),
        product_name: bios.get_value("SystemProductName").unwrap_or_default(),
        baseboard_manufacturer: bios
            .get_value("BaseBoardManufacturer")
            .unwrap_or_default(),
        baseboard_product: bios.get_value("BaseBoardProduct").unwrap_or_default(),
    })
}

#[cfg(not(windows))]
fn detect_system_fingerprint() -> Option<SystemFingerprint> {
    None
}

#[cfg(test)]
mod tests {
    use super::{should_force_software_renderer, SystemFingerprint};

    fn fingerprint(
        manufacturer: &str,
        product_name: &str,
        baseboard_manufacturer: &str,
        baseboard_product: &str,
    ) -> SystemFingerprint {
        SystemFingerprint {
            manufacturer: manufacturer.to_string(),
            product_name: product_name.to_string(),
            baseboard_manufacturer: baseboard_manufacturer.to_string(),
            baseboard_product: baseboard_product.to_string(),
        }
    }

    #[test]
    fn vmware_fingerprint_prefers_the_software_renderer() {
        let force_software = should_force_software_renderer(
            None,
            &fingerprint("VMware, Inc.", "VMware Virtual Platform", "", ""),
        );

        assert!(force_software);
    }

    #[test]
    fn microsoft_virtual_machine_fingerprint_prefers_the_software_renderer() {
        let force_software = should_force_software_renderer(
            None,
            &fingerprint("Microsoft Corporation", "Virtual Machine", "", ""),
        );

        assert!(force_software);
    }

    #[test]
    fn physical_machine_fingerprint_keeps_the_default_renderer() {
        let force_software = should_force_software_renderer(
            None,
            &fingerprint(
                "ASUSTeK COMPUTER INC.",
                "ROG STRIX Z790-A GAMING WIFI",
                "ASUSTeK COMPUTER INC.",
                "ROG STRIX Z790-A GAMING WIFI",
            ),
        );

        assert!(!force_software);
    }

    #[test]
    fn explicit_slint_backend_override_is_respected() {
        let force_software = should_force_software_renderer(
            Some("winit-femtovg"),
            &fingerprint("VMware, Inc.", "VMware Virtual Platform", "", ""),
        );

        assert!(!force_software);
    }
}
