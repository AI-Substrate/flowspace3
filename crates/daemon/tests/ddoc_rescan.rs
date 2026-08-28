//! Unchanged ddoc blobs still refresh world-derived metadata.

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fs3_core::ddoc_envelope::{DdocEdge, DdocGraph};
use fs3_core::{BlobRef, Config, DatabaseConfig, DdocSchemaFacts, RepoIdentity, content_hash};
use fs3_daemon::ddoc::DdocTooling;
use fs3_daemon::roots::ScanFileJob;
use fs3_daemon::scan::{self, PARSER_VERSION};
use fs3_daemon::wiring::AppState;
use serde_json::Value;

const SOURCE: &[u8] = br##"{
  "dd": { "schema": "review/rescan", "sweep_exclude": true },
  "sections": [
    { "name": "tasks", "value": [
      { "id": "tk-0001", "state": "checked", "result": "#checks/deep/tk-0001" }
    ] },
    { "name": "checks", "value": { "deep": { "tk-0001": [
      { "id": "ck-0001", "state": "unchecked" }
    ] } } }
  ]
}"##;

struct Fixture {
    database: support::FreshDatabase,
    state: AppState,
    root: PathBuf,
    worktree_id: i64,
    blob: BlobRef,
    job: Value,
}

async fn fixture(label: &str) -> Fixture {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let state = AppState::from_config(config).expect("wire fake daemon");
    fs3_store::migrate(&state.db)
        .await
        .expect("migrate test database");

    let root = support::temp_dir(label);
    std::fs::write(root.join("plan.dd.json"), SOURCE).expect("write ddoc source");
    let blob = BlobRef::new(content_hash(SOURCE)).expect("content hash is a blob key");
    let worktree_id = fs3_store::register_worktree(
        &state.db,
        &RepoIdentity::from_path(&root),
        &root.to_string_lossy(),
        Some("review"),
    )
    .await
    .expect("register worktree");
    fs3_store::sync_worktree_files(
        &state.db,
        worktree_id,
        &[("plan.dd.json".to_owned(), blob.clone())],
    )
    .await
    .expect("map ddoc source");
    let job = serde_json::to_value(ScanFileJob {
        worktree_id,
        identity: RepoIdentity::from_path(&root).key().to_owned(),
        path: "plan.dd.json".to_owned(),
        blob: blob.as_str().to_owned(),
    })
    .expect("serialize scan job");

    Fixture {
        database,
        state,
        root,
        worktree_id,
        blob,
        job,
    }
}

fn tooling(version: &str, terminal: &[&str]) -> DdocTooling {
    let facts = DdocSchemaFacts {
        schema: "review/rescan".to_owned(),
        prose_fields: BTreeMap::new(),
        string_fields: BTreeMap::new(),
        gate_terminal: terminal
            .iter()
            .map(|state| (*state).to_owned())
            .collect::<BTreeSet<_>>(),
    };
    DdocTooling {
        version: Some(version.to_owned()),
        facts: BTreeMap::from([("review/rescan".to_owned(), facts)]),
        graph: Some(DdocGraph {
            edges: vec![DdocEdge {
                from: "plan.dd.json".to_owned(),
                to: "plan.dd.json".to_owned(),
                address: "#checks/deep/tk-0001".to_owned(),
                rel: "derives".to_owned(),
                kind: "document".to_owned(),
                location: "$.sections[tasks].value[0].result".to_owned(),
            }],
        }),
    }
}

fn task(root: &fs3_core::Element) -> &fs3_core::DdocMeta {
    root.children[0].children[0]
        .ddoc
        .as_deref()
        .expect("task row metadata")
}

async fn stored(fixture: &Fixture) -> fs3_core::Element {
    fs3_store::get_elements(&fixture.state.db, &fixture.blob, PARSER_VERSION)
        .await
        .expect("read stored tree")
        .expect("tree exists")
}

async fn finish(fixture: Fixture) {
    let pool = fixture.state.db.clone();
    fixture.database.destroy(pool).await;
    let _ = std::fs::remove_dir_all(fixture.root);
}

#[tokio::test]
async fn unchanged_ddoc_reenriches_after_tooling_returns() {
    let fixture = fixture("ddoc_reenrich_present").await;
    scan::run(&fixture.state, fixture.job.clone())
        .await
        .expect("initial absent-tooling scan indexes rows");
    let first = stored(&fixture).await;
    assert!(task(&first).derived_state.is_none());
    assert!(task(&first).tooling_version.is_none());

    fixture
        .state
        .set_ddoc_tooling(fixture.worktree_id, tooling("dd-A", &["checked"]))
        .await;
    scan::run(&fixture.state, fixture.job.clone())
        .await
        .expect("unchanged blob re-enriches");
    let refreshed = stored(&fixture).await;
    assert_eq!(
        task(&refreshed)
            .derived_state
            .as_ref()
            .map(|state| state.complete),
        Some(false)
    );
    assert_eq!(task(&refreshed).gate_terminal, Some(true));
    assert_eq!(task(&refreshed).tooling_version.as_deref(), Some("dd-A"));
    assert_eq!(task(&refreshed).rels.len(), 1);

    finish(fixture).await;
}

#[tokio::test]
async fn unchanged_ddoc_rederives_when_tooling_version_changes() {
    let fixture = fixture("ddoc_rederive_version").await;
    fixture
        .state
        .set_ddoc_tooling(fixture.worktree_id, tooling("dd-A", &["checked"]))
        .await;
    scan::run(&fixture.state, fixture.job.clone())
        .await
        .expect("initial version A scan");
    let first = stored(&fixture).await;
    assert_eq!(
        task(&first)
            .derived_state
            .as_ref()
            .map(|state| state.complete),
        Some(false)
    );
    assert_eq!(task(&first).tooling_version.as_deref(), Some("dd-A"));

    fixture
        .state
        .set_ddoc_tooling(
            fixture.worktree_id,
            tooling("dd-B", &["checked", "unchecked"]),
        )
        .await;
    scan::run(&fixture.state, fixture.job.clone())
        .await
        .expect("same blob re-derives for version B");
    let refreshed = stored(&fixture).await;
    assert_eq!(
        task(&refreshed)
            .derived_state
            .as_ref()
            .map(|state| state.complete),
        Some(true)
    );
    assert_eq!(task(&refreshed).tooling_version.as_deref(), Some("dd-B"));

    finish(fixture).await;
}
