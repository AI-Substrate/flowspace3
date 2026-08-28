//! Elements: the parsed tree, blob-addressed.
//!
//! The unit of work here is a whole tree, not a row. `elements` is keyed by
//! `(blob_sha, parser_version, address, span_start)`, and a blob is the hash of
//! the file's bytes — so the same blob read by the same parser always yields
//! the same tree. Two consequences the API leans on:
//!
//! * an upsert can never leave a stale row behind, because the key set for a
//!   given blob and parser is fixed. There is no reconciling delete to write.
//! * [`get_elements`] is the cheap skip in the scan flow: elements already
//!   present means the parse has been done, by anyone, on any branch.
//!
//! `span_start` is in that key because `address` alone is NOT unique, on
//! purpose: the scanner emits `struct Rect` and `impl Rect` as two elements
//! sharing one address. Keying on address alone would have collapsed the pair
//! into one row, silently, on every scan.
//!
//! `path` and `has_error` are deliberately absent. A path is a fact about a
//! *worktree*, and lives in `worktree_files`; the same blob is at forty paths
//! across forty checkouts and duplicating it here would make the content layer
//! branch-shaped, which is the whole thing workshop 002 refuses.

use std::collections::{HashMap, HashSet};

use fs3_core::{BlobRef, Element, ElementKind, Span};
use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::{PgPool, StoreError};

/// Write a whole element tree for one blob, atomically.
///
/// `enrich` is the scanner's injected policy (decision D5): the store records
/// the verdict, it does not compute it. Passing the policy rather than a
/// pre-marked tree keeps the flag's single source of truth in the scanner's
/// settings, where the D5 discussion put it.
///
/// The write is one transaction because a half-written tree is worse than no
/// tree: [`get_elements`] would report the blob as parsed and hand back a
/// truncated shape, and the scan flow's skip would make that permanent.
///
/// Parent links are assigned as the walk descends — a child is inserted only
/// after the row it hangs from has an id, which is why this is a pre-order walk
/// and not a batch insert.
///
/// # Errors
/// [`StoreError::Query`] when the transaction fails; nothing is written.
pub async fn upsert_element_tree(
    pool: &PgPool,
    blob: &BlobRef,
    parser_version: &str,
    root: &Element,
    enrich: impl Fn(&Element) -> bool,
) -> Result<(), StoreError> {
    let mut tx = pool.begin().await?;

    // Explicit stack rather than recursion: an async fn cannot recurse without
    // boxing every level, and a file's tree is exactly the shape a Vec handles
    // for free.
    let mut pending: Vec<(&Element, Option<i64>)> = vec![(root, None)];
    while let Some((element, parent_id)) = pending.pop() {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO elements
               (blob_sha, parser_version, parent_id, kind, subkind, name, address,
                span_start, span_end, sibling_order, raw_text, raw_hash, enrich)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             ON CONFLICT (blob_sha, parser_version, address, span_start) DO UPDATE SET
               parent_id     = EXCLUDED.parent_id,
               kind          = EXCLUDED.kind,
               subkind       = EXCLUDED.subkind,
               name          = EXCLUDED.name,
               span_end      = EXCLUDED.span_end,
               sibling_order = EXCLUDED.sibling_order,
               raw_text      = EXCLUDED.raw_text,
               raw_hash      = EXCLUDED.raw_hash,
               enrich        = EXCLUDED.enrich
             RETURNING id",
        )
        .bind(blob.as_str())
        .bind(parser_version)
        .bind(parent_id)
        .bind(element.kind.as_str())
        .bind(&element.subkind)
        .bind(&element.name)
        .bind(&element.address)
        .bind(element.span.start_line as i32)
        .bind(element.span.end_line as i32)
        .bind(element.sibling_order as i32)
        .bind(&element.raw_text)
        .bind(element.raw_hash())
        .bind(enrich(element))
        .fetch_one(&mut *tx)
        .await?;

        // Reversed so the first child is popped first: the walk then reads in
        // source order, which makes the assigned ids readable in a dump.
        pending.extend(element.children.iter().rev().map(|child| (child, Some(id))));
    }

    tx.commit().await?;
    Ok(())
}

/// The requested blobs that have been parsed by `parser_version`.
///
/// `fs3_parsers::scan` always returns that root even for empty, binary, and
/// unknown-language files, and [`upsert_element_tree`] writes the whole tree
/// atomically. An absent row therefore means the parse did not complete and
/// the caller should retry it.
///
/// # Errors
/// [`StoreError::Query`] when the lookup fails.
pub async fn blobs_with_parser_version(
    pool: &PgPool,
    parser_version: &str,
    blobs: &[&str],
) -> Result<HashSet<String>, StoreError> {
    if blobs.is_empty() {
        return Ok(HashSet::new());
    }

    let rows = sqlx::query(
        "SELECT DISTINCT blob_sha
           FROM elements
          WHERE parser_version = $1 AND blob_sha = ANY($2)",
    )
    .bind(parser_version)
    .bind(blobs)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| Ok(row.try_get("blob_sha")?))
        .collect()
}

/// The file element for one blob, with its descendants nested, or `None` when
/// this blob has not been parsed by this parser.
///
/// `None` is the scan flow's signal to do the work; `Some` is the skip.
///
/// # Errors
/// [`StoreError::Query`] on failure; [`StoreError::Corrupt`] when the stored
/// rows do not form one tree — an unknown kind, no root, or more than one.
pub async fn get_elements(
    pool: &PgPool,
    blob: &BlobRef,
    parser_version: &str,
) -> Result<Option<Element>, StoreError> {
    let rows = sqlx::query(
        "SELECT id, parent_id, kind, subkind, name, address,
                span_start, span_end, sibling_order, raw_text
           FROM elements
          WHERE blob_sha = $1 AND parser_version = $2
          ORDER BY sibling_order, id",
    )
    .bind(blob.as_str())
    .bind(parser_version)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(None);
    }

    let mut nodes: HashMap<i64, Element> = HashMap::with_capacity(rows.len());
    // Insertion order within a parent is the query's `sibling_order` ordering,
    // so these lists are already in source order.
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut roots: Vec<i64> = Vec::new();

    for row in &rows {
        let id: i64 = row.try_get("id")?;
        match row.try_get::<Option<i64>, _>("parent_id")? {
            Some(parent) => children.entry(parent).or_default().push(id),
            None => roots.push(id),
        }
        nodes.insert(id, element_from_row(row)?);
    }

    let [root] = roots.as_slice() else {
        return Err(StoreError::Corrupt(fs3_core::Error::InvalidConfig(
            format!(
                "blob {} at parser_version {parser_version} has {} file roots, expected exactly one",
                blob.as_str(),
                roots.len()
            ),
        )));
    };

    assemble(*root, &mut nodes, &children).map(Some)
}

/// Hang every descendant of `id` beneath it, depth-first.
///
/// Plain recursion, not a stack: this is synchronous, and the depth is a
/// source file's nesting depth — tens, not thousands.
fn assemble(
    id: i64,
    nodes: &mut HashMap<i64, Element>,
    children: &HashMap<i64, Vec<i64>>,
) -> Result<Element, StoreError> {
    let mut element = nodes.remove(&id).ok_or_else(|| {
        StoreError::Corrupt(fs3_core::Error::InvalidConfig(format!(
            "element {id} is claimed as a child twice, or by a parent outside its own blob"
        )))
    })?;

    if let Some(ids) = children.get(&id) {
        element.children = ids
            .iter()
            .map(|child| assemble(*child, nodes, children))
            .collect::<Result<Vec<_>, _>>()?;
    }

    Ok(element)
}

/// Rebuild an element from its row, childless.
///
/// `raw_hash` is a stored column but is not read back: [`Element::new`] derives
/// it from `raw_text`, and deriving it is what makes "the hash changed" mean
/// "the text changed". Reading the stored copy instead would let a wrong row
/// pass itself off as right.
pub(crate) fn element_from_row(row: &PgRow) -> Result<Element, StoreError> {
    let kind: String = row.try_get("kind")?;
    Ok(Element::new(
        kind_from_str(&kind)?,
        row.try_get::<String, _>("subkind")?,
        row.try_get::<String, _>("name")?,
        row.try_get::<String, _>("address")?,
        Span::new(
            row.try_get::<i32, _>("span_start")? as u32,
            row.try_get::<i32, _>("span_end")? as u32,
        ),
        row.try_get::<String, _>("raw_text")?,
    )
    .with_sibling_order(row.try_get::<i32, _>("sibling_order")? as u32))
}

pub(crate) fn kind_from_str(value: &str) -> Result<ElementKind, StoreError> {
    match value {
        "file" => Ok(ElementKind::File),
        "container" => Ok(ElementKind::Container),
        "function" => Ok(ElementKind::Function),
        "section" => Ok(ElementKind::Section),
        "turn" => Ok(ElementKind::Turn),
        other => Err(StoreError::Corrupt(fs3_core::Error::InvalidConfig(
            format!("unknown element kind {other:?}"),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_round_trips_through_its_stored_spelling() {
        for kind in [
            ElementKind::File,
            ElementKind::Container,
            ElementKind::Function,
            ElementKind::Section,
            ElementKind::Turn,
        ] {
            assert_eq!(kind_from_str(kind.as_str()).unwrap(), kind);
        }
        // The spellings 0001 used, which migration 0002 renamed. A row that
        // still says `callable` means the migration did not run.
        assert!(kind_from_str("callable").is_err());
        assert!(kind_from_str("type").is_err());
        assert!(kind_from_str("block").is_err());
    }

    /// The rebuild has to refuse a shape it cannot express, rather than
    /// silently dropping the half it cannot reach.
    #[test]
    fn assembling_a_child_with_no_row_is_corruption_not_a_shrug() {
        let mut nodes = HashMap::new();
        nodes.insert(
            1,
            Element::new(
                ElementKind::File,
                "rust",
                "sample.rs",
                "sample.rs",
                Span::new(1, 1),
                "",
            ),
        );
        let children = HashMap::from([(1, vec![7])]);

        let error = assemble(1, &mut nodes, &children)
            .expect_err("a child id with no row must not be skipped");
        assert!(matches!(error, StoreError::Corrupt(_)), "{error}");
    }
}
