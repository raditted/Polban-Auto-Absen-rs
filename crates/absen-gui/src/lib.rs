//! Native desktop GUI library for Polban Presensi (`polban-absen-gui`).

pub mod app;
pub mod bridge;
pub mod credentials;
pub mod icon;
pub mod platform;
pub mod prefs;
pub mod theme;
pub mod ui;
pub mod updater;

pub use app::DesktopApp;
