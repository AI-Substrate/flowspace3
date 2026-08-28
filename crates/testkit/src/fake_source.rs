//! A jsonl-backed [`ConversationSource`] fake, and the deliberate defects that
//! prove the contract suite can fail.
//!
//! Two jobs, and the second is the load-bearing one. A contract suite that has
//! never been seen to go RED is a suite nobody can trust — it may assert
//! nothing at all. [`FakeDefect`] gives the suite four wrong readers to catch,
//! so `conversation_source_contract` is itself mutation-checked before four
//! coders stake a unit on it (plan 005, tk-c104).
//!
//! The fake also outlives phase 1: the ingest orchestrator needs a source it
//! can drive without a real store on disk, and this is that source.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use fs3_core::conversation_source::{
    ConversationSource, Harness, IngestInput, RawRecord, ReadBatch, SessionFile, SessionKind,
    SourceCursor,
};
use fs3_core::{Error, Result, TurnRole, TurnSource};

use crate::conversation_source::SourceFixture;

/// The main session file inside a [`FakeConversationSource`] root.
pub const FAKE_MAIN_FILE: &str = "main.jsonl";
/// Where the fake keeps child conversations.
pub const FAKE_CHILDREN_DIR: &str = "children";

/// A way for a reader to be wrong, so the contract suite can be seen catching it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FakeDefect {
    /// Behaves.
    #[default]
    None,
    /// Reads the whole file every time, cursor or not — the failure that turns
    /// every re-poll into a full re-ingest.
    IgnoresCursor,
    /// Returns the trailing partial line as if it were a record.
    EmitsTornRecords,
    /// Accepts a cursor belonging to another store and reads from zero.
    AcceptsForeignCursor,
    /// Emits one record twice under the same ordinal — claude's
    /// one-line-per-content-block bug, unmerged.
    DuplicateOrdinals,
}

/// A [`ConversationSource`] over a directory of newline-delimited records.
///
/// Record shape is deliberately minimal — `{"ordinal": "...", "at": "...",
/// "body": "..."}` — because the fake exists to exercise cursor mechanics, not
/// a dialect.
#[derive(Clone, Debug)]
pub struct FakeConversationSource {
    root: PathBuf,
    defect: FakeDefect,
}

impl FakeConversationSource {
    /// A well-behaved fake over `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            defect: FakeDefect::None,
        }
    }

    /// A fake that is wrong in one specific, named way.
    #[must_use]
    pub fn broken(root: impl Into<PathBuf>, defect: FakeDefect) -> Self {
        Self {
            root: root.into(),
            defect,
        }
    }

    fn main_path(&self) -> PathBuf {
        self.root.join(FAKE_MAIN_FILE)
    }

    fn children_dir(&self) -> PathBuf {
        self.root.join(FAKE_CHILDREN_DIR)
    }
}

impl ConversationSource for FakeConversationSource {
    fn harness(&self) -> Harness {
        Harness::Omp
    }

    fn resolve(&self, _input: &IngestInput) -> Result<Vec<SessionFile>> {
        let mut files = vec![SessionFile {
            path: self.main_path(),
            session_id: "fake-main".to_owned(),
            parent_session_id: None,
            kind: SessionKind::Main,
            harness: self.harness(),
        }];

        // Re-globbed on every resolve, exactly as a real sidecar directory must
        // be: children appear mid-session.
        if let Ok(entries) = std::fs::read_dir(self.children_dir()) {
            let mut children: Vec<PathBuf> = entries
                .filter_map(std::result::Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
                .collect();
            children.sort();
            for path in children {
                let session_id = path
                    .file_stem()
                    .and_then(std::ffi::OsStr::to_str)
                    .unwrap_or("child")
                    .to_owned();
                files.push(SessionFile {
                    path,
                    session_id,
                    parent_session_id: Some("fake-main".to_owned()),
                    kind: SessionKind::Subagent,
                    harness: self.harness(),
                });
            }
        }

        Ok(files)
    }

    fn read_incremental(
        &self,
        file: &SessionFile,
        cursor: Option<&SourceCursor>,
    ) -> Result<ReadBatch> {
        let offset = match cursor {
            None => 0,
            Some(SourceCursor::ByteOffset { offset, .. }) => *offset,
            Some(_) if self.defect == FakeDefect::AcceptsForeignCursor => 0,
            Some(_) => {
                return Err(Error::Provider(
                    "fake: a byte-offset store was handed another store's cursor".to_owned(),
                ));
            }
        };
        let start = if self.defect == FakeDefect::IgnoresCursor {
            0
        } else {
            offset
        };

        let bytes = std::fs::read(&file.path)
            .map_err(|error| Error::Provider(format!("fake: cannot read: {error}")))?;
        let tail = &bytes[usize::try_from(start).unwrap_or(0).min(bytes.len())..];

        let complete = match tail.iter().rposition(|byte| *byte == b'\n') {
            Some(index) => index + 1,
            None => 0,
        };
        let framed = if self.defect == FakeDefect::EmitsTornRecords {
            tail
        } else {
            &tail[..complete]
        };

        let text = std::str::from_utf8(framed)
            .map_err(|error| Error::Provider(format!("fake: not UTF-8: {error}")))?;
        let mut records = Vec::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            // The dangerous shape of this defect is not an error — it is a
            // reader that "recovers" from a half-written line and hands on a
            // silently truncated turn.
            let record = if self.defect == FakeDefect::EmitsTornRecords {
                parse(line).unwrap_or_else(|_| recovered(line))
            } else {
                parse(line)?
            };
            if self.defect == FakeDefect::DuplicateOrdinals {
                records.push(record.clone());
            }
            records.push(record);
        }

        Ok(ReadBatch {
            records,
            cursor: SourceCursor::ByteOffset {
                device: 0,
                inode: 0,
                offset: start + complete as u64,
            },
            rescanned: false,
        })
    }
}

fn parse(line: &str) -> Result<RawRecord> {
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|error| Error::Provider(format!("fake: not a record: {error}")))?;
    Ok(RawRecord {
        ordinal: value["ordinal"].as_str().unwrap_or_default().to_owned(),
        parent_ordinal: None,
        at: value["at"]
            .as_str()
            .unwrap_or("1970-01-01T00:00:00Z")
            .to_owned(),
        role: TurnRole::Agent,
        source: TurnSource::System,
        body: value["body"].as_str().unwrap_or_default().to_owned(),
        items: Vec::new(),
        head_sha: None,
    })
}

/// What a reader that refuses to admit a line was incomplete would produce.
fn recovered(line: &str) -> RawRecord {
    RawRecord {
        ordinal: format!("torn-{}", line.len()),
        parent_ordinal: None,
        at: "1970-01-01T00:00:00Z".to_owned(),
        role: TurnRole::Agent,
        source: TurnSource::System,
        body: line.to_owned(),
        items: Vec::new(),
        head_sha: None,
    }
}

/// A scratch directory of fake records, wired up as a [`SourceFixture`].
///
/// Owns its temporary directory and removes it on drop, so the contract suite's
/// growth and tearing cases never touch a committed fixture.
pub struct FakeSourceFixture {
    source: FakeConversationSource,
    root: PathBuf,
    seeded: usize,
    grown: usize,
    children: usize,
}

impl FakeSourceFixture {
    /// A fixture with `records` seeded records and `children` child files.
    ///
    /// # Panics
    /// If the scratch directory cannot be created or written.
    #[must_use]
    pub fn new(records: usize, children: usize, defect: FakeDefect) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after 1970")
            .as_nanos();
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("fs3-fake-source-{nanos}-{unique}"));
        std::fs::create_dir_all(&root).expect("scratch root");

        let mut body = String::new();
        for index in 0..records {
            body.push_str(&record_line(index));
        }
        std::fs::write(root.join(FAKE_MAIN_FILE), body).expect("seed main file");

        if children > 0 {
            let dir = root.join(FAKE_CHILDREN_DIR);
            std::fs::create_dir_all(&dir).expect("children dir");
            for child in 0..children {
                std::fs::write(
                    dir.join(format!("agent-{child}.jsonl")),
                    record_line(1000 + child),
                )
                .expect("seed child");
            }
        }

        Self {
            source: FakeConversationSource::broken(root.clone(), defect),
            root,
            seeded: records,
            grown: 0,
            children,
        }
    }

    fn main_path(&self) -> PathBuf {
        self.root.join(FAKE_MAIN_FILE)
    }

    fn append(&self, text: &str) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.main_path())
            .expect("open main for append");
        file.write_all(text.as_bytes()).expect("append");
    }
}

impl Drop for FakeSourceFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl SourceFixture for FakeSourceFixture {
    fn source(&self) -> &dyn ConversationSource {
        &self.source
    }

    fn input(&self) -> IngestInput {
        IngestInput::Native {
            session_id: "fake-main".to_owned(),
            harness: Harness::Omp,
            folder: self.root.clone(),
        }
    }

    fn expected_session_files(&self) -> usize {
        1 + self.children
    }

    fn expected_records(&self) -> usize {
        self.seeded
    }

    fn grow(&mut self) -> usize {
        const ADDED: usize = 2;
        let base = self.seeded + self.grown;
        let mut text = String::new();
        for index in 0..ADDED {
            text.push_str(&record_line(base + index));
        }
        self.append(&text);
        self.grown += ADDED;
        ADDED
    }

    fn begin_partial_record(&mut self) -> bool {
        let half = record_line(self.seeded + self.grown);
        let cut = half.len() / 2;
        self.append(&half[..cut]);
        true
    }

    fn finish_partial_record(&mut self) {
        let whole = record_line(self.seeded + self.grown);
        let cut = whole.len() / 2;
        self.append(&whole[cut..]);
        self.grown += 1;
    }
}

fn record_line(index: usize) -> String {
    format!(
        "{{\"ordinal\":\"r{index}\",\"at\":\"2026-08-28T00:00:{:02}Z\",\"body\":\"record {index}\"}}\n",
        index % 60
    )
}
