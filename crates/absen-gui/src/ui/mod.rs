//! Desktop GUI view components (`Calm Academic Utility`).

pub mod activity;
pub mod header;
pub mod icons;
pub mod jadwal;
pub mod settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompactTab {
    #[default]
    Jadwal,
    Activity,
}
