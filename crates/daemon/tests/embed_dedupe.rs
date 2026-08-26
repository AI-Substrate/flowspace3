//! The embed handler must not re-pay for vectors it already has.
//!
//! Content-addressed work is re-emitted ON PURPOSE — a crash between parse and
//! enrichment must not strand elements with no job pointing at them — so the
//! same embed job legitimately runs more than once. Re-emission being correct
//! is not the same as re-execution being free, and until 2026-08-26 it was not:
//! a live run measured 2.9x, 10,559 executions over 3,646 distinct jobs, every
//! repeat re-paying an API bill for vectors already in the table.
//!
//! These tests count PROVIDER CALLS, not rows. A test that asserted on stored
//! vectors would have passed throughout the leak, because the vectors were
//! always correct — they were just bought twice.

mod support;

use std::sync::Arc;

use fs3_core::{Config, DatabaseConfig};
use fs3_daemon::enrich;
use fs3_daemon::wiring::AppState;
use fs3_store::SourceKind;
use fs3_testkit::fakes::FakeEmbedder;
use serde_json::json;

const IDENTITY: &str = "git:github.com/fs3/probe";

/// A stack whose embedder is a fake we keep a handle on, so calls are countable.
async fn stack(label: &str) -> (support::FreshDatabase, AppState, Arc<FakeEmbedder>) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");

    // The composition root builds the fake at the STORE's width; keep that,
    // or the vectors will not fit the column and the test fails for a reason
    // that has nothing to do with what it is testing.
    let embedder = Arc::new(FakeEmbedder {
        dimensions: fs3_store::EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::default()
    });
    state.embedder = embedder.clone();
    (database, state, embedder)
}

/// `(hash, text)` pairs shaped like the real thing — a content hash is 64 hex
/// characters and the store's key column is not a free-text field.
fn items(range: std::ops::Range<u32>) -> Vec<(String, String)> {
    range
        .map(|n| (format!("{n:064x}"), format!("fn f{n}() {{}}")))
        .collect()
}

fn payload(items: &[(String, String)]) -> serde_json::Value {
    json!({ "identity": IDENTITY, "source": "raw", "items": items })
}

/// A job whose vectors are ALL already stored must make ZERO provider calls.
///
/// This is the whole saving. An empty batch that still made the round trip
/// would have fixed the accounting and not the bill.
#[tokio::test]
async fn a_fully_stored_batch_costs_nothing() {
    let (database, state, embedder) = stack("embed_dedupe_zero").await;
    let batch = items(0..3);

    enrich::embed(&state, payload(&batch))
        .await
        .expect("first run embeds");
    assert_eq!(
        embedder.call_count(),
        1,
        "the first run must actually embed"
    );
    let first_batch = embedder.calls.lock().expect("lock")[0].len();
    assert_eq!(first_batch, 3, "all three texts in one call");

    // The re-emission. Same job, same content, nothing changed.
    enrich::embed(&state, payload(&batch))
        .await
        .expect("the repeat succeeds");
    assert_eq!(
        embedder.call_count(),
        1,
        "a re-emitted job whose vectors are all stored must not call the \
         provider at all — it succeeded, it simply cost nothing"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// The half a zero-call test cannot see: a PARTIALLY stored batch must embed
/// exactly the missing items, and no others.
///
/// A filter that returned the wrong subset — everything, nothing, or the
/// complement — would still satisfy "an already-stored batch makes zero calls".
/// This is the assertion that pins WHICH items were bought.
#[tokio::test]
async fn a_partial_batch_embeds_only_what_is_missing() {
    let (database, state, embedder) = stack("embed_dedupe_partial").await;

    let first = items(0..3);
    enrich::embed(&state, payload(&first))
        .await
        .expect("first run");

    // Three stored, two new, interleaved so a filter that merely truncates or
    // takes a prefix cannot pass.
    let mut mixed = items(0..3);
    mixed.push(items(7..8).remove(0));
    mixed.push(items(8..9).remove(0));
    mixed.rotate_right(1);

    enrich::embed(&state, payload(&mixed))
        .await
        .expect("second run");

    assert_eq!(embedder.call_count(), 2, "one further call, not none");
    // Scoped so the guard is dropped before the next await — a std Mutex held
    // across an await point is a deadlock waiting for a scheduler that moves
    // the task.
    let mut bought = {
        let calls = embedder.calls.lock().expect("lock");
        calls[1].clone()
    };
    bought.sort();
    assert_eq!(
        bought,
        vec!["fn f7() {}".to_string(), "fn f8() {}".to_string()],
        "exactly the two missing texts — not the whole batch, not a prefix"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// `raw` and `smart` are different vectors for the same hash, and the filter
/// keys on all three primary-key columns.
///
/// Filtering on hash + model alone would treat a stored `raw` vector as
/// covering the `smart` one. That silently under-embeds and leaves a
/// permanently incomplete index that looks exactly like a working one — the
/// same undetectable failure class as a misaligned batch.
#[tokio::test]
async fn a_raw_vector_does_not_satisfy_the_smart_vector_for_the_same_hash() {
    let (database, state, embedder) = stack("embed_dedupe_kind").await;
    let batch = items(0..2);

    enrich::embed(&state, payload(&batch)).await.expect("raw");
    assert_eq!(embedder.call_count(), 1);

    let smart = json!({ "identity": IDENTITY, "source": "smart", "items": batch });
    enrich::embed(&state, smart).await.expect("smart");
    assert_eq!(
        embedder.call_count(),
        2,
        "the smart vectors are different text and different meaning; a stored \
         raw vector for the same hash must not suppress them"
    );

    let stored = fs3_store::existing_embedding_hashes(
        &state.db,
        &state.embedder_key(IDENTITY),
        SourceKind::Smart,
        &[batch[0].0.as_str()],
    )
    .await
    .expect("lookup");
    assert_eq!(stored.len(), 1, "and the smart vector really landed");

    let pool = state.db.clone();
    database.destroy(pool).await;
}
