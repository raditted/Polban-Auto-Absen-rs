//! System tray icon and menu abstraction powered by `tray-icon`.

use crate::icon::IconStatus;

/// Commands triggered from the OS system tray menu or double-click action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    ShowWindow,
    ToggleMonitoring,
    ScanNow,
    Quit,
}

/// Manages the native OS tray icon and menu items.
///
/// Stage 1 provides the compiled interface stub for Stage 2 (`gui-services`).
pub struct TrayManager {
    last_status: Option<IconStatus>,
    monitoring: bool,
}

impl TrayManager {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            last_status: None,
            monitoring: false,
        })
    }

    pub fn update_status(&mut self, status: IconStatus, monitoring: bool) {
        self.last_status = Some(status);
        self.monitoring = monitoring;
    }

    pub fn poll_commands(&self) -> Vec<TrayCommand> {
        Vec::new()
    }

    pub fn last_status(&self) -> Option<IconStatus> {
        self.last_status
    }

    pub fn is_monitoring(&self) -> bool {
        self.monitoring
    }
}
