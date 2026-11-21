//! Core library for Polban Auto Absen (v3.0).

pub mod client;
pub mod config;
pub mod engine;
pub mod event;
pub mod model;
pub mod parser;
pub mod state;
pub mod text;

pub use client::{AbsenClient, ClientError, DumpError, LoginOutcome, PostResponse};
pub use config::{Config, ConfigError};
pub use engine::{response_indicates_success, run, run_with_commands, EngineCommand, EngineError};
pub use event::{ChannelReporter, Event, Reporter, VecReporter};
pub use model::{
    generate_proof_id, sanitize_server_response, AbsenFormData, Action, AttendanceProof, Color,
    LogLevel, PortalPageSnapshot, ProofVerification, RowMeta, Skip, UserInfo,
};
pub use state::{
    proof_path_for, proof_screenshots_dir, prune_screenshot_files, ClickedState, ProofStore,
    StateError, MAX_PROOF_DAYS, MAX_PROOF_RECORDS,
};

pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
