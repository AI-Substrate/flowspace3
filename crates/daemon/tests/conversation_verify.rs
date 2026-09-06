//! Native verification changes HOME only in this single-test process.

mod support;

use fs3_core::{Config, DatabaseConfig, Harness, Turn, TurnRole, TurnSource};
use fs3_daemon::conversations::{IntakeRequest, intake};
use fs3_daemon::convo_ingest::{VerifyRequest, conversation_guid, verify};
use fs3_daemon::wiring::AppState;

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
#[test]
fn conversation_verify_contract() {
    let home = tempfile::tempdir().expect("isolated native stores");
    let sessions = home.path().join(".omp/agent/sessions/-workspace");
    std::fs::create_dir_all(&sessions).unwrap();
    for id in [
        "01a051b7-3b2c-7000-8987-3e66b28db4b6",
        "01a051b7-3b2c-7000-8987-000000000000",
        "01a051b7-3b2c-7000-8987-111111111111",
    ] {
        std::fs::write(sessions.join(format!("2026-09-06_{id}.jsonl")), []).unwrap();
    }
    // SAFETY: this binary has exactly one synchronous test; set HOME before
    // starting the runtime or any database/server threads.
    unsafe { std::env::set_var("HOME", home.path()) };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (database, state) = stack("conversation-verify-contract").await;
        let session = "01a051b7-3b2c-7000-8987-3e66b28db4b6";
        let guid = conversation_guid(Harness::Omp, session);
        store_at(
            &state,
            guid.as_str(),
            Some(ANCHOR),
            Some("/srv/anchored"),
            vec![turn(1, "delivered")],
        )
        .await;

        let report = verify(
            &state,
            &VerifyRequest {
                pij_id: None,
                session_id: Some(session.to_string()),
                harness: Some("omp".to_string()),
            },
        )
        .await
        .expect("the indexed session was delivered");
        assert_eq!(report.guid, guid.as_str());
        assert_eq!(report.address, guid.address());
        assert_eq!(report.turns, 1);
        assert_eq!(report.repo.as_deref(), Some(ANCHOR));
        assert_eq!(report.worktree.as_deref(), Some("/srv/anchored"));
        assert_eq!(report.last_turn_at, "2026-08-27T09:00:00Z");

        let absent_session = "01a051b7-3b2c-7000-8987-000000000000";
        let absent_guid = conversation_guid(Harness::Omp, absent_session);
        let absent = verify(
            &state,
            &VerifyRequest {
                pij_id: None,
                session_id: Some(absent_session.to_string()),
                harness: Some("omp".to_string()),
            },
        )
        .await
        .expect_err("a never-indexed session is not delivered");
        assert_eq!(absent.code, "FS3-E-QUERY-CONVERSATION-NOT-FOUND");
        assert_eq!(absent.details["guid"], absent_guid.as_str());
        assert!(!absent.details.contains_key("turns"));

        let empty_session = "01a051b7-3b2c-7000-8987-111111111111";
        let empty_guid = conversation_guid(Harness::Omp, empty_session);
        store_at(&state, empty_guid.as_str(), None, None, Vec::new()).await;
        let empty = verify(
            &state,
            &VerifyRequest {
                pij_id: None,
                session_id: Some(empty_session.to_string()),
                harness: Some("omp".to_string()),
            },
        )
        .await
        .expect_err("a zero-turn row delivered nothing");
        assert_eq!(empty.code, "FS3-E-QUERY-CONVERSATION-NOT-FOUND");
        assert_eq!(empty.details["guid"], empty_guid.as_str());
        assert_eq!(empty.details["turns"], 0);
        assert!(empty.message.contains("zero turns"));

        let auth = support::auth("conversation-verify-route");
        let base = support::spawn(fs3_daemon::router(state.clone(), auth.auth)).await;
        let envelope: serde_json::Value = reqwest::Client::new()
            .get(format!(
                "{base}/conversations/verify?session_id={session}&harness=omp"
            ))
            .bearer_auth(&auth.key)
            .send()
            .await
            .expect("verify route answers")
            .json()
            .await
            .expect("verify route returns an envelope");
        assert_eq!(envelope["ok"], true);
        assert_eq!(envelope["command"], "conversation verify");
        assert_eq!(envelope["data"]["guid"], guid.as_str());
        assert_eq!(envelope["data"]["last_turn_at"], "2026-08-27T09:00:00Z");

        let response = reqwest::Client::new()
            .get(format!(
                "{base}/conversations/verify?session_id={absent_session}&harness=omp"
            ))
            .bearer_auth(&auth.key)
            .send()
            .await
            .expect("negative verify route answers");
        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
        let negative: serde_json::Value = response
            .json()
            .await
            .expect("negative verify returns an envelope");
        assert_eq!(
            negative["error"]["code"],
            "FS3-E-QUERY-CONVERSATION-NOT-FOUND"
        );

        database.destroy(state.db).await;
    });
}
