//! Desktop notification dispatcher with custom Windows/macOS corner toasts and embedded audio.

use std::fmt;
use std::sync::{Arc, Mutex};

use super::toast_render::ToastKind;
use crate::theme::Palette;

/// Shared in-memory notification log used by deterministic tests.
pub type NotificationLog = Arc<Mutex<Vec<(String, String)>>>;

/// Optional callback invoked when a user clicks a custom desktop toast to reveal the main window.
pub type ToastWakeCallback = Arc<dyn Fn() + Send + Sync>;

/// Dispatches non-blocking desktop notifications and optional chime audio when enabled.
#[derive(Clone)]
pub struct Notifier {
    enabled: bool,
    sound_enabled: bool,
    sound_volume: u8,
    palette: Palette,
    wake_hwnd: isize,
    wake_callback: Option<ToastWakeCallback>,
    recorder: Option<NotificationLog>,
}

impl fmt::Debug for Notifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Notifier")
            .field("enabled", &self.enabled)
            .field("sound_enabled", &self.sound_enabled)
            .field("sound_volume", &self.sound_volume)
            .field("palette", &self.palette)
            .field("wake_hwnd", &self.wake_hwnd)
            .field("has_wake_callback", &self.wake_callback.is_some())
            .field("is_recorded", &self.recorder.is_some())
            .finish()
    }
}

fn clean_text(input: &str) -> String {
    input
        .replace(" \u{2014} ", ": ")
        .replace('\u{2014}', ", ")
        .replace(concat!(" ", "-", "-", " "), ": ")
}

impl Notifier {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            sound_enabled: true,
            sound_volume: 100,
            palette: Palette::DARK,
            wake_hwnd: 0,
            wake_callback: None,
            recorder: None,
        }
    }

    /// Create an in-memory recording notifier for deterministic unit/integration tests.
    pub fn new_recorded(enabled: bool) -> (Self, NotificationLog) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let notifier = Self {
            enabled,
            sound_enabled: true,
            sound_volume: 100,
            palette: Palette::DARK,
            wake_hwnd: 0,
            wake_callback: None,
            recorder: Some(Arc::clone(&log)),
        };
        (notifier, log)
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_sound_enabled(&mut self, enabled: bool) {
        self.sound_enabled = enabled;
    }

    pub fn is_sound_enabled(&self) -> bool {
        self.sound_enabled
    }

    pub fn set_sound_volume(&mut self, volume_pct: u8) {
        self.sound_volume = volume_pct.min(100);
    }

    pub fn sound_volume(&self) -> u8 {
        self.sound_volume
    }

    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
    }

    pub fn set_wake_hwnd(&mut self, hwnd: isize) {
        self.wake_hwnd = hwnd;
    }

    pub fn set_wake_callback(&mut self, cb: ToastWakeCallback) {
        self.wake_callback = Some(cb);
    }

    pub fn wake_callback(&self) -> Option<ToastWakeCallback> {
        self.wake_callback.clone()
    }

    pub fn is_recorded(&self) -> bool {
        self.recorder.is_some()
    }

    pub fn notify_minimized_to_tray(&self) {
        self.notify_info("Berjalan di Latar Belakang", "Klik ikon di system tray.");
    }

    pub fn notify_attendance_success(&self, course_name: &str) {
        let course = clean_text(course_name.trim());
        let body = if course.is_empty() {
            "Kehadiran tercatat.".to_string()
        } else {
            course
        };
        self.dispatch(ToastKind::Success, "Presensi Berhasil!", &body);
    }

    pub fn notify_auth_failed(&self, detail: &str) {
        let cleaned = clean_text(detail.trim());
        let body = if cleaned.is_empty() || cleaned.chars().count() > 38 {
            "Cek NIM & sandi di Pengaturan.".to_string()
        } else {
            cleaned
        };
        self.dispatch(ToastKind::Error, "Login Portal Gagal!", &body);
    }

    pub fn notify_info(&self, title: &str, body: &str) {
        let title = clean_text(title);
        let body = clean_text(body);
        self.dispatch(ToastKind::Info, &title, &body);
    }

    pub fn notify_test(&self) {
        self.dispatch(
            ToastKind::Success,
            "Notifikasi Aktif!",
            "Toast & suara siap digunakan.",
        );
    }

    fn dispatch(&self, kind: ToastKind, summary: &str, body: &str) {
        if !self.enabled {
            return;
        }
        let summary = summary.to_string();
        let body = body.to_string();
        if let Some(recorder) = &self.recorder {
            if let Ok(mut guard) = recorder.lock() {
                guard.push((summary, body));
            }
            return;
        }

        if self.sound_enabled && self.sound_volume > 0 {
            super::sound::play_notification_sound_with_volume(self.sound_volume);
        }

        #[cfg(target_os = "windows")]
        {
            super::toast_win32::show_toast(super::toast_win32::ToastRequest {
                kind,
                title: summary,
                body,
                palette: self.palette,
                wake_hwnd: self.wake_hwnd,
            });
        }

        #[cfg(target_os = "macos")]
        {
            let _ = self.wake_hwnd;
            super::toast_macos::show_toast(super::toast_macos::ToastRequest {
                kind,
                title: summary,
                body,
                palette: self.palette,
                on_open: self.wake_callback.clone(),
            });
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            let _ = (kind, self.palette, self.wake_hwnd, &self.wake_callback);
            let _ = std::thread::Builder::new()
                .name("polban-notify".to_string())
                .spawn(move || {
                    let mut note = notify_rust::Notification::new();
                    note.appname("Polban Presensi")
                        .summary(&summary)
                        .body(&body);
                    let _ = note.show();
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_removes_em_dash_and_double_dash() {
        assert_eq!(clean_text(concat!("A", " ", "-", "-", " ", "B")), "A: B");
        assert_eq!(clean_text("A \u{2014} B"), "A: B");
        assert_eq!(clean_text("A\u{2014}B"), "A, B");
    }

    #[test]
    fn disabled_notifier_does_not_panic_or_dispatch() {
        let mut notifier = Notifier::new(false);
        assert!(!notifier.is_enabled());
        assert!(notifier.is_sound_enabled());
        notifier.set_sound_enabled(false);
        assert!(!notifier.is_sound_enabled());
        notifier.set_palette(Palette::LIGHT);
        notifier.set_wake_hwnd(42);
        notifier.notify_attendance_success("Struktur Data");
        notifier.notify_auth_failed("Kredensial ditolak");
        notifier.notify_info("Info", "Pemantauan dimulai");
        notifier.notify_test();
        notifier.set_enabled(true);
        assert!(notifier.is_enabled());
    }

    #[test]
    fn recorded_notifier_captures_test_and_semantic_notifications() {
        let (mut notifier, log) = Notifier::new_recorded(true);
        assert!(notifier.is_recorded());
        assert!(notifier.is_sound_enabled());
        notifier.set_sound_enabled(false);
        notifier.set_palette(Palette::LIGHT);
        notifier.notify_test();
        notifier.notify_attendance_success("Basis Data");
        notifier.notify_auth_failed("Password salah");
        notifier.notify_minimized_to_tray();
        notifier.notify_attendance_success("");
        notifier
            .notify_auth_failed("Pesan galat sangat panjang melebihi tiga puluh delapan karakter");

        let entries = log.lock().unwrap().clone();
        assert_eq!(entries.len(), 6);
        assert_eq!(
            entries[0],
            (
                "Notifikasi Aktif!".to_string(),
                "Toast & suara siap digunakan.".to_string()
            )
        );
        assert_eq!(
            entries[1],
            ("Presensi Berhasil!".to_string(), "Basis Data".to_string())
        );
        assert_eq!(
            entries[2],
            (
                "Login Portal Gagal!".to_string(),
                "Password salah".to_string()
            )
        );
        assert_eq!(entries[3].0, "Berjalan di Latar Belakang");
        assert!(entries[3].1.contains("system tray"));
        assert_eq!(
            entries[4],
            (
                "Presensi Berhasil!".to_string(),
                "Kehadiran tercatat.".to_string()
            )
        );
        assert_eq!(
            entries[5],
            (
                "Login Portal Gagal!".to_string(),
                "Cek NIM & sandi di Pengaturan.".to_string()
            )
        );
    }

    #[test]
    fn sound_volume_defaults_and_clamps_to_100() {
        let mut notifier = Notifier::new(true);
        assert_eq!(notifier.sound_volume(), 100);

        let (recorded, _log) = Notifier::new_recorded(true);
        assert_eq!(recorded.sound_volume(), 100);

        notifier.set_sound_volume(150);
        assert_eq!(notifier.sound_volume(), 100);

        notifier.set_sound_volume(0);
        assert_eq!(notifier.sound_volume(), 0);

        notifier.set_sound_volume(65);
        assert_eq!(notifier.sound_volume(), 65);
        assert!(format!("{notifier:?}").contains("sound_volume: 65"));
    }
}
