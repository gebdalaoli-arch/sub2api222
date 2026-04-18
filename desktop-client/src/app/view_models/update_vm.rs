#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDialogState {
    Hidden,
    Checking,
    AvailableOptional,
    AvailableRequired,
    Downloading,
    ReadyToInstall,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateViewModel {
    pub state: UpdateDialogState,
    pub title: String,
    pub current_version: String,
    pub latest_version: String,
    pub summary: String,
    pub force_update: bool,
    pub primary_text: String,
    pub secondary_text: Option<String>,
}

impl UpdateViewModel {
    pub fn available(
        current_version: String,
        latest_version: String,
        force_update: bool,
        title: String,
        summary: String,
    ) -> Self {
        Self {
            state: if force_update {
                UpdateDialogState::AvailableRequired
            } else {
                UpdateDialogState::AvailableOptional
            },
            title,
            current_version,
            latest_version,
            summary,
            force_update,
            primary_text: "立即更新".to_string(),
            secondary_text: if force_update {
                None
            } else {
                Some("稍后".to_string())
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{UpdateDialogState, UpdateViewModel};

    #[test]
    fn update_vm_hides_secondary_action_for_force_update() {
        let vm = UpdateViewModel::available(
            "0.1.0".to_string(),
            "0.2.0".to_string(),
            true,
            "发现新版本".to_string(),
            "当前版本已停止支持".to_string(),
        );

        assert_eq!(vm.state, UpdateDialogState::AvailableRequired);
        assert_eq!(vm.primary_text, "立即更新");
        assert_eq!(vm.secondary_text, None);
    }
}
