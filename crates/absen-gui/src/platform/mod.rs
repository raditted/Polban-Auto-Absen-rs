//! OS integration modules (system tray, desktop notifications, audio, launch at login).

pub mod autostart;
pub mod notify;
pub mod sound;
#[cfg(any(target_os = "macos", test))]
pub mod toast_macos;
pub mod toast_render;
#[cfg(target_os = "windows")]
pub mod toast_win32;
pub mod tray;
#[cfg(target_os = "windows")]
pub mod win32;

pub use autostart::AutostartManager;
pub use notify::Notifier;
pub use tray::{supports_hide_to_tray, TrayCommand, TrayManager, TrayWaker};
