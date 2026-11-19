//! OS integration modules (system tray, desktop notifications, launch at login).

pub mod autostart;
pub mod notify;
pub mod tray;
#[cfg(target_os = "windows")]
pub mod win32;

pub use autostart::AutostartManager;
pub use notify::Notifier;
pub use tray::{TrayCommand, TrayManager, TrayWaker};
