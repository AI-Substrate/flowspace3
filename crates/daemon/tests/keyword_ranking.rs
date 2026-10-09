//! Keyword hits rank by how well they match, not a flat 1.0 (brief 2026-10-09).
//!
//! Seen before this: every lexical hit scored 1.0, so a turn that merely
//! quoted the question tied with the answer and won on recency, and a
//! multi-word query matched only where its words sat together. All fixtures
//! here are synthetic.

mod support;

use std::sync::Arc;

use fs3_core::{
    Config, DatabaseConfig, Element, ElementKind, Span, Turn, TurnItem, TurnRole, TurnSource,
};
use fs3_daemon::conversations::{IntakeRequest, intake};
use fs3_daemon::scope::Scope;
use fs3_daemon::search::{SearchRequest, search};
use fs3_daemon::wiring::AppState;
use fs3_testkit::fakes::FakeEmbedder;

const GUID: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430d1";

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

fn turn(turn_no: u32, role: TurnRole, body: &str) -> Turn {
    Turn {
        turn_no,
        role,
        source: TurnSource::Human,
        head_sha: None,
        at: "2026-10-09T09:00:00Z".to_string(),
        body: body.to_string(),
        items: Vec::<TurnItem>::new(),
    }
}

async fn store(state: &AppState, turns: Vec<Turn>) {
    intake(
        state,
        IntakeRequest {
            guid: GUID.to_string(),
            repo_identity: Some("git:github.com/fs3/ranking".to_string()),
            worktree: Some("/srv/ranking".to_string()),
            base_sha: None,
            title: Some("ranking fixtures".to_string()),
            started_at: "2026-10-09T09:00:00Z".to_string(),
            turns,
        },
    )
    .await
    .expect("intake accepts the batch");
    fs3_daemon::drain(state, 1).await;
}

fn ask(query: &str, source: Option<&str>) -> SearchRequest {
    SearchRequest {
        q: query.to_string(),
        source: source.map(str::to_string),
        limit: Some(20),
        ..SearchRequest::default()
    }
}

fn rank_of(results: &[fs3_core::views::search::Hit], turn_no: u32) -> Option<usize> {
    let address = format!("conv:{GUID}#t{turn_no}");
    results.iter().position(|hit| hit.address == address)
}

/// "haiku tps" used to match only where the two words sat together.
#[tokio::test]
async fn scattered_terms_match_and_the_phrase_ranks_above_them() {
    let (database, state) = stack("kw_scattered").await;
    store(
        &state,
        vec![
            turn(
                1,
                TurnRole::Agent,
                "The haiku model is quick: we measured 180 tps on the board.",
            ),
            turn(
                2,
                TurnRole::Agent,
                "Headline number: haiku tps was the fastest of the lot.",
            ),
            turn(3, TurnRole::Agent, "Nothing about either word in this one."),
        ],
    )
    .await;

    let outcome = search(
        &state,
        &ask("haiku tps", Some("conversation")),
        &Scope::unscoped(),
    )
    .await
    .expect("search answers");
    let scattered = rank_of(&outcome.results, 1).expect("scattered terms now match");
    let phrase = rank_of(&outcome.results, 2).expect("the phrase matches");
    assert!(
        phrase < scattered,
        "the whole phrase outranks scattered terms"
    );
    assert_eq!(outcome.results[scattered].match_field, "terms");
    assert!(outcome.results[phrase].score > outcome.results[scattered].score);
    assert!(
        outcome.results.iter().all(|hit| hit.score < 1.0),
        "turns are graded, never a flat 1.0"
    );

    database.destroy(state.db.clone()).await;
}

/// A person asking the question again must not outrank the turn that answers it.
#[tokio::test]
async fn the_answer_outranks_a_restated_question() {
    let (database, state) = stack("kw_restated").await;
    let answer = format!(
        "{} The comparison: haiku was cheapest per token, and on pricing the opus tier cost \
         five times more. I compared them across 21 models and built a board.",
        "Done. ".repeat(10)
    );
    store(
        &state,
        vec![
            turn(1, TurnRole::Human, "which agent compared haiku pricing"),
            turn(2, TurnRole::Agent, &answer),
        ],
    )
    .await;

    let outcome = search(
        &state,
        &ask("compared haiku pricing", Some("conversation")),
        &Scope::unscoped(),
    )
    .await
    .expect("search answers");
    let question = rank_of(&outcome.results, 1).expect("the question is still findable");
    let answered = rank_of(&outcome.results, 2).expect("the answer is found by its terms");
    assert!(answered < question, "{:#?}", outcome.results);
    assert!(
        outcome.results[question].score < 0.5,
        "a restated question is damped below every genuine match: {}",
        outcome.results[question].score
    );

    database.destroy(state.db.clone()).await;
}

/// Verbatim identity in code keeps its pinned 1.0: an identifier still hits first.
#[tokio::test]
async fn an_identifier_still_ranks_first() {
    let (database, state) = stack("kw_identifier").await;
    let function = Element::new(
        ElementKind::Function,
        "function_item",
        "embed_tokens_per_minute",
        "src/config.rs::embed_tokens_per_minute",
        Span::new(3, 5),
        "fn embed_tokens_per_minute() -> u64 { 4_000_000 }",
    );
    let root = Element::new(
        ElementKind::File,
        "rust",
        "src/config.rs",
        "src/config.rs",
        Span::new(1, 6),
        function.raw_text.clone(),
    )
    .with_children(vec![function.clone()]);
    let blob = fs3_core::BlobRef::new("b".repeat(40)).unwrap();
    fs3_store::upsert_element_tree(&state.db, &blob, "test@1", &root, |_| true)
        .await
        .unwrap();
    store(
        &state,
        vec![turn(
            1,
            TurnRole::Agent,
            "I set embed_tokens_per_minute to four million in the config.",
        )],
    )
    .await;

    let outcome = search(
        &state,
        &ask("embed_tokens_per_minute", None),
        &Scope::unscoped(),
    )
    .await
    .expect("search answers");
    let first = &outcome.results[0];
    assert_eq!(
        first.name, "embed_tokens_per_minute",
        "{:#?}",
        outcome.results
    );
    assert!((first.score - 1.0).abs() < f64::EPSILON);
    assert!(rank_of(&outcome.results, 1).is_some_and(|rank| rank > 0));

    database.destroy(state.db.clone()).await;
}

/// `fs3 "…"` typed as a whole prompt is a search, hidden like any other echo.
#[tokio::test]
async fn an_fs3_prompt_is_hidden_as_retrieval() {
    let (database, state) = stack("kw_fs3_prompt").await;
    store(
        &state,
        vec![
            turn(1, TurnRole::Human, "fs3 \"zebra quokka manifold\""),
            turn(
                2,
                TurnRole::Agent,
                "Findings on the zebra quokka manifold: it is a fixture.",
            ),
        ],
    )
    .await;

    let hidden = search(
        &state,
        &ask("zebra quokka manifold", Some("conversation")),
        &Scope::unscoped(),
    )
    .await
    .expect("search answers");
    assert!(
        rank_of(&hidden.results, 1).is_none(),
        "{:#?}",
        hidden.results
    );
    assert!(rank_of(&hidden.results, 2).is_some());
    assert!(hidden.filtered.retrieval_turns >= 1);

    let shown = search(
        &state,
        &SearchRequest {
            include_retrieval: true,
            ..ask("zebra quokka manifold", Some("conversation"))
        },
        &Scope::unscoped(),
    )
    .await
    .expect("search answers");
    assert!(rank_of(&shown.results, 1).is_some());

    database.destroy(state.db.clone()).await;
}
