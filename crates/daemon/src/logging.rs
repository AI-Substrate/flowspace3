//! The daemon's log destinations: a rolling FILE plus stdout.
//!
//! # Why this exists
//!
//! On 2026-08-27 the summarize lane panicked and died with 9,411 jobs pending,
//! and the only copy of the evidence was the scrollback of the terminal that
//! happened to be running the daemon. The daemon logged to stdout and nowhere
//! else, so there was nothing to read afterwards and nothing to attach to a
//! report. Phase-2 self-restart makes that worse rather than better: a daemon
//! that re-execs itself has no terminal at all.
//!
//! So: two layers, one subscriber.
//!
//! * **The file layer** is always ANSI-free, because escape sequences in a file
//!   are noise a reader has to strip before they can even grep it.
//! * **The stdout layer** keeps colour only when stdout is a TTY. Redirecting
//!   the daemon's stdout used to produce a file full of escape codes — the
//!   standing Linux tester hit exactly that.
//!
//! # The disk invariant
//!
//! `log_max_bytes * log_max_files` is a hard ceiling. It is enforced HERE, by
//! [`Roller`], and not by `tracing-appender`: that crate's rolling writer rolls
//! by CALENDAR only — hourly, daily, never — with no size cap and no retention
//! sweep, so a daemon that logs a gigabyte in an afternoon fills the disk with
//! its blessing. Rolling on size and deleting the oldest generation is a
//! hundred lines; taking a dependency that cannot state the invariant is not
//! cheaper, it is just quieter about failing.
//!
//! # Degrading, never crashing
//!
//! Nothing here returns `Result` to the caller. A log directory that cannot be
//! created or written is a reason to run on stdout alone and TELL somebody
//! (the user-messages queue, plus the `logs` row in `flowspace3 doctor`) — it
//! is never a reason to refuse to serve. The daemon's job is indexing, not
//! logging.

use std::fs::{self, File, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fs3_core::{DaemonConfig, LOG_FILE_NAME, UserMessage, resolve_log_dir, rolled_name};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// What [`init`] achieved, in terms the rest of the daemon can report.
///
/// Deliberately not an error type: every field is something to SAY, and the
/// two sayers are the boot line (once, at INFO) and the user-messages queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Logging {
    /// The active log file, when one could be opened.
    pub file: Option<PathBuf>,
    /// The directory logs were meant to go in, as it should be shown to a
    /// user: resolved when resolution worked, the configured string when it
    /// did not.
    pub directory: String,
    /// Why there is no file, when there is none. Already human-readable.
    pub problem: Option<String>,
}

impl Logging {
    /// What the queue should be saying about logging right now (req-0059).
    ///
    /// Level-triggered like every other producer, and declared exactly once
    /// per process because the condition is fixed for a process's lifetime:
    /// the log file is opened at startup and never reopened, so a daemon that
    /// starts successfully retracts the previous run's complaint by declaring
    /// an empty set.
    #[must_use]
    pub fn desired_messages(&self) -> Vec<UserMessage> {
        match &self.problem {
            None => Vec::new(),
            Some(reason) => vec![fs3_core::unwritable_message(&self.directory, reason)],
        }
    }
}

/// Install the daemon's subscriber and panic hook, and say what happened.
///
/// Call once, from the daemon's boot path, AFTER configuration is loaded —
/// which is the whole reason it lives here rather than in `main`: the file's
/// path, the retention caps and the filter are all configuration, and a
/// subscriber installed before the config is read can honour none of them.
///
/// Safe to call when a subscriber already exists (a test binary, say): the
/// install is attempted, and a refusal becomes the reported problem rather
/// than a panic.
pub fn init(config: &DaemonConfig) -> Logging {
    // `RUST_LOG` still wins. An operator debugging one run should not have to
    // edit a config file, and this is the behaviour the daemon already had.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.log_level.clone()));

    let home = std::env::var_os("HOME").map(PathBuf::from);
    let (directory, opened) = match resolve_log_dir(&config.log_dir, home.as_deref()) {
        Ok(directory) => {
            let opened = Roller::open(&directory, config.log_max_bytes, config.log_max_files)
                .map_err(|error| format!("{error}"));
            (directory.display().to_string(), opened)
        }
        Err(reason) => (config.log_dir.clone(), Err(reason)),
    };

    // Colour on a terminal, never in a pipe: this is cheetah's finding 5, and
    // it applies to the redirected case as much as to the file.
    let stdout_layer = tracing_subscriber::fmt::layer().with_ansi(io::stdout().is_terminal());

    let (file, mut problem) = match opened {
        Ok(roller) => {
            let path = roller.active_path();
            let file_layer = tracing_subscriber::fmt::layer()
                // Never coloured. A log file is read by `grep`, by an agent,
                // and by whoever is pasting it into an incident report.
                .with_ansi(false)
                .with_writer(RollingWriter::new(roller));
            let installed = tracing_subscriber::registry()
                .with(filter)
                .with(stdout_layer)
                .with(file_layer)
                .try_init();
            match installed {
                Ok(()) => (Some(path), None),
                Err(error) => (None, Some(format!("{error}"))),
            }
        }
        Err(reason) => {
            // Stdout alone still beats silence, so the subscriber is installed
            // either way.
            let installed = tracing_subscriber::registry()
                .with(filter)
                .with(stdout_layer)
                .try_init();
            let reason = match installed {
                Ok(()) => reason,
                Err(error) => format!("{reason}; and the log subscriber was refused: {error}"),
            };
            (None, Some(reason))
        }
    };

    if let Err(error) = install_panic_hook() {
        // Worth saying, not worth failing over: without the hook a panic still
        // reaches stderr, it just does not reach the file.
        problem = Some(match problem {
            Some(existing) => format!("{existing}; {error}"),
            None => error,
        });
    }

    Logging {
        file,
        directory,
        problem,
    }
}

/// Send panics through `tracing`, so they land in the log file.
///
/// This is deliverable 5 of the packet and the direct fix for the motivating
/// incident: a panic in a spawned task unwinds inside that task, and the only
/// thing that observes it is the panic hook. Without this, `tokio::spawn`'s
/// default handling puts the message on stderr — outside the subscriber, and
/// therefore outside the file.
///
/// The previous hook is CALLED, not replaced: stderr keeps behaving the way a
/// Rust program is expected to, and the file gains a copy.
///
/// # Errors
/// Never, today — the signature exists so a future hook that can genuinely
/// fail does not have to change every caller.
fn install_panic_hook() -> Result<(), String> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = panic_message(info.payload());
        let location = info
            .location()
            .map_or_else(|| "unknown".to_string(), ToString::to_string);
        // Honours `RUST_BACKTRACE`; `Disabled`/`Unsupported` render as a short
        // note rather than a wall of nothing.
        let backtrace = std::backtrace::Backtrace::capture();

        tracing::error!(
            panic = %payload,
            location = %location,
            backtrace = %backtrace,
            "a thread panicked — this is the evidence the log file exists for"
        );

        previous(info);
    }));
    Ok(())
}

/// The human-readable half of a panic payload.
///
/// `panic!("literal")` yields a `&str` and `panic!("{x}")` yields a `String`;
/// anything else is a `panic_any` nobody in this codebase does.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload.downcast_ref::<&str>().map_or_else(
        || {
            payload
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_else(|| "a panic payload of an unprintable type".to_string())
        },
        |text| (*text).to_string(),
    )
}

/// A `MakeWriter` over one shared [`Roller`].
///
/// One lock, taken per event. `fmt` formats into a thread-local buffer and
/// hands the writer one `write_all`, so the lock is held for a single `write`
/// syscall and events never interleave halfway through a line.
#[derive(Clone, Debug)]
pub struct RollingWriter(Arc<Mutex<Roller>>);

impl RollingWriter {
    /// Share one roller across every thread that logs.
    #[must_use]
    pub fn new(roller: Roller) -> Self {
        Self(Arc::new(Mutex::new(roller)))
    }
}

impl Write for &RollingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // A poisoned lock is recovered rather than propagated: the writer is
        // poisoned precisely when a thread panicked while logging, and that is
        // the moment the log matters most. Refusing to write then would lose
        // the panic that the hook above is trying to record.
        let mut roller = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        roller.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut roller = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        roller.flush()
    }
}

impl<'a> MakeWriter<'a> for RollingWriter {
    type Writer = &'a RollingWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self
    }
}

/// A log file that rolls on size and keeps a bounded number of generations.
///
/// `flowspace3.log` is the active file; `flowspace3.log.1` is the most recently
/// rolled, up to `flowspace3.log.{max_files - 1}`. Rolling renames each
/// generation one further back and deletes the one that falls off the end, so
/// the directory holds `max_files` files forever — no unbounded growth, which
/// is the invariant this whole module exists to keep.
#[derive(Debug)]
pub struct Roller {
    directory: PathBuf,
    max_bytes: u64,
    max_files: u32,
    file: File,
    written: u64,
}

impl Roller {
    /// Open (or create) the active log file in `directory`.
    ///
    /// Appends: a restart continues the same file rather than destroying the
    /// evidence of why the previous process stopped.
    ///
    /// # Errors
    /// Anything that stops the directory being created or the file being
    /// opened for append — a read-only filesystem, a permission denial, or a
    /// path component that is not a directory.
    pub fn open(directory: &Path, max_bytes: u64, max_files: u32) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        let path = directory.join(LOG_FILE_NAME);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata()?.len();

        Ok(Self {
            directory: directory.to_path_buf(),
            // Config validation refuses zero for both, but this type is public
            // and a zero here would mean rolling on every byte forever.
            max_bytes: max_bytes.max(1),
            max_files: max_files.max(1),
            file,
            written,
        })
    }

    /// Where the active file is.
    #[must_use]
    pub fn active_path(&self) -> PathBuf {
        self.directory.join(LOG_FILE_NAME)
    }

    /// Retire the active file and start a new one.
    fn roll(&mut self) -> io::Result<()> {
        self.file.flush()?;

        // How many ROLLED generations may exist: the cap counts the active
        // file, so a cap of 1 keeps no history at all.
        let generations = self.max_files - 1;
        if generations == 0 {
            self.file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(self.active_path())?;
            self.written = 0;
            return Ok(());
        }

        // Oldest first: delete the generation about to fall off the end, then
        // walk backwards so nothing is ever renamed onto a file that is still
        // wanted.
        remove_if_present(&self.directory.join(rolled_name(generations)))?;
        for generation in (1..generations).rev() {
            rename_if_present(
                &self.directory.join(rolled_name(generation)),
                &self.directory.join(rolled_name(generation + 1)),
            )?;
        }
        rename_if_present(&self.active_path(), &self.directory.join(rolled_name(1)))?;

        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.active_path())?;
        self.written = 0;
        Ok(())
    }
}

impl Write for Roller {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Roll BEFORE writing, and never on an empty file: an event larger
        // than the whole cap has to land somewhere, and rolling first would
        // spin producing empty files. The ceiling is therefore
        // `max_bytes + one event` per file, which is the honest statement of
        // it — and one an 8 MB default makes academic.
        if self.written > 0 && self.written.saturating_add(buf.len() as u64) > self.max_bytes {
            self.roll()?;
        }

        let written = self.file.write(buf)?;
        self.written = self.written.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Delete a path, treating "it was not there" as success.
fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Rename a path, treating "it was not there" as success.
///
/// A fresh log directory has no generations yet, and a daemon that rolled
/// twice has fewer than the cap allows. Both are ordinary.
fn rename_if_present(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_default()
    }

    fn files(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .expect("the log directory")
            .map(|entry| {
                entry
                    .expect("a dir entry")
                    .file_name()
                    .display()
                    .to_string()
            })
            .collect();
        names.sort();
        names
    }

    /// The invariant, stated as a test: write far past the cap and the
    /// directory still holds exactly `max_files` files.
    #[test]
    fn writing_past_the_cap_rolls_and_never_exceeds_the_retention() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let mut roller = Roller::open(directory.path(), 100, 3).expect("opening the log");

        // 40 x 20 bytes = 800 bytes through a 100-byte file: eight rolls.
        for index in 0..40 {
            writeln!(roller, "event {index:013}").expect("writing an event");
        }
        roller.flush().expect("flushing");

        assert_eq!(
            files(directory.path()),
            vec![
                "flowspace3.log".to_string(),
                "flowspace3.log.1".to_string(),
                "flowspace3.log.2".to_string(),
            ],
            "three files kept, and no more, however much is written"
        );

        for name in files(directory.path()) {
            let size = fs::metadata(directory.path().join(&name))
                .expect("stat")
                .len();
            assert!(size <= 100 + 20, "{name} is {size} bytes, past the cap");
        }
    }

    /// Rolling must not lose the newest events, and `.1` must be the file that
    /// was active a moment ago — the generations run oldest-highest.
    #[test]
    fn the_newest_events_are_in_the_active_file_and_older_ones_behind_it() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let mut roller = Roller::open(directory.path(), 40, 3).expect("opening the log");

        writeln!(roller, "oldest").expect("write");
        writeln!(roller, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").expect("write");
        writeln!(roller, "newest").expect("write");
        roller.flush().expect("flush");

        assert!(read(&directory.path().join("flowspace3.log")).contains("newest"));
        assert!(read(&directory.path().join("flowspace3.log.1")).contains("AAAA"));
        assert!(read(&directory.path().join("flowspace3.log.2")).contains("oldest"));
    }

    /// A cap of one means "the active file only": rolling truncates rather
    /// than keeping a generation nobody asked for.
    #[test]
    fn a_cap_of_one_file_keeps_only_the_active_one() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let mut roller = Roller::open(directory.path(), 20, 1).expect("opening the log");

        writeln!(roller, "first entry, long enough to fill it").expect("write");
        writeln!(roller, "second").expect("write");
        roller.flush().expect("flush");

        assert_eq!(files(directory.path()), vec!["flowspace3.log".to_string()]);
        let live = read(&directory.path().join("flowspace3.log"));
        assert!(live.contains("second"), "{live:?}");
        assert!(!live.contains("first entry"), "{live:?}");
    }

    /// A restart must not destroy the evidence of why the last process died.
    #[test]
    fn reopening_appends_rather_than_truncating() {
        let directory = tempfile::tempdir().expect("a temp dir");

        let mut first = Roller::open(directory.path(), 10_000, 3).expect("opening the log");
        writeln!(first, "from the first process").expect("write");
        first.flush().expect("flush");
        drop(first);

        let mut second = Roller::open(directory.path(), 10_000, 3).expect("reopening the log");
        writeln!(second, "from the second process").expect("write");
        second.flush().expect("flush");

        let live = read(&directory.path().join("flowspace3.log"));
        assert!(live.contains("from the first process"), "{live:?}");
        assert!(live.contains("from the second process"), "{live:?}");
    }

    #[test]
    fn a_panic_payload_is_rendered_from_either_shape() {
        let literal: Box<dyn std::any::Any + Send> = Box::new("a literal panic");
        let formatted: Box<dyn std::any::Any + Send> = Box::new("a formatted panic".to_string());

        assert_eq!(panic_message(literal.as_ref()), "a literal panic");
        assert_eq!(panic_message(formatted.as_ref()), "a formatted panic");
    }

    /// An unwritable destination has to produce a message with the reason in
    /// it — "logging is degraded" with no cause is a row nobody can act on.
    #[test]
    fn an_unwritable_destination_declares_one_message_carrying_the_reason() {
        let degraded = Logging {
            file: None,
            directory: "/proc/nope/logs".to_string(),
            problem: Some("permission denied".to_string()),
        };

        let messages = degraded.desired_messages();
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0].text.contains("permission denied"),
            "{messages:?}"
        );
        assert!(messages[0].text.contains("/proc/nope/logs"), "{messages:?}");
    }

    /// The healthy case declares NOTHING, which is what retracts the previous
    /// run's complaint.
    #[test]
    fn a_healthy_log_declares_no_messages() {
        let healthy = Logging {
            file: Some(PathBuf::from("/tmp/logs/flowspace3.log")),
            directory: "/tmp/logs".to_string(),
            problem: None,
        };

        assert!(healthy.desired_messages().is_empty());
    }
}
