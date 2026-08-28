//! The embed handler must not re-pay for vectors it already has, and must not
//! pay at all for content nothing holds.
//!
//! Content-addressed work is re-emitted ON PURPOSE — a crash between parse and
//! enrichment must not strand elements with no job pointing at them — so the
//! same embed job legitimately runs more than once. Re-emission being correct
//! is not the same as re-execution being free, and until 2026-08-26 it was not:
//! a live run measured 2.9x, 10,559 executions over 3,646 distinct jobs, every
//! repeat re-paying an API bill for vectors already in the table.
//!
//! The second guard is the reference one (req-0057), which `summarize` had from
//! the start and this handler did not: a job for content no registered root
//! maps any more must not reach the provider. The dedupe filter LOOKS like it
//! covers that and does not — it asks whether a text has already been bought,
//! never whether it is still worth buying, so a NEW hash for dead content sails
//! through. Measured cost of the gap: 4,436 raw vectors bought for a gitignored
//! tree the watcher should never have indexed.
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
use support::{hold, items};

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
    hold(&state, "zero", &batch).await;

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

    // Everything either run will offer, held up front: this test is about the
    // dedupe filter, so nothing here may be dropped by the reference guard.
    let mut all = items(0..3);
    all.extend(items(7..9));
    hold(&state, "partial", &all).await;

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
    hold(&state, "kind", &batch).await;

    // A summary whose TEXT is the item's text, so its `text_hash` is the same
    // hash the smart batch carries — the shape the guard's smart leg reads,
    // and the shape `summarize` really produces when it enqueues its own
    // vector.
    for (_, text) in &batch {
        fs3_store::put_smart_content(
            &state.db,
            &fs3_core::content_hash(text.as_bytes()),
            &state.summarizer_key(IDENTITY),
            &fs3_core::Summary {
                text: text.clone(),
                tags: vec!["held".to_string()],
                extras: std::collections::BTreeMap::new(),
            },
        )
        .await
        .expect("storing a summary");
    }

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

/// The reference guard: a batch of content no registered root holds must never
/// reach the provider.
///
/// Nothing is held here at all, which is what a job outliving its root looks
/// like from the handler's side — and what the watcher produced 4,436 times by
/// indexing a gitignored tree. The dedupe filter cannot save this: every hash
/// is new, so it passes them all through.
#[tokio::test]
async fn a_batch_nothing_references_never_reaches_the_provider() {
    let (database, state, embedder) = stack("embed_guard_unheld").await;

    enrich::embed(&state, payload(&items(0..3)))
        .await
        .expect("the job completes — it simply cost nothing");

    assert_eq!(
        embedder.call_count(),
        0,
        "unreferenced content must be dropped BEFORE the provider call, not \
         embedded and then collected"
    );
    assert_eq!(
        fs3_store::existing_embedding_hashes(
            &state.db,
            &state.embedder_key(IDENTITY),
            SourceKind::Raw,
            &items(0..3)
                .iter()
                .map(|(hash, _)| hash.clone())
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )
        .await
        .expect("lookup")
        .len(),
        0,
        "and nothing was stored for it either"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// The half the zero-call test cannot see: a MIXED batch must embed exactly the
/// held items.
///
/// A guard that dropped everything would satisfy the test above; a guard that
/// dropped nothing would satisfy a test that only counted calls. This one pins
/// WHICH texts were bought, so both failures are visible.
#[tokio::test]
async fn a_mixed_batch_embeds_only_the_items_something_still_holds() {
    let (database, state, embedder) = stack("embed_guard_mixed").await;

    let held = items(0..2);
    hold(&state, "mixed", &held).await;

    // Held and unheld interleaved, so a guard that takes a prefix or a suffix
    // cannot pass.
    let mut batch = items(5..7);
    batch.insert(1, held[0].clone());
    batch.push(held[1].clone());

    enrich::embed(&state, payload(&batch))
        .await
        .expect("the job completes");

    assert_eq!(embedder.call_count(), 1, "one call, for the survivors");
    let mut bought = {
        let calls = embedder.calls.lock().expect("lock");
        calls[0].clone()
    };
    bought.sort();
    assert_eq!(
        bought,
        vec!["fn f0() {}".to_string(), "fn f1() {}".to_string()],
        "exactly the held texts — the unheld ones are not in the call at all"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}
