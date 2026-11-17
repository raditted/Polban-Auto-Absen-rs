//! Core library for Polban Auto Absen (v3.0).

/// Placeholder version helper for F0 workspace scaffold verification.
pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_3_0_0() {
        assert_eq!(core_version(), "3.0.0");
    }
}
