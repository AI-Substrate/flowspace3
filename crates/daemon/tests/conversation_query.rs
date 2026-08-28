//! The conversation query surface: search, window, outline, list, remove.
//!
//! The test that matters most here is
//! [`a_hit_on_text_shared_with_code_resolves_to_the_turn`]. Enrichment is keyed
//! by content, so a turn quoting a line of code and the code itself are ONE
//! raw hash with two element rows — which is the saving working. The resolver
//! that turns a vector hit back into an element takes the lowest-id row
//! carrying that hash, so without the kind predicate bound on BOTH sides of the
//! query, a conversation search resolves to the CODE twin and prints an `el:`
//! address for a turn. Nothing errors; the answer is just quietly the wrong
//! thing.

mod support;

use fs3_core::views::read::GetPayload;
use fs3_core::{Config, DatabaseConfig, Turn, TurnRole, TurnSource};
use fs3_daemon::conversations::{IntakeRequest, intake};
use fs3_daemon::read::{GetRequest, TreeRequest};
use fs3_daemon::scope::Scope;
use fs3_daemon::search::{SearchRequest, search};
use fs3_daemon::wiring::AppState;

const GUID: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
const OTHER: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c9";
const ANCHOR: &str = "git:github.com/fs3/anchored";

async fn stack(label: &str) -> (support::FreshDatabase, AppState) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let state = AppState::from_config(config).expect("the fake stack wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    (database, state)
}

fn turn(turn_no: u32, body: &str) -> Turn {
    Turn {
        turn_no,
        role: if turn_no % 2 == 1 {
            TurnRole::Human
        } else {
            TurnRole::Agent
        },
        source: TurnSource::Peer,
        head_sha: None,
        at: "2026-08-27T09:00:00Z".to_string(),
        body: body.to_string(),
        items: Vec::new(),
    }
}

async fn store(state: &AppState, guid: &str, turns: Vec<Turn>) {
    intake(
        state,
        IntakeRequest {
            guid: guid.to_string(),
            repo_identity: Some(ANCHOR.to_string()),
            worktree: Some("/srv/anchored".to_string()),
            base_sha: None,
            title: Some("a fleet session".to_string()),
            started_at: "2026-08-27T09:00:00Z".to_string(),
            turns,
        },
    )
    .await
    .expect("intake accepts the batch");
}

/// Drain the queue so the turns actually have vectors to be found by.
async fn drain(state: &AppState) {
    fs3_daemon::drain(state, 1).await;
}

/// A scope standing in one repository, the way `--repo` resolves.
fn scoped(identity: &str) -> Scope {
    Scope {
        repo: Some(identity.to_string()),
        ..Scope::unscoped()
    }
}

fn ask(query: &str, source: Option<&str>) -> SearchRequest {
    SearchRequest {
        q: query.to_string(),
        source: source.map(str::to_string),
        limit: Some(20),
        ..SearchRequest::default()
    }
}

/// `--source conversation` returns turns, and the default returns code — the
/// two corpora never blend by accident (ac-0004).
#[tokio::test]
async fn conversations_answer_only_when_asked_for() {
    let (database, state) = stack("conv-query-scope").await;
    store(
        &state,
        GUID,
        vec![turn(1, "the anchor is a pointer, not ownership")],
    )
    .await;
    drain(&state).await;

    let hits = search(
        &state,
        &ask("anchor ownership", Some("conversation")),
        &Scope::unscoped(),
    )
    .await
    .expect("a conversation search");
    assert_eq!(hits.results.len(), 1);
    assert_eq!(hits.results[0].kind, "turn");
    assert_eq!(
        hits.results[0].address,
        format!("conv:{GUID}#t1"),
        "a turn hit carries a conv: address, not an el: one"
    );

    // The default is code, and there is no code in this database — so the
    // honest answer is "nothing", not "here is a conversation".
    let code = search(&state, &ask("anchor ownership", None), &Scope::unscoped()).await;
    assert!(
        code.map(|hits| hits.results.is_empty()).unwrap_or(true),
        "conversations are opt-in: opinions at a point in time must not be \
         blended into answers about current truth"
    );

    database.destroy(state.db).await;
}

/// **Critic finding 4.** A turn whose text is shared with code has one raw hash
/// and two element rows; the resolver takes the lowest id. Without the kind
/// predicate bound in the resolution join as well as the candidate CTE, this
/// search resolves to the code element and prints an `el:` address for a turn.
#[tokio::test]
async fn a_hit_on_text_shared_with_code_resolves_to_the_turn() {
    let (database, state) = stack("conv-query-dedupe").await;

    // The CODE first, so its element row has the LOWER id — the exact ordering
    // that makes a resolver without a kind predicate pick the wrong row.
    let shared = "fn collect_garbage(pool: &PgPool) -> Result<Reclaimed, StoreError>";
    let blob = fs3_core::BlobRef::new("c".repeat(40)).expect("a blob key");
    let root = "/srv/code";
    let identity = fs3_core::RepoIdentity::from_path(std::path::Path::new(root));
    let worktree = fs3_store::register_worktree(&state.db, &identity, root, Some("main"))
        .await
        .expect("registering");
    fs3_store::sync_worktree_files(
        &state.db,
        worktree,
        &[("src/gc.rs".to_string(), blob.clone())],
    )
    .await
    .expect("mapping");
    fs3_store::upsert_element_tree(
        &state.db,
        &blob,
        "test-parser@1",
        &fs3_core::Element::new(
            fs3_core::ElementKind::Function,
            "function_item",
            "collect_garbage",
            "src/gc.rs::collect_garbage",
            fs3_core::Span::new(1, 1),
            shared,
        ),
        |_| false,
    )
    .await
    .expect("storing the code");

    // Then a turn that quotes it verbatim — one raw hash, two element rows.
    store(&state, GUID, vec![turn(1, shared)]).await;
    drain(&state).await;

    let hits = search(
        &state,
        &ask("collect garbage", Some("conversation")),
        &Scope::unscoped(),
    )
    .await
    .expect("a conversation search");

    assert_eq!(hits.results.len(), 1, "the shared text is found once");
    assert_eq!(
        hits.results[0].kind, "turn",
        "and it resolves to the TURN, not to the code element sharing its hash"
    );
    assert_eq!(hits.results[0].address, format!("conv:{GUID}#t1"));

    // And the mirror: a code search on the same text resolves to the code.
    let code = search(&state, &ask("collect garbage", None), &Scope::unscoped())
        .await
        .expect("a code search");
    assert_eq!(code.results.len(), 1);
    assert_eq!(code.results[0].kind, "function");
    assert!(code.results[0].address.starts_with("el:"));

    database.destroy(state.db).await;
}

/// Anchor filters compose with the conversation scope — the promise workshop
/// 005 makes and the reason the search CTE grew a second reference leg. A turn
/// has no `worktree_files` row at all, so a repo filter that only knew about
/// live paths would answer every conversation query with silence.
#[tokio::test]
async fn an_anchor_filter_narrows_conversations_instead_of_erasing_them() {
    let (database, state) = stack("conv-query-anchor").await;
    store(
        &state,
        GUID,
        vec![turn(1, "anchored to the flowspace repository")],
    )
    .await;
    drain(&state).await;

    let mine = search(
        &state,
        &SearchRequest {
            repo: Some(ANCHOR.to_string()),
            ..ask("anchored repository", Some("conversation"))
        },
        &scoped(ANCHOR),
    )
    .await
    .expect("an anchored search");
    assert_eq!(
        mine.results.len(),
        1,
        "the conversation is anchored to this repository, so it answers"
    );

    let elsewhere = search(
        &state,
        &SearchRequest {
            repo: Some("git:github.com/fs3/other".to_string()),
            ..ask("anchored repository", Some("conversation"))
        },
        &scoped("git:github.com/fs3/other"),
    )
    .await;
    assert!(
        elsewhere
            .map(|hits| hits.results.is_empty())
            .unwrap_or(true),
        "and a different anchor excludes it — the filter narrows rather than no-ops"
    );

    database.destroy(state.db).await;
}

/// The window: the caller picks the reach, the answer is contiguous and in
/// order, and it is honest where the conversation ends (ac-0005).
#[tokio::test]
async fn a_window_reads_around_a_turn_and_stops_at_the_edges() {
    let (database, state) = stack("conv-query-window").await;
    let turns: Vec<Turn> = (1..=6)
        .map(|n| turn(n, &format!("turn number {n}")))
        .collect();
    store(&state, GUID, turns).await;

    let payload = fs3_daemon::read::get(
        &state,
        &GetRequest {
            address: format!("conv:{GUID}#t3"),
            before: Some(1),
            after: Some(2),
            ..GetRequest::default()
        },
        &Scope::unscoped(),
    )
    .await
    .expect("a window")
    .0;

    let GetPayload::Conversation(window) = payload else {
        panic!("a conv: address must answer with a conversation");
    };
    assert_eq!(window.address, format!("conv:{GUID}"));
    assert_eq!(window.turns, 6, "the total is reported, not just the slice");
    assert_eq!(window.around, 3);
    assert_eq!(
        window.window.iter().map(|t| t.turn_no).collect::<Vec<_>>(),
        vec![2, 3, 4, 5]
    );
    assert_eq!(window.window[0].address, format!("conv:{GUID}#t2"));
    assert_eq!(window.window[0].role, "agent");
    assert_eq!(window.window[0].source, "peer");
    assert_eq!(window.repo.as_deref(), Some(ANCHOR));

    // Past the end: what exists, not padding.
    let payload = fs3_daemon::read::get(
        &state,
        &GetRequest {
            address: format!("conv:{GUID}#t6"),
            before: Some(0),
            after: Some(50),
            ..GetRequest::default()
        },
        &Scope::unscoped(),
    )
    .await
    .expect("a window at the edge")
    .0;
    let GetPayload::Conversation(window) = payload else {
        panic!("a conversation");
    };
    assert_eq!(window.window.len(), 1);

    database.destroy(state.db).await;
}

/// A bare `conv:<guid>` is "show me the start", not an error: it is the address
/// workshop 003 defines for a whole conversation, and `get` has to take it.
#[tokio::test]
async fn a_conversation_address_with_no_ordinal_starts_at_the_beginning() {
    let (database, state) = stack("conv-query-start").await;
    store(
        &state,
        GUID,
        vec![turn(1, "the first thing said"), turn(2, "the second")],
    )
    .await;

    let payload = fs3_daemon::read::get(
        &state,
        &GetRequest {
            address: format!("conv:{GUID}"),
            ..GetRequest::default()
        },
        &Scope::unscoped(),
    )
    .await
    .expect("a window")
    .0;

    let GetPayload::Conversation(window) = payload else {
        panic!("a conversation");
    };
    assert_eq!(window.around, 1);
    assert_eq!(window.window.len(), 2, "the default reach covers both");

    database.destroy(state.db).await;
}

/// `tree conv:<guid>` is the outline: role, source, timestamp, first line —
/// enough to choose a turn, never the turn itself.
#[tokio::test]
async fn a_conversation_tree_is_its_turn_outline() {
    let (database, state) = stack("conv-query-tree").await;
    store(
        &state,
        GUID,
        vec![
            turn(1, "first line\nsecond line that must not appear"),
            turn(2, "a reply"),
        ],
    )
    .await;

    let result = fs3_daemon::read::tree(
        &state,
        &TreeRequest {
            address: Some(format!("conv:{GUID}")),
            ..TreeRequest::default()
        },
        &Scope::unscoped(),
    )
    .await
    .expect("an outline");

    assert_eq!(result.kind, "conversation");
    assert_eq!(result.target, "a fleet session");
    assert_eq!(result.total, 2);
    assert_eq!(result.entries.len(), 2);

    let first = &result.entries[0];
    assert_eq!(first.kind, "turn");
    assert_eq!(first.name, "first line", "the first line only");
    assert_eq!(
        first.address.as_deref(),
        Some(format!("conv:{GUID}#t1").as_str())
    );
    assert_eq!(first.role.as_deref(), Some("human"));
    assert_eq!(first.source.as_deref(), Some("peer"));
    assert_eq!(first.at.as_deref(), Some("2026-08-27T09:00:00Z"));
    assert_eq!(result.entries[1].role.as_deref(), Some("agent"));

    database.destroy(state.db).await;
}

/// `conversation list` narrows by anchor, and `conversation remove` forgets one
/// conversation without touching its neighbour (ac-000a).
#[tokio::test]
async fn listing_narrows_and_removing_forgets_exactly_one() {
    let (database, state) = stack("conv-query-manage").await;
    store(&state, GUID, vec![turn(1, "the first conversation")]).await;
    store(&state, OTHER, vec![turn(1, "the second conversation")]).await;

    let all =
        fs3_daemon::conversations::list(&state, &fs3_daemon::conversations::ListRequest::default())
            .await
            .expect("listing");
    assert_eq!(all.conversations.len(), 2);
    assert!(all.conversations[0].address.starts_with("conv:"));
    assert_eq!(all.conversations[0].turns, 1);

    let narrowed = fs3_daemon::conversations::list(
        &state,
        &fs3_daemon::conversations::ListRequest {
            repo: Some("git:github.com/fs3/nothing".to_string()),
            path: None,
        },
    )
    .await
    .expect("listing");
    assert!(narrowed.conversations.is_empty(), "the filter narrows");

    let removed = fs3_daemon::conversations::remove(
        &state,
        &fs3_daemon::conversations::RemoveRequest {
            guid: GUID.to_string(),
        },
    )
    .await
    .expect("removing");
    assert!(removed.existed);
    assert_eq!(removed.turns, 1);
    assert_eq!(removed.elements, 1);

    let left =
        fs3_daemon::conversations::list(&state, &fs3_daemon::conversations::ListRequest::default())
            .await
            .expect("listing");
    assert_eq!(left.conversations.len(), 1);
    assert_eq!(left.conversations[0].guid, OTHER);

    database.destroy(state.db).await;
}
