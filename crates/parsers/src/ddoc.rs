//! Pure deterministic-document parsing.
//!
//! A ddoc is dispatched by the daemon, not by [`crate::scan`], because only the
//! daemon can resolve schema facts. The composer pastes this dispatch after the
//! adapter has resolved `facts`:
//!
//! ```text
//! use fs3_parsers::ddoc;
//!
//! let tree = if ddoc::is_ddoc_source(path) {
//!     ddoc::scan(path, bytes, facts)?
//! } else {
//!     fs3_parsers::scan(path, bytes)?
//! };
//! ```
//!
//! `facts` is `Option<&DdocSchemaFacts>`; `None` is the explicit fallback path.
//! This module performs no I/O, schema lookup, or process spawning.

use std::path::Path;

use fs3_core::ddoc::{
    DDOC_GENERATED_BANNER, DDOC_GENERATED_SUFFIX, DDOC_SOURCE_SUFFIX, DdocAddress, DdocMeta,
    DdocSchemaFacts, EmbedBasis,
};
use fs3_core::{BlobRef, Element, ElementKind, ElementTree, Span, content_hash};
use serde_json::{Map, Value};

use crate::ScanError;

const LANGUAGE: &str = "ddoc";
const SECTION_SUBKIND: &str = "ddoc_section";
const ROW_SUBKIND: &str = "ddoc_row";

/// Whether `path` has dd's exact source suffix.
#[must_use]
pub fn is_ddoc_source(path: &Path) -> bool {
    path.to_string_lossy().ends_with(DDOC_SOURCE_SUFFIX)
}

/// Whether `path` or the first line identifies a generated ddoc projection.
#[must_use]
pub fn is_generated_sibling(path: &Path, bytes: &[u8]) -> bool {
    if path.to_string_lossy().ends_with(DDOC_GENERATED_SUFFIX) {
        return true;
    }
    let first_line = bytes
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    first_line.strip_suffix(b"\r").unwrap_or(first_line) == DDOC_GENERATED_BANNER.as_bytes()
}

/// Parse one `*.dd.json` into a file-rooted element tree.
///
/// Objects and arrays are traversed to whatever depth the document contains.
/// An object with a string `id` is one row and is not traversed further: row
/// fields are fields, and dd rows do not nest. Objects without ids remain
/// structural and every intervening object key enters the row trail.
///
/// # Errors
///
/// Returns [`ScanError::Unparseable`] when the bytes are not valid JSON.
pub fn scan(
    path: &Path,
    bytes: &[u8],
    facts: Option<&DdocSchemaFacts>,
) -> Result<ElementTree, ScanError> {
    let path_text = path.to_string_lossy().into_owned();
    let document: Value = serde_json::from_slice(bytes).map_err(|_| ScanError::Unparseable {
        path: path_text.clone(),
        language: LANGUAGE,
    })?;
    let source = std::str::from_utf8(bytes).map_err(|_| ScanError::Unparseable {
        path: path_text.clone(),
        language: LANGUAGE,
    })?;
    let span = Span::new(1, line_count(source));
    let blob = BlobRef::new(content_hash(bytes))
        .expect("a sha-256 hex digest is always a valid content key");

    let schema = document
        .pointer("/dd/schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let sweep_excluded = document
        .pointer("/dd/sweep_exclude")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let sections = document
        .get("sections")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let doc_title = sections.iter().find_map(|section| {
        (section.get("name").and_then(Value::as_str) == Some("meta"))
            .then(|| section.pointer("/value/title").and_then(Value::as_str))
            .flatten()
            .map(str::to_owned)
    });

    let children = sections
        .iter()
        .filter_map(|section| {
            section_element(
                &path_text,
                section,
                schema,
                sweep_excluded,
                doc_title.as_deref(),
                facts,
                span,
            )
        })
        .enumerate()
        .map(|(order, element)| element.with_sibling_order(order as u32))
        .collect();

    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&path_text);
    let root = Element::new(ElementKind::File, LANGUAGE, name, &path_text, span, source)
        .with_children(children);

    Ok(ElementTree {
        path: path_text,
        blob,
        has_error: false,
        root,
    })
}

#[allow(clippy::too_many_arguments)]
fn section_element(
    path: &str,
    section: &Value,
    schema: &str,
    sweep_excluded: bool,
    doc_title: Option<&str>,
    facts: Option<&DdocSchemaFacts>,
    span: Span,
) -> Option<Element> {
    let name = section.get("name")?.as_str()?;
    let value = section.get("value").unwrap_or(&Value::Null);
    let mut rows = Vec::new();
    let mut trail = vec![name.to_owned()];
    collect_rows(
        path,
        name,
        value,
        &mut trail,
        schema,
        sweep_excluded,
        doc_title,
        facts,
        span,
        &mut rows,
    );

    let address = DdocAddress {
        file: path.to_owned(),
        trail: vec![name.to_owned()],
    }
    .render();
    if rows.is_empty() {
        Some(Element::new(
            ElementKind::Section,
            SECTION_SUBKIND,
            name,
            address,
            span,
            section_text(name, value),
        ))
    } else {
        Some(
            Element::new(
                ElementKind::Container,
                SECTION_SUBKIND,
                name,
                address,
                span,
                "",
            )
            .with_children(rows),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_rows(
    path: &str,
    section: &str,
    value: &Value,
    trail: &mut Vec<String>,
    schema: &str,
    sweep_excluded: bool,
    doc_title: Option<&str>,
    facts: Option<&DdocSchemaFacts>,
    span: Span,
    rows: &mut Vec<Element>,
) {
    match value {
        Value::Object(object) => {
            if let Some(id) = object.get("id").and_then(Value::as_str) {
                let mut row_trail = trail.clone();
                row_trail.push(id.to_owned());
                let address = DdocAddress {
                    file: path.to_owned(),
                    trail: row_trail.clone(),
                }
                .render();
                let (raw_text, embed_basis) = row_text(section, &row_trail, object, facts);
                let mut meta = DdocMeta::new(&address, schema, row_trail, embed_basis);
                meta.state = object
                    .get("state")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                meta.doc_title = doc_title.map(str::to_owned);
                meta.sweep_excluded = sweep_excluded;

                rows.push(
                    Element::new(ElementKind::Row, ROW_SUBKIND, id, address, span, raw_text)
                        .with_sibling_order(rows.len() as u32)
                        .with_ddoc(meta),
                );
                return;
            }

            for (key, child) in object {
                trail.push(key.clone());
                collect_rows(
                    path,
                    section,
                    child,
                    trail,
                    schema,
                    sweep_excluded,
                    doc_title,
                    facts,
                    span,
                    rows,
                );
                trail.pop();
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_rows(
                    path,
                    section,
                    child,
                    trail,
                    schema,
                    sweep_excluded,
                    doc_title,
                    facts,
                    span,
                    rows,
                );
            }
        }
        _ => {}
    }
}

fn row_text(
    section: &str,
    trail: &[String],
    row: &Map<String, Value>,
    facts: Option<&DdocSchemaFacts>,
) -> (String, EmbedBasis) {
    let mut lines = Vec::new();
    let parents = &trail[..trail.len().saturating_sub(1)];
    if !parents.is_empty() {
        lines.push(parents.join(" / "));
    }

    if let Some((prose_fields, string_fields)) = facts.and_then(|facts| facts.embeddable(section)) {
        append_fields(&mut lines, row, prose_fields, "");
        append_fields(&mut lines, row, string_fields, "context ");
        (lines.join("\n"), EmbedBasis::SchemaDeclared)
    } else {
        for (field, value) in row {
            if matches!(field.as_str(), "id" | "state") {
                continue;
            }
            if let Some(text) = value.as_str() {
                lines.push(format!("{field}: {text}"));
            }
        }
        (lines.join("\n"), EmbedBasis::Fallback)
    }
}

fn append_fields(
    lines: &mut Vec<String>,
    row: &Map<String, Value>,
    fields: &[String],
    prefix: &str,
) {
    for field in fields {
        if let Some(text) = row.get(field).and_then(Value::as_str) {
            lines.push(format!("{prefix}{field}: {text}"));
        }
    }
}

fn section_text(name: &str, value: &Value) -> String {
    match value {
        Value::String(text) => format!("{name}\n{text}"),
        _ => format!(
            "{name}\n{}",
            serde_json::to_string_pretty(value).expect("an in-memory JSON value always serializes")
        ),
    }
}

fn line_count(source: &str) -> u32 {
    source.lines().count().max(1) as u32
}
