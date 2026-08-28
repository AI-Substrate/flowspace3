//! Pure parsers for the JSON envelopes emitted by `ddocs`.
//!
//! The envelope is dd's compatibility boundary. Per-verb payloads are parsed
//! defensively: unknown fields are ignored, while a missing field required to
//! make a truthful claim is an error rather than an invented default.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{DdocRel, DdocSchemaFacts};

/// The corpus edge graph returned by `ddocs graph`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdocGraph {
    /// Edges from every document in the corpus.
    pub edges: Vec<DdocEdge>,
}

/// One resolved edge in dd's corpus graph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdocEdge {
    /// Repo-relative source document path.
    pub from: String,
    /// Repo-relative target document or file path.
    pub to: String,
    /// Target address as emitted by dd, relative to the source document.
    pub address: String,
    /// Open relation name. Unknown relations remain first-class.
    pub rel: String,
    /// `document` or `file`; older ddocs versions omit it.
    pub kind: String,
    /// JSONPath of the declaration in the source document.
    pub location: String,
}

impl DdocEdge {
    /// The resolved target carried onto a row relation.
    #[must_use]
    pub fn resolved_target(&self) -> String {
        match self.address.split_once('#') {
            Some((_, fragment)) => format!("{}#{fragment}", self.to),
            None => self.to.clone(),
        }
    }

    /// Convert this edge into the core relation shape.
    #[must_use]
    pub fn relation(&self) -> DdocRel {
        DdocRel {
            rel: self.rel.clone(),
            target: self.resolved_target(),
            kind: self.kind.clone(),
            location: self.location.clone(),
        }
    }
}

/// dd's schema-resolution answer. The file itself is parsed separately.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdocSchemaRef {
    /// Qualified schema name from `data.name`.
    pub schema: String,
    /// Exact file path resolved by dd's four-root ladder.
    pub path: String,
    /// Resolved gate-terminal vocabulary, including custom enums.
    pub gate_terminal: BTreeSet<String>,
}

/// Why a ddocs envelope or resolved schema could not make a truthful value.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum DdocEnvelopeError {
    /// Input was not JSON.
    #[error("ddocs output is not valid JSON: {0}")]
    InvalidJson(String),
    /// A required field is absent or has the wrong type.
    #[error("ddocs output is missing or has an invalid {0}")]
    InvalidField(&'static str),
    /// The caller handed a parser an envelope from another verb.
    #[error("expected {expected}, got command {actual:?}")]
    UnexpectedCommand {
        /// Expected command spelling.
        expected: &'static str,
        /// Actual command spelling.
        actual: String,
    },
    /// Error/unconfigured output cannot be treated as verb data.
    #[error("ddocs command reported status {0:?}")]
    UnusableStatus(String),
}

/// Parse a `ddocs graph` envelope.
///
/// Absolute edge paths are normalised against `data.root` here, once, so every
/// downstream inverse-index key is repo-relative by construction.
pub fn parse_graph(json: &str) -> Result<DdocGraph, DdocEnvelopeError> {
    let envelope = parse_envelope(json, "ddocs graph")?;
    allow_data_status(&envelope)?;
    let data = data(&envelope)?;
    let root = string(data, "root", "data.root")?;
    let edges = data
        .get("edges")
        .and_then(Value::as_array)
        .ok_or(DdocEnvelopeError::InvalidField("data.edges"))?;

    let edges = edges
        .iter()
        .map(|edge| {
            let object = edge
                .as_object()
                .ok_or(DdocEnvelopeError::InvalidField("data.edges[]"))?;
            let from = normalise_path(string(object, "from", "data.edges[].from")?, root);
            let to = normalise_path(string(object, "to", "data.edges[].to")?, root);
            let kind = object
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or(DdocRel::KIND_DOCUMENT)
                .to_owned();
            Ok(DdocEdge {
                from,
                to,
                address: string(object, "address", "data.edges[].address")?.to_owned(),
                rel: string(object, "rel", "data.edges[].rel")?.to_owned(),
                kind,
                location: string(object, "location", "data.edges[].location")?.to_owned(),
            })
        })
        .collect::<Result<Vec<_>, DdocEnvelopeError>>()?;

    Ok(DdocGraph { edges })
}

/// Parse dd's schema-resolution answer.
pub fn parse_schema_show(json: &str) -> Result<DdocSchemaRef, DdocEnvelopeError> {
    let envelope = parse_envelope(json, "ddocs schema show")?;
    allow_ok_status(&envelope)?;
    let data = data(&envelope)?;
    let gate_terminal = data
        .get("gate_terminal")
        .and_then(Value::as_array)
        .ok_or(DdocEnvelopeError::InvalidField("data.gate_terminal"))?
        .iter()
        .map(|state| {
            state
                .as_str()
                .map(str::to_owned)
                .ok_or(DdocEnvelopeError::InvalidField("data.gate_terminal[]"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;

    Ok(DdocSchemaRef {
        schema: string(data, "name", "data.name")?.to_owned(),
        path: string(data, "path", "data.path")?.to_owned(),
        gate_terminal,
    })
}

/// Parse the schema file dd resolved into the facts consumed by the row parser.
pub fn parse_schema_file(
    json: &str,
    resolved: &DdocSchemaRef,
) -> Result<DdocSchemaFacts, DdocEnvelopeError> {
    let schema: Value = serde_json::from_str(json)
        .map_err(|error| DdocEnvelopeError::InvalidJson(error.to_string()))?;
    let sections = schema
        .get("sections")
        .and_then(Value::as_object)
        .ok_or(DdocEnvelopeError::InvalidField("schema.sections"))?;

    let mut prose_fields = BTreeMap::new();
    let mut string_fields = BTreeMap::new();
    for (section, declaration) in sections {
        let mut prose = BTreeSet::new();
        let mut strings = BTreeSet::new();
        collect_declared_fields(
            declaration.get("shape").unwrap_or(declaration),
            &mut prose,
            &mut strings,
        );
        if !prose.is_empty() {
            prose_fields.insert(section.clone(), prose.into_iter().collect());
        }
        if !strings.is_empty() {
            string_fields.insert(section.clone(), strings.into_iter().collect());
        }
    }

    Ok(DdocSchemaFacts {
        schema: resolved.schema.clone(),
        prose_fields,
        string_fields,
        gate_terminal: resolved.gate_terminal.clone(),
    })
}

/// Parse the installed binary version.
pub fn parse_version(json: &str) -> Result<String, DdocEnvelopeError> {
    let envelope = parse_envelope(json, "version")?;
    allow_ok_status(&envelope)?;
    Ok(string(data(&envelope)?, "version", "data.version")?.to_owned())
}

/// Parse validation findings for row metadata.
///
/// Only a `ddocs validate` envelope is accepted. In particular, an `ok`
/// `ddocs links` envelope with no issues is not evidence that a document is
/// healthy.
pub fn parse_validate(json: &str) -> Result<Vec<String>, DdocEnvelopeError> {
    let envelope = parse_envelope(json, "ddocs validate")?;
    let status = envelope
        .get("status")
        .and_then(Value::as_str)
        .ok_or(DdocEnvelopeError::InvalidField("status"))?;
    let issues = match status {
        "ok" | "degraded" => envelope.pointer("/data/issues"),
        "error" => envelope.pointer("/error/details/issues"),
        other => return Err(DdocEnvelopeError::UnusableStatus(other.to_owned())),
    }
    .and_then(Value::as_array)
    .ok_or(DdocEnvelopeError::InvalidField("validation issues"))?;
    Ok(issues.iter().map(format_issue).collect())
}

fn parse_envelope(json: &str, expected: &'static str) -> Result<Value, DdocEnvelopeError> {
    let envelope: Value = serde_json::from_str(json)
        .map_err(|error| DdocEnvelopeError::InvalidJson(error.to_string()))?;
    let actual = envelope
        .get("command")
        .and_then(Value::as_str)
        .ok_or(DdocEnvelopeError::InvalidField("command"))?;
    if actual != expected {
        return Err(DdocEnvelopeError::UnexpectedCommand {
            expected,
            actual: actual.to_owned(),
        });
    }
    Ok(envelope)
}

fn allow_ok_status(envelope: &Value) -> Result<(), DdocEnvelopeError> {
    let status = envelope
        .get("status")
        .and_then(Value::as_str)
        .ok_or(DdocEnvelopeError::InvalidField("status"))?;
    if status == "ok" {
        Ok(())
    } else {
        Err(DdocEnvelopeError::UnusableStatus(status.to_owned()))
    }
}

fn allow_data_status(envelope: &Value) -> Result<(), DdocEnvelopeError> {
    let status = envelope
        .get("status")
        .and_then(Value::as_str)
        .ok_or(DdocEnvelopeError::InvalidField("status"))?;
    if matches!(status, "ok" | "degraded") {
        Ok(())
    } else {
        Err(DdocEnvelopeError::UnusableStatus(status.to_owned()))
    }
}

fn data(envelope: &Value) -> Result<&serde_json::Map<String, Value>, DdocEnvelopeError> {
    envelope
        .get("data")
        .and_then(Value::as_object)
        .ok_or(DdocEnvelopeError::InvalidField("data"))
}

fn string<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
    label: &'static str,
) -> Result<&'a str, DdocEnvelopeError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(DdocEnvelopeError::InvalidField(label))
}

fn collect_declared_fields(
    shape: &Value,
    prose: &mut BTreeSet<String>,
    strings: &mut BTreeSet<String>,
) {
    let Some(object) = shape.as_object() else {
        return;
    };
    if let Some(fields) = object.get("fields").and_then(Value::as_object) {
        for (name, declaration) in fields {
            match declaration.get("type").and_then(Value::as_str) {
                Some("text") => {
                    prose.insert(name.clone());
                }
                Some("string") => {
                    strings.insert(name.clone());
                }
                _ => {}
            }
        }
    }
    for key in ["items", "valuesShape"] {
        if let Some(child) = object.get(key) {
            collect_declared_fields(child, prose, strings);
        }
    }
}

fn normalise_path(path: &str, root: &str) -> String {
    let path = Path::new(path);
    if let Ok(relative) = path.strip_prefix(root) {
        return slash_path(relative);
    }
    slash_path(&lexical_normalise(path))
}

fn lexical_normalise(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn slash_path(path: &Path) -> String {
    path.components()
        .filter_map(|part| match part {
            Component::RootDir | Component::Prefix(_) | Component::CurDir => None,
            Component::ParentDir => Some("..".to_owned()),
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn format_issue(issue: &Value) -> String {
    let Some(object) = issue.as_object() else {
        return issue.to_string();
    };
    let severity = object.get("severity").and_then(Value::as_str);
    let class = object.get("class").and_then(Value::as_str);
    let location = object.get("location").and_then(Value::as_str);
    let message = object.get("message").and_then(Value::as_str);
    match (severity, class, location, message) {
        (Some(severity), Some(class), Some(location), Some(message)) => {
            format!("{severity} {class} at {location}: {message}")
        }
        (_, _, _, Some(message)) => message.to_owned(),
        _ => issue.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION: &str = include_str!("../../daemon/fixtures/ddocs/version.json");
    const GRAPH_LIVE: &str =
        include_str!("../../daemon/fixtures/ddocs/graph-live-document-kind.json");
    const GRAPH_MISSING_KIND: &str =
        include_str!("../../daemon/fixtures/ddocs/graph-authored-missing-kind.json");
    const GRAPH_FILE_KIND: &str =
        include_str!("../../daemon/fixtures/ddocs/graph-recorded-file-kind.json");
    const SCHEMA_SHOW: &str = include_str!("../../daemon/fixtures/ddocs/schema-builder-plan.json");
    const SCHEMA_FILE: &str =
        include_str!("../../daemon/fixtures/ddocs/schema-builder-plan-file.json");
    const VALIDATE: &str = include_str!("../../daemon/fixtures/ddocs/validate-plan.json");
    const VALIDATE_DEGRADED: &str =
        include_str!("../../daemon/fixtures/ddocs/validate-degraded-file-links.json");
    const VALIDATE_ERROR: &str =
        include_str!("../../daemon/fixtures/ddocs/validate-error-schema.json");
    const LINKS_EMPTY: &str =
        include_str!("../../daemon/fixtures/ddocs/links-malformed-ok-empty.json");

    #[test]
    fn parses_recorded_version_envelope() {
        assert_eq!(parse_version(VERSION).unwrap(), "0.1.0");
    }

    #[test]
    fn parses_recorded_graph_and_normalises_paths() {
        let graph = parse_graph(GRAPH_LIVE).unwrap();
        assert!(!graph.edges.is_empty());
        assert!(graph.edges.iter().all(|edge| !edge.from.starts_with('/')));
        assert!(graph.edges.iter().all(|edge| !edge.to.starts_with('/')));
        assert!(graph.edges.iter().all(|edge| edge.kind == "document"));
    }

    #[test]
    fn missing_graph_kind_defaults_to_document() {
        let graph = parse_graph(GRAPH_MISSING_KIND).unwrap();
        assert_eq!(graph.edges[0].kind, DdocRel::KIND_DOCUMENT);
    }

    #[test]
    fn recorded_file_kind_survives_graph_parsing() {
        let graph = parse_graph(GRAPH_FILE_KIND).unwrap();
        assert_eq!(graph.edges[0].kind, DdocRel::KIND_FILE);
        assert!(!graph.edges[0].to.starts_with('/'));
    }

    #[test]
    fn parses_recorded_schema_resolution_and_file_shapes() {
        let resolved = parse_schema_show(SCHEMA_SHOW).unwrap();
        let facts = parse_schema_file(SCHEMA_FILE, &resolved).unwrap();
        assert_eq!(facts.schema, "builder/plan");
        assert_eq!(
            facts.prose_fields.get("acceptance_criteria"),
            Some(&vec!["claim".to_owned()])
        );
        assert!(
            facts
                .string_fields
                .get("acceptance_criteria")
                .unwrap()
                .contains(&"note".to_owned())
        );
        assert_eq!(
            facts.prose_fields.get("done_when"),
            Some(&vec!["assertion".to_owned()])
        );
        assert!(facts.gate_terminal.contains("checked"));
    }

    #[test]
    fn parses_recorded_validate_findings() {
        assert!(parse_validate(VALIDATE).unwrap().is_empty());
    }

    #[test]
    fn degraded_and_error_validate_envelopes_preserve_findings() {
        let degraded = parse_validate(VALIDATE_DEGRADED).unwrap();
        assert_eq!(degraded.len(), 1);
        assert!(degraded[0].contains("address-target-missing"));
        let error = parse_validate(VALIDATE_ERROR).unwrap();
        assert_eq!(error.len(), 1);
        assert!(error[0].contains("schema-unresolvable"));
    }

    #[test]
    fn empty_links_issues_are_not_accepted_as_health() {
        assert!(matches!(
            parse_validate(LINKS_EMPTY),
            Err(DdocEnvelopeError::UnexpectedCommand { .. })
        ));
    }
}
