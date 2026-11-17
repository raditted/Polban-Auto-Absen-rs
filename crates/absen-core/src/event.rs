//! Engine events and thread-safe reporter abstraction.

use std::sync::{mpsc, Arc, Mutex};

use crate::model::{Action, LogLevel, Skip, UserInfo};

/// Events emitted by the attendance engine to any UI/plain/test reporter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Log {
        code: String,
        message: String,
        level: LogLevel,
    },
    AuthStatus(String),
    UserInfo(UserInfo),
    Engine {
        name: String,
        interval: u64,
    },
    CycleIncrement,
    Jadwal {
        actions: Vec<Action>,
        skips: Vec<Skip>,
    },
    LastScan(String),
    MarkResult {
        sig: String,
        success: bool,
        nama: String,
    },
    Countdown(u32),
}

/// Sink for engine events. Must be thread-safe so the engine can run on a worker thread.
pub trait Reporter: Send + Sync {
    fn emit(&self, event: Event);
}

impl<R: Reporter + ?Sized> Reporter for Arc<R> {
    fn emit(&self, event: Event) {
        (**self).emit(event);
    }
}

impl<F> Reporter for F
where
    F: Fn(Event) + Send + Sync,
{
    fn emit(&self, event: Event) {
        self(event);
    }
}

/// `Reporter` adapter backed by `std::sync::mpsc::Sender<Event>`.
pub struct ChannelReporter {
    tx: Mutex<mpsc::Sender<Event>>,
}

impl ChannelReporter {
    pub fn new(tx: mpsc::Sender<Event>) -> Self {
        Self { tx: Mutex::new(tx) }
    }
}

impl Reporter for ChannelReporter {
    fn emit(&self, event: Event) {
        if let Ok(guard) = self.tx.lock() {
            let _ = guard.send(event);
        }
    }
}

/// In-memory `Reporter` useful in unit and integration tests.
#[derive(Default)]
pub struct VecReporter {
    events: Mutex<Vec<Event>>,
}

impl VecReporter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<Event> {
        self.events.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl Reporter for VecReporter {
    fn emit(&self, event: Event) {
        if let Ok(mut guard) = self.events.lock() {
            guard.push(event);
        }
    }
}
