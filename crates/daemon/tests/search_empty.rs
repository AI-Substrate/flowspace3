//! An empty search result is three different facts wearing one face.
//!
//! "Nothing is indexed yet", "there IS an index but this model cannot see it",
//! and "genuinely no match" are indistinguishable to a caller who is handed an
//! empty list — and only the last one is an answer. The middle case is the
//! dangerous one: vectors are keyed by `model_key`, so changing embedder or
//! width leaves an intact index unreachable without deleting a row, and
//! reporting that as "no results" is a confident lie about the user's own code.
//!
//! (Prompted by pij-devoted-cattle's observation that an empty result is the
//! most misread signal in this system — three unrelated subsystems, one shape.)

mod support;

use std::sync::Arc;

use fs3_core::{Config, DatabaseConfig};
use fs3_daemon::search::{SearchRequest, search};
use fs3_daemon::wiring::AppState;
use fs3_store::{NewEmbedding, SourceKind};
use fs3_testkit::fakes::FakeEmbedder;

async fn stack(label: &str) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    state.embedder = Arc::new(FakeEmbedder {
        dimensions: fs3_store::EMBEDDING_DIMENSIONS,
        ..FakeEmbedder::default()
    });
    (database, state)
}

fn ask(text: &str) -> SearchRequest {
    SearchRequest {
        q: text.to_string(),
        ..SearchRequest::default()
    }
}

/// Store one vector under a model key nothing will search for.
async fn seed_under(state: &AppState, model_key: &str) {
    let vector = vec![0.1f32; fs3_store::EMBEDDING_DIMENSIONS];
    fs3_store::put_embeddings(
        &state.db,
        model_key,
        &[NewEmbedding {
            source_hash: &"a".repeat(64),
            source_kind: SourceKind::Raw,
            vector: &vector,
        }],
    )
    .await
    .expect("seeds");
}

/// With nothing indexed at all, searching must SAY so rather than answering
/// "no results" — which reads as a fact about the user's code.
#[tokio::test]
async fn an_unindexed_store_says_so_instead_of_answering_nothing() {
    let (database, state) = stack("empty_no_index").await;

    let failure = search(&state, &ask("parser"))
        .await
        .expect_err("an empty store is not an answer");

    assert_eq!(failure.code, "FS3-E-QUERY-NO-INDEX");
    assert!(
        !failure.fix.is_empty(),
        "and it must say what to do about it"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// The dangerous case: an index exists, built by a different embedder, and is
/// invisible to the active one.
///
/// Nothing about "no results" would ever have told the user that changing
/// provider or vector width was the cause, and the index is sitting right
/// there, whole.
#[tokio::test]
async fn an_index_under_another_model_is_named_rather_than_hidden() {
    let (database, state) = stack("empty_other_model").await;
    seed_under(&state, "text-embedding-3-small@1024").await;

    let failure = search(&state, &ask("parser"))
        .await
        .expect_err("an unreachable index is not an empty answer");

    assert_eq!(failure.code, "FS3-E-QUERY-NO-INDEX");
    assert!(
        failure.message.contains("text-embedding-3-small@1024"),
        "the model that DID build the index must be named: {}",
        failure.message
    );
    assert!(
        failure.fix.contains("add") || failure.fix.contains("select"),
        "and the fix must offer both ways out: {}",
        failure.fix
    );
    assert!(
        failure.details.contains_key("stored_models"),
        "with the facts structured, not only prose"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}

/// When the active model DOES have an index, zero hits is a real answer and
/// must stay one. Dressing it up as an error would be its own lie.
#[tokio::test]
async fn a_genuine_no_match_stays_an_empty_answer() {
    let (database, state) = stack("empty_real_answer").await;
    let model_key = state.embedder_key("git:whatever");
    seed_under(&state, &model_key).await;

    let results = search(&state, &ask("something absent"))
        .await
        .expect("an indexed store answers");
    assert!(
        results.results.is_empty(),
        "no match is a legitimate empty result"
    );

    let pool = state.db.clone();
    database.destroy(pool).await;
}
