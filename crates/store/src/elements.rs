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
use sqlx::types::Json;

use crate::{PgPool, StoreError};
/// Outcome of writing a parsed tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElementTreeWrite {
    /// This path owns the shared tree, either newly inserted or refreshed.
    Stored,
    /// Another path already owns the tree for these exact bytes and parser.
    Reused { stored_path: String },
}

/// A root-count inconsistency found while reading a parsed tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementTreeInconsistency {
    pub blob_sha: String,
    pub parser_version: String,
    pub paths: Vec<String>,
}

/// A tree read that preserves dirty-data evidence instead of failing on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementTreeRead {
    pub tree: Option<Element>,
    pub inconsistency: Option<ElementTreeInconsistency>,
}

/// Write a whole element tree for one blob, atomically.
///
/// The first file path to store a `(blob, parser_version)` owns its shared
/// element tree. A concurrent writer for another path converges on that tree
/// and returns [`ElementTreeWrite::Reused`] instead of failing the scan. The
/// path itself remains in `worktree_files`; only the duplicate content tree is
/// collapsed.
///
/// # Errors
/// [`StoreError::Query`] when the transaction fails; nothing is written.
pub async fn upsert_element_tree(
    pool: &PgPool,
    blob: &BlobRef,
    parser_version: &str,
    root: &Element,
    enrich: impl Fn(&Element) -> bool,
) -> Result<ElementTreeWrite, StoreError> {
    let mut tx = pool.begin().await?;
    let inserted: Option<i64> = sqlx::query_scalar(
        "INSERT INTO elements
           (blob_sha, parser_version, parent_id, kind, subkind, name, address,
            span_start, span_end, sibling_order, raw_text, raw_hash, enrich, ddoc)
         VALUES ($1, $2, NULL, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         ON CONFLICT DO NOTHING
         RETURNING id",
    )
    .bind(blob.as_str())
    .bind(parser_version)
    .bind(root.kind.as_str())
    .bind(&root.subkind)
    .bind(&root.name)
    .bind(&root.address)
    .bind(root.span.start_line as i32)
    .bind(root.span.end_line as i32)
    .bind(root.sibling_order as i32)
    .bind(&root.raw_text)
    .bind(root.raw_hash())
    .bind(enrich(root))
    .bind(root.ddoc.as_deref().map(Json))
    .fetch_optional(&mut *tx)
    .await?;

    let root_id = match inserted {
        Some(id) => id,
        None => {
            let row = sqlx::query(
                "SELECT id, address
                   FROM elements
                  WHERE blob_sha = $1 AND parser_version = $2
                    AND parent_id IS NULL
                    AND ((address = $3 AND span_start = $4)
                         OR ($5 = 'file' AND kind = 'file'))
                  ORDER BY CASE WHEN address = $3 AND span_start = $4 THEN 0 ELSE 1 END, id
                  LIMIT 1
                  FOR UPDATE",
            )
            .bind(blob.as_str())
            .bind(parser_version)
            .bind(&root.address)
            .bind(root.span.start_line as i32)
            .bind(root.kind.as_str())
            .fetch_one(&mut *tx)
            .await?;
            let stored_path: String = row.try_get("address")?;
            if stored_path != root.address {
                tx.commit().await?;
                return Ok(ElementTreeWrite::Reused { stored_path });
            }

            let id: i64 = row.try_get("id")?;
            sqlx::query(
                "UPDATE elements
                    SET subkind = $2, name = $3, span_start = $4, span_end = $5,
                        sibling_order = $6, raw_text = $7, raw_hash = $8,
                        enrich = $9, ddoc = $10
                  WHERE id = $1",
            )
            .bind(id)
            .bind(&root.subkind)
            .bind(&root.name)
            .bind(root.span.start_line as i32)
            .bind(root.span.end_line as i32)
            .bind(root.sibling_order as i32)
            .bind(&root.raw_text)
            .bind(root.raw_hash())
            .bind(enrich(root))
            .bind(root.ddoc.as_deref().map(Json))
            .execute(&mut *tx)
            .await?;
            id
        }
    };

    // Explicit stack rather than recursion: an async fn cannot recurse without
    // boxing every level. The root is handled above because its partial unique
    // index is also the concurrent-writer convergence point.
    let mut pending: Vec<(&Element, i64)> = root
        .children
        .iter()
        .rev()
        .map(|child| (child, root_id))
        .collect();
    while let Some((element, parent_id)) = pending.pop() {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO elements
               (blob_sha, parser_version, parent_id, kind, subkind, name, address,
                span_start, span_end, sibling_order, raw_text, raw_hash, enrich, ddoc)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
             ON CONFLICT (blob_sha, parser_version, address, span_start) DO UPDATE SET
               parent_id = EXCLUDED.parent_id, kind = EXCLUDED.kind,
               subkind = EXCLUDED.subkind, name = EXCLUDED.name,
               span_end = EXCLUDED.span_end, sibling_order = EXCLUDED.sibling_order,
               raw_text = EXCLUDED.raw_text, raw_hash = EXCLUDED.raw_hash,
               enrich = EXCLUDED.enrich, ddoc = EXCLUDED.ddoc
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
        .bind(element.ddoc.as_deref().map(Json))
        .fetch_one(&mut *tx)
        .await?;

        pending.extend(element.children.iter().rev().map(|child| (child, id)));
    }

    tx.commit().await?;
    Ok(ElementTreeWrite::Stored)
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

/// Read one shared element tree and preserve any root-count inconsistency.
///
/// Dirty historical data is still readable: the lowest-id file root is the
/// deterministic survivor, while `inconsistency` names every stored root path.
/// No root produces `tree: None` plus the same report instead of a hard error.
///
/// # Errors
/// [`StoreError::Query`] on failure; [`StoreError::Corrupt`] when a row cannot
/// be represented by the element model.
pub async fn get_elements(
    pool: &PgPool,
    blob: &BlobRef,
    parser_version: &str,
) -> Result<ElementTreeRead, StoreError> {
    let rows = sqlx::query(
        "SELECT id, parent_id, kind, subkind, name, address,
                span_start, span_end, sibling_order, raw_text, ddoc
           FROM elements
          WHERE blob_sha = $1 AND parser_version = $2
          ORDER BY sibling_order, id",
    )
    .bind(blob.as_str())
    .bind(parser_version)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(ElementTreeRead {
            tree: None,
            inconsistency: None,
        });
    }

    let mut nodes: HashMap<i64, Element> = HashMap::with_capacity(rows.len());
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

    let paths = roots
        .iter()
        .filter_map(|id| nodes.get(id).map(|node| node.address.clone()))
        .collect::<Vec<_>>();
    let inconsistency = (roots.len() != 1).then(|| ElementTreeInconsistency {
        blob_sha: blob.as_str().to_string(),
        parser_version: parser_version.to_string(),
        paths,
    });
    let survivor = roots
        .iter()
        .find(|id| {
            nodes
                .get(id)
                .is_some_and(|node| node.kind == ElementKind::File)
        })
        .copied();
    let tree = survivor
        .map(|id| assemble(id, &mut nodes, &children))
        .transpose()?;

    Ok(ElementTreeRead {
        tree,
        inconsistency,
    })
}

/// List every `(blob, parser_version)` whose stored rows do not have one root.
pub async fn element_tree_inconsistencies(
    pool: &PgPool,
) -> Result<Vec<ElementTreeInconsistency>, StoreError> {
    let rows = sqlx::query(
        "WITH file_groups AS (
             SELECT DISTINCT blob_sha, parser_version
               FROM elements
              WHERE kind = 'file'
         )
         SELECT groups.blob_sha, groups.parser_version,
                array_agg(elements.address ORDER BY elements.id)
                    FILTER (WHERE elements.parent_id IS NULL) AS paths,
                count(elements.id) FILTER (WHERE elements.parent_id IS NULL) AS roots
           FROM file_groups groups
           JOIN elements USING (blob_sha, parser_version)
          GROUP BY groups.blob_sha, groups.parser_version
         HAVING count(elements.id) FILTER (WHERE elements.parent_id IS NULL) <> 1
          ORDER BY groups.blob_sha, groups.parser_version",
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(ElementTreeInconsistency {
                blob_sha: row.try_get("blob_sha")?,
                parser_version: row.try_get("parser_version")?,
                paths: row
                    .try_get::<Option<Vec<String>>, _>("paths")?
                    .unwrap_or_default(),
            })
        })
        .collect()
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
    let mut element = Element::new(
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
    .with_sibling_order(row.try_get::<i32, _>("sibling_order")? as u32);
    // Some internal projections use this decoder without selecting optional
    // metadata. Those code-only paths predate ddocs and remain valid; the ddoc
    // round-trip query selects the column and its contract test proves it.
    element.ddoc = match row.try_get::<Option<Json<fs3_core::DdocMeta>>, _>("ddoc") {
        Ok(meta) => meta.map(|Json(meta)| Box::new(meta)),
        Err(sqlx::Error::ColumnNotFound(_)) => None,
        Err(error) => return Err(error.into()),
    };
    Ok(element)
}

pub(crate) fn kind_from_str(value: &str) -> Result<ElementKind, StoreError> {
    match value {
        "file" => Ok(ElementKind::File),
        "container" => Ok(ElementKind::Container),
        "function" => Ok(ElementKind::Function),
        "section" => Ok(ElementKind::Section),
        "turn" => Ok(ElementKind::Turn),
        "row" => Ok(ElementKind::Row),
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
            ElementKind::Row,
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
