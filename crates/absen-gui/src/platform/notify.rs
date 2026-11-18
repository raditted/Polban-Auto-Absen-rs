//! Desktop notification wrapper powered by `notify-rust`.

/// Dispatches non-blocking desktop notifications when enabled in preferences.
#[derive(Debug, Clone)]
pub struct Notifier {
    enabled: bool,
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

    pub fn notify_attendance_success(&self, _course_name: &str) {}

    pub fn notify_auth_failed(&self, _detail: &str) {}

    pub fn notify_info(&self, _title: &str, _body: &str) {}
}
