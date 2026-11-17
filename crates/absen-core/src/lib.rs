//! Core library for Polban Auto Absen (v3.0).

pub mod client;
pub mod config;
pub mod engine;
pub mod event;
pub mod model;
pub mod parser;
pub mod state;
pub mod text;

pub use client::{AbsenClient, ClientError, DumpError, LoginOutcome};
pub use config::{Config, ConfigError};
pub use engine::{response_indicates_success, run, EngineError};
pub use event::{ChannelReporter, Event, Reporter, VecReporter};
pub use model::{AbsenFormData, Action, Color, LogLevel, Skip, UserInfo};
pub use state::{ClickedState, StateError};

pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
