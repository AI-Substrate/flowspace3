//! Enrichment: summaries and vectors, through the provider registry.
//!
//! Two job kinds, both keyed by `raw_hash` — the hash of the text they describe,
//! never an element id (workshop 002, decision D2). That is what makes the same
//! function body on forty branches ONE summary and ONE pair of vectors, and it
//! is why a parser bump re-mints every element row while costing nothing here.
//!
//! # Per-repo instance resolution
//!
//! Each job carries the repo identity it came from, and the handler asks
//! [`AppState::embedder_for`] / [`AppState::summarizer_for`] for the instance
//! that repository selected. The resolution is a map lookup over `Arc`s wired at
//! startup — no client is constructed mid-flight, and two repositories naming
//! one instance share one HTTP client.
//!
//! # Batching and concurrency
//!
//! An `embed` job carries a BATCH of texts, not one: an embeddings API charges
//! per call as much as per token, and one call for sixteen texts is the
//! difference between a scan that finishes and one that rate-limits. Batches are
//! formed at enqueue time, where the whole tree is in hand.
//!
//! Concurrency across jobs is the runner's (N claimers), so there is no
//! semaphore here — the queue IS the semaphore, and its width is one config
//! value rather than two that can disagree.

use fs3_core::envelope::Failure;
use fs3_core::{Element, catalog};
use fs3_store::{NewEmbedding, SourceKind};
use serde::{Deserialize, Serialize};

use crate::roots::ScanFileJob;
use crate::runner::{fail, payload};
use crate::wiring::AppState;

/// Job kind: summarise one raw text.
pub const SUMMARIZE: &str = "summarize";
/// Job kind: embed a batch of texts.
pub const EMBED: &str = "embed";

/// How many texts ride in one `embed` job.
///
/// Sixteen is the fs2-proven shape: large enough that per-call overhead stops
/// dominating, small enough that one provider hiccup re-runs sixteen texts
/// rather than a whole file's worth.
pub const EMBED_BATCH: usize = 16;

/// Summarise one element's text.
///
/// The job carries everything the summariser reads — kind, name, address, span,
/// body — rather than an id to look up. Two reasons, and the second is the real
/// one: a queue row that describes its own work cannot be invalidated by a
/// parser bump re-minting the element rows between enqueue and claim, and the
/// lookup it replaces ("one element carrying this hash") would have picked an
/// arbitrary representative anyway, because the same body lives at many
/// addresses. Making the representative explicit at enqueue time is the honest
/// version of the same choice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummarizeJob {
    /// The repo whose provider selection applies.
    pub identity: String,
    /// The dirtiness key, and the key the summary is stored under.
    pub raw_hash: String,
    /// The element to summarise, as it was when the tree was scanned.
    pub element: Element,
}

impl SummarizeJob {
    /// One live job per `(raw_hash, repo)`.
    ///
    /// Keyed by content, so forty branches holding one body enqueue once. The
    /// repo rides along because two repositories may select different
    /// summarisers, and their answers are different rows.
    #[must_use]
    pub fn dedupe_key(&self) -> String {
        format!("summarize:{}:{}", self.identity, self.raw_hash)
    }
}

/// Embed a batch of texts under one source kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbedJob {
    /// The repo whose provider selection applies.
    pub identity: String,
    /// `raw` or `smart` — which space these vectors belong to.
    pub source: String,
    /// `(source_hash, text)` pairs. The text rides in the payload rather than
    /// being re-read, because the element row it came from may be re-minted by
    /// a parser bump between enqueue and claim while the hash stays the answer.
    pub items: Vec<(String, String)>,
}

impl EmbedJob {
    /// One live job per batch, keyed by the batch's first hash.
    ///
    /// The batch is deterministic — same tree, same order, same grouping — so
    /// this collapses duplicate enqueues of the same work without needing to
    /// hash the whole batch.
    #[must_use]
    pub fn dedupe_key(&self) -> String {
        let first = self
            .items
            .first()
            .map_or("empty", |(hash, _)| hash.as_str());
        format!("embed:{}:{}:{}", self.identity, self.source, first)
    }

    fn source_kind(&self) -> Result<SourceKind, Failure> {
        match self.source.as_str() {
            "raw" => Ok(SourceKind::Raw),
            "smart" => Ok(SourceKind::Smart),
            other => Err(Failure::new(
                &catalog::QUEUE_JOB_FAILED,
                format!("unknown embedding source {other:?}"),
            )
            .retryable(false)),
        }
    }
}

/// Whether this element's own text earns a raw vector.
///
/// Everything does, with ONE exception: a file element that has parsed children.
/// Its `raw_text` is the concatenation of those children, so its vector is a
/// blurred average of texts that are already indexed individually — it carries
/// nothing they do not, it is the largest text in the tree to store, and
/// because it contains every token in the file it competes with every one of
/// its own parts on every query about that file. A search for "validate an
/// expired session token" should answer with the function, not with the file
/// the function is in.
///
/// A file with NO children is the opposite case: prose, an unknown language, a
/// grammar fs3 does not have. There the file element IS the content, and
/// skipping it would make the file unsearchable. That is why the rule is
/// "covered by its children", not "is a file".
fn earns_raw_vector(element: &Element) -> bool {
    element.kind != fs3_core::ElementKind::File || element.children.is_empty()
}

/// Queue the enrichment a freshly-scanned tree earns.
///
/// Raw vectors for every element its children do not already cover, summaries
/// only for the ones the policy marked. Both are jobs rather than inline work:
/// a slow provider must never hold up the next file's parse, and the queue is
/// where the concurrency already lives.
///
/// # Errors
/// Store failures while enqueueing.
pub async fn enqueue_for_tree(
    state: &AppState,
    job: &ScanFileJob,
    root: &Element,
    enrich: &impl Fn(&Element) -> bool,
) -> Result<(), Failure> {
    let mut raw_batch: Vec<(String, String)> = Vec::with_capacity(EMBED_BATCH);

    for element in root.iter() {
        if earns_raw_vector(element) {
            raw_batch.push((element.raw_hash().to_string(), element.raw_text.clone()));
        }
        if raw_batch.len() == EMBED_BATCH {
            enqueue_embed(
                state,
                &job.identity,
                SourceKind::Raw,
                std::mem::take(&mut raw_batch),
            )
            .await?;
        }

        if enrich(element) {
            // The element is cloned WITHOUT its children: the summariser reads
            // one declaration's own body, and a container's payload would
            // otherwise carry every descendant's text a second time.
            let mut subject = element.clone();
            subject.children.clear();
            let summarize = SummarizeJob {
                identity: job.identity.clone(),
                raw_hash: element.raw_hash().to_string(),
                element: subject,
            };
            enqueue(state, SUMMARIZE, &summarize.dedupe_key(), &summarize).await?;
        }
    }

    if !raw_batch.is_empty() {
        enqueue_embed(state, &job.identity, SourceKind::Raw, raw_batch).await?;
    }
    Ok(())
}

/// Run one `summarize` job: call the repo's summariser, store the answer, and
/// queue the summary's own vector.
///
/// # Errors
/// A provider failure (retryable — a rate limit clears) or a store failure.
pub async fn summarize(state: &AppState, value: serde_json::Value) -> Result<(), Failure> {
    let job: SummarizeJob = payload(value)?;
    let model_key = state.summarizer_key(&job.identity);

    // Content-addressed skip: another branch, or an earlier attempt of this
    // same job, may already have paid for this text.
    if fs3_store::get_smart_content(&state.db, &job.raw_hash, &model_key)
        .await
        .map_err(fail)?
        .is_some()
    {
        return Ok(());
    }

    let summary = state
        .summarizer_for(&job.identity)
        .summarize(&job.element)
        .await
        .map_err(fail)?;

    // PRD req 36's band is a database CHECK, so a provider that ignored the
    // instruction would fail the insert with a constraint error naming nothing
    // useful. Catching it here names the provider and the count.
    if !summary.has_valid_tags() {
        return Err(Failure::new(
            &catalog::PROVIDER_FAILED,
            format!(
                "summarizer returned {} tags for {}; PRD req 36 requires 1-5",
                summary.tags.len(),
                job.element.address
            ),
        )
        .retryable(true));
    }

    fs3_store::put_smart_content(&state.db, &job.raw_hash, &model_key, &summary)
        .await
        .map_err(fail)?;

    // The summary is now content in its own right, and it gets its own vector —
    // keyed by the hash of the summary TEXT, which is what lets a smart hit
    // resolve back to the code it describes.
    let text_hash = fs3_core::content_hash(summary.text.as_bytes());
    enqueue_embed(
        state,
        &job.identity,
        SourceKind::Smart,
        vec![(text_hash, summary.text)],
    )
    .await
}

/// Run one `embed` job: one provider call for the whole batch, one transaction
/// for the vectors.
///
/// # Errors
/// A provider failure (retryable), a width mismatch (not retryable — the fix is
/// a different model), or a store failure.
pub async fn embed(state: &AppState, value: serde_json::Value) -> Result<(), Failure> {
    let job: EmbedJob = payload(value)?;
    let source_kind = job.source_kind()?;
    if job.items.is_empty() {
        return Ok(());
    }

    let model_key = state.embedder_key(&job.identity);
    let texts: Vec<String> = job.items.iter().map(|(_, text)| text.clone()).collect();

    let vectors = state
        .embedder_for(&job.identity)
        .embed(&texts)
        .await
        .map_err(fail)?;

    // A provider that returns a different number of vectors than it was given
    // texts has silently misaligned the batch, and storing it would attach every
    // vector to the wrong hash — a corruption that looks exactly like a working
    // index until somebody searches.
    if vectors.len() != job.items.len() {
        return Err(Failure::new(
            &catalog::PROVIDER_FAILED,
            format!(
                "embedder returned {} vectors for {} texts; the batch cannot be aligned",
                vectors.len(),
                job.items.len()
            ),
        )
        .retryable(false));
    }

    let rows: Vec<NewEmbedding<'_>> = job
        .items
        .iter()
        .zip(&vectors)
        .map(|((hash, _), vector)| NewEmbedding {
            source_hash: hash,
            source_kind,
            vector,
        })
        .collect();

    fs3_store::put_embeddings(&state.db, &model_key, &rows)
        .await
        .map_err(fail)
}

/// Enqueue one embed batch.
async fn enqueue_embed(
    state: &AppState,
    identity: &str,
    source: SourceKind,
    items: Vec<(String, String)>,
) -> Result<(), Failure> {
    if items.is_empty() {
        return Ok(());
    }
    let job = EmbedJob {
        identity: identity.to_string(),
        source: source.as_str().to_string(),
        items,
    };
    enqueue(state, EMBED, &job.dedupe_key(), &job).await
}

async fn enqueue<T: Serialize>(
    state: &AppState,
    kind: &str,
    dedupe_key: &str,
    job: &T,
) -> Result<(), Failure> {
    let payload = serde_json::to_value(job)
        .map_err(|error| Failure::new(&catalog::QUEUE_JOB_FAILED, error.to_string()))?;
    fs3_store::enqueue_job(
        &state.db,
        kind,
        dedupe_key,
        &payload,
        std::time::Duration::ZERO,
    )
    .await
    .map_err(fail)
}

#[cfg(test)]
/// A childless element to summarise, for building a job by hand in tests.
fn subject(address: &str) -> Element {
    Element::new(
        fs3_core::ElementKind::Function,
        "function_item",
        "f",
        address,
        fs3_core::Span::new(1, 12),
        "fn f() {}",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keyed by CONTENT, so forty branches holding one body enqueue one job —
    /// the D2 saving, made visible at the queue rather than only at the table.
    #[test]
    fn a_summarize_job_is_keyed_by_content_not_by_element() {
        let job = |address: &str| SummarizeJob {
            identity: "git:github.com/AI-Substrate/flowspace3".to_string(),
            raw_hash: "abc123".to_string(),
            element: subject(address),
        };
        assert_eq!(
            job("a.rs::f").dedupe_key(),
            job("b.rs::g").dedupe_key(),
            "one body is one piece of work however many places hold it"
        );
    }

    /// Two repositories may select different summarisers, so their answers are
    /// different rows and must not collapse into one job.
    #[test]
    fn two_repositories_summarise_the_same_text_separately() {
        let job = |identity: &str| SummarizeJob {
            identity: identity.to_string(),
            raw_hash: "abc123".to_string(),
            element: subject("a.rs::f"),
        };
        assert_ne!(job("repo-a").dedupe_key(), job("repo-b").dedupe_key());
    }

    /// Raw and smart vectors of the same text are different rows in different
    /// spaces; one dedupe key for both would lose one of them.
    #[test]
    fn raw_and_smart_batches_do_not_collide() {
        let job = |source: &str| EmbedJob {
            identity: "repo".to_string(),
            source: source.to_string(),
            items: vec![("hash".to_string(), "text".to_string())],
        };
        assert_ne!(job("raw").dedupe_key(), job("smart").dedupe_key());
    }

    #[test]
    fn an_unknown_source_is_terminal_rather_than_retried() {
        let job = EmbedJob {
            identity: "repo".to_string(),
            source: "conversation".to_string(),
            items: vec![],
        };
        let failure = job
            .source_kind()
            .expect_err("that space does not exist yet");
        assert!(!failure.retryable);
    }
}
