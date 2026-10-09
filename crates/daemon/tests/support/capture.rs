//! Read the log a human would have read, per test, without losing lines.
//!
//! One subscriber serves the whole test binary, installed once and never
//! replaced; each line goes to the capture of the thread that emitted it.
//!
//! A subscriber per test (`tracing::subscriber::set_default`) looks
//! equivalent, but it loses events under load. Every scoped dispatcher created
//! or dropped by a parallel test makes tracing-core rebuild its global callsite
//! interest and max-level caches, and an event emitted mid-rebuild can be
//! skipped. Measured on 2026-10-09: 3 of 80 runs of `streaming` under load lost
//! one line, though every job was done exactly once.
//!
//! Lines are routed by thread, so a test sees what its own thread logged. A
//! default `#[tokio::test]` runs on one thread, spawned tasks included, which
//! is the same reach the per-test subscriber had.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, Once};

use tracing_subscriber::fmt::MakeWriter;

thread_local! {
    static CURRENT: RefCell<Option<Captured>> = const { RefCell::new(None) };
}

/// Everything logged at INFO and above on this thread while installed.
#[derive(Clone, Default)]
pub struct Captured(Arc<Mutex<Vec<u8>>>);

/// Stops capturing when dropped.
pub struct Installed;

impl Drop for Installed {
    fn drop(&mut self) {
        CURRENT.with(|current| current.borrow_mut().take());
    }
}

impl Captured {
    /// Capture this thread's log lines until the returned guard is dropped.
    pub fn install(&self) -> Installed {
        static GLOBAL: Once = Once::new();
        GLOBAL.call_once(|| {
            let subscriber = tracing_subscriber::fmt()
                .with_writer(ToCurrentThread)
                .with_max_level(tracing::Level::INFO)
                .with_ansi(false)
                .finish();
            tracing::subscriber::set_global_default(subscriber)
                .expect("no other global subscriber in a test binary");
        });
        CURRENT.with(|current| *current.borrow_mut() = Some(self.clone()));
        Installed
    }

    pub fn text(&self) -> String {
        String::from_utf8(self.0.lock().expect("the log is not poisoned").clone())
            .expect("log output is utf-8")
    }

    pub fn lines(&self) -> Vec<String> {
        self.text().lines().map(str::to_string).collect()
    }
}

/// The writer handed to the global subscriber: appends to whichever capture
/// the emitting thread installed, and discards the line if none.
struct ToCurrentThread;

impl std::io::Write for ToCurrentThread {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        CURRENT.with(|current| {
            if let Some(captured) = current.borrow().as_ref() {
                captured
                    .0
                    .lock()
                    .expect("the log is not poisoned")
                    .extend(buf);
            }
        });
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for ToCurrentThread {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        ToCurrentThread
    }
}
