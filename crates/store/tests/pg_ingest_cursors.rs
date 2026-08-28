//! Cursors and the ordinal ledger, against a real Postgres.
//!
//! The load-bearing tests in this file are
//! [`a_rescan_after_rotation_appends_nothing_through_the_store`] and
//! [`two_sessions_on_one_conversation_number_above_each_other`]. The first is
//! why the ledger exists: a reader whose file rotated restarts from zero and
//! hands back the WHOLE conversation, and if the ledger cannot recognise those
//! records the conversation is stored twice — silently, because a duplicated
//! conversation looks exactly like a busy one.
//!
//! The second is why the high-water mark comes from the stored TURNS rather
//! than from the ledger. `turn_no` is the conversation's primary key, so a
//! per-session mark was an inference about a one-session-per-conversation
//! mapping; where that mapping does not hold, two sessions both number from 1,
//! `append_turns` drops the collisions idempotently, and this module records
//! them as stored. Turns that vanish while every call reports success.
//!
//! Both are mutation-checked at the seam they guard: drop the `seen` lookup
//! from `fs3_core::prepare_batch` and the first fails on the turn count; put
//! the high-water mark back on `ingest_ledger` and the second fails on the
//! turn numbers.

mod support;

use std::collections::BTreeSet;

use fs3_core::{
    Conversation, ConversationId, Element, Harness, RawRecord, SourceCursor, Turn, TurnRole,
    TurnSource, prepare_batch,
};
use fs3_store::ingest_cursors::{
    commit_poll, conversation_for, forget_session, ledger_view, load_cursor, sessions_for,
};
use fs3_store::{PgPool, append_turns, delete_conversation, upsert_conversation};
use support::FreshDatabase;

const SESSION: &str = "9f2c0a44-1f1e-4d2a-9a3c-2b7d8e5f0011";

fn id(nibble: char) -> ConversationId {
    ConversationId::new(format!("6ba7b810-9dad-11d1-80b4-00c04fd430{nibble}7"))
        .expect("a canonical uuid")
}

fn conversation(guid: &ConversationId) -> Conversation {
    Conversation {
        guid: guid.clone(),
        repo_identity: None,
        worktree: Some("/srv/checkout".to_string()),
        base_sha: None,
        title: Some("a tailed conversation".to_string()),
        started_at: "2026-08-28T09:00:00Z".to_string(),
        parent: None,
    }
}

fn record(ordinal: &str, body: &str) -> RawRecord {
    RawRecord {
        ordinal: ordinal.to_string(),
        parent_ordinal: None,
        at: "2026-08-28T09:00:00Z".to_string(),
        role: TurnRole::Agent,
        source: TurnSource::System,
        body: body.to_string(),
        items: Vec::new(),
        head_sha: None,
    }
}

/// A turn as another ingest path would have written it — no ordinal anywhere,
/// which is exactly what a transcript import leaves behind.
fn imported_turn(turn_no: u32, body: &str) -> Turn {
    Turn {
        turn_no,
        role: TurnRole::Human,
        source: TurnSource::Human,
        head_sha: None,
        at: "2026-08-28T08:00:00Z".to_string(),
        body: body.to_string(),
        items: Vec::new(),
    }
}

/// A migrated pool with one conversation to hang cursors off.
async fn seeded(database: &FreshDatabase, guid: &ConversationId) -> PgPool {
    let pool = database.migrated_pool().await;
    upsert_conversation(&pool, &conversation(guid))
        .await
        .expect("the conversation header should store");
    pool
}

/// Store turns the way the orchestrator will, and report how many were new.
///
/// The size gate is a literal `false`: these tests are about numbering, and a
/// verdict they do not assert on should not be one they have to compute.
async fn append(pool: &PgPool, guid: &ConversationId, turns: &[Turn]) -> usize {
    append_turns(pool, guid, turns, |_: &Element| false)
        .await
        .expect("turns should append")
        .accepted
        .len()
}

/// One full poll: look, decide, store, record. The sequence the snap-in recipe
/// prescribes, so these tests fail if that sequence stops working.
async fn poll(
    pool: &PgPool,
    harness: Harness,
    session: &str,
    guid: &ConversationId,
    records: &[RawRecord],
    cursor: &SourceCursor,
) -> (usize, usize) {
    let ordinals: Vec<&str> = records.iter().map(|r| r.ordinal.as_str()).collect();
    let view = ledger_view(pool, harness, session, guid, &ordinals)
        .await
        .expect("the ledger view should read");
    let prepared = prepare_batch(records, &view.seen, view.next_turn_no);
    let accepted = append(pool, guid, &prepared.turns).await;
    commit_poll(pool, harness, session, guid, cursor, &prepared.ledger)
        .await
        .expect("the poll should commit");
    (accepted, prepared.deduped)
}

/// A cursor that only lives inside one process makes the SECOND ingest a full
/// re-read, which is the entire cost model of this plan. All three variants,
/// because each store resumes in different terms and a serialisation that
/// works for one proves nothing about the others.
/// The anomaly alarm fires on a MIXED batch, which is the shape it exists for.
///
/// Round 2 of cross-model review found the first guard gated on
/// `prepared.deduped == 0`, which disabled it on exactly the ordinary
/// rescan-plus-growth batch: one already-seen ordinal makes `deduped` nonzero,
/// and a colliding NEW turn would then be classified already-stored while the
/// cursor and ledger committed past it. This pins the property the guard reads:
/// `prepare_batch` removes every seen record BEFORE `append_turns`, so
/// `already_stored` counts only turns the ledger called new — and it can be
/// nonzero while `deduped` is too.
#[tokio::test]
async fn a_mixed_batch_still_reports_a_ledger_and_table_disagreement() {
    let database = FreshDatabase::create().await;
    let guid = id('e');
    let pool = seeded(&database, &guid).await;
    let harness = Harness::Omp;
    let session = "mixed-batch";

    // First poll: two records, both stored and both ledgered.
    let first = [record("a", "one"), record("b", "two")];
    let view = ledger_view(&pool, harness, session, &guid, &["a", "b"])
        .await
        .unwrap();
    let prepared = prepare_batch(&first, &view.seen, view.next_turn_no);
    assert_eq!(prepared.turns.len(), 2);
    append(&pool, &guid, &prepared.turns).await;
    commit_poll(
        &pool,
        harness,
        session,
        &guid,
        &SourceCursor::Seq { seq: 2 },
        &prepared.ledger,
    )
    .await
    .unwrap();

    // Second poll is MIXED: "a" is already in the ledger, "c" is new. That
    // makes `deduped` nonzero — the condition that used to silence the alarm.
    let second = [record("a", "one"), record("c", "three")];
    let view = ledger_view(&pool, harness, session, &guid, &["a", "c"])
        .await
        .unwrap();
    let prepared = prepare_batch(&second, &view.seen, view.next_turn_no);
    assert_eq!(prepared.deduped, 1, "the ledger recognised the old record");
    assert_eq!(prepared.turns.len(), 1, "and only the new one is prepared");

    // Simulate the disagreement the guard watches for: another poll stored that
    // turn under the same number while this one was deciding.
    append(&pool, &guid, &prepared.turns).await;
    let appended = append_turns(&pool, &guid, &prepared.turns, |_| false)
        .await
        .unwrap();

    assert_eq!(
        appended.already_stored, 1,
        "the store had a turn the ledger called new — nonzero even though \
         deduped was also nonzero, which is the case the old qualifier hid"
    );
    assert!(appended.accepted.is_empty());

    database.destroy(pool).await;
}

#[tokio::test]
async fn every_cursor_variant_round_trips_through_postgres() {
    let guid = id('a');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    let cases = [
        (
            Harness::Claude,
            SourceCursor::ByteOffset {
                device: 16_777_234,
                inode: 92_233_720_368,
                offset: 4_096,
            },
        ),
        (Harness::PijLedger, SourceCursor::Seq { seq: 4_211 }),
        (Harness::MetricsDb, SourceCursor::RowId { rowid: 90_210 }),
    ];

    for (harness, cursor) in &cases {
        commit_poll(&pool, *harness, SESSION, &guid, cursor, &[])
            .await
            .expect("the cursor should commit");

        let loaded = load_cursor(&pool, *harness, SESSION)
            .await
            .expect("the cursor should load");

        assert_eq!(
            loaded.as_ref(),
            Some(cursor),
            "{harness} must resume in exactly the terms it stopped in"
        );
    }

    database.destroy(pool).await;
}

/// A device/inode pair is a u64 and real ones are large. JSON numbers are
/// where that quietly becomes an f64 and loses its low bits — an offset that
/// comes back one byte wrong resumes mid-record forever.
#[tokio::test]
async fn a_cursor_survives_the_largest_values_its_types_allow() {
    let guid = id('b');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    let cursor = SourceCursor::ByteOffset {
        device: u64::MAX,
        inode: u64::MAX - 1,
        offset: u64::MAX - 2,
    };
    commit_poll(&pool, Harness::Omp, SESSION, &guid, &cursor, &[])
        .await
        .expect("the cursor should commit");

    assert_eq!(
        load_cursor(&pool, Harness::Omp, SESSION).await.unwrap(),
        Some(cursor),
        "no precision may be lost on the way through jsonb"
    );

    database.destroy(pool).await;
}

/// "Never read" and "read and forgotten" must be the same answer, so a first
/// ingest and a re-ingest take the identical path with no branch.
#[tokio::test]
async fn an_unread_session_has_no_cursor() {
    let guid = id('c');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    assert_eq!(
        load_cursor(&pool, Harness::Claude, "never-read")
            .await
            .unwrap(),
        None
    );

    database.destroy(pool).await;
}

/// THE case this unit exists for, end to end through Postgres.
///
/// Poll one stores the conversation. The file then rotates, so poll two comes
/// back with `rescanned = true` and the WHOLE conversation again. Nothing new
/// may be stored.
#[tokio::test]
async fn a_rescan_after_rotation_appends_nothing_through_the_store() {
    let guid = id('d');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    let whole = [
        record("r1", "first"),
        record("r2", "second"),
        record("r3", "third"),
    ];

    let before = SourceCursor::ByteOffset {
        device: 1,
        inode: 2,
        offset: 300,
    };
    let (accepted, deduped) = poll(&pool, Harness::Omp, SESSION, &guid, &whole, &before).await;
    assert_eq!(accepted, 3, "the first ingest stores everything");
    assert_eq!(deduped, 0);

    // The file rotated: the reader restarts from zero, so the same three
    // records come back under a NEW inode.
    let after = SourceCursor::ByteOffset {
        device: 1,
        inode: 99,
        offset: 300,
    };
    let (accepted, deduped) = poll(&pool, Harness::Omp, SESSION, &guid, &whole, &after).await;

    assert_eq!(
        accepted, 0,
        "a rescan of an unchanged conversation must append ZERO turns — storing \
         them again duplicates the whole conversation and looks like a busy session"
    );
    assert_eq!(deduped, 3);

    let stored: i64 =
        sqlx::query_scalar("SELECT count(*) FROM turns WHERE conversation_id = $1::uuid")
            .bind(guid.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, 3, "and the conversation still holds exactly three");

    database.destroy(pool).await;
}

/// The other half: a rotation that also GREW stores only the growth, numbered
/// after what is already there.
#[tokio::test]
async fn a_rescan_that_grew_stores_only_the_delta() {
    let guid = id('e');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    let first_pass = [record("r1", "first"), record("r2", "second")];
    poll(
        &pool,
        Harness::Claude,
        SESSION,
        &guid,
        &first_pass,
        &SourceCursor::Seq { seq: 2 },
    )
    .await;

    let after_rotation = [
        record("r1", "first"),
        record("r2", "second"),
        record("r3", "third"),
    ];
    let ordinals: Vec<&str> = after_rotation.iter().map(|r| r.ordinal.as_str()).collect();
    let view = ledger_view(&pool, Harness::Claude, SESSION, &guid, &ordinals)
        .await
        .unwrap();
    let prepared = prepare_batch(&after_rotation, &view.seen, view.next_turn_no);

    assert_eq!(prepared.turns.len(), 1);
    assert_eq!(prepared.turns[0].body, "third");
    assert_eq!(
        prepared.turns[0].turn_no, 3,
        "numbered after the two already stored"
    );
    assert_eq!(prepared.ledger, vec![("r3", 3)]);

    database.destroy(pool).await;
}

/// TWO SESSIONS, ONE CONVERSATION. The second must number ABOVE the first's
/// turns, not restart at 1.
///
/// This is the failure that made the high-water mark come from the turns:
/// `append_turns` is idempotent on `(conversation_id, turn_no)`, so a second
/// session numbering from 1 would have its turns dropped on conflict while
/// `commit_poll` recorded them as stored — turns that vanish while every call
/// reports success. Mutation-check: point `ledger_view` back at
/// `ingest_ledger` and this fails on the numbers.
#[tokio::test]
async fn two_sessions_on_one_conversation_number_above_each_other() {
    let guid = id('f');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    let main = [
        record("m1", "from the main file"),
        record("m2", "and again"),
    ];
    let (accepted, _) = poll(
        &pool,
        Harness::Claude,
        "session-one",
        &guid,
        &main,
        &SourceCursor::Seq { seq: 2 },
    )
    .await;
    assert_eq!(accepted, 2);

    // A different session, same conversation. Its ordinals are its own.
    let other = [record("o1", "from the other session")];
    let ordinals: Vec<&str> = other.iter().map(|r| r.ordinal.as_str()).collect();
    let view = ledger_view(&pool, Harness::Claude, "session-two", &guid, &ordinals)
        .await
        .unwrap();

    assert!(
        view.seen.is_empty(),
        "another session's ordinals are not this session's"
    );
    assert_eq!(
        view.next_turn_no, 3,
        "but the NUMBER comes from the conversation, which already holds two turns"
    );

    let prepared = prepare_batch(&other, &view.seen, view.next_turn_no);
    let accepted = append(&pool, &guid, &prepared.turns).await;

    assert_eq!(
        accepted, 1,
        "the second session's turn is stored rather than dropped on conflict"
    );

    let stored: i64 =
        sqlx::query_scalar("SELECT count(*) FROM turns WHERE conversation_id = $1::uuid")
            .bind(guid.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, 3, "all three turns are really there");

    database.destroy(pool).await;
}

/// A conversation filled by transcript import is then TAILED. The tailed turns
/// must land above the imported ones.
///
/// The import path writes no ledger rows — there is no ordinal to write — so
/// an inferred per-session mark would start at 1 and every tailed turn would
/// collide with an imported one and be dropped in silence.
#[tokio::test]
async fn tailing_a_previously_imported_conversation_appends_above_the_import() {
    let guid = id('a');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    // The import: three turns, no ledger, no cursor, no ordinals.
    let imported = [
        imported_turn(1, "imported one"),
        imported_turn(2, "imported two"),
        imported_turn(3, "imported three"),
    ];
    assert_eq!(append(&pool, &guid, &imported).await, 3);

    // Now tail the live session for the first time.
    let tailed = [record("t1", "live turn")];
    let (accepted, deduped) = poll(
        &pool,
        Harness::Omp,
        SESSION,
        &guid,
        &tailed,
        &SourceCursor::Seq { seq: 1 },
    )
    .await;

    assert_eq!(
        accepted, 1,
        "the tailed turn is stored, not dropped onto an imported turn's number"
    );
    assert_eq!(deduped, 0);

    let numbers: Vec<i32> = sqlx::query_scalar(
        "SELECT turn_no FROM turns WHERE conversation_id = $1::uuid ORDER BY turn_no",
    )
    .bind(guid.as_str())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        numbers,
        vec![1, 2, 3, 4],
        "dense from 1, with the tailed turn at 4"
    );

    database.destroy(pool).await;
}

/// An ordinal's number is assigned once and never moves. A retried poll must
/// not renumber a turn that is already stored under its original number.
#[tokio::test]
async fn a_retried_poll_leaves_an_ordinals_number_where_it_was() {
    let guid = id('b');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    let cursor = SourceCursor::Seq { seq: 9 };
    commit_poll(
        &pool,
        Harness::PijLedger,
        SESSION,
        &guid,
        &cursor,
        &[("r1", 1), ("r2", 2)],
    )
    .await
    .unwrap();

    // The same ordinals offered again under different numbers, as a confused
    // retry would.
    commit_poll(
        &pool,
        Harness::PijLedger,
        SESSION,
        &guid,
        &cursor,
        &[("r1", 77), ("r2", 78)],
    )
    .await
    .expect("a retry must not error");

    let numbers: Vec<i32> = sqlx::query_scalar(
        "SELECT turn_no FROM ingest_ledger
          WHERE harness = $1 AND session_id = $2 ORDER BY ordinal",
    )
    .bind(Harness::PijLedger.as_str())
    .bind(SESSION)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        numbers,
        vec![1, 2],
        "the original numbers stand — a retry may not renumber stored turns"
    );

    database.destroy(pool).await;
}

/// A poll that found nothing still moved over the bytes it inspected, and
/// forgetting that is a full re-read next time.
#[tokio::test]
async fn an_empty_poll_still_advances_the_cursor() {
    let guid = id('c');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    commit_poll(
        &pool,
        Harness::Omp,
        SESSION,
        &guid,
        &SourceCursor::ByteOffset {
            device: 1,
            inode: 2,
            offset: 8_192,
        },
        &[],
    )
    .await
    .unwrap();

    let loaded = load_cursor(&pool, Harness::Omp, SESSION).await.unwrap();
    assert_eq!(
        loaded,
        Some(SourceCursor::ByteOffset {
            device: 1,
            inode: 2,
            offset: 8_192
        })
    );

    database.destroy(pool).await;
}

/// Forgetting a session takes its ledger with it, so a re-ingest is a clean
/// first read rather than a dedupe against turns that no longer exist.
#[tokio::test]
async fn forgetting_a_session_takes_its_ledger() {
    let guid = id('d');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    commit_poll(
        &pool,
        Harness::Omp,
        SESSION,
        &guid,
        &SourceCursor::Seq { seq: 3 },
        &[("r1", 1), ("r2", 2), ("r3", 3)],
    )
    .await
    .unwrap();

    let forgotten = forget_session(&pool, Harness::Omp, SESSION).await.unwrap();
    assert!(forgotten.existed);
    assert_eq!(forgotten.ledger_rows, 3);

    assert_eq!(
        load_cursor(&pool, Harness::Omp, SESSION).await.unwrap(),
        None
    );
    let view = ledger_view(&pool, Harness::Omp, SESSION, &guid, &["r1"])
        .await
        .unwrap();
    assert!(view.seen.is_empty(), "the ledger went with the cursor");

    database.destroy(pool).await;
}

/// Forgetting something that was never tailed is not an error, and says so.
#[tokio::test]
async fn forgetting_an_untailed_session_reports_nothing_reclaimed() {
    let guid = id('e');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    let forgotten = forget_session(&pool, Harness::Claude, "never-tailed")
        .await
        .unwrap();
    assert!(!forgotten.existed, "there was nothing to forget");
    assert_eq!(forgotten.ledger_rows, 0);

    database.destroy(pool).await;
}

/// A cursor into a conversation nobody stores any more would resume an ingest
/// that appends to nothing. The cascade is what keeps that impossible.
#[tokio::test]
async fn removing_a_conversation_forgets_how_to_resume_it() {
    let guid = id('f');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    commit_poll(
        &pool,
        Harness::Omp,
        SESSION,
        &guid,
        &SourceCursor::Seq { seq: 1 },
        &[("r1", 1)],
    )
    .await
    .unwrap();

    delete_conversation(&pool, &guid).await.unwrap();

    assert_eq!(
        load_cursor(&pool, Harness::Omp, SESSION).await.unwrap(),
        None,
        "the cursor went with the conversation"
    );

    let orphans: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingest_ledger WHERE harness = $1 AND session_id = $2",
    )
    .bind(Harness::Omp.as_str())
    .bind(SESSION)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(orphans, 0, "and so did its ledger");

    database.destroy(pool).await;
}

/// One Claude session is a main file plus N subagent sidecars, each cursored
/// separately (recipe gotcha 6). The composer needs to find them all.
#[tokio::test]
async fn every_session_tailed_for_a_conversation_is_listed() {
    let guid = id('a');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    for (harness, session) in [
        (Harness::Claude, "main"),
        (Harness::Claude, "sidecar-1"),
        (Harness::Omp, "elsewhere"),
    ] {
        commit_poll(
            &pool,
            harness,
            session,
            &guid,
            &SourceCursor::Seq { seq: 1 },
            &[],
        )
        .await
        .unwrap();
    }

    let sessions = sessions_for(&pool, &guid).await.unwrap();

    assert_eq!(
        sessions,
        vec![
            (Harness::Claude, "main".to_string()),
            (Harness::Claude, "sidecar-1".to_string()),
            (Harness::Omp, "elsewhere".to_string()),
        ]
    );

    database.destroy(pool).await;
}

/// The closed set of stores, enforced by the database rather than by everyone
/// remembering. A fifth store is a stop-and-ask, and this is where it stops.
#[tokio::test]
async fn an_unknown_harness_is_refused_by_the_database() {
    let guid = id('b');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    let failure = sqlx::query(
        "INSERT INTO ingest_cursors (harness, session_id, conversation_id, cursor)
         VALUES ('cursed', 'x', $1::uuid, '{}'::jsonb)",
    )
    .bind(guid.as_str())
    .execute(&pool)
    .await
    .expect_err("an unknown harness must not store");

    assert!(
        failure.to_string().contains("ingest_cursors_harness_known"),
        "the named constraint is what refuses it: {failure}"
    );

    database.destroy(pool).await;
}

/// The ledger is asked about a BATCH, not loaded whole: a long-running seat is
/// thousands of rows and a poll only cares about the handful it just read.
#[tokio::test]
async fn the_ledger_answers_only_about_the_ordinals_it_was_asked_about() {
    let guid = id('c');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    commit_poll(
        &pool,
        Harness::Omp,
        SESSION,
        &guid,
        &SourceCursor::Seq { seq: 3 },
        &[("r1", 1), ("r2", 2), ("r3", 3)],
    )
    .await
    .unwrap();

    let view = ledger_view(&pool, Harness::Omp, SESSION, &guid, &["r2", "unheard-of"])
        .await
        .unwrap();

    assert_eq!(
        view.seen,
        BTreeSet::from(["r2".to_string()]),
        "only the asked-about ordinals come back"
    );

    database.destroy(pool).await;
}

/// Two sessions of the same store must not dedupe against each other: an
/// ordinal is the store's natural id and means nothing outside the session
/// that minted it. Kept alongside
/// [`two_sessions_on_one_conversation_number_above_each_other`], which proves
/// the other half — that they DO share the numbering axis.
#[tokio::test]
async fn two_sessions_keep_separate_ledgers() {
    let guid = id('d');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    commit_poll(
        &pool,
        Harness::Claude,
        "session-one",
        &guid,
        &SourceCursor::Seq { seq: 1 },
        &[("shared-ordinal", 1)],
    )
    .await
    .unwrap();

    let view = ledger_view(
        &pool,
        Harness::Claude,
        "session-two",
        &guid,
        &["shared-ordinal"],
    )
    .await
    .unwrap();

    assert!(
        view.seen.is_empty(),
        "another session's ordinal is not this session's"
    );

    database.destroy(pool).await;
}

/// A session may not move conversations, and the refusal must leave the ledger
/// exactly as it was.
///
/// The silent version of this is the worst failure shape in the unit: the
/// ledger is keyed `(harness, session_id, ordinal)` and carries no
/// conversation, so a rebind strands its rows on the old conversation. The
/// newly named one then dedupes every record it is offered and stays
/// permanently empty, while the CLI, the ledger and `commit_poll` all report
/// success. Mutation-check: turn the refusal back into `DO UPDATE SET
/// conversation_id` and this fails on the error, then on the ledger.
#[tokio::test]
async fn a_session_may_not_be_rebound_to_another_conversation() {
    let first = id('a');
    let second = id('b');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &first).await;
    upsert_conversation(&pool, &conversation(&second))
        .await
        .expect("the second conversation should store");

    let records = [record("r1", "first"), record("r2", "second")];
    poll(
        &pool,
        Harness::Omp,
        SESSION,
        &first,
        &records,
        &SourceCursor::Seq { seq: 2 },
    )
    .await;

    // The same session, offered under a different conversation.
    let failure = commit_poll(
        &pool,
        Harness::Omp,
        SESSION,
        &second,
        &SourceCursor::Seq { seq: 3 },
        &[("r3", 3)],
    )
    .await
    .expect_err("a session may not move conversations");

    let message = failure.to_string();
    assert!(
        message.contains("already tails conversation"),
        "the refusal must say what happened: {message}"
    );

    // Nothing was written: not the cursor, not the ledger.
    let cursor = load_cursor(&pool, Harness::Omp, SESSION).await.unwrap();
    assert_eq!(
        cursor,
        Some(SourceCursor::Seq { seq: 2 }),
        "the cursor still points where the accepted poll left it"
    );

    let ordinals: Vec<String> = sqlx::query_scalar(
        "SELECT ordinal FROM ingest_ledger
          WHERE harness = $1 AND session_id = $2 ORDER BY ordinal",
    )
    .bind(Harness::Omp.as_str())
    .bind(SESSION)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        ordinals,
        vec!["r1".to_string(), "r2".to_string()],
        "the refused poll's ordinal was not recorded"
    );

    let stranded: i64 =
        sqlx::query_scalar("SELECT count(*) FROM turns WHERE conversation_id = $1::uuid")
            .bind(second.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stranded, 0, "and the second conversation was never touched");

    database.destroy(pool).await;
}

/// Resolution is a LOOKUP, not a mint — and this is the read that makes it
/// one. No row means a first ingest, so the caller mints exactly once.
#[tokio::test]
async fn an_untailed_session_belongs_to_no_conversation_yet() {
    let guid = id('c');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    assert_eq!(
        conversation_for(&pool, Harness::Claude, "never-tailed")
            .await
            .unwrap(),
        None,
        "absent means first ingest, which is the only time a caller may mint"
    );

    database.destroy(pool).await;
}

/// The lookup and the guard must read the SAME row, not two rows that agree by
/// luck: whatever `commit_poll` committed under is what `conversation_for`
/// answers, so a composition root that looks up before minting cannot produce
/// the rebind `SessionRebound` exists to refuse.
#[tokio::test]
async fn the_lookup_answers_with_the_conversation_the_poll_committed_under() {
    let guid = id('d');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    let records = [record("r1", "first")];
    poll(
        &pool,
        Harness::Omp,
        SESSION,
        &guid,
        &records,
        &SourceCursor::Seq { seq: 1 },
    )
    .await;

    let resolved = conversation_for(&pool, Harness::Omp, SESSION)
        .await
        .unwrap()
        .expect("a tailed session belongs to a conversation");

    assert_eq!(
        resolved, guid,
        "the lookup reads the row the poll wrote, so resolution and the rebind \
         guard cannot disagree about which conversation this session is"
    );

    // And the guard agrees: offering the session under anything else is refused.
    let other = id('e');
    upsert_conversation(&pool, &conversation(&other))
        .await
        .unwrap();
    commit_poll(
        &pool,
        Harness::Omp,
        SESSION,
        &other,
        &SourceCursor::Seq { seq: 2 },
        &[],
    )
    .await
    .expect_err("the guard refuses what the lookup would have prevented");

    database.destroy(pool).await;
}
