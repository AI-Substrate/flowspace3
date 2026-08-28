//! Incremental line framing for the append-only jsonl stores.
//!
//! Three of the four conversation stores are jsonl files a live agent is still
//! writing to, and all three resume the same way: remember a byte offset, read
//! forward, and stop at the last COMPLETE line. That logic is here, once,
//! rather than three times in three readers — it is the part of a reader that
//! is not about a dialect at all, and the part where a subtle bug (a torn
//! record, an offset that advanced past a half-written line) corrupts a
//! conversation silently.
//!
//! Frozen with the [`ConversationSource`] seam in plan 005 phase 1: the readers
//! were written in parallel against this, so its behaviour is contract, not
//! implementation detail.
//!
//! What lives HERE: framing, the tail buffer, rotation and truncation
//! detection. What lives in the cursor-state service: durable persistence of
//! the cursor between runs, and the ledger of ordinals that a post-rotation
//! rescan is deduplicated against. Detection is per-store and cheap; the
//! ledger is per-conversation and durable, and they are different jobs.
//!
//! # One store is not purely append-only
//!
//! Measured while harvesting the omp fixtures (2026-08-28): an omp session
//! jsonl opens with a FIXED-WIDTH title slot that the harness rewrites IN
//! PLACE as the session is renamed. A byte-offset cursor survives that only
//! because the slot's width does not change — no byte after it ever moves —
//! and it is invisible to the checks below, since an in-place rewrite alters
//! neither the file's length nor its inode. So: offsets are legal for that
//! store from the first record onward, the title line's CONTENT read at
//! ingest time may be stale by the time the conversation ends, and a reader
//! that wants the current title must re-read line 0 rather than trusting what
//! its first poll saw. If that slot ever becomes variable-width, every cursor
//! for the store is invalidated at once and this is the note that says so.
//!
//! [`ConversationSource`]: fs3_core::ConversationSource

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use fs3_core::{Error, Result, SourceCursor};

/// One incremental read of a line-oriented file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TailRead {
    /// The complete lines after the supplied cursor, blank lines dropped.
    pub lines: Vec<String>,
    /// Where the next read resumes — always immediately after a newline.
    pub cursor: SourceCursor,
    /// Whether the file was rotated or truncated and this is a full re-read.
    pub rescanned: bool,
}

/// Read every complete line after `cursor`, leaving a partial tail unread.
///
/// The three rules that make a live file safe to read:
///
/// 1. **Stop at the last newline.** A writer can be mid-line at read time
///    (recipe gotcha 7 — a session grew 808k to 904k output tokens between two
///    surveys on one day). Bytes after the final newline are left unconsumed
///    and the cursor does not advance past them, so the next poll sees that
///    record whole.
/// 2. **Identity, not just size.** The cursor carries `st_dev`/`st_ino`
///    alongside the offset. A different inode means the path now names a
///    different file, and reading on at the old offset would splice two
///    conversations together.
/// 3. **A shrinking file has rotated.** Size below the held offset cannot
///    happen to an append-only file, so it is a truncation or a replacement:
///    restart from zero and say so, because the caller must dedupe rather than
///    append.
///
/// # Errors
/// [`Error::Provider`] when the file cannot be opened, stat-ed, sought or read.
pub fn read_lines(path: &Path, cursor: Option<&SourceCursor>) -> Result<TailRead> {
    let mut file = File::open(path).map_err(|error| io_failure(path, "open", &error))?;
    let metadata = file
        .metadata()
        .map_err(|error| io_failure(path, "stat", &error))?;
    let (device, inode) = identity(&metadata);
    let size = metadata.len();

    let held = match cursor {
        None => None,
        Some(SourceCursor::ByteOffset {
            device: held_device,
            inode: held_inode,
            offset,
        }) => Some((*held_device, *held_inode, *offset)),
        Some(other) => {
            return Err(Error::Provider(format!(
                "{}: a byte-offset store was handed a {} cursor",
                path.display(),
                cursor_kind(other)
            )));
        }
    };

    // A first read is not a rescan: there is nothing to have rotated away from.
    let (start, rescanned) = match held {
        None => (0, false),
        Some((held_device, held_inode, offset)) => {
            let moved = (held_device, held_inode) != (device, inode);
            let shrank = size < offset;
            if moved || shrank {
                (0, true)
            } else {
                (offset, false)
            }
        }
    };

    if start == size {
        return Ok(TailRead {
            lines: Vec::new(),
            cursor: SourceCursor::ByteOffset {
                device,
                inode,
                offset: start,
            },
            rescanned,
        });
    }

    file.seek(SeekFrom::Start(start))
        .map_err(|error| io_failure(path, "seek", &error))?;
    let mut buffer = Vec::with_capacity(usize::try_from(size.saturating_sub(start)).unwrap_or(0));
    file.read_to_end(&mut buffer)
        .map_err(|error| io_failure(path, "read", &error))?;

    // Everything up to and including the final newline is complete; whatever
    // follows it is the writer's half-finished record and stays for next time.
    let complete = match buffer.iter().rposition(|byte| *byte == b'\n') {
        Some(index) => index + 1,
        None => 0,
    };

    let lines = std::str::from_utf8(&buffer[..complete])
        .map_err(|error| {
            Error::Provider(format!(
                "{}: bytes from offset {start} are not UTF-8: {error}",
                path.display()
            ))
        })?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect();

    Ok(TailRead {
        lines,
        cursor: SourceCursor::ByteOffset {
            device,
            inode,
            offset: start + complete as u64,
        },
        rescanned,
    })
}

/// The file's identity, as far as the platform will say.
///
/// On unix this is `(st_dev, st_ino)`, which is what makes a rotation
/// detectable at all. Elsewhere there is no cheap stable equivalent, so
/// identity degrades to a constant and rotation is caught only by the
/// size-below-offset rule — weaker, and deliberately not pretended otherwise.
#[cfg(unix)]
fn identity(metadata: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino())
}

#[cfg(not(unix))]
fn identity(_metadata: &std::fs::Metadata) -> (u64, u64) {
    (0, 0)
}

fn cursor_kind(cursor: &SourceCursor) -> &'static str {
    match cursor {
        SourceCursor::ByteOffset { .. } => "byte-offset",
        SourceCursor::Seq { .. } => "sequence",
        SourceCursor::RowId { .. } => "rowid",
    }
}

fn io_failure(path: &Path, action: &str, error: &std::io::Error) -> Error {
    Error::Provider(format!("{}: cannot {action}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory that removes itself. `tempfile` would be a
    /// dependency bought for four tests; this is the whole of what they need.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the clock is after 1970")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("fs3-tail-{name}-{nanos}"));
            std::fs::create_dir_all(&path).expect("scratch dir");
            Self(path)
        }

        fn file(&self, name: &str) -> std::path::PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn append(path: &Path, text: &str) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open for append");
        file.write_all(text.as_bytes()).expect("append");
    }

    #[test]
    fn a_first_read_yields_every_line_and_a_resumable_cursor() {
        let scratch = Scratch::new("first");
        let path = scratch.file("a.jsonl");
        append(&path, "{\"a\":1}\n{\"a\":2}\n");

        let read = read_lines(&path, None).expect("read");

        assert_eq!(read.lines, vec!["{\"a\":1}", "{\"a\":2}"]);
        assert!(
            !read.rescanned,
            "a first read has nothing to have rotated away from"
        );
        let SourceCursor::ByteOffset { offset, .. } = read.cursor else {
            panic!("a file store must return a byte-offset cursor");
        };
        assert_eq!(offset, 16, "the cursor lands after the final newline");
    }

    #[test]
    fn re_reading_from_the_returned_cursor_yields_nothing() {
        let scratch = Scratch::new("resume");
        let path = scratch.file("a.jsonl");
        append(&path, "{\"a\":1}\n");

        let first = read_lines(&path, None).expect("first read");
        let second = read_lines(&path, Some(&first.cursor)).expect("second read");

        assert!(
            second.lines.is_empty(),
            "an unchanged file must yield no records, not the same ones again"
        );
        assert_eq!(second.cursor, first.cursor);
        assert!(!second.rescanned);
    }

    #[test]
    fn appended_bytes_yield_only_the_delta() {
        let scratch = Scratch::new("delta");
        let path = scratch.file("a.jsonl");
        append(&path, "{\"a\":1}\n");
        let first = read_lines(&path, None).expect("first read");

        append(&path, "{\"a\":2}\n{\"a\":3}\n");
        let second = read_lines(&path, Some(&first.cursor)).expect("second read");

        assert_eq!(
            second.lines,
            vec!["{\"a\":2}", "{\"a\":3}"],
            "re-polling must cost only what is new — this IS the incremental claim"
        );
        assert!(!second.rescanned);
    }

    #[test]
    fn a_half_written_line_is_not_returned_and_does_not_move_the_cursor() {
        let scratch = Scratch::new("torn");
        let path = scratch.file("a.jsonl");
        append(&path, "{\"a\":1}\n");
        let first = read_lines(&path, None).expect("first read");

        // The writer is mid-record: no trailing newline yet.
        append(&path, "{\"a\":2,\"partial\"");
        let second = read_lines(&path, Some(&first.cursor)).expect("second read");

        assert!(
            second.lines.is_empty(),
            "half a record is not a record: {:?}",
            second.lines
        );
        assert_eq!(
            second.cursor, first.cursor,
            "the cursor must not advance past an incomplete line, or the record is lost forever"
        );

        // And when the writer finishes it, the whole record arrives once.
        append(&path, ",\"b\":2}\n");
        let third = read_lines(&path, Some(&second.cursor)).expect("third read");
        assert_eq!(third.lines, vec!["{\"a\":2,\"partial\",\"b\":2}"]);
    }

    #[test]
    fn a_truncated_file_is_reported_as_a_rescan_rather_than_read_from_the_old_offset() {
        let scratch = Scratch::new("truncate");
        let path = scratch.file("a.jsonl");
        append(&path, "{\"a\":1}\n{\"a\":2}\n");
        let first = read_lines(&path, None).expect("first read");

        std::fs::write(&path, "{\"a\":9}\n").expect("rewrite shorter");
        let second = read_lines(&path, Some(&first.cursor)).expect("second read");

        assert!(
            second.rescanned,
            "a file smaller than the held offset cannot be an append — the caller must dedupe"
        );
        assert_eq!(second.lines, vec!["{\"a\":9}"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_replaced_file_is_detected_by_inode_even_when_it_is_the_same_size() {
        let scratch = Scratch::new("rotate");
        let path = scratch.file("a.jsonl");
        append(&path, "{\"a\":1}\n");
        let first = read_lines(&path, None).expect("first read");

        // Same path, same length, different file — the case a size check alone
        // reads as "nothing new" while an entire conversation is missed.
        let replacement = scratch.file("b.jsonl");
        std::fs::write(&replacement, "{\"a\":2}\n").expect("write replacement");
        std::fs::rename(&replacement, &path).expect("rotate into place");

        let second = read_lines(&path, Some(&first.cursor)).expect("second read");

        assert!(
            second.rescanned,
            "a new inode at the same path is a rotation"
        );
        assert_eq!(second.lines, vec!["{\"a\":2}"]);
    }

    #[test]
    fn a_cursor_from_another_store_is_refused_rather_than_ignored() {
        let scratch = Scratch::new("wrong-cursor");
        let path = scratch.file("a.jsonl");
        append(&path, "{\"a\":1}\n");

        let error = read_lines(&path, Some(&SourceCursor::Seq { seq: 3 }))
            .expect_err("a sequence cursor cannot address a byte-offset store");

        assert!(
            error.to_string().contains("sequence"),
            "the refusal must name the cursor it was handed: {error}"
        );
    }

    #[test]
    fn blank_lines_are_dropped_but_still_consumed() {
        let scratch = Scratch::new("blank");
        let path = scratch.file("a.jsonl");
        append(&path, "{\"a\":1}\n\n{\"a\":2}\n");

        let read = read_lines(&path, None).expect("read");

        assert_eq!(read.lines, vec!["{\"a\":1}", "{\"a\":2}"]);
        let SourceCursor::ByteOffset { offset, .. } = read.cursor else {
            panic!("byte-offset cursor");
        };
        assert_eq!(
            offset, 17,
            "a dropped line must still advance the cursor, or it is re-read forever"
        );
    }
}
