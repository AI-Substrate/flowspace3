//! Cursors and the ordinal ledger, against a real Postgres.
//!
//! The load-bearing test in this file is
//! [`a_rescan_after_rotation_appends_nothing_through_the_store`]. Everything
//! else is ordinary round-tripping; that one is the whole reason the ledger
//! exists. A reader whose file rotated restarts from zero and hands back the
//! WHOLE conversation with `rescanned = true`, and if the ledger cannot
//! recognise those records the conversation is stored a second time — silently,
//! because a duplicated conversation looks exactly like a busy one.
//!
//! It is mutation-checked at the seam it guards: drop the `seen` lookup from
//! `fs3_core::prepare_batch` and this fails on the turn count, not on a
//! detail.

mod support;

use std::collections::BTreeSet;

use fs3_core::{Conversation, ConversationId, Harness, SourceCursor, prepare_batch};
use fs3_store::ingest_cursors::{commit_poll, forget_session, ledger_view, load_cursor,
    sessions_for};
use fs3_store::{PgPool, delete_conversation, upsert_conversation};
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
    }
}

fn record(ordinal: &str, body: &str) -> fs3_core::RawRecord {
    fs3_core::RawRecord {
        ordinal: ordinal.to_string(),
        parent_ordinal: None,
        at: "2026-08-28T09:00:00Z".to_string(),
        role: fs3_core::TurnRole::Agent,
        source: fs3_core::TurnSource::System,
        body: body.to_string(),
        items: Vec::new(),
        head_sha: None,
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

/// A cursor that only lives inside one process makes the SECOND ingest a full
/// re-read, which is the entire cost model of this plan. All three variants,
/// because each store resumes in different terms and a serialisation that
/// works for one proves nothing about the others.
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
        load_cursor(&pool, Harness::Claude, "never-read").await.unwrap(),
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

    // Poll one: nothing is known, so everything is new.
    let view = ledger_view(&pool, Harness::Omp, SESSION, &["r1", "r2", "r3"])
        .await
        .unwrap();
    assert_eq!(view.next_turn_no, 1, "an untouched session starts at 1");

    let first = prepare_batch(&whole, &view.seen, view.next_turn_no);
    assert_eq!(first.turns.len(), 3);
    commit_poll(
        &pool,
        Harness::Omp,
        SESSION,
        &guid,
        &SourceCursor::ByteOffset {
            device: 1,
            inode: 2,
            offset: 300,
        },
        &first.ledger,
    )
    .await
    .unwrap();

    // Poll two: the file rotated. The reader restarts from zero and hands back
    // everything it can see, with a NEW inode.
    let view = ledger_view(&pool, Harness::Omp, SESSION, &["r1", "r2", "r3"])
        .await
        .unwrap();
    assert_eq!(view.seen.len(), 3, "the ledger remembers all three");
    assert_eq!(view.next_turn_no, 4);

    let rescan = prepare_batch(&whole, &view.seen, view.next_turn_no);

    assert!(
        rescan.turns.is_empty(),
        "a rescan of an unchanged conversation must append ZERO turns — storing \
         them again duplicates the whole conversation and looks like a busy session"
    );
    assert_eq!(rescan.deduped, 3);

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
    let view = ledger_view(&pool, Harness::Claude, SESSION, &["r1", "r2"])
        .await
        .unwrap();
    let prepared = prepare_batch(&first_pass, &view.seen, view.next_turn_no);
    commit_poll(
        &pool,
        Harness::Claude,
        SESSION,
        &guid,
        &SourceCursor::Seq { seq: 2 },
        &prepared.ledger,
    )
    .await
    .unwrap();

    let after_rotation = [
        record("r1", "first"),
        record("r2", "second"),
        record("r3", "third"),
    ];
    let view = ledger_view(&pool, Harness::Claude, SESSION, &["r1", "r2", "r3"])
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

/// An ordinal's number is assigned once and never moves. A retried poll must
/// not renumber a turn that is already stored under its original number.
#[tokio::test]
async fn a_retried_poll_leaves_an_ordinals_number_where_it_was() {
    let guid = id('f');
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

    let view = ledger_view(&pool, Harness::PijLedger, SESSION, &["r1", "r2"])
        .await
        .unwrap();
    assert_eq!(
        view.next_turn_no, 3,
        "the original numbers stand — a retry may not renumber stored turns"
    );

    database.destroy(pool).await;
}

/// A poll that found nothing still moved over the bytes it inspected, and
/// forgetting that is a full re-read next time.
#[tokio::test]
async fn an_empty_poll_still_advances_the_cursor() {
    let guid = id('a');
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
    let guid = id('b');
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

    assert_eq!(load_cursor(&pool, Harness::Omp, SESSION).await.unwrap(), None);
    let view = ledger_view(&pool, Harness::Omp, SESSION, &["r1"])
        .await
        .unwrap();
    assert!(view.seen.is_empty(), "the ledger went with the cursor");
    assert_eq!(view.next_turn_no, 1, "and a re-ingest starts over");

    database.destroy(pool).await;
}

/// Forgetting something that was never tailed is not an error, and says so.
#[tokio::test]
async fn forgetting_an_untailed_session_reports_nothing_reclaimed() {
    let guid = id('c');
    let database = FreshDatabase::create().await;
    let pool = seeded(&database, &guid).await;

    let forgotten = forget_session(&pool, Harness::Claude, "never-tailed")
        .await
        .unwrap();
    assert_eq!(forgotten.existed, false);
    assert_eq!(forgotten.ledger_rows, 0);

    database.destroy(pool).await;
}

/// A cursor into a conversation nobody stores any more would resume an ingest
/// that appends to nothing. The cascade is what keeps that impossible.
#[tokio::test]
async fn removing_a_conversation_forgets_how_to_resume_it() {
    let guid = id('d');
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
    let view = ledger_view(&pool, Harness::Omp, SESSION, &["r1"])
        .await
        .unwrap();
    assert!(view.seen.is_empty(), "and so did its ledger");

    database.destroy(pool).await;
}

/// One Claude session is a main file plus N subagent sidecars, each cursored
/// separately (recipe gotcha 6). The composer needs to find them all.
#[tokio::test]
async fn every_session_tailed_for_a_conversation_is_listed() {
    let guid = id('e');
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
    let guid = id('f');
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
    let guid = id('a');
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

    let view = ledger_view(&pool, Harness::Omp, SESSION, &["r2", "unheard-of"])
        .await
        .unwrap();

    assert_eq!(
        view.seen,
        BTreeSet::from(["r2".to_string()]),
        "only the asked-about ordinals come back"
    );
    assert_eq!(view.next_turn_no, 4, "but the high-water mark is the whole session");

    database.destroy(pool).await;
}

/// Two sessions of the same store must not dedupe against each other: the
/// ordinal namespace is per conversation, and `uuid` collisions across two
/// claude sessions are ordinary.
#[tokio::test]
async fn two_sessions_keep_separate_ledgers() {
    let guid = id('b');
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

    let view = ledger_view(&pool, Harness::Claude, "session-two", &["shared-ordinal"])
        .await
        .unwrap();

    assert!(
        view.seen.is_empty(),
        "another session's ordinal is not this session's"
    );
    assert_eq!(view.next_turn_no, 1);

    database.destroy(pool).await;
}
