//! Conversation intake: what a post costs, and what a re-post does not.
//!
//! The load-bearing property is the iterative-append contract (req-0027): a
//! conversation grows across many posts, and a post that carries turns already
//! stored must enqueue NOTHING. These tests assert on the QUEUE rather than on
//! rows, because the rows were never the risk — the primary key protects them.
//! What a duplicate post would have cost is a second summary and a second pair
//! of vectors for text already paid for, and only the queue can show that.

mod support;

use fs3_core::{Config, DatabaseConfig, ToolInput, Turn, TurnItem, TurnRole, TurnSource};
use fs3_daemon::conversations::{IntakeRequest, UNANCHORED, intake};
use fs3_daemon::wiring::AppState;
use fs3_store::PgPool;

const ANCHOR: &str = "git:github.com/fs3/anchored";
const GUID: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
const OTHER_GUID: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c9";

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
        role: TurnRole::Agent,
        source: TurnSource::Peer,
        head_sha: None,
        at: "2026-08-27T09:00:00Z".to_string(),
        body: body.to_string(),
        items: Vec::new(),
    }
}

fn request(guid: &str, turns: Vec<Turn>) -> IntakeRequest {
    IntakeRequest {
        guid: guid.to_string(),
        repo_identity: Some(ANCHOR.to_string()),
        worktree: Some("/srv/anchored".to_string()),
        base_sha: None,
        title: Some("a fleet session".to_string()),
        started_at: "2026-08-27T09:00:00Z".to_string(),
        turns,
    }
}

/// Below the byte gate.
fn short() -> Turn {
    turn(1, "ship it")
}

/// Over the byte gate, and DIFFERENT per ordinal.
///
/// The ordinal has to be in the text, not just in `turn_no`: a turn's content
/// address deliberately excludes its position, so two turns of the same words
/// are one piece of paid work. That is the property
/// [`identical_turns_in_two_conversations_cost_one_enrichment`] exists to
/// prove — and it is exactly what would make a "two jobs" assertion elsewhere
/// silently measure the dedupe instead of the delta.
fn long(turn_no: u32) -> Turn {
    turn(
        turn_no,
        &format!("turn {turn_no}: we ruled that the anchor is a pointer, not ownership. ")
            .repeat(12),
    )
}

/// Over the gate, and the SAME words whatever the ordinal or conversation.
fn same_words(turn_no: u32) -> Turn {
    turn(
        turn_no,
        &"the words two agents both happened to say, at length, more than once. ".repeat(8),
    )
}

/// Every queued job of a kind, with its payload identity.
async fn queued(pool: &PgPool, kind: &str) -> Vec<(String, serde_json::Value)> {
    sqlx::query_as("SELECT dedupe_key, payload FROM jobs WHERE kind = $1 ORDER BY dedupe_key")
        .bind(kind)
        .fetch_all(pool)
        .await
        .expect("reading the queue")
}

/// The gate decides per turn: below it, raw vector only; at or above it, a
/// summary as well. Both paths in one post, so the assertion is about the
/// GATE and not about intake being wired at all.
#[tokio::test]
async fn the_size_gate_decides_which_turns_earn_a_summary() {
    let (database, state) = stack("conv-gate").await;

    let report = intake(&state, request(GUID, vec![short(), long(2)]))
        .await
        .expect("intake accepts the batch");

    assert_eq!(report.accepted, 2);
    assert_eq!(report.already_stored, 0);
    assert_eq!(
        report.summarized, 1,
        "only the turn at or above the floor earns an LLM call"
    );

    let summaries = queued(&state.db, "summarize").await;
    assert_eq!(summaries.len(), 1, "one summarize job, for the long turn");

    // Both turns are embedded, and they ride ONE batch — the same batching the
    // scanner gets, not a job per turn.
    let embeds = queued(&state.db, "embed").await;
    assert_eq!(embeds.len(), 1);
    let items = embeds[0].1["items"].as_array().expect("a batch of items");
    assert_eq!(items.len(), 2, "every turn earns a raw vector");
    assert_eq!(embeds[0].1["source"], "raw");

    database.destroy(state.db).await;
}

/// **The iterative-append contract.** A second post carrying one old turn and
/// one new one must enqueue work for the NEW turn only.
///
/// Mutation check: have `intake` enqueue from the posted batch rather than from
/// what the store accepted, and the summarize count here goes to 2.
#[tokio::test]
async fn a_re_post_enqueues_only_the_delta() {
    let (database, state) = stack("conv-delta").await;

    intake(&state, request(GUID, vec![long(1)]))
        .await
        .expect("first post");
    let after_first = queued(&state.db, "summarize").await.len();
    assert_eq!(after_first, 1);

    // Turn 1 again — already stored — plus a genuinely new turn 2.
    let second = intake(&state, request(GUID, vec![long(1), long(2)]))
        .await
        .expect("second post");

    assert_eq!(second.already_stored, 1, "the overlap is recognised");
    assert_eq!(second.accepted, 1);
    assert_eq!(second.summarized, 1, "and only the new turn is charged for");

    assert_eq!(
        queued(&state.db, "summarize").await.len(),
        2,
        "two summarize jobs in total, not three"
    );

    // A post that is ENTIRELY overlap costs nothing at all.
    let third = intake(&state, request(GUID, vec![long(1), long(2)]))
        .await
        .expect("third post");
    assert_eq!(third.accepted, 0);
    assert_eq!(third.summarized, 0);
    assert_eq!(queued(&state.db, "summarize").await.len(), 2);

    database.destroy(state.db).await;
}

/// Agents repeat themselves. The same words in two different conversations are
/// two addressable turns and ONE piece of paid work, because both jobs are
/// keyed by content.
#[tokio::test]
async fn identical_turns_in_two_conversations_cost_one_enrichment() {
    let (database, state) = stack("conv-dedupe").await;

    intake(&state, request(GUID, vec![same_words(1)]))
        .await
        .expect("first conversation");
    intake(&state, request(OTHER_GUID, vec![same_words(7)]))
        .await
        .expect("second conversation");

    let summaries = queued(&state.db, "summarize").await;
    assert_eq!(
        summaries.len(),
        1,
        "one dedupe key for one body, however many conversations said it"
    );

    let embeds = queued(&state.db, "embed").await;
    assert_eq!(embeds.len(), 1, "and one embed batch, for the same reason");

    // Both turns are stored and addressable, though — the dedupe is about
    // spend, never about losing a turn.
    let addresses: Vec<String> =
        sqlx::query_scalar("SELECT address FROM elements WHERE kind = 'turn' ORDER BY address")
            .fetch_all(&state.db)
            .await
            .expect("reading turn elements");
    assert_eq!(addresses.len(), 2);

    database.destroy(state.db).await;
}

/// Anchored conversations are charged to their repository, so they get whatever
/// provider that repository selected — the same treatment as its code.
#[tokio::test]
async fn an_anchored_conversation_is_enriched_under_its_repos_identity() {
    let (database, state) = stack("conv-anchored").await;

    let report = intake(&state, request(GUID, vec![long(1)]))
        .await
        .expect("intake");
    assert_eq!(report.identity, ANCHOR);

    let summaries = queued(&state.db, "summarize").await;
    assert_eq!(summaries[0].1["identity"], ANCHOR);

    database.destroy(state.db).await;
}

/// An unanchored conversation rides a reserved identity, and an ORPHANED
/// anchor — one naming a repository that was never registered — is not a
/// special case at all: provider resolution is a map lookup with a default, so
/// an identity nobody configured takes the default with no branch anywhere.
#[tokio::test]
async fn unanchored_and_orphaned_conversations_both_resolve_to_a_provider() {
    let (database, state) = stack("conv-unanchored").await;

    let mut unanchored = request(GUID, vec![long(1)]);
    unanchored.repo_identity = None;
    unanchored.worktree = None;
    let report = intake(&state, unanchored).await.expect("intake");
    assert_eq!(report.identity, UNANCHORED);
    assert_eq!(report.summarized, 1, "and it is still enriched");

    // The orphan: an anchor naming a repository this store has never heard of.
    // Nothing is registered in this database at all, so ANCHOR above was
    // already orphaned — which is the point. Both resolve.
    let orphaned = request(OTHER_GUID, vec![long(2)]);
    let report = intake(&state, orphaned).await.expect("intake");
    assert_eq!(report.identity, ANCHOR);

    // Both providers resolve to the default instance, so both write vectors
    // into the same comparable space.
    assert_eq!(
        state.embedder_key(UNANCHORED),
        state.embedder_key(ANCHOR),
        "an unconfigured identity takes the default embedder, so the spaces match"
    );

    database.destroy(state.db).await;
}

/// The payload policy is enforced HERE, not trusted from the client: a posted
/// turn with an unshaped write body and an unshaped tool result is stored
/// shaped, and the hash is the hash of the SHAPED form.
#[tokio::test]
async fn the_payload_policy_is_enforced_at_intake_not_trusted() {
    let (database, state) = stack("conv-policy").await;

    let mut unshaped = turn(1, "wrote the module");
    unshaped.items = vec![
        TurnItem::ToolCall {
            tool: "write".to_string(),
            input: ToolInput::Verbatim {
                text: format!("crates/store/src/conversations.rs\n{}", "x".repeat(40_000)),
            },
        },
        TurnItem::ToolResult {
            tool: "bash".to_string(),
            head: "y".repeat(20_000),
            total_bytes: 0,
            truncated: false,
        },
    ];

    intake(&state, request(GUID, vec![unshaped]))
        .await
        .expect("intake");

    let stored = fs3_store::window(
        &state.db,
        &fs3_core::ConversationId::new(GUID).unwrap(),
        1,
        0,
        0,
    )
    .await
    .expect("reading the turn back");

    let TurnItem::ToolCall { input, .. } = &stored[0].items[0] else {
        panic!("still a call");
    };
    assert_eq!(
        *input,
        ToolInput::Elided {
            path: "crates/store/src/conversations.rs".to_string(),
            bytes: 40_034,
        },
        "the write body never reached the database"
    );

    let TurnItem::ToolResult {
        head,
        total_bytes,
        truncated,
        ..
    } = &stored[0].items[1]
    else {
        panic!("still a result");
    };
    assert_eq!(head.len(), 512);
    assert_eq!(*total_bytes, 20_000);
    assert!(*truncated);

    // And the enrichment was queued for the SHAPED text, not the posted one:
    // the raw text of the embed item is the canonical form of what is stored.
    let embeds = queued(&state.db, "embed").await;
    let text = embeds[0].1["items"][0][1].as_str().expect("the raw text");
    assert!(
        text.len() < 2_000,
        "the enqueued text is the shaped form, not 60KB of posted payload"
    );
    assert_eq!(text, stored[0].canonical());

    database.destroy(state.db).await;
}

/// Removing the anchored repository, then collecting, must leave the
/// conversation and everything it paid for intact — the anchor is a pointer,
/// not ownership (ac-0007), and a stored turn is a root of reference.
#[tokio::test]
async fn removing_the_anchor_and_collecting_leaves_the_conversation_whole() {
    let (database, state) = stack("conv-gc").await;

    let root = "/srv/anchored-root";
    let identity = fs3_core::RepoIdentity::from_path(std::path::Path::new(root));
    let worktree = fs3_store::register_worktree(&state.db, &identity, root, Some("main"))
        .await
        .expect("registering");
    fs3_store::sync_worktree_files(
        &state.db,
        worktree,
        &[(
            "src/a.rs".to_string(),
            fs3_core::BlobRef::new("a".repeat(40)).unwrap(),
        )],
    )
    .await
    .expect("mapping a file");

    let mut anchored = request(GUID, vec![long(1)]);
    anchored.repo_identity = Some(identity.key().to_string());
    intake(&state, anchored).await.expect("intake");

    fs3_store::remove_root(&state.db, root)
        .await
        .expect("removing the anchored repo must not fail on the anchor");
    let reclaimed = fs3_store::collect_garbage(&state.db)
        .await
        .expect("collecting");

    assert_eq!(
        reclaimed.elements, 0,
        "a stored turn is a root of reference: {reclaimed:?}"
    );
    assert_eq!(
        reclaimed.jobs, 0,
        "and so its queued enrichment is not garbage either: {reclaimed:?}"
    );
    assert_eq!(
        fs3_store::window(
            &state.db,
            &fs3_core::ConversationId::new(GUID).unwrap(),
            1,
            0,
            0
        )
        .await
        .expect("reading")
        .len(),
        1
    );

    database.destroy(state.db).await;
}

/// A guid that is not a conversation id is a usage error with a fix, not a
/// 500 and not a silently-minted second conversation.
#[tokio::test]
async fn a_malformed_guid_is_refused_with_a_fix() {
    let (database, state) = stack("conv-badguid").await;

    let failure = intake(&state, request("not-a-uuid", vec![short()]))
        .await
        .expect_err("a malformed guid cannot address a conversation");
    assert!(!failure.retryable);
    assert!(
        failure.fix.contains("uuid"),
        "the fix must say what to post: {}",
        failure.fix
    );

    // Turn 0 is the other way to have no address: the sequence starts at 1.
    let failure = intake(&state, request(GUID, vec![turn(0, "before the start")]))
        .await
        .expect_err("turn_no 0 is not a position in a sequence");
    assert!(!failure.retryable);

    database.destroy(state.db).await;
}
