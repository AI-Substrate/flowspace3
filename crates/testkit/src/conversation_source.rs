//! The shared contract every [`ConversationSource`] must pass.
//!
//! Four readers for four stores were written in parallel in plan 005. What
//! makes that safe is not review but this: one suite, run over every reader's
//! own golden fixtures, defining "done" identically for all of them. A reader
//! that passes here is finished; a reader that does not is not, whatever its
//! author believes (tenet 5 — done is mechanical).
//!
//! The five cases are the incremental-reading claim, decomposed:
//!
//! 1. **resolve finds the files** — including the sidecars a session grows.
//! 2. **a read from `None` yields everything**, with a resumable cursor.
//! 3. **a re-read from that cursor yields nothing** — polling an unchanged
//!    conversation must be free, or ingest is a full re-read wearing a hat.
//! 4. **appended bytes yield only the delta** — this is plan-005 ac-0003.
//! 5. **a half-written record yields nothing and does not move the cursor** —
//!    live files are the normal case, not the exceptional one (recipe gotcha 7).
//!
//! Plus one the stores get for free: a cursor from a different store is
//! REFUSED rather than silently read as zero.
//!
//! ```no_run
//! # use fs3_testkit::{SourceFixture, conversation_source_contract};
//! # fn run(fixture: &mut dyn SourceFixture) {
//! conversation_source_contract(fixture);
//! # }
//! ```

use fs3_core::{ConversationSource, IngestInput, SessionFile, SessionKind, SourceCursor};

/// A reader plus a mutable copy of its fixtures, as the contract needs them.
///
/// Implement this over a SCRATCH copy of the golden fixtures, never the
/// committed ones: cases 4 and 5 write to the store on purpose.
pub trait SourceFixture {
    /// The reader under test.
    fn source(&self) -> &dyn ConversationSource;

    /// What to ask it for.
    fn input(&self) -> IngestInput;

    /// How many session files `resolve` must find, sidecars included.
    fn expected_session_files(&self) -> usize;

    /// How many records a full read of the MAIN session file must yield.
    fn expected_records(&self) -> usize;

    /// Append `n` further real records to the main session file, returning `n`.
    ///
    /// Real records, not invented ones: a fixture that grows by something its
    /// store would never write proves the suite, not the reader.
    fn grow(&mut self) -> usize;

    /// Begin one more record and stop halfway, leaving no line terminator.
    ///
    /// Return `false` for a store that cannot be torn — a sqlite database
    /// commits a row or does not have it — and the torn-record case is then
    /// skipped rather than faked.
    fn begin_partial_record(&mut self) -> bool;

    /// Finish the record [`SourceFixture::begin_partial_record`] began.
    ///
    /// Only called when that returned `true`.
    fn finish_partial_record(&mut self);
}

/// Run every case. Panics on the first violation, naming the claim that broke.
///
/// # Panics
/// On any contract violation — that is the reporting channel.
pub fn conversation_source_contract(fixture: &mut dyn SourceFixture) {
    let files = resolve_finds_the_session_files(fixture);
    let main = main_file(&files);

    let first = full_read_yields_everything(fixture, &main);
    let cursor = re_read_from_the_cursor_yields_nothing(fixture, &main, first.1);
    let cursor = appended_records_yield_only_the_delta(fixture, &main, cursor, &first.0);
    a_half_written_record_is_not_returned(fixture, &main, cursor.clone());
    a_foreign_cursor_is_refused(fixture, &main, &cursor);
}

fn resolve_finds_the_session_files(fixture: &dyn SourceFixture) -> Vec<SessionFile> {
    let source = fixture.source();
    let files = source
        .resolve(&fixture.input())
        .expect("contract: resolve must find the addressed conversation");

    assert_eq!(
        files.len(),
        fixture.expected_session_files(),
        "contract: resolve must find every file of the conversation, sidecars included \
         — a subagent that appears mid-session is a child conversation, and a reader \
         that resolves once loses it"
    );
    assert!(
        files.iter().all(|file| file.harness == source.harness()),
        "contract: every resolved file must be stamped with the store that produced it"
    );
    assert_eq!(
        files
            .iter()
            .filter(|file| file.kind == SessionKind::Main)
            .count(),
        1,
        "contract: a conversation has exactly one main session file; the rest are children"
    );
    for child in files
        .iter()
        .filter(|file| file.kind == SessionKind::Subagent)
    {
        assert!(
            child.parent_session_id.is_some(),
            "contract: a subagent conversation must name its parent, or its work is invisible"
        );
    }
    files
}

fn main_file(files: &[SessionFile]) -> SessionFile {
    files
        .iter()
        .find(|file| file.kind == SessionKind::Main)
        .expect("contract: resolve must return a main session file")
        .clone()
}

fn full_read_yields_everything(
    fixture: &dyn SourceFixture,
    main: &SessionFile,
) -> (Vec<String>, SourceCursor) {
    let batch = fixture
        .source()
        .read_incremental(main, None)
        .expect("contract: a read from no cursor must succeed");

    assert_eq!(
        batch.records.len(),
        fixture.expected_records(),
        "contract: a read from no cursor must yield the whole conversation"
    );
    assert!(
        !batch.rescanned,
        "contract: a first read has no cursor to have rotated away from, so it is not a rescan"
    );

    let mut ordinals = Vec::with_capacity(batch.records.len());
    for record in &batch.records {
        assert!(
            !record.ordinal.is_empty(),
            "contract: every record needs its store's natural ordinal — it is the dedupe key \
             a post-rotation rescan depends on"
        );
        assert!(
            !record.at.is_empty(),
            "contract: every record needs a timestamp, even though it is never a cursor"
        );
        ordinals.push(record.ordinal.clone());
    }
    let mut unique = ordinals.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        ordinals.len(),
        "contract: ordinals must be unique within a conversation — duplicates make dedupe \
         after a rotation impossible, and claude's one-line-per-content-block is exactly \
         how they arise"
    );

    (ordinals, batch.cursor)
}

fn re_read_from_the_cursor_yields_nothing(
    fixture: &dyn SourceFixture,
    main: &SessionFile,
    cursor: SourceCursor,
) -> SourceCursor {
    let batch = fixture
        .source()
        .read_incremental(main, Some(&cursor))
        .expect("contract: a re-read from the returned cursor must succeed");

    assert!(
        batch.records.is_empty(),
        "contract: polling an unchanged conversation must yield nothing — it returned {} \
         record(s), which means every poll re-ingests the whole conversation",
        batch.records.len()
    );
    assert_eq!(
        batch.cursor, cursor,
        "contract: an empty poll must leave the cursor exactly where it was"
    );
    assert!(
        !batch.rescanned,
        "contract: nothing changed, so nothing rotated"
    );
    batch.cursor
}

fn appended_records_yield_only_the_delta(
    fixture: &mut dyn SourceFixture,
    main: &SessionFile,
    cursor: SourceCursor,
    already_seen: &[String],
) -> SourceCursor {
    let added = fixture.grow();
    assert!(
        added > 0,
        "contract: the fixture must be able to grow, or case 4 proves nothing"
    );

    let batch = fixture
        .source()
        .read_incremental(main, Some(&cursor))
        .expect("contract: a read after growth must succeed");

    assert_eq!(
        batch.records.len(),
        added,
        "contract: a second ingest must cost only the turns that are new (ac-0003)"
    );
    assert!(
        !batch.rescanned,
        "contract: an append is not a rotation — claiming one forces a needless full dedupe"
    );
    for record in &batch.records {
        assert!(
            !already_seen.contains(&record.ordinal),
            "contract: record {:?} was already delivered before the append",
            record.ordinal
        );
    }
    assert_ne!(
        batch.cursor, cursor,
        "contract: consuming new records must advance the cursor"
    );
    batch.cursor
}

fn a_half_written_record_is_not_returned(
    fixture: &mut dyn SourceFixture,
    main: &SessionFile,
    cursor: SourceCursor,
) {
    if !fixture.begin_partial_record() {
        // A transactional store cannot be torn. Nothing to prove, nothing faked.
        return;
    }

    let torn = fixture
        .source()
        .read_incremental(main, Some(&cursor))
        .expect("contract: reading a file with a half-written record must succeed, not error");

    assert!(
        torn.records.is_empty(),
        "contract: half a record is not a record — a writer mid-line at read time must \
         yield nothing rather than a truncated turn"
    );
    assert_eq!(
        torn.cursor, cursor,
        "contract: the cursor must not advance past an incomplete record, or that record \
         is lost for good once the writer finishes it"
    );

    fixture.finish_partial_record();

    let completed = fixture
        .source()
        .read_incremental(main, Some(&torn.cursor))
        .expect("contract: reading the finished record must succeed");
    assert_eq!(
        completed.records.len(),
        1,
        "contract: the record that was half-written must arrive exactly once, whole"
    );
}

fn a_foreign_cursor_is_refused(
    fixture: &dyn SourceFixture,
    main: &SessionFile,
    native: &SourceCursor,
) {
    let foreign = match native {
        SourceCursor::ByteOffset { .. } => SourceCursor::Seq { seq: 0 },
        SourceCursor::Seq { .. } | SourceCursor::RowId { .. } => SourceCursor::ByteOffset {
            device: 0,
            inode: 0,
            offset: 0,
        },
    };

    let outcome = fixture.source().read_incremental(main, Some(&foreign));

    assert!(
        outcome.is_err(),
        "contract: a cursor from another store must be REFUSED — read as zero it would \
         silently re-ingest an entire conversation"
    );
}
