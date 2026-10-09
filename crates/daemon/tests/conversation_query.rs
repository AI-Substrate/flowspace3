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
use fs3_core::{
    Config, DatabaseConfig, ElementKind, ToolBox, ToolInput, Turn, TurnItem, TurnRole, TurnSource,
};
use fs3_daemon::conversations::{IntakeRequest, intake};
use fs3_daemon::read::{GetRequest, TreeRequest};
use fs3_daemon::scope::{Scope, ScopeSource};
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
    store_at(state, guid, Some(ANCHOR), Some("/srv/anchored"), turns).await;
}

async fn store_at(
    state: &AppState,
    guid: &str,
    repo_identity: Option<&str>,
    worktree: Option<&str>,
    turns: Vec<Turn>,
) {
    intake(
        state,
        IntakeRequest {
            guid: guid.to_string(),
            repo_identity: repo_identity.map(str::to_string),
            worktree: worktree.map(str::to_string),
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
        source: ScopeSource::Flag,
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

/// Default and `all` include conversations; `--source code` narrows them out.
#[tokio::test]
async fn default_search_includes_conversations_and_source_can_narrow() {
    let (database, state) = stack("conv-query-scope").await;
    store(
        &state,
        GUID,
        vec![turn(1, "the anchor is a pointer, not ownership")],
    )
    .await;
    drain(&state).await;

    let hits = search(&state, &ask("anchor ownership", None), &Scope::unscoped())
        .await
        .expect("the default mixed search");
    assert_eq!(hits.results.len(), 1);
    assert_eq!(hits.results[0].kind, "turn");
    assert_eq!(
        hits.results[0].address,
        format!("conv:{GUID}#t1"),
        "a turn hit carries a conv: address, not an el: one"
    );

    let code = search(
        &state,
        &ask("anchor ownership", Some("code")),
        &Scope::unscoped(),
    )
    .await;
    assert!(
        code.map(|hits| hits.results.is_empty()).unwrap_or(true),
        "the code source narrows conversation turns out"
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
    // Since keyword search matches every term (2026-10-09), the turn that
    // quotes the code is a legitimate second hit in its own right; what must
    // hold is that the CODE row resolves to the code element, exactly once.
    let functions: Vec<_> = code
        .results
        .iter()
        .filter(|hit| hit.kind == "function")
        .collect();
    assert_eq!(functions.len(), 1, "{:#?}", code.results);
    assert!(functions[0].address.starts_with("el:"));
    assert_eq!(
        code.results[0].kind, "function",
        "verbatim code still leads"
    );

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

/// Default search is a mixed corpus, but ownership still comes from the
/// conversation anchor. The null worktree is deliberate: repo attribution is
/// sufficient when the conversation did not record a checkout.
#[tokio::test]
async fn default_scope_admits_only_its_repository_conversations() {
    let (database, state) = stack("conv-query-two-repos").await;
    let repo_a = "git:github.com/fs3/repo-a";
    let repo_b = "git:github.com/fs3/repo-b";
    store_at(
        &state,
        GUID,
        Some(repo_a),
        None,
        vec![turn(1, "shared search topic from repository alpha")],
    )
    .await;
    store_at(
        &state,
        OTHER,
        Some(repo_b),
        None,
        vec![turn(1, "shared search topic from repository beta")],
    )
    .await;
    drain(&state).await;

    let mine = search(
        &state,
        &ask("shared search topic", None),
        &Scope {
            repo: Some(repo_a.to_string()),
            worktree: Some("/srv/repo-a".to_string()),
            ..Scope::unscoped()
        },
    )
    .await
    .expect("default scoped mixed search");
    assert_eq!(mine.results.len(), 1);
    assert_eq!(mine.results[0].address, format!("conv:{GUID}#t1"));
    assert_eq!(mine.composition.conversation, 1);
    assert_eq!(mine.composition.code + mine.composition.doc, 0);

    let all = search(
        &state,
        &ask("shared search topic", None),
        &Scope::unscoped(),
    )
    .await
    .expect("default unscoped mixed search");
    assert_eq!(all.results.len(), 2);
    assert_eq!(all.composition.conversation, 2);

    database.destroy(state.db).await;
}

/// A cwd-scoped search says what the same query ranks in OTHER repositories;
/// a named `--repo`, a later page, or an unscoped search does not.
#[tokio::test]
async fn a_cwd_scoped_search_counts_what_other_repositories_hold() {
    let (database, state) = stack("conv-query-elsewhere").await;
    let repo_a = "git:github.com/fs3/repo-a";
    let repo_b = "git:github.com/fs3/repo-b";
    store_at(
        &state,
        GUID,
        Some(repo_a),
        None,
        vec![turn(1, "shared search topic from repository alpha")],
    )
    .await;
    store_at(
        &state,
        OTHER,
        Some(repo_b),
        None,
        vec![turn(1, "shared search topic from repository beta")],
    )
    .await;
    drain(&state).await;

    let from_cwd = Scope {
        repo: Some(repo_a.to_string()),
        source: ScopeSource::Cwd,
        ..Scope::unscoped()
    };
    let mine = search(&state, &ask("shared search topic", None), &from_cwd)
        .await
        .expect("cwd-scoped search");
    assert_eq!(mine.results.len(), 1, "the page itself stays scoped");
    let elsewhere = mine
        .elsewhere
        .expect("the other repository's hit is counted");
    assert_eq!(elsewhere.hits, 1);
    assert_eq!(elsewhere.repos.len(), 1);
    assert_eq!(elsewhere.repos[0].repo, repo_b);
    assert_eq!(elsewhere.of_top, 20);

    let named = search(&state, &ask("shared search topic", None), &scoped(repo_a))
        .await
        .expect("named-repo search");
    assert!(
        named.elsewhere.is_none(),
        "--repo meant exactly that repository"
    );

    let mut later = ask("shared search topic", None);
    later.offset = Some(1);
    let page_two = search(&state, &later, &from_cwd)
        .await
        .expect("second page");
    assert!(
        page_two.elsewhere.is_none(),
        "only the first page carries the hint"
    );

    let everything = search(
        &state,
        &ask("shared search topic", None),
        &Scope::unscoped(),
    )
    .await
    .expect("unscoped search");
    assert!(
        everything.elsewhere.is_none(),
        "nothing is elsewhere when all was searched"
    );

    database.destroy(state.db).await;
}

/// Conversation imports historically stored the remote without the identity
/// scheme, while query scopes use the canonical `git:` identity. Both forms
/// name the same repository and must meet at one normalization seam.
#[tokio::test]
async fn legacy_repo_identity_remains_gettable_and_searchable() {
    let (database, state) = stack("conv-query-legacy-repo-identity").await;
    let stored_repo = "https://github.com/fs3/legacy-anchor.git";
    let canonical_repo = "git:github.com/fs3/legacy-anchor";
    let worktree = "/srv/legacy-anchor";
    let identity =
        fs3_core::RepoIdentity::from_remote_parts(Some("github.com"), "fs3/legacy-anchor")
            .expect("a canonical repository identity");
    fs3_store::register_worktree(&state.db, &identity, worktree, Some("main"))
        .await
        .expect("registering the conversation worktree");
    store_at(
        &state,
        GUID,
        Some(stored_repo),
        Some(worktree),
        vec![turn(1, "legacy anchored conversation readback")],
    )
    .await;
    drain(&state).await;

    let payload = fs3_daemon::read::get(
        &state,
        &GetRequest {
            address: format!("conv:{GUID}#t1"),
            ..GetRequest::default()
        },
        &scoped(canonical_repo),
    )
    .await
    .expect("canonical scope gets the legacy-anchored conversation")
    .0;
    let GetPayload::Conversation(window) = payload else {
        panic!("a conversation window");
    };
    assert_eq!(
        window.window[0].body,
        "legacy anchored conversation readback"
    );

    let scoped_hits = search(
        &state,
        &ask(
            "legacy anchored conversation readback",
            Some("conversation"),
        ),
        &scoped(canonical_repo),
    )
    .await
    .expect("canonical scope searches the legacy-anchored conversation");
    assert_eq!(scoped_hits.results.len(), 1);
    assert_eq!(scoped_hits.results[0].address, format!("conv:{GUID}#t1"));

    let unscoped_hits = search(
        &state,
        &ask(
            "legacy anchored conversation readback",
            Some("conversation"),
        ),
        &Scope::unscoped(),
    )
    .await
    .expect("unscoped search reads the legacy-anchored conversation");
    assert_eq!(unscoped_hits.results.len(), 1);
    assert_eq!(unscoped_hits.results[0].address, format!("conv:{GUID}#t1"));

    database.destroy(state.db).await;
}

#[tokio::test]
async fn store_search_filters_make_the_conversation_pin_meaningful() {
    let (database, state) = stack("conv-query-hard-pin").await;
    for guid in [GUID, OTHER] {
        store_at(
            &state,
            guid,
            Some(ANCHOR),
            None,
            vec![turn(1, "shared pinned transcript fact")],
        )
        .await;
    }

    let base = fs3_store::SearchFilters {
        kinds: Some(vec![ElementKind::Turn]),
        limit: 10,
        ..fs3_store::SearchFilters::default()
    };
    let broad = fs3_store::search_lexical(&state.db, "shared pinned transcript fact", &base)
        .await
        .expect("unpinned lexical search");
    assert_eq!(broad.len(), 2, "absence of the pin keeps both transcripts");

    let pinned = fs3_store::SearchFilters {
        conversation: Some(GUID.to_string()),
        ..base.clone()
    };
    let lexical = fs3_store::search_lexical(&state.db, "shared pinned transcript fact", &pinned)
        .await
        .expect("pinned lexical search");
    assert_eq!(lexical.len(), 1);
    assert_eq!(lexical[0].element.address, format!("conv:{GUID}#t1"));

    // The exact-text leg is sufficient for the mutation: both transcripts hold
    // one shared content hash, and the chosen element address must still name
    // the pinned transcript.

    database.destroy(state.db).await;
}

#[tokio::test]
async fn ask_tools_search_and_get_conversations_under_the_same_scope() {
    let (database, state) = stack("conv-query-ask-tools").await;
    store_at(
        &state,
        GUID,
        Some(ANCHOR),
        None,
        vec![turn(1, "conversation evidence for the ask loop")],
    )
    .await;
    store_at(
        &state,
        OTHER,
        Some("git:github.com/fs3/foreign"),
        None,
        vec![turn(1, "foreign conversation evidence")],
    )
    .await;
    drain(&state).await;
    let tools = fs3_daemon::ask::IndexTools::new(
        &state,
        Scope {
            repo: Some(ANCHOR.to_string()),
            source: ScopeSource::Flag,
            worktree: Some("/srv/current-checkout".to_string()),
            ..Scope::unscoped()
        },
    );

    let found = tools
        .call("search", r#"{"query":"conversation evidence"}"#)
        .await
        .expect("ask search sees the anchored turn");
    assert!(found.evidence);
    assert!(found.content.contains(&format!("conv:{GUID}#t1")));

    let read = tools
        .call("get", &format!(r#"{{"address":"conv:{GUID}#t1"}}"#))
        .await
        .expect("ask get sees the same anchored turn");
    assert!(read.evidence);
    assert!(
        read.content
            .contains("conversation evidence for the ask loop")
    );

    let escaped = tools
        .call("get", &format!(r#"{{"address":"conv:{OTHER}#t1"}}"#))
        .await;
    assert!(
        escaped.is_err(),
        "an unpinned model-proposed address must not escape the ask corpus"
    );

    let cwd_tools = fs3_daemon::ask::IndexTools::new(
        &state,
        Scope {
            repo: Some(ANCHOR.to_string()),
            source: ScopeSource::Cwd,
            cwd: Some("/srv/current-checkout".to_string()),
            worktree: Some("/srv/current-checkout".to_string()),
            warnings: Vec::new(),
        },
    );
    let error = cwd_tools
        .call("get", &format!(r#"{{"address":"conv:{OTHER}#t1"}}"#))
        .await
        .expect_err("the post-resolution guard rejects a cwd-scoped escape");
    assert!(
        error
            .to_string()
            .contains("outside the caller's immutable repository scope"),
        "the compensating guard itself must fire: {error}"
    );

    database.destroy(state.db).await;
}

/// A body-less turn remains readable and explains where its typed content lives.
#[tokio::test]
async fn get_explains_body_less_turns() {
    let (database, state) = stack("conv-query-get-empty").await;
    let mut items_only = turn(1, "");
    items_only.items.push(TurnItem::ToolCall {
        tool: "read".to_string(),
        input: ToolInput::Verbatim {
            text: "AGENTS.md".to_string(),
        },
    });
    store(&state, GUID, vec![items_only]).await;

    let payload = fs3_daemon::read::get(
        &state,
        &GetRequest {
            address: format!("conv:{GUID}#t1"),
            ..GetRequest::default()
        },
        &scoped(ANCHOR),
    )
    .await
    .expect("scoped get reads its conversation")
    .0;
    let GetPayload::Conversation(window) = payload else {
        panic!("a conversation");
    };
    let turn = &window.window[0];
    assert!(turn.body.is_empty());
    assert_eq!(
        turn.body_empty_reason.as_deref(),
        Some("the stored turn contains typed items but no prose")
    );
    assert_eq!(turn.items.len(), 1, "typed content remains intact");

    database.destroy(state.db).await;
}

/// Backlog row 101 and o-prime reply 002 reverse the former repository-scope
/// assertion: a canonical address is authoritative when scope came from cwd.
#[tokio::test]
async fn get_conv_cross_worktree() {
    let (database, state) = stack("conv-query-get-authoritative").await;
    store(
        &state,
        GUID,
        vec![turn(1, "address-authoritative evidence")],
    )
    .await;

    for (repo, worktree) in [
        (ANCHOR, "/srv/a-different-checkout"),
        ("git:github.com/fs3/a-different-repo", "/srv/foreign"),
    ] {
        let payload = fs3_daemon::read::get(
            &state,
            &GetRequest {
                address: format!("conv:{GUID}#t1"),
                ..GetRequest::default()
            },
            &Scope {
                repo: Some(repo.to_string()),
                source: ScopeSource::Cwd,
                cwd: Some(worktree.to_string()),
                worktree: Some(worktree.to_string()),
                warnings: Vec::new(),
            },
        )
        .await
        .expect("an explicit conv address ignores cwd-derived scope")
        .0;
        let GetPayload::Conversation(window) = payload else {
            panic!("a conversation");
        };
        assert_eq!(window.address, format!("conv:{GUID}"));
    }

    database.destroy(state.db).await;
}

#[tokio::test]
async fn conv_not_found_messages() {
    let (database, state) = stack("conv-query-get-misses").await;
    store(&state, GUID, vec![turn(1, "known conversation")]).await;

    let outside = fs3_daemon::read::get(
        &state,
        &GetRequest {
            address: format!("conv:{GUID}#t1"),
            ..GetRequest::default()
        },
        &scoped("git:github.com/fs3/other"),
    )
    .await
    .expect_err("an explicit --repo remains a real filter");
    let absent_guid = "00000000-0000-4000-8000-000000000000";
    let absent = fs3_daemon::read::get(
        &state,
        &GetRequest {
            address: format!("conv:{absent_guid}#t1"),
            ..GetRequest::default()
        },
        &Scope::unscoped(),
    )
    .await
    .expect_err("a globally absent guid is not found");

    assert_ne!(outside.message, absent.message);
    assert!(
        outside
            .message
            .contains("outside the explicitly requested scope")
    );
    assert!(absent.message.contains("no conversation"));
    assert_eq!(outside.details["guid"], GUID);
    assert_eq!(
        outside.details["requested_repo"],
        "git:github.com/fs3/other"
    );
    assert_eq!(absent.details["guid"], absent_guid);
    assert!(!absent.details.contains_key("requested_repo"));

    database.destroy(state.db).await;
}

#[tokio::test]
async fn composition_counts_every_thresholded_source_beyond_top_k() {
    let (database, state) = stack("conv-query-composition").await;
    let phrase = "composition needle";
    store(
        &state,
        GUID,
        vec![turn(1, &format!("{phrase} in a conversation"))],
    )
    .await;

    let identity = fs3_core::RepoIdentity::from_remote_parts(Some("github.com"), "fs3/anchored")
        .expect("remote identity");
    store(
        &state,
        OTHER,
        vec![turn(1, "a deliberately unrelated low-score conversation")],
    )
    .await;
    let worktree =
        fs3_store::register_worktree(&state.db, &identity, "/srv/anchored", Some("main"))
            .await
            .expect("registering mixed fixture");
    let blob = fs3_core::BlobRef::new("d".repeat(40)).expect("blob key");
    let file = fs3_core::Element::new(
        fs3_core::ElementKind::File,
        "source_file",
        "mixed.md",
        "mixed.md",
        fs3_core::Span::new(1, 3),
        phrase,
    )
    .with_children(vec![
        fs3_core::Element::new(
            fs3_core::ElementKind::Function,
            "function_item",
            "first",
            "mixed.md::first",
            fs3_core::Span::new(1, 1),
            phrase,
        ),
        fs3_core::Element::new(
            fs3_core::ElementKind::Section,
            "atx_heading",
            "notes",
            "mixed.md::notes",
            fs3_core::Span::new(2, 3),
            format!("{phrase} in documentation"),
        ),
    ]);
    fs3_store::upsert_element_tree(&state.db, &blob, "test-parser@1", &file, |_| false)
        .await
        .expect("storing mixed fixture");
    fs3_store::sync_worktree_files(&state.db, worktree, &[("mixed.md".to_string(), blob)])
        .await
        .expect("mapping mixed fixture");
    drain(&state).await;

    let outcome = search(
        &state,
        &SearchRequest {
            q: phrase.to_string(),
            min_score: Some(1.0),
            limit: Some(1),
            ..SearchRequest::default()
        },
        &Scope {
            repo: Some(ANCHOR.to_string()),
            worktree: Some("/srv/anchored".to_string()),
            ..Scope::unscoped()
        },
    )
    .await
    .expect("mixed search");

    assert_eq!(outcome.results.len(), 1, "top-k still limits returned rows");
    assert!(outcome.truncated);
    assert_eq!(outcome.composition.code, 1);
    assert_eq!(
        outcome.composition.conversation, 1,
        "the second conversation falls below min-score and must not be counted"
    );
    assert_eq!(outcome.composition.doc, 1);

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

fn tool(turn_no: u32, role: TurnRole, item: TurnItem) -> Turn {
    Turn {
        role,
        items: vec![item],
        ..turn(turn_no, "")
    }
}

fn search_call(text: &str) -> TurnItem {
    TurnItem::ToolCall {
        tool: "Bash".to_string(),
        input: ToolInput::Verbatim {
            text: text.to_string(),
        },
    }
}

fn printed(head: &str) -> TurnItem {
    TurnItem::ToolResult {
        tool: "Bash".to_string(),
        head: head.to_string(),
        total_bytes: head.len() as u64,
        truncated: false,
    }
}

/// An agent asking fs3 the question, and the envelope it got back, must not
/// outrank the conversation that holds the answer — and hiding them is said
/// out loud, with the flag that shows them.
#[tokio::test]
async fn retrieval_echoes_are_hidden_by_default_and_disclosed() {
    let (database, state) = stack("conv-query-echoes").await;
    store(
        &state,
        GUID,
        vec![
            turn(1, "what sets the retry backoff ceiling?"),
            tool(
                2,
                TurnRole::Agent,
                search_call(r#"{"command":"flowspace3 search \"retry backoff ceiling\" --json"}"#),
            ),
            tool(
                3,
                TurnRole::Human,
                printed(
                    "{\"ok\": true, \"command\": \"search\", \"v\": 1, \"data\": {\"q\": \"retry backoff ceiling\"}}",
                ),
            ),
            turn(4, "The retry backoff ceiling is thirty seconds, set by the runner."),
        ],
    )
    .await;
    drain(&state).await;

    let hidden = search(
        &state,
        &ask("retry backoff ceiling", Some("conversation")),
        &Scope::unscoped(),
    )
    .await
    .expect("search answers");
    let addresses: Vec<&str> = hidden
        .results
        .iter()
        .map(|hit| hit.address.as_str())
        .collect();
    assert!(
        addresses.contains(&format!("conv:{GUID}#t4").as_str()),
        "{addresses:?}"
    );
    assert!(
        addresses.contains(&format!("conv:{GUID}#t1").as_str()),
        "{addresses:?}"
    );
    for echo in [2, 3] {
        let address = format!("conv:{GUID}#t{echo}");
        assert!(
            !addresses.contains(&address.as_str()),
            "{address} leaked: {addresses:?}"
        );
    }
    assert!(
        hidden.filtered.retrieval_turns >= 2,
        "{:?}",
        hidden.filtered
    );
    assert_eq!(hidden.filtered.include_flag, Some("--include-retrieval"));
    assert!(
        hidden
            .filtered
            .clause()
            .is_some_and(|clause| clause.contains("--include-retrieval")),
        "the disclosure names the flag"
    );

    let everything = search(
        &state,
        &SearchRequest {
            include_retrieval: true,
            ..ask("retry backoff ceiling", Some("conversation"))
        },
        &Scope::unscoped(),
    )
    .await
    .expect("search answers");
    let addresses: Vec<&str> = everything
        .results
        .iter()
        .map(|hit| hit.address.as_str())
        .collect();
    for turn_no in 1..=4 {
        let address = format!("conv:{GUID}#t{turn_no}");
        assert!(
            addresses.contains(&address.as_str()),
            "{address} missing: {addresses:?}"
        );
    }
    assert_eq!(everything.filtered.retrieval_turns, 0);
    assert_eq!(everything.filtered.include_flag, None);

    drop(database);
}

/// When every match is retrieval traffic the page is empty for a reason the
/// caller can act on, not "nothing matched".
#[tokio::test]
async fn a_page_of_only_echoes_says_so() {
    let (database, state) = stack("conv-query-only-echoes").await;
    store(
        &state,
        GUID,
        vec![
            tool(
                1,
                TurnRole::Agent,
                search_call("flowspace3 search \"zebra quokka manifold\""),
            ),
            tool(
                2,
                TurnRole::Human,
                printed("no hits for zebra quokka manifold"),
            ),
        ],
    )
    .await;
    drain(&state).await;

    let outcome = search(
        &state,
        &ask("zebra quokka manifold", Some("conversation")),
        &Scope::unscoped(),
    )
    .await
    .expect("search answers");
    assert!(outcome.results.is_empty(), "{:?}", outcome.results);
    let reason = outcome.empty_because.expect("the emptiness is explained");
    assert_eq!(reason.reason, "retrieval_hidden");
    assert!(
        reason
            .hint
            .is_some_and(|hint| hint.contains("--include-retrieval"))
    );

    drop(database);
}
