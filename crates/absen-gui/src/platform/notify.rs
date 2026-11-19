//! Desktop notification wrapper powered by `notify-rust`.

/// Dispatches non-blocking desktop notifications when enabled in preferences.
#[derive(Debug, Clone)]
pub struct Notifier {
    enabled: bool,
}

fn clean_text(input: &str) -> String {
    input
        .replace(" \u{2014} ", ": ")
        .replace('\u{2014}', ", ")
        .replace(concat!(" ", "-", "-", " "), ": ")
}

impl Notifier {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn notify_attendance_success(&self, course_name: &str) {
        let course = clean_text(course_name.trim());
        let body = if course.is_empty() {
            "Presensi kuliah berhasil dicatat.".to_string()
        } else {
            format!("Presensi berhasil dicatat untuk {course}.")
        };
        self.dispatch("Presensi Berhasil", &body);
    }

    pub fn notify_auth_failed(&self, detail: &str) {
        let cleaned = clean_text(detail.trim());
        let body = if cleaned.is_empty() {
            "Login portal akademik gagal. Periksa NIM dan kata sandi di Pengaturan.".to_string()
        } else {
            format!("Login portal akademik gagal: {cleaned}")
        };
        self.dispatch("Login Portal Gagal", &body);
    }

    pub fn notify_info(&self, title: &str, body: &str) {
        let title = clean_text(title);
        let body = clean_text(body);
        self.dispatch(&title, &body);
    }

    fn dispatch(&self, summary: &str, body: &str) {
        if !self.enabled {
            return;
        }
        let summary = summary.to_string();
        let body = body.to_string();
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
        notifier.notify_attendance_success("Struktur Data");
        notifier.notify_auth_failed("Kredensial ditolak");
        notifier.notify_info("Info", "Pemantauan dimulai");
        notifier.set_enabled(true);
        assert!(notifier.is_enabled());
    }
}
