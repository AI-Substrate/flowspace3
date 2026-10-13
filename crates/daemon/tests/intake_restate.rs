//! Restating the conversation turns stored before the 2026-10-13 intake fixes.
//!
//! Turns are append-only, so the reader fixes reach only new turns; migration
//! 0030 queues every stored conversation and the `intake-restate` pass corrects
//! it in place. These pin what the pass must and must not do: correct the
//! stored row, re-enrich ONLY the turns whose text changed, change nothing on a
//! second run, and hold back while the enrichment queue is deep.

mod support;

use fs3_core::{
    Config, Conversation, ConversationId, DatabaseConfig, ToolInput, Turn, TurnItem, TurnRole,
    TurnSource,
};
use fs3_daemon::intake_restate::IntakeRestate;
use fs3_daemon::reconcile::Reconcile;
use fs3_daemon::wiring::AppState;
use fs3_store::PgPool;

const GUID: &str = "0c0ffee0-0000-4000-8000-0000000000a1";

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

fn turn(
    turn_no: u32,
    role: TurnRole,
    source: TurnSource,
    body: &str,
    items: Vec<TurnItem>,
) -> Turn {
    Turn {
        turn_no,
        role,
        source,
        head_sha: None,
        at: "2026-09-20T10:00:00Z".to_string(),
        body: body.to_string(),
        items,
    }
}

/// The JSON input a claude `Write` records, on one line.
fn write_input() -> String {
    serde_json::json!({"file_path": "/w/src/lib.rs", "content": "fn main() {}\n".repeat(200)})
        .to_string()
}

/// A claude conversation as the old intake stored it, queued as migration
/// 0030 queues every conversation that existed when it ran.
async fn stored_the_old_way(state: &AppState) -> ConversationId {
    let guid = ConversationId::new(GUID).expect("a guid");
    fs3_store::upsert_conversation(
        &state.db,
        &Conversation {
            guid: guid.clone(),
            repo_identity: None,
            worktree: None,
            base_sha: None,
            title: Some("old intake".to_string()),
            started_at: "2026-09-20T10:00:00Z".to_string(),
            parent: None,
        },
    )
    .await
    .expect("a header");
    let input = write_input();
    let turns = [
        turn(
            1,
            TurnRole::Human,
            TurnSource::Human,
            "fix the watcher",
            vec![],
        ),
        turn(
            2,
            TurnRole::Agent,
            TurnSource::System,
            "",
            vec![TurnItem::ToolCall {
                tool: "Write".to_string(),
                input: ToolInput::Elided {
                    path: input.clone(),
                    bytes: input.len() as u64,
                },
            }],
        ),
        turn(
            3,
            TurnRole::Human,
            TurnSource::System,
            "",
            vec![TurnItem::ToolResult {
                tool: "Write".to_string(),
                head: "File created successfully".to_string(),
                total_bytes: 25,
                truncated: false,
            }],
        ),
        turn(
            4,
            TurnRole::Human,
            TurnSource::Peer,
            "<task-notification>\n<task-id>b1</task-id>\n<status>completed</status>\n</task-notification>",
            vec![],
        ),
        turn(
            5,
            TurnRole::Human,
            TurnSource::Peer,
            "Another Claude session sent a message:\n<cross-session-message from=\"uds:/x\">go</cross-session-message>",
            vec![],
        ),
    ];
    fs3_store::append_turns(&state.db, &guid, &turns, |_| false)
        .await
        .expect("stored");
    sqlx::query(
        "UPDATE conversations SET agent_harness = 'claude', restate_after = 0 WHERE guid = $1::uuid",
    )
    .bind(GUID)
    .execute(&state.db)
    .await
    .expect("queued as 0030 queues it");
    guid
}

/// Each turn's role, source and stored hash, in order.
async fn stored_turns(pool: &PgPool) -> Vec<(i32, String, String, String)> {
    sqlx::query_as(
        "SELECT turn_no, role, source, blob_sha FROM turns
          WHERE conversation_id = $1::uuid ORDER BY turn_no",
    )
    .bind(GUID)
    .fetch_all(pool)
    .await
    .expect("turns")
}

/// Every turn element: its address, hash and row id.
async fn turn_elements(pool: &PgPool) -> Vec<(String, String, i64)> {
    sqlx::query_as(
        "SELECT address, raw_hash, id FROM elements WHERE kind = 'turn' ORDER BY address",
    )
    .fetch_all(pool)
    .await
    .expect("elements")
}

/// The embed and summarize jobs, by payload.
async fn enrichment_jobs(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT payload::text FROM jobs WHERE kind IN ('embed', 'summarize') ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .expect("jobs")
}

async fn drain(pass: &mut IntakeRestate) -> usize {
    let mut changed = 0;
    for _ in 0..10 {
        let pass = pass.reconcile().await.expect("a pass");
        if pass.changed == 0 {
            break;
        }
        changed += pass.changed;
    }
    changed
}

#[tokio::test]
async fn stored_turns_are_restated_and_only_changed_text_is_enriched_again() {
    let (_database, state) = stack("restate-once").await;
    let guid = stored_the_old_way(&state).await;
    let before = turn_elements(&state.db).await;
    let stored = stored_turns(&state.db).await;

    let changed = drain(&mut IntakeRestate::new(state.clone())).await;

    assert_eq!(changed, 3, "the write path, the tool result and the notice");
    let after = stored_turns(&state.db).await;
    let who: Vec<(&str, &str)> = after
        .iter()
        .map(|(_, role, source, _)| (role.as_str(), source.as_str()))
        .collect();
    assert_eq!(
        who,
        [
            ("human", "human"),
            ("agent", "system"),
            ("agent", "system"),
            ("human", "system"),
            ("human", "peer"),
        ]
    );

    let window = fs3_store::window(&state.db, &guid, 2, 0, 0)
        .await
        .expect("a window");
    let TurnItem::ToolCall {
        input: ToolInput::Elided { path, .. },
        ..
    } = &window[0].items[0]
    else {
        panic!("the write is still elided: {:?}", window[0].items);
    };
    assert_eq!(path, "/w/src/lib.rs", "the body is gone, the path is kept");

    let elements = turn_elements(&state.db).await;
    assert_eq!(elements.len(), before.len(), "one element per turn, still");
    for ((address, hash, id), (_, old_hash, old_id)) in elements.iter().zip(&before) {
        if address.ends_with("#t2") {
            assert_ne!(hash, old_hash, "the write turn's text changed");
            assert_eq!(hash, &after[1].3, "turn and element agree on the new text");
        } else {
            assert_eq!(
                (hash, id),
                (old_hash, old_id),
                "{address}: role or source alone keeps the element and its vectors"
            );
        }
    }
    assert_eq!(
        stored[2].3, after[2].3,
        "the tool result's hash did not move"
    );

    let jobs = enrichment_jobs(&state.db).await;
    assert_eq!(
        jobs.len(),
        1,
        "one vector for the one changed text: {jobs:?}"
    );
    assert!(jobs[0].contains(&after[1].3), "{jobs:?}");
    assert_eq!(
        fs3_store::conversations_to_restate(&state.db)
            .await
            .expect("count"),
        0
    );
}

#[tokio::test]
async fn a_second_run_changes_nothing() {
    let (_database, state) = stack("restate-twice").await;
    stored_the_old_way(&state).await;
    drain(&mut IntakeRestate::new(state.clone())).await;
    let turns = stored_turns(&state.db).await;
    let elements = turn_elements(&state.db).await;
    let jobs = enrichment_jobs(&state.db).await;

    // Queue it again, as a restart before the cursor cleared would leave it.
    sqlx::query("UPDATE conversations SET restate_after = 0 WHERE guid = $1::uuid")
        .bind(GUID)
        .execute(&state.db)
        .await
        .expect("requeued");
    let changed = drain(&mut IntakeRestate::new(state.clone())).await;

    assert_eq!(changed, 0);
    assert_eq!(stored_turns(&state.db).await, turns);
    assert_eq!(turn_elements(&state.db).await, elements);
    assert_eq!(enrichment_jobs(&state.db).await, jobs);
    assert_eq!(
        fs3_store::conversations_to_restate(&state.db)
            .await
            .expect("count"),
        0
    );
}

#[tokio::test]
async fn a_deep_enrichment_queue_holds_the_restatement_back() {
    let (_database, state) = stack("restate-held").await;
    stored_the_old_way(&state).await;
    fs3_store::enqueue_job(
        &state.db,
        "embed",
        "embed:live-work",
        &serde_json::json!({"items": []}),
        std::time::Duration::ZERO,
    )
    .await
    .expect("live work");
    let stored = stored_turns(&state.db).await;

    let changed = drain(&mut IntakeRestate::with_ceiling(state.clone(), 0)).await;

    assert_eq!(changed, 0, "one pending job is over a ceiling of zero");
    assert_eq!(stored_turns(&state.db).await, stored);
    assert_eq!(
        fs3_store::conversations_to_restate(&state.db)
            .await
            .expect("count"),
        1,
        "still queued for when the queue drains"
    );
}

#[tokio::test]
async fn a_conversation_stored_after_the_fix_is_never_queued() {
    let (_database, state) = stack("restate-new").await;
    fs3_store::upsert_conversation(
        &state.db,
        &Conversation {
            guid: ConversationId::new(GUID).expect("a guid"),
            repo_identity: None,
            worktree: None,
            base_sha: None,
            title: None,
            started_at: "2026-10-13T10:00:00Z".to_string(),
            parent: None,
        },
    )
    .await
    .expect("a header");
    assert_eq!(
        fs3_store::conversations_to_restate(&state.db)
            .await
            .expect("count"),
        0
    );
}
