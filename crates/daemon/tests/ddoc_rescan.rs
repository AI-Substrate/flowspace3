//! Unchanged ddoc blobs still refresh world-derived metadata.

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fs3_core::ddoc_envelope::{DdocEdge, DdocGraph};
use fs3_core::{
    BlobRef, Config, DatabaseConfig, DdocSchemaFacts, EmbedBasis, RepoIdentity, content_hash,
};
use fs3_daemon::ddoc::DdocTooling;
use fs3_daemon::roots::ScanFileJob;
use fs3_daemon::scan::{self, PARSER_VERSION};
use fs3_daemon::wiring::AppState;
use serde_json::Value;

const SOURCE: &[u8] = br##"{
  "dd": { "schema": "review/rescan", "sweep_exclude": true },
  "sections": [
    { "name": "tasks", "value": [
      { "id": "tk-0001", "state": "checked", "title": "Schema-ranked text", "noise": "Fallback-only text", "result": "#checks/deep/tk-0001" }
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

fn tooling_without_facts(version: &str) -> DdocTooling {
    let mut tooling = tooling(version, &["checked"]);
    tooling.facts.clear();
    tooling
}

fn tooling_with_task_prose(version: &str) -> DdocTooling {
    let mut tooling = tooling(version, &["checked"]);
    tooling
        .facts
        .get_mut("review/rescan")
        .expect("fixture schema facts")
        .prose_fields
        .insert("tasks".to_owned(), vec!["title".to_owned()]);
    tooling
}

fn task(root: &fs3_core::Element) -> &fs3_core::DdocMeta {
    root.children[0].children[0]
        .ddoc
        .as_deref()
        .expect("task row metadata")
}

fn row_content(root: &fs3_core::Element) -> Vec<(String, String, String)> {
    fn collect(element: &fs3_core::Element, rows: &mut Vec<(String, String, String)>) {
        if element.kind == fs3_core::ElementKind::Row {
            rows.push((
                element.address.clone(),
                element.raw_text.clone(),
                element.raw_hash().to_owned(),
            ));
        }
        for child in &element.children {
            collect(child, rows);
        }
    }

    let mut rows = Vec::new();
    collect(root, &mut rows);
    rows
}

async fn stored(fixture: &Fixture) -> fs3_core::Element {
    fs3_store::get_elements(&fixture.state.db, &fixture.blob, PARSER_VERSION)
        .await
        .expect("read stored tree")
        .tree
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

#[tokio::test]
async fn unchanged_same_version_ddoc_reparses_when_schema_facts_appear() {
    let fixture = fixture("ddoc_reparse_facts").await;
    fixture
        .state
        .set_ddoc_tooling(fixture.worktree_id, tooling_without_facts("0.1.0"))
        .await;
    scan::run(&fixture.state, fixture.job.clone())
        .await
        .expect("initial same-version scan without field facts");
    let first = stored(&fixture).await;
    assert_eq!(task(&first).embed_basis, EmbedBasis::Fallback);
    assert_eq!(task(&first).tooling_version.as_deref(), Some("0.1.0"));
    let fallback_text = first.children[0].children[0].raw_text.clone();
    assert!(fallback_text.contains("Fallback-only text"));

    fixture
        .state
        .set_ddoc_tooling(fixture.worktree_id, tooling_with_task_prose("0.1.0"))
        .await;
    scan::run(&fixture.state, fixture.job.clone())
        .await
        .expect("same version reparses after facts appear");
    let refreshed = stored(&fixture).await;
    assert_eq!(task(&refreshed).embed_basis, EmbedBasis::SchemaDeclared);
    assert_eq!(task(&refreshed).tooling_version.as_deref(), Some("0.1.0"));
    let declared_text = &refreshed.children[0].children[0].raw_text;
    assert!(declared_text.contains("Schema-ranked text"));
    assert!(!declared_text.contains("Fallback-only text"));
    assert_ne!(declared_text, &fallback_text);

    finish(fixture).await;
}

#[tokio::test]
async fn unchanged_current_snapshot_reparse_preserves_row_text_and_hashes() {
    let fixture = fixture("ddoc_reparse_identity").await;
    fixture
        .state
        .set_ddoc_tooling(fixture.worktree_id, tooling_with_task_prose("0.1.0"))
        .await;
    scan::run(&fixture.state, fixture.job.clone())
        .await
        .expect("initial current-snapshot scan");
    let before = row_content(&stored(&fixture).await);

    scan::run(&fixture.state, fixture.job.clone())
        .await
        .expect("presented unchanged ddoc reparses");
    let after = row_content(&stored(&fixture).await);
    assert_eq!(after, before);

    finish(fixture).await;
}
