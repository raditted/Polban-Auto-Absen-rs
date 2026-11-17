//! Core library for Polban Auto Absen (v3.0).

pub mod config;
pub mod model;
pub mod parser;
pub mod state;
pub mod text;

pub use config::{Config, ConfigError};
pub use model::{AbsenFormData, Action, Color, LogLevel, Skip, UserInfo};
pub use state::{ClickedState, StateError};

pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
