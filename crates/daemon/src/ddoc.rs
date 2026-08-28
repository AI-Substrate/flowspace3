//! Imperative adapter for dd's `ddocs` binary.
//!
//! One [`probe`] produces one corpus snapshot: the installed version, schemas
//! resolved by dd's own four-root ladder, and the whole edge graph. Callers
//! slice that snapshot per file; they never run `ddocs links` on a scan path.
//!
//! # Composer snap-in recipe
//!
//! No config key or refresh job is added. `AppState` owns one snapshot per
//! registered worktree:
//!
//! ```text
//! pub ddocs: Arc<RwLock<BTreeMap<i64, Arc<DdocTooling>>>>
//! ```
//!
//! `AppState::from_config` constructs the empty map. `add_root` and
//! `rescan_root`, after canonicalisation and registration, await exactly one
//! `probe(&root)`, replace that `worktree_id` entry, then enqueue the root's
//! scan batch. `scan::run` reads `job.worktree_id`; a missing entry uses
//! `DdocTooling::absent()`. Never probe per file and never use a process-global
//! cache.
//!
//! For each `*.dd.json` scan: select facts by exact `dd.schema`; call
//! `scan_ddoc_bytes`; upsert the tree; call `replace_file_refs` with the same
//! blob and `crate::scan::PARSER_VERSION` (including `&[]` for a successful
//! graph with no file edges); surface every unattached address through
//! [`record_unattached`]; and upsert the amended tree again.
//!
//! Accepted staleness: a batch sees the graph captured at that corpus event's
//! start. A mid-batch row may lack edges until the next add/rescan. A per-file
//! refresh, generation counter, TTL, or hidden invalidation scheme is outside
//! this unit because no cheap corpus change detector exists.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::{Read, Seek};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use fs3_core::ddoc_envelope::{
    DdocGraph, DdocSchemaRef, parse_graph, parse_schema_file, parse_schema_show, parse_validate,
    parse_version,
};
use fs3_core::{DdocRel, DdocSchemaFacts, Element, ElementKind, ElementTree, derive_state};
use fs3_store::DdocFileRef;
use serde_json::Value;

/// A corpus snapshot resolved through one installed ddocs binary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DdocTooling {
    /// The binary version that produced this snapshot.
    pub version: Option<String>,
    /// Schema facts keyed by the exact qualified `dd.schema` value.
    pub facts: BTreeMap<String, DdocSchemaFacts>,
    /// One whole-corpus graph, absent when tooling could not be trusted.
    pub graph: Option<DdocGraph>,
}

impl DdocTooling {
    /// Honest degradation: rows can still parse, but world-derived facts cannot.
    #[must_use]
    pub fn absent() -> Self {
        Self::default()
    }

    /// Whether this snapshot came from no usable `ddocs` binary.
    #[must_use]
    pub fn is_absent(&self) -> bool {
        self.version.is_none()
    }

    /// Facts for exactly this document schema. No scan-order fallback exists.
    #[must_use]
    pub fn facts_for(&self, schema: &str) -> Option<&DdocSchemaFacts> {
        self.facts.get(schema)
    }
}

/// Probe one corpus through the `ddocs` binary on `PATH`.
///
/// Missing binary, non-zero exit, timeout, or unparseable version/graph output
/// returns [`DdocTooling::absent`], never an error. Rows remain indexable.
pub async fn probe(root: &Path) -> DdocTooling {
    probe_with_binary(root, OsStr::new("ddocs")).await
}

async fn probe_with_binary(root: &Path, binary: &OsStr) -> DdocTooling {
    let Some(version_json) = run(binary, root, &["--json", "version"]).await else {
        return DdocTooling::absent();
    };
    let Ok(version) = parse_version(&version_json) else {
        return DdocTooling::absent();
    };
    let Some(graph_json) = run(binary, root, &["--json", "graph"]).await else {
        return DdocTooling::absent();
    };
    let Ok(graph) = parse_graph(&graph_json) else {
        return DdocTooling::absent();
    };

    let facts = schema_names(binary, root).await.unwrap_or_default();
    let mut resolved_facts = BTreeMap::new();
    for name in facts {
        if let Some(facts) = resolve_schema(binary, root, &name).await {
            resolved_facts.insert(name, facts);
        }
    }

    DdocTooling {
        version: Some(version),
        facts: resolved_facts,
        graph: Some(graph),
    }
}

async fn schema_names(binary: &OsStr, root: &Path) -> Option<Vec<String>> {
    let json = run(binary, root, &["--json", "schema", "list"]).await?;
    let envelope: Value = serde_json::from_str(&json).ok()?;
    if envelope.get("status")?.as_str()? != "ok" {
        return None;
    }
    envelope
        .pointer("/data/schemas")?
        .as_array()?
        .iter()
        .map(|schema| schema.get("name")?.as_str().map(str::to_owned))
        .collect()
}

async fn resolve_schema(binary: &OsStr, root: &Path, schema: &str) -> Option<DdocSchemaFacts> {
    let output = run(binary, root, &["--json", "schema", "show", schema]).await?;
    let resolved = parse_schema_show(&output).ok()?;
    load_schema(&resolved)
}

fn load_schema(resolved: &DdocSchemaRef) -> Option<DdocSchemaFacts> {
    let bytes = std::fs::read_to_string(&resolved.path).ok()?;
    parse_schema_file(&bytes, resolved).ok()
}

/// Validate one document. Failure becomes a finding, never a scan failure.
pub async fn validate(root: &Path, path: &Path) -> Vec<String> {
    let target = path.to_string_lossy();
    let Some(output) = run(
        OsStr::new("ddocs"),
        root,
        &["--json", "validate", target.as_ref()],
    )
    .await
    else {
        return vec!["ddocs validation unavailable".to_owned()];
    };
    parse_validate(&output)
        .unwrap_or_else(|error| vec![format!("ddocs validation unavailable: {error}")])
}

/// Slice one corpus graph into row-addressed relations for `file`.
///
/// dd gives source positions (`value[N]`), not source ids. The position is
/// resolved against the *same* [`ElementTree`] snapshot that produced the row,
/// and only the row's stable [`Element::address`] escapes this function. No
/// position is persisted, so reordering resolves anew rather than changing row
/// identity.
#[must_use]
pub fn edges_for(graph: &DdocGraph, file: &str, tree: &ElementTree) -> Vec<(String, DdocRel)> {
    let file = clean_relative(file);
    graph
        .edges
        .iter()
        .filter(|edge| clean_relative(&edge.from) == file)
        .filter_map(|edge| {
            let relation = edge.relation();
            if relation.is_pressure_sentinel() {
                return None;
            }
            source_row(tree, &edge.location).map(|row| (row.address.clone(), relation))
        })
        .collect()
}

/// File edges for one parsed document, keyed by stable source-row address.
#[must_use]
pub fn file_refs(graph: &DdocGraph, file: &str, tree: &ElementTree) -> Vec<DdocFileRef> {
    edges_for(graph, file, tree)
        .into_iter()
        .filter(|(_, relation)| relation.is_file_edge())
        .map(|(address, relation)| DdocFileRef {
            element_id: 0,
            address,
            path: relation.target,
            rel: relation.rel,
            location: relation.location,
        })
        .collect()
}

/// Attach graph, gate, derived-state, and validation facts to parsed rows.
pub fn enrich_tree(tree: &mut ElementTree, tooling: &DdocTooling, findings: &[String]) {
    let relations = tooling
        .graph
        .as_ref()
        .map(|graph| edges_for(graph, &tree.path, tree))
        .unwrap_or_default()
        .into_iter()
        .fold(
            BTreeMap::<String, Vec<DdocRel>>::new(),
            |mut by_row, (address, relation)| {
                by_row.entry(address).or_default().push(relation);
                by_row
            },
        );

    let assertion_groups = collect_assertion_groups(&tree.root);
    visit_rows_mut(&mut tree.root, &mut |row| {
        let Some(meta) = row.ddoc.as_mut() else {
            return;
        };
        meta.findings.extend(findings.iter().cloned());
        meta.rels = relations.get(&row.address).cloned().unwrap_or_default();

        let Some(facts) = tooling.facts_for(&meta.schema) else {
            return;
        };
        meta.gate_terminal = meta
            .state
            .as_deref()
            .map(|state| facts.gate_terminal.contains(state));
        if let Some(group) = derived_group(&meta.rels)
            && let Some(entries) = assertion_groups.get(group)
        {
            meta.derived_state = Some(derive_state(
                entries
                    .iter()
                    .map(|(id, state)| (id.as_str(), state.as_deref())),
                &facts.gate_terminal,
            ));
        }
    });
}

/// Surface every unresolved source address as a finding on that row.
///
/// Returns how many findings were attached. Repeated addresses remain repeated,
/// matching [`fs3_store::FileRefOutcome::unattached`] input-order semantics.
pub fn record_unattached(tree: &mut ElementTree, unattached: &[String]) -> usize {
    let mut attached = 0;
    for address in unattached {
        visit_rows_mut(&mut tree.root, &mut |row| {
            if row.address == *address
                && let Some(meta) = row.ddoc.as_mut()
            {
                meta.findings
                    .push(format!("file edge source was not stored: {address}"));
                attached += 1;
            }
        });
    }
    attached
}

fn source_row<'a>(tree: &'a ElementTree, location: &str) -> Option<&'a Element> {
    let (section, keys, index) = parse_row_location(location)?;
    let container = tree
        .root
        .children
        .iter()
        .find(|element| element.kind == ElementKind::Container && element.name == section)?;
    container
        .children
        .iter()
        .filter(|row| {
            row.kind == ElementKind::Row
                && row.ddoc.as_deref().is_some_and(|meta| {
                    let inner = &meta.trail[1..meta.trail.len().saturating_sub(1)];
                    inner == keys.as_slice()
                })
        })
        .nth(index)
}

fn parse_row_location(location: &str) -> Option<(String, Vec<String>, usize)> {
    let rest = location.strip_prefix("$.sections[")?;
    let (section, rest) = rest.split_once(']')?;
    let mut rest = rest.strip_prefix(".value")?;
    let mut parts = Vec::new();
    while let Some(after_open) = rest.strip_prefix('[') {
        let (part, after_close) = after_open.split_once(']')?;
        parts.push(part.to_owned());
        rest = after_close;
    }
    let index = parts.last()?.parse().ok()?;
    parts.pop();
    Some((section.to_owned(), parts, index))
}

fn collect_assertion_groups(root: &Element) -> BTreeMap<String, Vec<(String, Option<String>)>> {
    let mut groups = BTreeMap::<String, Vec<(String, Option<String>)>>::new();
    visit_rows(root, &mut |row| {
        let Some(meta) = row.ddoc.as_deref() else {
            return;
        };
        if meta.section == "done_when" && meta.trail.len() >= 3 {
            groups
                .entry(meta.trail[1].clone())
                .or_default()
                .push((meta.id.clone(), meta.state.clone()));
        }
    });
    groups
}

fn derived_group(relations: &[DdocRel]) -> Option<&str> {
    relations.iter().find_map(|relation| {
        if relation.rel != "derives" {
            return None;
        }
        let (_, fragment) = relation.target.split_once('#')?;
        fragment.strip_prefix("done_when/")
    })
}

fn visit_rows(element: &Element, visitor: &mut impl FnMut(&Element)) {
    if element.kind == ElementKind::Row {
        visitor(element);
    }
    for child in &element.children {
        visit_rows(child, visitor);
    }
}

fn visit_rows_mut(element: &mut Element, visitor: &mut impl FnMut(&mut Element)) {
    if element.kind == ElementKind::Row {
        visitor(element);
    }
    for child in &mut element.children {
        visit_rows_mut(child, visitor);
    }
}

fn clean_relative(path: &str) -> String {
    path.trim_start_matches("./").replace('\\', "/")
}

async fn run(binary: &OsStr, root: &Path, args: &[&str]) -> Option<String> {
    let binary = binary.to_owned();
    let root = root.to_owned();
    let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    tokio::task::spawn_blocking(move || run_blocking(&binary, &root, &args))
        .await
        .ok()
        .flatten()
}

fn run_blocking(binary: &OsStr, root: &Path, args: &[String]) -> Option<String> {
    const TIMEOUT: Duration = Duration::from_secs(30);
    // A graph can exceed an OS pipe buffer. Capture into an anonymous file so
    // the child never blocks waiting for us to drain stdout while we wait.
    let mut stdout = tempfile::tempfile().ok()?;
    let child_stdout = stdout.try_clone().ok()?;
    let mut child = Command::new(binary)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) if status.success() => {
                stdout.rewind().ok()?;
                let mut output = String::new();
                stdout.read_to_string(&mut output).ok()?;
                return Some(output);
            }
            Some(_) => return None,
            None if started.elapsed() < TIMEOUT => std::thread::sleep(Duration::from_millis(10)),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs3_core::DdocSchemaFacts;
    use std::collections::BTreeSet;

    const FILE_GRAPH: &str = include_str!("../fixtures/ddocs/graph-recorded-file-kind.json");

    fn file_tree(bytes: &[u8]) -> ElementTree {
        fs3_parsers::scan_ddoc(Path::new("docs/plans/nested/notes.dd.json"), bytes, None).unwrap()
    }

    #[tokio::test]
    async fn missing_binary_degrades_without_error() {
        let tooling =
            probe_with_binary(Path::new("."), OsStr::new("ddocs-does-not-exist-u2")).await;
        assert!(tooling.version.is_none());
        assert!(tooling.facts.is_empty());
        assert!(tooling.graph.is_none());
        assert!(tooling.is_absent());
        assert!(
            !DdocTooling {
                version: Some("0.1.0".to_owned()),
                facts: BTreeMap::new(),
                graph: Some(DdocGraph::default()),
            }
            .is_absent()
        );
    }

    #[test]
    fn unreadable_resolved_schema_is_none_not_empty_facts() {
        let resolved = DdocSchemaRef {
            schema: "builder/plan".to_owned(),
            path: "/definitely/missing/fs3-u2-schema.json".to_owned(),
            gate_terminal: BTreeSet::from(["checked".to_owned()]),
        };
        assert!(load_schema(&resolved).is_none());
    }

    #[test]
    fn recorded_file_edges_are_repo_relative_and_attach_to_the_source_row() {
        let mut graph = parse_graph(FILE_GRAPH).unwrap();
        let mut document_edge = graph.edges[0].clone();
        document_edge.kind = DdocRel::KIND_DOCUMENT.to_owned();
        graph.edges.push(document_edge);
        let bytes = include_bytes!("../fixtures/ddocs/file-link-corpus/notes.dd.json");
        let tree = file_tree(bytes);
        assert!(
            tree.root.children[1].children[0]
                .ddoc
                .as_ref()
                .unwrap()
                .sweep_excluded
        );
        let refs = file_refs(&graph, &tree.path, &tree);
        assert_eq!(refs.len(), 2);
        assert_eq!(
            refs[0].address,
            "docs/plans/nested/notes.dd.json#tasks/tk-a1b2"
        );
        assert_eq!(refs[0].path, "src/library.ts");
        assert!(
            refs.iter()
                .all(|reference| !reference.path.starts_with('/'))
        );
        assert_eq!(refs[1].path, "docs/plans/handbook.md");
        assert_ne!(refs[1].path, "../handbook.md");
    }

    #[test]
    fn row_positions_are_resolved_only_against_the_current_snapshot() {
        let first = br##"{"dd":{"schema":"builder/plan"},"sections":[{"name":"tasks","value":[{"id":"tk-0001","done":"#done_when/tk-0001"},{"id":"tk-0002","done":"#done_when/tk-0002"}]}]}"##;
        let reversed = br##"{"dd":{"schema":"builder/plan"},"sections":[{"name":"tasks","value":[{"id":"tk-0002","done":"#done_when/tk-0002"},{"id":"tk-0001","done":"#done_when/tk-0001"}]}]}"##;
        let edge = fs3_core::ddoc_envelope::DdocEdge {
            from: "tasks.dd.json".to_owned(),
            to: "tasks.dd.json".to_owned(),
            address: "#done_when/tk-0002".to_owned(),
            rel: "derives".to_owned(),
            kind: "document".to_owned(),
            location: "$.sections[tasks].value[1].done".to_owned(),
        };
        let graph = DdocGraph { edges: vec![edge] };
        let first = fs3_parsers::scan_ddoc(Path::new("tasks.dd.json"), first, None).unwrap();
        let reversed = fs3_parsers::scan_ddoc(Path::new("tasks.dd.json"), reversed, None).unwrap();
        assert_eq!(
            edges_for(&graph, "tasks.dd.json", &first)[0].0,
            "tasks.dd.json#tasks/tk-0002"
        );
        assert_eq!(
            edges_for(&graph, "tasks.dd.json", &reversed)[0].0,
            "tasks.dd.json#tasks/tk-0001"
        );
    }

    #[test]
    fn unknown_rel_survives_and_pressure_sentinel_does_not() {
        let bytes =
            br#"{"dd":{"schema":"x/y"},"sections":[{"name":"tasks","value":[{"id":"tk-0001"}]}]}"#;
        let tree = fs3_parsers::scan_ddoc(Path::new("tasks.dd.json"), bytes, None).unwrap();
        let edge = |rel: &str, address: &str| fs3_core::ddoc_envelope::DdocEdge {
            from: "tasks.dd.json".to_owned(),
            to: address.to_owned(),
            address: address.to_owned(),
            rel: rel.to_owned(),
            kind: "document".to_owned(),
            location: "$.sections[tasks].value[0].edge".to_owned(),
        };
        let graph = DdocGraph {
            edges: vec![
                edge("invented_rel", "#tasks/tk-0001"),
                edge("invented_rel", "not-applicable"),
                edge("pressure", "not-applicable"),
            ],
        };
        let relations = edges_for(&graph, "tasks.dd.json", &tree);
        assert_eq!(relations.len(), 2);
        assert!(
            relations
                .iter()
                .all(|(_, relation)| relation.rel == "invented_rel")
        );
        assert!(
            relations
                .iter()
                .any(|(_, relation)| relation.target == "not-applicable")
        );
    }

    #[test]
    fn dynamic_key_and_index_resolve_against_the_same_tree() {
        let bytes = br#"{"dd":{"schema":"builder/plan"},"sections":[{"name":"done_when","value":{"tk-0001":[{"id":"dw-0001"}],"tk-0002":[{"id":"dw-0002"}]}}]}"#;
        let tree = fs3_parsers::scan_ddoc(Path::new("dynamic.dd.json"), bytes, None).unwrap();
        let graph = DdocGraph {
            edges: vec![fs3_core::ddoc_envelope::DdocEdge {
                from: "dynamic.dd.json".to_owned(),
                to: "target.dd.json".to_owned(),
                address: "target.dd.json#items/x-0001".to_owned(),
                rel: "custom".to_owned(),
                kind: "document".to_owned(),
                location: "$.sections[done_when].value[tk-0002][0].edge".to_owned(),
            }],
        };
        let edges = edges_for(&graph, "dynamic.dd.json", &tree);
        assert_eq!(edges[0].0, "dynamic.dd.json#done_when/tk-0002/dw-0002");
    }

    #[test]
    fn unattached_addresses_become_ordered_row_findings() {
        let bytes =
            br#"{"dd":{"schema":"x/y"},"sections":[{"name":"tasks","value":[{"id":"tk-0001"}]}]}"#;
        let mut tree = fs3_parsers::scan_ddoc(Path::new("tasks.dd.json"), bytes, None).unwrap();
        let address = "tasks.dd.json#tasks/tk-0001".to_owned();
        assert_eq!(record_unattached(&mut tree, &[address.clone(), address]), 2);
        let findings = &tree.root.children[0].children[0]
            .ddoc
            .as_ref()
            .unwrap()
            .findings;
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn derived_state_wins_over_the_stored_state() {
        let bytes = br##"{"dd":{"schema":"builder/plan"},"sections":[{"name":"tasks","value":[{"id":"tk-0001","state":"checked","done":"#done_when/tk-0001"}]},{"name":"done_when","value":{"tk-0001":[{"id":"dw-0001","state":"unchecked"}]}}]}"##;
        let mut tree = fs3_parsers::scan_ddoc(Path::new("tasks.dd.json"), bytes, None).unwrap();
        let graph = DdocGraph {
            edges: vec![fs3_core::ddoc_envelope::DdocEdge {
                from: "tasks.dd.json".to_owned(),
                to: "tasks.dd.json".to_owned(),
                address: "#done_when/tk-0001".to_owned(),
                rel: "derives".to_owned(),
                kind: "document".to_owned(),
                location: "$.sections[tasks].value[0].done".to_owned(),
            }],
        };
        let mut facts = BTreeMap::new();
        facts.insert(
            "builder/plan".to_owned(),
            DdocSchemaFacts {
                schema: "builder/plan".to_owned(),
                prose_fields: BTreeMap::new(),
                string_fields: BTreeMap::new(),
                gate_terminal: BTreeSet::from(["checked".to_owned()]),
            },
        );
        enrich_tree(
            &mut tree,
            &DdocTooling {
                version: Some("0.1.0".to_owned()),
                facts,
                graph: Some(graph),
            },
            &[],
        );
        let task = &tree.root.children[0].children[0];
        assert_eq!(task.ddoc.as_ref().unwrap().gate_terminal, Some(true));
        assert_eq!(
            task.ddoc.as_ref().unwrap().effective_state(),
            Some((false, true))
        );
    }
}
