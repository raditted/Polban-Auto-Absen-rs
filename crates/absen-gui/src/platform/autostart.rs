//! Launch-at-login registration powered by `auto-launch`.

use auto_launch::{AutoLaunch, AutoLaunchBuilder};

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

    fn build_launcher(&self) -> Result<AutoLaunch, String> {
        let exe_path = std::env::current_exe()
            .map_err(|e| format!("Gagal menemukan path executable aplikasi: {e}"))?;
        let exe_str = exe_path
            .to_str()
            .ok_or_else(|| "Path executable tidak valid sebagai UTF-8.".to_string())?;

        AutoLaunchBuilder::new()
            .set_app_name(&self.app_name)
            .set_app_path(exe_str)
            .set_use_launch_agent(true)
            .set_args(&["--minimized"])
            .build()
            .map_err(|e| format!("Gagal menyiapkan konfigurasi autostart: {e}"))
    }

    pub fn is_enabled(&self) -> Result<bool, String> {
        let launcher = self.build_launcher()?;
        launcher
            .is_enabled()
            .map_err(|e| format!("Gagal memeriksa status autostart: {e}"))
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let launcher = self.build_launcher()?;
        if enabled {
            launcher
                .enable()
                .map_err(|e| format!("Gagal mengaktifkan autostart: {e}"))
        } else {
            let currently_enabled = launcher.is_enabled().unwrap_or(false);
            if currently_enabled {
                launcher
                    .disable()
                    .map_err(|e| format!("Gagal menonaktifkan autostart: {e}"))
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_manager_builds_launcher_with_minimized_arg() {
        let mgr = AutostartManager::new("Polban Presensi Test");
        assert_eq!(mgr.app_name(), "Polban Presensi Test");
        let launcher = mgr.build_launcher().unwrap();
        assert_eq!(launcher.get_app_name(), "Polban Presensi Test");
        assert_eq!(launcher.get_args(), &["--minimized".to_string()]);
    }
}
