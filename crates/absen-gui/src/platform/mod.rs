//! OS integration modules (system tray, desktop notifications, launch at login).

pub mod autostart;
pub mod notify;
pub mod tray;

pub use autostart::AutostartManager;
pub use notify::Notifier;
pub use tray::{TrayCommand, TrayManager};
