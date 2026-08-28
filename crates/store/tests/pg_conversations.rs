//! Conversations in the store: append, read, remove — and, above all, survive.
//!
//! The load-bearing test in this file is
//! [`a_conversation_no_worktree_holds_survives_a_full_gc_sweep`]. Everything
//! else here is ordinary behaviour; that one is the reason the second reference
//! leg exists. An imported conversation has no registered worktree and never
//! will, so under the pre-conversation predicate — "content is held while a
//! live path maps its blob" — every turn element, and then every summary and
//! vector the import paid for, is garbage on the first sweep. Silently: an
//! empty search result looks exactly like "no match".
//!
//! It is mutation-checked. Delete either leg of `held_by_a_live_root!` in
//! `store/src/roots.rs` and that test fails; delete the turn leg and it fails
//! at the element level, which is the level that cascades.

mod support;

use std::path::Path;

use fs3_core::conversation::PARSER_VERSION as CONVERSATION_PARSER;
use fs3_core::{
    BlobRef, Conversation, ConversationId, Element, ElementKind, RepoIdentity, Span, Summary,
    ToolInput, Turn, TurnItem, TurnRole, TurnSource, earns_summary,
};
use fs3_store::{
    AnchorFilter, PgPool, append_turns, collect_garbage, delete_conversation, enqueue_job,
    get_elements, get_smart_content, list_conversations, outline, put_smart_content,
    raw_hash_is_referenced, register_worktree, remove_root, sync_worktree_files,
    upsert_conversation, upsert_element_tree, window,
};
use std::collections::BTreeMap;
use std::time::Duration;
use support::{FreshDatabase, PARSER_VERSION, unique_blob};

const SUMMARIZER: &str = "fake-summarizer@v1";

/// The size gate the tests write under, in bytes. A literal, because these
/// tests are about the store recording a verdict rather than computing one.
const SUMMARY_FLOOR: usize = 64;

fn id(nibble: char) -> ConversationId {
    ConversationId::new(format!("6ba7b810-9dad-11d1-80b4-00c04fd430{nibble}7"))
        .expect("a canonical uuid")
}

fn conversation(guid: &ConversationId, repo: Option<&str>) -> Conversation {
    Conversation {
        guid: guid.clone(),
        repo_identity: repo.map(str::to_string),
        worktree: Some("/srv/checkout".to_string()),
        base_sha: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
        title: Some("a conversation".to_string()),
        started_at: "2026-08-27T09:00:00Z".to_string(),
        parent: None,
    }
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
        head_sha: Some("fedcba9876543210fedcba9876543210fedcba98".to_string()),
        at: "2026-08-27T09:00:00Z".to_string(),
        body: body.to_string(),
        items: Vec::new(),
    }
}

/// The size-gate policy, injected the way the daemon will inject it.
fn gate(element: &Element) -> bool {
    earns_summary(&element.raw_text, SUMMARY_FLOOR)
}

async fn store_conversation(pool: &PgPool, guid: &ConversationId, turns: &[Turn]) {
    upsert_conversation(pool, &conversation(guid, Some("github.com/x/anchored")))
        .await
        .expect("storing the header");
    append_turns(pool, guid, turns, gate)
        .await
        .expect("appending turns");
}

/// A conversation grows across many posts, and a re-post of an overlap must
/// change nothing: no duplicate rows, and — because the caller enqueues
/// enrichment from `accepted` — no second provider bill (ac-0001, ac-0009).
#[tokio::test]
async fn appending_is_idempotent_and_reports_only_the_delta() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let guid = id('a');

    upsert_conversation(&pool, &conversation(&guid, None))
        .await
        .expect("storing the header");

    let first = append_turns(&pool, &guid, &[turn(1, "hello"), turn(2, "hi")], gate)
        .await
        .expect("first post");
    assert_eq!(first.accepted.len(), 2);
    assert_eq!(first.already_stored, 0);

    // The second post OVERLAPS: turn 2 is re-sent, turn 3 is new.
    let second = append_turns(&pool, &guid, &[turn(2, "hi"), turn(3, "onwards")], gate)
        .await
        .expect("second post");
    assert_eq!(
        second.already_stored, 1,
        "the overlapping turn was already stored"
    );
    assert_eq!(
        second.accepted.len(),
        1,
        "only the new turn may be enqueued for enrichment"
    );
    assert_eq!(second.accepted[0].address, guid.turn_address(3));

    let stored = window(&pool, &guid, 1, 0, 100).await.expect("reading back");
    assert_eq!(stored.len(), 3, "three turns, not four");

    // A re-post of a turn whose body has since been edited must NOT rewrite the
    // stored one: turns are what was said, and what was said does not change.
    let rewritten = turn(2, "actually something else");
    let third = append_turns(&pool, &guid, std::slice::from_ref(&rewritten), gate)
        .await
        .expect("third post");
    assert_eq!(third.accepted.len(), 0);
    assert_eq!(
        window(&pool, &guid, 2, 0, 0).await.expect("re-reading")[0].body,
        "hi",
        "a stored turn is never disturbed by a later post"
    );

    database.destroy(pool).await;
}

/// The header is upserted on every post, so a later post must only ever teach
/// it more: a title it does not mention survives, and the start time can move
/// earlier but never later.
#[tokio::test]
async fn a_re_posted_header_learns_but_never_forgets() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let guid = id('b');

    upsert_conversation(&pool, &conversation(&guid, Some("github.com/x/one")))
        .await
        .expect("first header");

    let mut later = conversation(&guid, None);
    later.title = None;
    later.started_at = "2026-08-27T11:00:00Z".to_string();
    upsert_conversation(&pool, &later)
        .await
        .expect("second header");

    let listed = list_conversations(&pool, AnchorFilter::default())
        .await
        .expect("listing");
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].title.as_deref(),
        Some("a conversation"),
        "a post that says nothing about the title must not erase it"
    );
    assert_eq!(
        listed[0].repo_identity.as_deref(),
        Some("github.com/x/one"),
        "nor about the anchor"
    );
    assert_eq!(
        listed[0].started_at, "2026-08-27T09:00:00Z",
        "a conversation cannot begin later than it already began"
    );

    // Earlier IS news: a client that discovers an older first turn corrects it.
    let mut earlier = conversation(&guid, None);
    earlier.started_at = "2026-08-27T08:00:00Z".to_string();
    upsert_conversation(&pool, &earlier)
        .await
        .expect("third header");
    let listed = list_conversations(&pool, AnchorFilter::default())
        .await
        .expect("listing");
    assert_eq!(listed[0].started_at, "2026-08-27T08:00:00Z");

    database.destroy(pool).await;
}

/// **The reason the second reference leg exists.**
///
/// An imported conversation is anchored to a repository that need never be
/// registered — and here, is not. Every level of GC must leave it alone: its
/// queued enrichment (level 0), its turn elements (level 1), and the summary
/// those elements carry (level 2). The spend guard must agree, because a job
/// already claimed is one GC can never reach.
///
/// Mutation check: remove the `turns` leg from `held_by_a_live_root!` and this
/// fails on the element assertion, then cascades to the summary.
#[tokio::test]
async fn a_conversation_no_worktree_holds_survives_a_full_gc_sweep() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let guid = id('c');

    // Long enough to be over the gate, so it has a summary worth protecting.
    let body = "we ruled that the anchor is a pointer and not ownership, \
                which is why removing a repo leaves its conversations alone";
    let turns = [turn(1, body)];
    store_conversation(&pool, &guid, &turns).await;

    let raw_hash = turns[0].blob_sha();
    put_smart_content(
        &pool,
        &raw_hash,
        SUMMARIZER,
        &Summary {
            text: "a ruling about anchors".to_string(),
            tags: vec!["anchors".to_string()],
            extras: BTreeMap::new(),
        },
    )
    .await
    .expect("storing the summary");

    enqueue_job(
        &pool,
        "summarize",
        "summarize:conversations:pending",
        &serde_json::json!({ "raw_hash": raw_hash, "identity": "github.com/x/anchored" }),
        Duration::ZERO,
    )
    .await
    .expect("queueing enrichment");

    assert!(
        raw_hash_is_referenced(&pool, &raw_hash)
            .await
            .expect("asking the spend guard"),
        "a stored turn is a ROOT of reference: the guard must let its enrichment be paid for"
    );

    let reclaimed = collect_garbage(&pool).await.expect("collecting");
    assert_eq!(
        reclaimed.jobs, 0,
        "the queued enrichment is for content a turn still carries: {reclaimed:?}"
    );
    assert_eq!(
        reclaimed.elements, 0,
        "a conversation with no worktree is not garbage: {reclaimed:?}"
    );
    assert_eq!(
        reclaimed.summaries, 0,
        "and neither is what it paid for: {reclaimed:?}"
    );

    assert!(
        get_smart_content(&pool, &raw_hash, SUMMARIZER)
            .await
            .expect("reading back")
            .is_some()
    );
    assert_eq!(
        window(&pool, &guid, 1, 0, 0)
            .await
            .expect("reading the turn")
            .len(),
        1
    );

    database.destroy(pool).await;
}

/// Anchors are pointers, not ownership (ac-0007): removing the anchored repo
/// must leave the conversation whole. Under the workshop's original FK sketch
/// this could not even be attempted — `remove_root` deletes the repos row, and
/// the delete would have raised a foreign-key violation.
#[tokio::test]
async fn removing_the_anchored_repo_leaves_the_conversation_intact() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let guid = id('d');

    let root = "/srv/anchored";
    let identity = RepoIdentity::from_path(Path::new(root));
    let worktree = register_worktree(&pool, &identity, root, Some("main"))
        .await
        .expect("registering the anchor repo");
    sync_worktree_files(&pool, worktree, &[("src/a.rs".to_string(), unique_blob())])
        .await
        .expect("mapping a file");

    upsert_conversation(&pool, &conversation(&guid, Some(identity.key())))
        .await
        .expect("storing the header");
    append_turns(&pool, &guid, &[turn(1, "anchored here")], gate)
        .await
        .expect("appending");

    remove_root(&pool, root)
        .await
        .expect("removing the anchored repo must not fail on the anchor");
    collect_garbage(&pool).await.expect("collecting");

    let listed = list_conversations(&pool, AnchorFilter::default())
        .await
        .expect("listing");
    assert_eq!(listed.len(), 1, "the conversation outlives the repo");
    assert_eq!(
        listed[0].repo_identity.as_deref(),
        Some(identity.key()),
        "and remembers its anchor, so re-adding the repo re-links it"
    );
    assert_eq!(
        window(&pool, &guid, 1, 0, 0).await.expect("reading").len(),
        1
    );

    database.destroy(pool).await;
}

/// The dedupe that makes conversations affordable: the same words in two
/// different conversations are two addressable turns sharing ONE raw hash, so
/// they share one paid summary and one pair of vectors.
#[tokio::test]
async fn identical_turns_in_two_conversations_share_one_enrichment_key() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let (first, second) = (id('e'), id('f'));

    let repeated = "harness checks green, opening the PR now";
    store_conversation(&pool, &first, &[turn(1, repeated)]).await;
    store_conversation(&pool, &second, &[turn(9, repeated)]).await;

    let rows: Vec<(String, String)> = sqlx_pairs(
        &pool,
        "SELECT address, raw_hash FROM elements WHERE kind = 'turn' ORDER BY address",
    )
    .await;

    assert_eq!(rows.len(), 2, "two addressable turns");
    assert_ne!(rows[0].0, rows[1].0, "at different addresses");
    assert_eq!(
        rows[0].1, rows[1].1,
        "sharing one enrichment key — this is the whole spend story"
    );

    database.destroy(pool).await;
}

/// Turn elements live in a reserved `parser_version` namespace. Without it, a
/// canonical form that happens to hash equal to a source file's blob would make
/// that file's next scan read rootless turn rows and hard-fail as corrupt.
#[tokio::test]
async fn a_turn_element_is_invisible_to_a_code_scan_of_the_same_bytes() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let guid = id('0');

    let text = "fn collide() {}";
    let turns = [turn(1, text)];
    store_conversation(&pool, &guid, &turns).await;

    // A code blob whose id IS the turn's content address — the collision.
    let blob = BlobRef::new(turns[0].blob_sha()).expect("a turn hash is a legal blob key");
    upsert_element_tree(
        &pool,
        &blob,
        PARSER_VERSION,
        &Element::new(
            ElementKind::File,
            "rust",
            "collide.rs",
            "src/collide.rs",
            Span::new(1, 1),
            text,
        ),
        |element| element.kind != ElementKind::File,
    )
    .await
    .expect("storing the colliding parse");

    let tree = get_elements(&pool, &blob, PARSER_VERSION)
        .await
        .expect("a code scan must not trip over turn rows")
        .expect("the file element is there");
    assert_eq!(tree.kind, ElementKind::File);

    // The namespace is what kept them apart: same blob, two parser versions.
    let namespaces: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT parser_version FROM elements WHERE blob_sha = $1 ORDER BY 1",
    )
    .bind(blob.as_str())
    .fetch_all(&pool)
    .await
    .expect("reading the namespaces back");
    assert_eq!(
        namespaces,
        {
            let mut both = vec![CONVERSATION_PARSER.to_string(), PARSER_VERSION.to_string()];
            both.sort();
            both
        },
        "one blob, two parser versions — that separation is what makes the collision harmless"
    );
    assert_eq!(
        window(&pool, &guid, 1, 0, 0).await.expect("reading").len(),
        1,
        "and the turn is still reachable in its own right"
    );

    database.destroy(pool).await;
}

/// The windowed fetch: contiguous, ordered, and honest where the conversation
/// ends rather than padding to the count asked for (ac-0005).
#[tokio::test]
async fn a_window_is_ordered_and_honest_at_both_edges() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let guid = id('1');

    let turns: Vec<Turn> = (1..=6).map(|n| turn(n, &format!("turn {n}"))).collect();
    store_conversation(&pool, &guid, &turns).await;

    let middle = window(&pool, &guid, 3, 1, 2).await.expect("windowing");
    assert_eq!(
        middle.iter().map(|t| t.turn_no).collect::<Vec<_>>(),
        vec![2, 3, 4, 5],
        "the caller's own -1/+2 around turn 3, in order"
    );

    // Past the start: no negative ordinals, no padding, no error.
    let head = window(&pool, &guid, 2, 10, 0).await.expect("windowing");
    assert_eq!(
        head.iter().map(|t| t.turn_no).collect::<Vec<_>>(),
        vec![1, 2]
    );

    // Past the end: the same honesty.
    let tail = window(&pool, &guid, 5, 0, 10).await.expect("windowing");
    assert_eq!(
        tail.iter().map(|t| t.turn_no).collect::<Vec<_>>(),
        vec![5, 6]
    );

    // And the round trip is lossless where it matters.
    assert_eq!(middle[1].role, turns[2].role);
    assert_eq!(middle[1].source, TurnSource::Peer);
    assert_eq!(middle[1].head_sha, turns[2].head_sha);
    assert_eq!(middle[1].at, "2026-08-27T09:00:00Z");
    assert_eq!(
        middle[1].blob_sha(),
        turns[2].blob_sha(),
        "the stored form round-trips to the same content address"
    );

    database.destroy(pool).await;
}

/// Typed sub-items survive the JSONB round trip as typed values, which is what
/// makes the payload policy a contract rather than a formatting convention.
#[tokio::test]
async fn turn_items_round_trip_through_jsonb() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let guid = id('2');

    let mut rich = turn(1, "ran the gate");
    rich.items = vec![
        TurnItem::ToolCall {
            tool: "bash".to_string(),
            input: ToolInput::Verbatim {
                text: "cargo test --all".to_string(),
            },
        },
        TurnItem::ToolCall {
            tool: "write".to_string(),
            input: ToolInput::Elided {
                path: "crates/store/src/conversations.rs".to_string(),
                bytes: 21_878,
            },
        },
        TurnItem::ToolResult {
            tool: "bash".to_string(),
            head: "running 9 tests".to_string(),
            total_bytes: 91_233,
            truncated: true,
        },
    ];
    store_conversation(&pool, &guid, std::slice::from_ref(&rich)).await;

    let stored = window(&pool, &guid, 1, 0, 0).await.expect("reading back");
    assert_eq!(stored[0].items, rich.items);
    assert_eq!(
        stored[0].blob_sha(),
        rich.blob_sha(),
        "items are part of the canonical form, so they are part of the hash"
    );

    let rows = outline(&pool, &guid).await.expect("outlining");
    assert_eq!(
        rows[0].items, 3,
        "the outline counts them without carrying them"
    );

    database.destroy(pool).await;
}

/// The outline is the cheap browse: enough to choose a turn, never the turn.
#[tokio::test]
async fn an_outline_carries_the_first_line_and_nothing_more() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let guid = id('3');

    store_conversation(
        &pool,
        &guid,
        &[
            turn(1, "first line\nsecond line\nthird line"),
            turn(2, "a single line"),
        ],
    )
    .await;

    let rows = outline(&pool, &guid).await.expect("outlining");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].turn_no, 1);
    assert_eq!(
        rows[0].first_line, "first line",
        "the first line only — the rest is what `get` is for"
    );
    assert_eq!(rows[0].role, TurnRole::Human);
    assert_eq!(rows[1].role, TurnRole::Agent);
    assert_eq!(rows[1].at, "2026-08-27T09:00:00Z");

    database.destroy(pool).await;
}

/// Anchor filters narrow by repository and by path prefix, and a path prefix is
/// a PREFIX — not a `LIKE` pattern that reads `_` as a wildcard.
#[tokio::test]
async fn listing_narrows_by_anchor() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let mut here = conversation(&id('4'), Some("github.com/x/here"));
    here.worktree = Some("/srv/a_b/deep".to_string());
    let mut elsewhere = conversation(&id('5'), Some("github.com/x/elsewhere"));
    elsewhere.worktree = Some("/srv/axb".to_string());
    for header in [&here, &elsewhere] {
        upsert_conversation(&pool, header).await.expect("storing");
    }

    let by_repo = list_conversations(
        &pool,
        AnchorFilter {
            repo: Some("github.com/x/here"),
            path_prefix: None,
            guid: None,
        },
    )
    .await
    .expect("listing by repo");
    assert_eq!(by_repo.len(), 1);
    assert_eq!(by_repo[0].guid, here.guid);

    let by_path = list_conversations(
        &pool,
        AnchorFilter {
            repo: None,
            path_prefix: Some("/srv/a_b"),
            guid: None,
        },
    )
    .await
    .expect("listing by path");
    assert_eq!(
        by_path.len(),
        1,
        "`_` is a literal in a path, not a single-character wildcard"
    );
    assert_eq!(by_path[0].guid, here.guid);

    assert_eq!(
        list_conversations(&pool, AnchorFilter::default())
            .await
            .expect("listing all")
            .len(),
        2
    );

    database.destroy(pool).await;
}

/// Removing a conversation takes its turns and its turn elements — and stops
/// there. A twin conversation carrying the same words keeps its own turn, and
/// the shared enrichment survives because an element still carries it
/// (ac-000a).
#[tokio::test]
async fn removing_a_conversation_spares_a_twins_turn_and_the_shared_summary() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let (doomed, twin) = (id('6'), id('7'));

    let repeated = "the same sentence in two different conversations entirely";
    let turns = [turn(1, repeated)];
    store_conversation(&pool, &doomed, &turns).await;
    store_conversation(&pool, &twin, &[turn(4, repeated)]).await;

    let raw_hash = turns[0].blob_sha();
    put_smart_content(
        &pool,
        &raw_hash,
        SUMMARIZER,
        &Summary {
            text: "shared".to_string(),
            tags: vec!["shared".to_string()],
            extras: BTreeMap::new(),
        },
    )
    .await
    .expect("storing the summary");

    let removed = delete_conversation(&pool, &doomed).await.expect("removing");
    assert!(removed.existed);
    assert_eq!(removed.turns, 1);
    assert_eq!(removed.elements, 1, "its OWN turn element, by address");

    let reclaimed = collect_garbage(&pool).await.expect("collecting");
    assert_eq!(
        reclaimed.summaries, 0,
        "the twin still carries the raw hash: {reclaimed:?}"
    );
    assert!(
        get_smart_content(&pool, &raw_hash, SUMMARIZER)
            .await
            .expect("reading back")
            .is_some()
    );
    assert_eq!(
        window(&pool, &twin, 4, 0, 0)
            .await
            .expect("reading the twin")
            .len(),
        1,
        "deleting by address, not by blob — the twin's identical turn stays"
    );
    assert!(
        window(&pool, &doomed, 1, 0, 0)
            .await
            .expect("reading the removed one")
            .is_empty()
    );

    // Removing what is not there is an answer, not a failure.
    assert!(
        !delete_conversation(&pool, &doomed)
            .await
            .expect("second removal")
            .existed
    );

    database.destroy(pool).await;
}

/// Once nothing carries a removed conversation's content, the ordinary
/// three-level GC reclaims what it paid for — no special case (ac-000a).
#[tokio::test]
async fn a_removed_conversations_unshared_enrichment_is_reclaimed_by_gc() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let guid = id('8');

    let turns = [turn(1, "a sentence nobody else in this database ever said")];
    store_conversation(&pool, &guid, &turns).await;
    let raw_hash = turns[0].blob_sha();
    put_smart_content(
        &pool,
        &raw_hash,
        SUMMARIZER,
        &Summary {
            text: "lonely".to_string(),
            tags: vec!["lonely".to_string()],
            extras: BTreeMap::new(),
        },
    )
    .await
    .expect("storing the summary");

    delete_conversation(&pool, &guid).await.expect("removing");

    let reclaimed = collect_garbage(&pool).await.expect("collecting");
    assert!(
        reclaimed.summaries > 0,
        "with no element carrying it, the summary is ordinary garbage: {reclaimed:?}"
    );
    assert!(
        get_smart_content(&pool, &raw_hash, SUMMARIZER)
            .await
            .expect("reading back")
            .is_none()
    );

    database.destroy(pool).await;
}

/// The size gate is the caller's verdict, recorded — not the store's, computed.
#[tokio::test]
async fn the_size_gate_verdict_is_stored_per_turn() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;
    let guid = id('9');

    let long = "x".repeat(SUMMARY_FLOOR + 1);
    store_conversation(&pool, &guid, &[turn(1, "short"), turn(2, &long)]).await;

    let rows: Vec<(String, bool)> = sqlx_flags(
        &pool,
        "SELECT address, enrich FROM elements WHERE kind = 'turn' ORDER BY address",
    )
    .await;
    let verdicts: BTreeMap<&str, bool> = rows
        .iter()
        .map(|(address, enrich)| (address.as_str(), *enrich))
        .collect();

    assert!(
        !verdicts[guid.turn_address(1).as_str()],
        "a five-word turn does not earn an LLM call"
    );
    assert!(
        verdicts[guid.turn_address(2).as_str()],
        "one over the floor does"
    );

    database.destroy(pool).await;
}

/// A claude subagent sidecar is a SEPARATE conversation that knows its parent,
/// and the link survives the ingest job that established it.
///
/// Cross-model review found (F-002) that ac-0004's promised link lived only in
/// an in-memory `SessionFile` and an `IngestReport` the async worker discarded:
/// after the job settled, nothing could navigate from a child to its parent.
/// This asserts the durable half — write the two conversations the way ingest
/// does, then read the relationship back through the SAME public list API a CLI
/// caller uses, with no in-memory state left over.
#[tokio::test]
async fn a_child_conversation_knows_its_parent_after_the_job_settles() {
    let database = FreshDatabase::create().await;
    let pool = database.migrated_pool().await;

    let parent_guid = ConversationId::new("11111111-1111-4111-8111-111111111111").unwrap();
    let child_guid = ConversationId::new("22222222-2222-4222-8222-222222222222").unwrap();

    let mut parent = conversation(&parent_guid, None);
    parent.title = Some("session main".to_string());
    upsert_conversation(&pool, &parent).await.unwrap();

    let mut child = conversation(&child_guid, None);
    child.title = Some("subagent agent-a018".to_string());
    child.parent = Some(parent_guid.clone());
    upsert_conversation(&pool, &child).await.unwrap();

    let listed = list_conversations(&pool, AnchorFilter::default())
        .await
        .expect("listing reads the relationship back");

    let stored_child = listed
        .iter()
        .find(|row| row.guid == child_guid)
        .expect("the child is listed");
    assert_eq!(
        stored_child.parent.as_ref(),
        Some(&parent_guid),
        "the child navigates to its parent through the public API, not through ingest state"
    );

    let stored_parent = listed
        .iter()
        .find(|row| row.guid == parent_guid)
        .expect("the parent is listed");
    assert_eq!(
        stored_parent.parent, None,
        "a main session has no parent; the column is null for the common case"
    );

    // A later poll that does not know the parent must not ERASE one an earlier
    // poll established — the ingest path re-upserts the header on every poll,
    // and only the poll that resolved the sidecar carries the link.
    let mut forgetful = conversation(&child_guid, None);
    forgetful.parent = None;
    upsert_conversation(&pool, &forgetful).await.unwrap();

    let relisted = list_conversations(&pool, AnchorFilter::default())
        .await
        .expect("listing again");
    assert_eq!(
        relisted
            .iter()
            .find(|row| row.guid == child_guid)
            .and_then(|row| row.parent.as_ref()),
        Some(&parent_guid),
        "the link is learned once and never forgotten"
    );

    database.destroy(pool).await;
}

/// Two text columns of every row a statement returns.
///
/// The store owns the sqlx edge, so these tests reach past its API only to
/// ASSERT on rows it wrote — never to write.
async fn sqlx_pairs(pool: &PgPool, statement: &str) -> Vec<(String, String)> {
    sqlx::query_as(statement)
        .fetch_all(pool)
        .await
        .expect("reading rows back")
}

async fn sqlx_flags(pool: &PgPool, statement: &str) -> Vec<(String, bool)> {
    sqlx::query_as(statement)
        .fetch_all(pool)
        .await
        .expect("reading rows back")
}
