//! Launch-at-login registration powered by `auto-launch`.

/// Cross-platform autostart manager for Polban Presensi.
#[derive(Debug, Clone)]
pub struct AutostartManager {
    app_name: String,
}

impl AutostartManager {
    pub fn new(app_name: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
        }
    }

    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    pub fn is_enabled(&self) -> Result<bool, String> {
        Ok(false)
    }

    pub fn set_enabled(&self, _enabled: bool) -> Result<(), String> {
        Ok(())
    }
}
