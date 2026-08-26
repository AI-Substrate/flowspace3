//! The `scan_file` handler: bytes on disk become element rows.
//!
//! The whole job in one sentence: read the file the queue named, ask the pure
//! scanner what is in it, write the tree under the blob it hashed to, and queue
//! the enrichment the new content earns.
//!
//! # The content-addressed skip
//!
//! Before parsing anything, [`fs3_store::get_elements`] is asked whether this
//! `(blob, parser_version)` has been seen. `Some` means somebody has already
//! parsed these exact bytes with this exact parser — on another branch, in
//! another checkout, on another day — and the work is skipped. That is not a
//! cache; it is the same answer by construction, because a blob IS the hash of
//! the bytes.
//!
//! It is also where the plan's idempotence claim is paid: a re-scan of an
//! unchanged tree enqueues no scans at all (the path→blob map is identical), and
//! a scan that does run over already-known bytes writes nothing and enqueues no
//! enrichment.
//!
//! # Which blob key is used, and why it matters
//!
//! `fs3_parsers::scan` computes its own `tree.blob` — a sha-256 of the bytes,
//! the PRD req 23 content key for folders with no git to ask. The store row is
//! written under the GIT blob id instead, the one the queue payload carries,
//! because that is the key `worktree_files` uses and the two must meet at the
//! same value for a search hit to resolve to a path. Passing the blob explicitly
//! rather than reading `tree.blob` is what makes the choice visible.

use std::path::PathBuf;

use fs3_core::envelope::Failure;
use fs3_core::{BlobRef, Element, ElementKind, catalog, needs_summary};

use crate::enrich;
use crate::roots::ScanFileJob;
use crate::runner::{fail, payload};
use crate::wiring::AppState;

/// Parse one file and record it.
///
/// # Errors
/// A [`Failure`] carrying the catalog code for whatever went wrong. A file that
/// vanished between enqueue and claim is NOT a failure: it is a file that is no
/// longer there, and the job completes having done nothing.
pub async fn run(state: &AppState, value: serde_json::Value) -> Result<(), Failure> {
    let job: ScanFileJob = payload(value)?;

    let Some(worktree) = fs3_store::list_worktrees(&state.db)
        .await
        .map_err(fail)?
        .into_iter()
        .find(|worktree| worktree.id == job.worktree_id)
    else {
        // The root was removed while the job sat in the queue. Nothing to do,
        // and nothing wrong — completing is the honest outcome.
        return Ok(());
    };

    let absolute = PathBuf::from(&worktree.root_path).join(&job.path);
    let bytes = match std::fs::read(&absolute) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(Failure::new(
                &catalog::SCAN_ROOT_NOT_FOUND,
                format!("cannot read {}: {error}", absolute.display()),
            )
            .with_fix(
                "check the file is readable by the daemon's user; it will be re-scanned on the \
                 next `flowspace3 scan`",
            ));
        }
    };

    let blob = BlobRef::new(job.blob.clone()).map_err(|error| {
        Failure::new(&catalog::QUEUE_JOB_FAILED, error.to_string()).retryable(false)
    })?;

    // The skip. Cheap, and correct for every branch and checkout at once.
    if fs3_store::get_elements(&state.db, &blob, PARSER_VERSION)
        .await
        .map_err(fail)?
        .is_some()
    {
        return Ok(());
    }

    let tree = fs3_parsers::scan(std::path::Path::new(&job.path), &bytes).map_err(fail)?;

    // The enrichment policy (decision D5) is the scanner's to inject and the
    // store's to record. A file element is excluded because summarising a whole
    // file duplicates what its parts already say; everything else earns a
    // summary once it clears the configured line floor.
    let min_lines = state.config.indexing.summary_min_lines;
    let enrich_policy =
        |element: &Element| element.kind != ElementKind::File && needs_summary(element, min_lines);

    fs3_store::upsert_element_tree(&state.db, &blob, PARSER_VERSION, &tree.root, enrich_policy)
        .await
        .map_err(fail)?;

    // Every element gets a raw-content vector; only enrich-marked ones get a
    // summary. Both are queued rather than done inline, so a slow provider
    // never holds up the next file's parse.
    enrich::enqueue_for_tree(state, &job, &tree.root, &enrich_policy).await?;

    Ok(())
}

/// The parser identity element rows are keyed by.
///
/// A literal rather than a crate version: what invalidates a parse is a change
/// in the GRAMMARS or the extraction walk, not a patch release of the daemon.
/// Bumping it re-mints every element row and costs nothing in the content layer
/// — enrichment is keyed by `raw_hash`, so a re-parse that produces the same
/// text pays for no LLM calls (workshop 002, decision D2).
pub const PARSER_VERSION: &str = "fs3-parsers@1";

#[cfg(test)]
mod tests {
    use super::*;
    use fs3_core::Span;

    fn element(kind: ElementKind, lines: u32) -> Element {
        Element::new(kind, "x", "n", "a::n", Span::new(1, lines), "body")
    }

    /// The policy in one place, asserted rather than assumed: a file root never
    /// earns its own summary, and a small function rides on its parent's.
    #[test]
    fn the_enrichment_policy_excludes_file_roots_and_small_elements() {
        let min_lines = 10;
        let policy = |el: &Element| el.kind != ElementKind::File && needs_summary(el, min_lines);

        assert!(!policy(&element(ElementKind::File, 400)));
        assert!(!policy(&element(ElementKind::Function, 9)));
        assert!(policy(&element(ElementKind::Function, 10)));
        assert!(policy(&element(ElementKind::Container, 40)));
    }
}
