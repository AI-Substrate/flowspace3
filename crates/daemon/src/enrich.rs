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

use std::collections::{HashMap, VecDeque};

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

/// A provider input stays below the 8,192-token hosted-model cap with enough
/// headroom for the byte-based estimate to be conservative rather than exact.
const CHUNK_WINDOW_TOKENS: usize = 7_500;
/// Two hundred tokens preserve meaning that straddles a window boundary while
/// adding little request overhead compared with a 7,500-token window.
const CHUNK_OVERLAP_TOKENS: usize = 200;
/// One tightening reaches the one-byte-per-token lower bound.
const MAX_HEAL_ROUNDS: usize = 1;
// A prepared chunk must always be splittable into a legal request by itself.
const _: () = assert!(CHUNK_WINDOW_TOKENS <= crate::batch::TOKEN_BUDGET);

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChunkText {
    chunk_no: usize,
    text: String,
}

/// Split one text into overlapping, UTF-8-safe provider inputs.
///
/// For `len > window`, the count is
/// `1 + ceil((len - window) / (window - overlap))`: overlap advances each
/// subsequent window by the smaller step, not by the full window width.
fn chunk_plan(text: &str, window_tokens: usize, overlap_tokens: usize) -> Vec<ChunkText> {
    assert!(window_tokens > 0, "a chunk window must be non-zero");
    assert!(
        overlap_tokens < window_tokens,
        "chunk overlap must be smaller than its window"
    );

    let window_bytes = fs3_core::tokens::input_budget_bytes(window_tokens).max(1);
    let overlap_bytes = overlap_tokens
        .saturating_mul(fs3_core::BYTES_PER_TOKEN)
        .min(window_bytes.saturating_sub(1));
    chunk_plan_bytes(text, window_bytes, overlap_bytes)
}

fn chunk_plan_bytes(text: &str, window_bytes: usize, overlap_bytes: usize) -> Vec<ChunkText> {
    assert!(window_bytes > 0, "a chunk window must be non-zero");
    assert!(
        overlap_bytes < window_bytes,
        "chunk overlap must be smaller than its window"
    );
    if text.len() <= window_bytes {
        return vec![ChunkText {
            chunk_no: 0,
            text: text.to_string(),
        }];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let target_end = start.saturating_add(window_bytes).min(text.len());
        let mut end = target_end;
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            end = text[start..]
                .char_indices()
                .nth(1)
                .map_or(text.len(), |(offset, _)| start + offset);
        }

        chunks.push(ChunkText {
            chunk_no: chunks.len(),
            text: text[start..end].to_string(),
        });
        if end == text.len() {
            break;
        }

        let target_start = end.saturating_sub(overlap_bytes);
        let mut next = target_start;
        while next > start && !text.is_char_boundary(next) {
            next -= 1;
        }
        if next <= start {
            next = text[start..]
                .char_indices()
                .nth(1)
                .map_or(end, |(offset, _)| start + offset);
        }
        start = next.min(end);
    }

    chunks
}

struct PreparedChunk<'a> {
    source_hash: &'a str,
    source_bytes: usize,
    heal_round: usize,
}

struct PreparedCall<'a> {
    chunks: Vec<PreparedChunk<'a>>,
    texts: Vec<String>,
    estimated_tokens: usize,
}

fn budget_prepared<'a>(
    chunks: Vec<PreparedChunk<'a>>,
    texts: Vec<String>,
) -> VecDeque<PreparedCall<'a>> {
    debug_assert_eq!(chunks.len(), texts.len());
    let mut calls = VecDeque::new();
    let mut current = PreparedCall {
        chunks: Vec::new(),
        texts: Vec::new(),
        estimated_tokens: 0,
    };

    for (chunk, text) in chunks.into_iter().zip(texts) {
        let cost = fs3_core::estimate_tokens(&text);
        debug_assert!(cost <= crate::batch::TOKEN_BUDGET);
        if !current.texts.is_empty() && current.estimated_tokens + cost > crate::batch::TOKEN_BUDGET
        {
            calls.push_back(current);
            current = PreparedCall {
                chunks: Vec::new(),
                texts: Vec::new(),
                estimated_tokens: 0,
            };
        }
        current.estimated_tokens += cost;
        current.chunks.push(chunk);
        current.texts.push(text);
    }
    if !current.texts.is_empty() {
        calls.push_back(current);
    }
    calls
}

fn embedding_text_is_nonblank(source_hash: &str, source_kind: SourceKind, text: &str) -> bool {
    if !text.trim().is_empty() {
        return true;
    }
    tracing::warn!(
        %source_hash,
        kind = source_kind.as_str(),
        "dropping empty or whitespace-only embedding input at mint"
    );
    false
}

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
    /// One live job per batch, keyed by the WHOLE batch's contents.
    ///
    /// The first item's hash is not enough, and the failure is silent. Two
    /// different batches routinely share a first element — two branches whose
    /// files agree at the top and diverge below, or an edited file whose first
    /// declaration did not move — and `enqueue_job`'s `ON CONFLICT` REPLACES
    /// the payload of the live row. The displaced batch's remaining items are
    /// then never embedded: no error, no failed job, just elements that search
    /// cannot see.
    ///
    /// Hashing every item makes the key mean "this exact set of texts". Sorted
    /// first, so a batch assembled in a different order is recognised as the
    /// same work rather than paid for twice.
    #[must_use]
    pub fn dedupe_key(&self) -> String {
        let mut hashes: Vec<&str> = self.items.iter().map(|(hash, _)| hash.as_str()).collect();
        hashes.sort_unstable();
        let digest = fs3_core::content_hash(hashes.join("\n").as_bytes());
        format!("embed:{}:{}:{}", self.identity, self.source, digest)
    }

    fn source_kind(&self) -> Result<SourceKind, Failure> {
        source_kind_of(&self.source)
    }
}

/// Read the `source` field of an embed job or a merged batch.
///
/// One parser for both, so a batch and the job it came from can never disagree
/// about which vector space they are writing into.
///
/// # Errors
/// Not retryable: an unknown source is a defect in whoever enqueued it, and no
/// number of attempts will make `"raw "` mean `"raw"`.
pub fn source_kind_of(source: &str) -> Result<SourceKind, Failure> {
    match source {
        "raw" => Ok(SourceKind::Raw),
        "smart" => Ok(SourceKind::Smart),
        other => Err(Failure::new(
            &catalog::QUEUE_JOB_FAILED,
            format!("unknown embedding source {other:?}"),
        )
        .retryable(false)),
    }
}

/// Whether this element's own text earns a raw vector.
///
/// Empty text never earns a vector: it contributes no meaning, and resolving
/// one shared empty hash among many structural elements produces an arbitrary
/// hit that looks authoritative.
///
/// Non-empty elements do, with one further exception: a file element that has
/// parsed children. Its `raw_text` is the concatenation of those children, so
/// its vector is a noisier duplicate of vectors we already write. A file with
/// NO children is the opposite case: prose, an unknown language, a grammar fs3
/// does not have. There the file element IS the content.
fn earns_raw_vector(element: &Element) -> bool {
    if element.kind == fs3_core::ElementKind::File && !element.children.is_empty() {
        return false;
    }
    embedding_text_is_nonblank(element.raw_hash(), SourceKind::Raw, &element.raw_text)
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

/// Queue the enrichment a batch of newly-stored turns earns.
///
/// The conversation twin of [`enqueue_for_tree`], and deliberately the SAME
/// two job kinds with the same shapes, dedupe keys and lanes: a conversation
/// rides the existing engine, it does not get one of its own (workshop 005,
/// C1). Nothing here is configured per conversation, because there is nothing
/// new to configure.
///
/// Two differences from a code tree, both of them about shape rather than
/// policy. Turns are flat, so there is no "covered by its children" exception —
/// every turn earns its raw vector. And the summary gate is a BYTE floor
/// (workshop 005) rather than a line floor: a turn is one position in a
/// sequence, so lines cannot tell a five-word "ship it" from the same turn
/// carrying a 4KB tool result.
///
/// Takes ONLY the turns the store actually accepted, which is what makes a
/// re-post of an overlapping batch free: an already-stored turn never reaches
/// here, so nobody is paid twice.
///
/// Returns how many turns earned their own summary.
///
/// # Errors
/// Store failures while enqueueing.
pub async fn enqueue_for_turns(
    state: &AppState,
    identity: &str,
    turns: &[Element],
    summary_floor_bytes: usize,
) -> Result<usize, Failure> {
    let mut raw_batch: Vec<(String, String)> = Vec::with_capacity(EMBED_BATCH.min(turns.len()));
    let mut summarized = 0;

    for element in turns {
        let earns_vector = earns_raw_vector(element);
        if earns_vector {
            raw_batch.push((element.raw_hash().to_string(), element.raw_text.clone()));
        }
        if raw_batch.len() == EMBED_BATCH {
            enqueue_embed(
                state,
                identity,
                SourceKind::Raw,
                std::mem::take(&mut raw_batch),
            )
            .await?;
        }

        if earns_vector
            && fs3_core::conversation::earns_summary(&element.raw_text, summary_floor_bytes)
        {
            let summarize = SummarizeJob {
                identity: identity.to_string(),
                raw_hash: element.raw_hash().to_string(),
                element: element.clone(),
            };
            enqueue(state, SUMMARIZE, &summarize.dedupe_key(), &summarize).await?;
            summarized += 1;
        }
    }

    if !raw_batch.is_empty() {
        enqueue_embed(state, identity, SourceKind::Raw, raw_batch).await?;
    }
    Ok(summarized)
}

/// Re-queue vectors the content layer is missing, up to `limit` texts.
///
/// The recovery path for a level-0 GC defect that has already run: an `embed`
/// job carries a BATCH as `items` and has no `raw_hash` field, so until this
/// binary the unreferenced-jobs predicate read every one of them as garbage
/// and deleted any batch still pending when a pass landed. Nothing failed;
/// the content simply never became searchable, and the queue's own memory
/// recorded no work outstanding.
///
/// Derives its backlog from the SCHEMA — content with no vector row — rather
/// than from the queue, which is what makes it self-healing across a defect
/// that destroyed the queue rows. Cheap when there is nothing to do: one
/// indexed read per space returning nothing.
///
/// Uses the DEFAULT embedder's model key. A per-repo embedder writes into its
/// own vector space and its rows are not missing from that space, so sweeping
/// them here under the default key would re-queue work that exists; the repo's
/// own scans remain their path back.
///
/// Returns how many texts were queued.
///
/// # Errors
/// Store failures while reading the backlog or enqueueing.
pub async fn requeue_missing_vectors(state: &AppState, limit: i64) -> Result<usize, Failure> {
    let model_key = state.embedder.key();
    let missing = fs3_store::missing_embeddings(&state.db, &model_key, limit)
        .await
        .map_err(fail)?;
    if missing.is_empty() {
        return Ok(0);
    }

    // Grouped by space, because one job embeds one space: `source_kind` is part
    // of the vector row's primary key, and a batch that mixed them would write
    // half its vectors under the wrong one.
    let queued = missing.len();
    for kind in [SourceKind::Raw, SourceKind::Smart] {
        let mut batch: Vec<(String, String)> = Vec::with_capacity(EMBED_BATCH);
        for item in missing.iter().filter(|item| item.source_kind == kind) {
            batch.push((item.source_hash.clone(), item.text.clone()));
            if batch.len() == EMBED_BATCH {
                enqueue_embed(state, RECOVERY_IDENTITY, kind, std::mem::take(&mut batch)).await?;
            }
        }
        if !batch.is_empty() {
            enqueue_embed(state, RECOVERY_IDENTITY, kind, batch).await?;
        }
    }

    Ok(queued)
}

/// The identity a recovery batch is charged to.
///
/// A recovery sweep reads the DEFAULT provider's space and cannot know which
/// repository each orphaned text came from — content is keyed by hash, and one
/// hash belongs to elements of many blobs in many repos, which is the whole
/// point of decision D2. An identity nothing configured resolves to the default
/// provider, which is the space the backlog was read from, so the answer lands
/// exactly where the sweep decided it was missing.
///
/// `conv:` because it is a namespace [`fs3_core::RepoIdentity`] structurally
/// cannot mint, so this can never shadow a real repository's selection.
const RECOVERY_IDENTITY: &str = "conv:recovery";

/// Re-queue summaries the content layer is missing, up to `limit` elements.
///
/// The same hole one shelf up from [`requeue_missing_vectors`], and it had the
/// same two causes: the level-0 defect this binary fixes reaped `summarize`
/// jobs whose batch-shaped siblings it could not read, and
/// [`fs3_store::missing_enrichment`] — the decision-D6 sweep written for
/// exactly this — had no production caller at all. It existed, and only tests
/// ever ran it.
///
/// An element with no summary is not broken, only thinner: it still has its raw
/// vector, so search can still reach it. That is why this is a quiet
/// reconciliation rather than an alarm — but it is real spend that was
/// authorised and never delivered, and nothing else would ever notice, because
/// a scan of an unchanged tree enqueues nothing.
///
/// # Errors
/// Store failures while reading the backlog or enqueueing.
pub async fn requeue_missing_summaries(state: &AppState, limit: i64) -> Result<usize, Failure> {
    let model_key = state.summarizer.key();
    let missing = fs3_store::missing_enrichment(&state.db, &model_key, limit)
        .await
        .map_err(fail)?;

    for item in &missing {
        let job = SummarizeJob {
            identity: RECOVERY_IDENTITY.to_string(),
            raw_hash: item.raw_hash.clone(),
            element: item.element.clone(),
        };
        enqueue(state, SUMMARIZE, &job.dedupe_key(), &job).await?;
    }

    Ok(missing.len())
}

/// Run one `summarize` job: call the repo's summariser, store the answer, and
/// queue the summary's own vector.
///
/// # Errors
/// A provider failure (retryable — a rate limit clears) or a store failure.
pub async fn summarize(state: &AppState, value: serde_json::Value) -> Result<(), Failure> {
    let job: SummarizeJob = payload(value)?;

    // The point of spend (req-0057). A root removed while this sat in the
    // queue leaves work for content nothing maps any more, and summarising it
    // pays a provider for something nobody can ever search. GC reaps such jobs
    // on its own cadence, but the queue drains faster than GC runs — and a job
    // CLAIMED before the removal landed is one GC can never reach.
    //
    // Keyed by raw_hash rather than by blob deliberately: one raw hash can
    // belong to elements of many blobs, so it stays worth paying for while ANY
    // of them is still referenced. Same predicate GC uses at level two.
    if !fs3_store::raw_hash_is_referenced(&state.db, &job.raw_hash)
        .await
        .map_err(fail)?
    {
        tracing::debug!(
            raw_hash = %job.raw_hash,
            "skipping enrichment for content no registered root holds"
        );
        return Ok(());
    }

    let model_key = state.summarizer_key(&job.identity);

    // Content-addressed skip: another branch, or an earlier attempt of this
    // same job, may already have paid for this text.
    //
    // It RE-EMITS the smart embed rather than returning early, for the same
    // reason the scan skip does. The summary and its vector are written in
    // separate steps, so a crash or a provider outage in between — or a retry
    // of this very job after the summary landed but before its embed was
    // enqueued — would otherwise leave a summary that is never embedded: paid
    // for, stored, and unreachable by semantic search, with nothing reporting a
    // problem. Enqueueing costs nothing when the work is already done, because
    // the embed job is keyed by content and its handler is itself idempotent.
    if let Some(existing) = fs3_store::get_smart_content(&state.db, &job.raw_hash, &model_key)
        .await
        .map_err(fail)?
    {
        let text_hash = fs3_core::content_hash(existing.text.as_bytes());
        if !embedding_text_is_nonblank(&text_hash, SourceKind::Smart, &existing.text) {
            return Ok(());
        }
        return enqueue_embed(
            state,
            &job.identity,
            SourceKind::Smart,
            vec![(text_hash, existing.text)],
        )
        .await;
    }

    let summarizer = state.summarizer_for(&job.identity);

    // THE SAME GUARD, on the other provider.
    //
    // A chat model's window is one to two orders of magnitude larger than an
    // embedding model's per-input cap, so this fires far more rarely — but it
    // is a cliff of exactly the same shape, and "larger" is not "absent". A
    // generated file, a vendored bundle or a data table that the scanner hands
    // back as ONE element is measured in hundreds of kilobytes, and a prompt
    // built around it is refused on arrival and refused identically on every
    // retry.
    //
    // The failure mode is WORSE here than on the embed side, which is why this
    // is not left to the provider. An embeddings endpoint answers an oversized
    // input with a 400 naming the cap. A chat endpoint may instead truncate
    // the prompt itself and answer anyway — and a summary of a silently
    // truncated prompt is indistinguishable, in the store and in search
    // results, from a summary of the whole element. Cutting it here is what
    // makes the shortfall knowable.
    let element = match fs3_core::fit_to_cap(&job.element.raw_text, summarizer.max_input_tokens()) {
        None => std::borrow::Cow::Borrowed(&job.element),
        Some(prefix) => {
            tracing::warn!(
                address = %job.element.address,
                cap = summarizer.max_input_tokens(),
                from_bytes = job.element.raw_text.len(),
                to_bytes = prefix.len(),
                "element exceeds the summarizer's prompt budget; summarising a prefix of it"
            );
            let mut shortened = job.element.clone();
            shortened.raw_text = prefix.to_string();
            std::borrow::Cow::Owned(shortened)
        }
    };

    let mut summary = summarizer.summarize(&element).await.map_err(fail)?;

    // Recorded through `extras`, the staging area migration 0006 created for
    // exactly this: a fact worth persisting that has not earned a column. It
    // is deliberately NOT part of `text_hash` — the hash addresses the summary
    // TEXT, and folding a flag into it would re-key every smart vector.
    if matches!(element, std::borrow::Cow::Owned(_)) {
        summary
            .extras
            .insert("truncated_input".to_string(), serde_json::json!(true));
    }

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
    if !embedding_text_is_nonblank(&text_hash, SourceKind::Smart, &summary.text) {
        return Ok(());
    }
    enqueue_embed(
        state,
        &job.identity,
        SourceKind::Smart,
        vec![(text_hash, summary.text)],
    )
    .await
}

/// Run one `embed` job: one provider call for the batch, one transaction for
/// the vectors.
///
/// # Errors
/// A provider failure (retryable), a width mismatch (not retryable — the fix is
/// a different model), or a store failure.
pub async fn embed(state: &AppState, value: serde_json::Value) -> Result<(), Failure> {
    let job: EmbedJob = payload(value)?;
    let source_kind = job.source_kind()?;
    embed_items(state, &job.identity, source_kind, &job.items).await
}

/// Embed `items` for one repo and one source kind, skipping what is stored.
///
/// Split out from [`embed`] because a MERGED batch — k claimed jobs whose
/// items ride in one provider call — needs exactly this and has no single
/// payload to hand over. Both paths therefore share the dedupe filter and the
/// alignment assert rather than growing two copies that can drift.
///
/// Callers must group by `(identity, source_kind)` before merging: the
/// identity selects the EMBEDDER, so items from two repos with different
/// embedders cannot share a call, and the kind is part of the storage key.
///
/// # Errors
/// A provider failure (retryable), a width mismatch (not retryable), or a
/// store failure.
pub async fn embed_items(
    state: &AppState,
    identity: &str,
    source_kind: fs3_store::SourceKind,
    all: &[(String, String)],
) -> Result<(), Failure> {
    if all.is_empty() {
        return Ok(());
    }

    let model_key = state.embedder_key(identity);

    // The dirty-is-a-missing-row doctrine, applied to COST rather than
    // correctness. Content-addressed work is re-emitted on purpose — a crash
    // between parse and enrichment must not strand elements with no job — but
    // re-emission was re-PAYING: measured at 2.9x on a live run (10,559
    // executions over 3,646 distinct jobs), because this handler called the
    // provider for its whole batch unconditionally.
    //
    // Keeping the re-emission and making re-execution free is the whole fix.
    let hashes: Vec<&str> = all.iter().map(|(hash, _)| hash.as_str()).collect();
    let stored = fs3_store::existing_embedding_hashes(&state.db, &model_key, source_kind, &hashes)
        .await
        .map_err(fail)?;

    let items: Vec<&(String, String)> = all
        .iter()
        .filter(|(hash, _)| !stored.contains(hash))
        .collect();

    // Nothing missing: the job succeeded, it simply cost nothing. Returning
    // before the provider call is the saving — an empty batch that still made
    // the round trip would have fixed the accounting and not the bill.
    if items.is_empty() {
        return Ok(());
    }

    // THE SPEND GUARD, on the other provider (req-0057).
    //
    // `summarize` has had this since the day it could pay for a removed root's
    // content; `embed` did not, and the asymmetry was invisible because the
    // dedupe filter above LOOKS like a spend guard. It is not: it asks whether
    // this text has already been bought, never whether it is still worth
    // buying. A NEW hash for content nothing maps any more sails straight
    // through it to the provider. Measured when the watcher pulled a
    // gitignored tree into the index: 4,436 raw vectors bought for content the
    // next full walk unreferenced, with the summarize guard saving the ~26,000
    // summaries beside them purely because it existed.
    //
    // Placed after the dedupe filter and before `embedder_for`: the cheapest
    // question first, and the provider reached only by items that survive both.
    let live = fs3_store::referenced_source_hashes(
        &state.db,
        source_kind,
        &items
            .iter()
            .map(|(hash, _)| hash.as_str())
            .collect::<Vec<_>>(),
    )
    .await
    .map_err(fail)?;

    // Counted after the filter, never as `len() - live.len()`: a merged batch
    // may carry one hash twice, and a count that assumed otherwise would
    // report spending that never happened.
    let offered = items.len();
    let items: Vec<&(String, String)> = items
        .into_iter()
        .filter(|(hash, _)| live.contains(hash))
        .collect();
    let dropped = offered - items.len();

    if dropped > 0 {
        // INFO, not DEBUG: this is money not spent, and the one place a
        // reader can see the guard working. Counted rather than listed — a
        // batch is sixteen hashes and a log line is not a ledger.
        tracing::info!(
            dropped,
            kept = items.len(),
            kind = source_kind.as_str(),
            "skipping embeds for content no registered root holds"
        );
    }

    if items.is_empty() {
        return Ok(());
    }

    let embedder = state.embedder_for(identity);

    // THE PER-INPUT GUARD.
    //
    // The batch planner budgets the SUM of a request; this prepares each
    // individual member. A provider can still count denser than our estimate.
    // Its typed cap rejection becomes a bounded request to split only the
    // offending member more tightly, never a retry of identical bytes.
    let cap = embedder.max_input_tokens();
    let mut prepared = Vec::with_capacity(items.len());
    let mut texts = Vec::with_capacity(items.len());
    for (hash, text) in &items {
        for chunk in chunk_plan(text, CHUNK_WINDOW_TOKENS, CHUNK_OVERLAP_TOKENS) {
            if fs3_core::estimate_tokens(&chunk.text) > cap {
                tracing::warn!(
                    source_hash = %hash,
                    kind = source_kind.as_str(),
                    cap,
                    bytes = chunk.text.len(),
                    "input exceeds the model's per-input cap after chunking"
                );
            }
            prepared.push(PreparedChunk {
                source_hash: hash,
                source_bytes: text.len(),
                heal_round: 0,
            });
            texts.push(chunk.text);
        }
    }

    let mut pending = budget_prepared(prepared, texts);
    let mut completed = Vec::new();
    let mut vectors = Vec::new();
    while let Some(mut call) = pending.pop_front() {
        let started = std::time::Instant::now();
        match embedder.embed(&call.texts).await {
            Ok(returned) => {
                tracing::info!(
                    kind = EMBED,
                    source = source_kind.as_str(),
                    items = call.texts.len(),
                    tokens = call.estimated_tokens,
                    outcome = "ok",
                    ms = started.elapsed().as_millis() as u64,
                    "embed: sent batch of {} texts",
                    call.texts.len()
                );
                if returned.len() != call.texts.len() {
                    return Err(Failure::new(
                        &catalog::PROVIDER_FAILED,
                        format!(
                            "embedder returned {} vectors for {} texts; the batch cannot be aligned",
                            returned.len(),
                            call.texts.len()
                        ),
                    )
                    .retryable(false));
                }
                completed.extend(call.chunks);
                vectors.extend(returned);
            }
            Err(fs3_core::Error::InputTooLong { input_index, .. }) => {
                tracing::info!(
                    kind = EMBED,
                    source = source_kind.as_str(),
                    items = call.texts.len(),
                    tokens = call.estimated_tokens,
                    outcome = "re-split",
                    ms = started.elapsed().as_millis() as u64,
                    "embed: provider cap rejection"
                );

                let index = match input_index.filter(|index| *index < call.texts.len()) {
                    Some(index) => index,
                    None if call.texts.len() > 1 => {
                        let at = call.texts.len() / 2;
                        let right = PreparedCall {
                            chunks: call.chunks.split_off(at),
                            texts: call.texts.split_off(at),
                            estimated_tokens: 0,
                        };
                        let mut halves = budget_prepared(call.chunks, call.texts);
                        let mut right = budget_prepared(right.chunks, right.texts);
                        while let Some(part) = right.pop_back() {
                            pending.push_front(part);
                        }
                        while let Some(part) = halves.pop_back() {
                            pending.push_front(part);
                        }
                        continue;
                    }
                    None => 0,
                };

                let chunk = call.chunks.remove(index);
                let text = call.texts.remove(index);
                if chunk.heal_round >= MAX_HEAL_ROUNDS {
                    let ratio = fs3_core::tokens::input_budget_bytes(CHUNK_WINDOW_TOKENS)
                        .checked_shr(chunk.heal_round as u32)
                        .unwrap_or(1)
                        .max(1)
                        / CHUNK_WINDOW_TOKENS;
                    return Err(Failure::new(
                        &catalog::PROVIDER_FAILED,
                        format!(
                            "input {} ({} bytes) still exceeds the provider cap after {} heal round(s); final ratio {} byte/token",
                            chunk.source_hash,
                            chunk.source_bytes,
                            MAX_HEAL_ROUNDS,
                            ratio.max(1)
                        ),
                    )
                    .retryable(false));
                }

                let next_round = chunk.heal_round + 1;
                let window_bytes = fs3_core::tokens::input_budget_bytes(CHUNK_WINDOW_TOKENS)
                    .checked_shr(next_round as u32)
                    .unwrap_or(1)
                    .max(1);
                let overlap_bytes = CHUNK_OVERLAP_TOKENS * fs3_core::BYTES_PER_TOKEN;
                let replacements = chunk_plan_bytes(&text, window_bytes, overlap_bytes);
                for replacement in replacements.into_iter().rev() {
                    call.chunks.insert(
                        index,
                        PreparedChunk {
                            source_hash: chunk.source_hash,
                            source_bytes: chunk.source_bytes,
                            heal_round: next_round,
                        },
                    );
                    call.texts.insert(index, replacement.text);
                }
                let mut retry = budget_prepared(call.chunks, call.texts);
                while let Some(part) = retry.pop_back() {
                    pending.push_front(part);
                }
            }
            Err(error) => {
                tracing::info!(
                    kind = EMBED,
                    source = source_kind.as_str(),
                    items = call.texts.len(),
                    tokens = call.estimated_tokens,
                    outcome = "error",
                    ms = started.elapsed().as_millis() as u64,
                    error = %error,
                    "embed: sent batch of {} texts",
                    call.texts.len()
                );
                return Err(fail(error));
            }
        }
    }

    // No rows are written until EVERY sub-call succeeds and aligns. Re-splits
    // are numbered only after all calls complete, so replacing one input cannot
    // collide with a sibling chunk number.
    debug_assert_eq!(vectors.len(), completed.len());
    let mut next_chunk = HashMap::<&str, i16>::new();
    let mut rows = Vec::with_capacity(completed.len());
    for (chunk, vector) in completed.iter().zip(&vectors) {
        let chunk_no = next_chunk.entry(chunk.source_hash).or_insert(0);
        rows.push(NewEmbedding {
            source_hash: chunk.source_hash,
            source_kind,
            chunk_no: *chunk_no,
            vector,
            truncated: false,
        });
        *chunk_no = chunk_no.checked_add(1).ok_or_else(|| {
            Failure::new(
                &catalog::PROVIDER_FAILED,
                format!(
                    "input {} needs more chunks than the store can address",
                    chunk.source_hash
                ),
            )
            .retryable(false)
        })?;
    }

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
    fn ascii_bytes(bytes: usize) -> String {
        "x".repeat(bytes)
    }
    #[test]
    fn chunk_plan_keeps_cap_minus_one_and_exactly_cap_whole() {
        let window_bytes = fs3_core::tokens::input_budget_bytes(4);
        for bytes in [window_bytes - 1, window_bytes] {
            let text = ascii_bytes(bytes);
            assert_eq!(
                chunk_plan(&text, 4, 1),
                vec![ChunkText { chunk_no: 0, text }]
            );
        }
    }
    #[test]
    fn chunk_plan_splits_one_byte_over_cap() {
        let window_bytes = fs3_core::tokens::input_budget_bytes(4);
        let chunks = chunk_plan(&ascii_bytes(window_bytes + 1), 4, 1);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].chunk_no, 0);
        assert_eq!(chunks[1].chunk_no, 1);
    }
    #[test]
    fn chunk_plan_uses_the_overlap_step_for_three_windows_of_input() {
        // 12 bytes per window, 3 bytes overlap: 1 + ceil((36 - 12) / 9) = 4.
        let chunks = chunk_plan(&ascii_bytes(36), 6, 1);
        assert_eq!(chunks.len(), 4);
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.chunk_no)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
    }

    fn reassemble_overlaps(chunks: &[ChunkText]) -> String {
        let mut whole = chunks.first().expect("at least one chunk").text.clone();
        for chunk in &chunks[1..] {
            let overlap = (0..=whole.len().min(chunk.text.len()))
                .rev()
                .find(|&bytes| {
                    whole.is_char_boundary(whole.len() - bytes)
                        && chunk.text.is_char_boundary(bytes)
                        && whole.ends_with(&chunk.text[..bytes])
                })
                .expect("zero bytes always overlap");
            whole.push_str(&chunk.text[overlap..]);
        }
        whole
    }

    #[test]
    fn chunk_plan_preserves_a_phrase_crossing_the_window_boundary() {
        let text = "123456789needleTAIL";
        let chunks = chunk_plan(text, 6, 2);
        assert!(
            chunks.iter().any(|chunk| chunk.text.contains("needle")),
            "the overlap must keep a boundary-spanning phrase intact: {chunks:?}"
        );
        assert_eq!(reassemble_overlaps(&chunks), text);
    }

    #[test]
    fn chunk_plan_snaps_utf8_boundaries_and_always_advances() {
        let text = "aé中🙂z";
        let chunks = chunk_plan(text, 2, 1);
        assert!(chunks.len() > 1, "the fixture must cross a window");
        assert_eq!(reassemble_overlaps(&chunks), text);
        assert!(chunks.iter().all(|chunk| !chunk.text.is_empty()));

        let wider_than_window = "🙂🙂";
        let tiny = chunk_plan(wider_than_window, 1, 0);
        assert_eq!(
            tiny.iter()
                .map(|chunk| chunk.text.as_str())
                .collect::<String>(),
            wider_than_window
        );
        assert_eq!(tiny.len(), 2, "one wide character per progressing chunk");
    }

    #[test]
    fn chunk_plan_alignment_measurement_for_ruled_corpus() {
        let oversized: String = (0..2_000)
            .map(|n| {
                format!("fn handler_{n:04}(request: Request) -> Response {{ dispatch(request) }}\n")
            })
            .collect();
        let request_whale: String = (0..10_000)
            .map(|n| {
                format!(
                    "fn request_whale_{n:05}(input: Request) -> Response {{ dispatch(input) }}\n"
                )
            })
            .collect();
        let prod_case = ascii_bytes(20_872);
        let corpus = [
            ("oversized", oversized),
            ("request_whale", request_whale),
            ("prod_20_872", prod_case),
        ];

        let old_window = CHUNK_WINDOW_TOKENS * fs3_core::BYTES_PER_TOKEN;
        let overlap = CHUNK_OVERLAP_TOKENS * fs3_core::BYTES_PER_TOKEN;
        let old_count = |bytes: usize| {
            if bytes <= old_window {
                1
            } else {
                1 + (bytes - old_window).div_ceil(old_window - overlap)
            }
        };
        let before: Vec<usize> = corpus
            .iter()
            .map(|(_, text)| old_count(text.len()))
            .collect();
        let after: Vec<usize> = corpus
            .iter()
            .map(|(_, text)| chunk_plan(text, CHUNK_WINDOW_TOKENS, CHUNK_OVERLAP_TOKENS).len())
            .collect();

        println!(
            "chunk_plan ruled corpus: oversized {}→{}, request_whale {}→{}, prod_20_872 {}→{}, total {}→{}",
            before[0],
            after[0],
            before[1],
            after[1],
            before[2],
            after[2],
            before.iter().sum::<usize>(),
            after.iter().sum::<usize>()
        );
        assert_eq!(
            corpus
                .iter()
                .map(|(_, text)| text.len())
                .collect::<Vec<_>>(),
            vec![136_000, 710_000, 20_872]
        );
        assert_eq!(before, vec![7, 33, 1]);
        assert_eq!(after, vec![10, 50, 2]);
    }

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

    /// The collision the first-hash key allowed, and the reason it was silent:
    /// `enqueue_job`'s `ON CONFLICT` REPLACES the live row's payload, so the
    /// displaced batch's items were simply never embedded — no error, no failed
    /// job, just elements search cannot see.
    #[test]
    fn two_batches_sharing_a_first_element_do_not_collide() {
        let batch = |tail: &str| EmbedJob {
            identity: "repo".to_string(),
            source: "raw".to_string(),
            items: vec![
                ("shared-head".to_string(), "fn head() {}".to_string()),
                (tail.to_string(), "fn tail() {}".to_string()),
            ],
        };
        assert_ne!(
            batch("branch-a-tail").dedupe_key(),
            batch("branch-b-tail").dedupe_key(),
            "two branches agreeing at the top and diverging below are different work"
        );
    }

    /// The same texts assembled in a different order are the SAME work, and
    /// paying for them twice is the mirror-image bug of the collision above.
    #[test]
    fn batch_order_does_not_change_the_key() {
        let items = |order: [&str; 2]| EmbedJob {
            identity: "repo".to_string(),
            source: "raw".to_string(),
            items: order
                .iter()
                .map(|hash| ((*hash).to_string(), "text".to_string()))
                .collect(),
        };
        assert_eq!(
            items(["aaa", "bbb"]).dedupe_key(),
            items(["bbb", "aaa"]).dedupe_key()
        );
    }

    /// A batch differing only in its LAST item must still be distinguishable —
    /// the edited-file case, where the head is unchanged.
    #[test]
    fn a_batch_differing_only_in_its_tail_is_different_work() {
        let batch = |last: &str| EmbedJob {
            identity: "repo".to_string(),
            source: "raw".to_string(),
            items: (0..15)
                .map(|n| (format!("hash-{n}"), "text".to_string()))
                .chain(std::iter::once((last.to_string(), "text".to_string())))
                .collect(),
        };
        assert_ne!(batch("before").dedupe_key(), batch("after").dedupe_key());
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

    /// A turn is a content type, not a second pipeline (workshop 005, C1): the
    /// SAME `SummarizeJob` carries it, with no field added and no lane
    /// configured. This is the end of the `ElementKind::Turn` wiring — the
    /// widened CHECK constraint in migration 0013 is unreachable from Rust
    /// unless a turn element can actually reach the queue and come back whole.
    #[test]
    fn a_turn_element_rides_the_ordinary_summarize_job() {
        let conversation = fs3_core::ConversationId::new("6ba7b810-9dad-11d1-80b4-00c04fd430c8")
            .expect("a canonical uuid");
        let turn = fs3_core::Turn {
            turn_no: 42,
            role: fs3_core::TurnRole::Agent,
            source: fs3_core::TurnSource::Peer,
            head_sha: None,
            at: "2026-08-27T09:00:00Z".to_string(),
            body: "the gate is green, opening the PR".to_string(),
            items: Vec::new(),
        };
        let element = turn.element(&conversation);

        let job = SummarizeJob {
            identity: "git:github.com/AI-Substrate/flowspace3".to_string(),
            raw_hash: turn.blob_sha(),
            element: element.clone(),
        };

        let wire = serde_json::to_value(&job).expect("a turn job serialises");
        let back: SummarizeJob = serde_json::from_value(wire).expect("and comes back");
        assert_eq!(back, job, "the payload survives the queue unchanged");
        assert_eq!(back.element.kind, fs3_core::ElementKind::Turn);
        assert_eq!(
            back.element.address,
            "conv:6ba7b810-9dad-11d1-80b4-00c04fd430c8#t42"
        );
        assert_eq!(
            back.raw_hash,
            back.element.raw_hash(),
            "the turn's content address IS the element's dirtiness key"
        );

        // And it dedupes on content like every other body: the same words in
        // another conversation are one piece of paid work, not two.
        let elsewhere = fs3_core::ConversationId::new("6ba7b810-9dad-11d1-80b4-00c04fd430c9")
            .expect("a canonical uuid");
        let twin = SummarizeJob {
            element: turn.element(&elsewhere),
            ..job.clone()
        };
        assert_eq!(job.dedupe_key(), twin.dedupe_key());

        // A turn earns a raw vector on its own terms — the file-element
        // exception is about parsed children, which a turn never has.
        assert!(earns_raw_vector(&element));
    }

    #[test]
    fn empty_structural_container_has_no_raw_vector_while_its_row_does() {
        let row = Element::new(
            fs3_core::ElementKind::Row,
            "ddoc_row",
            "ac-0001",
            "plan.dd.json#criteria/ac-0001",
            fs3_core::Span::new(1, 20),
            "criterion text",
        );
        let container = Element::new(
            fs3_core::ElementKind::Container,
            "ddoc_section",
            "criteria",
            "plan.dd.json#criteria",
            fs3_core::Span::new(1, 20),
            "",
        )
        .with_children(vec![row.clone()]);

        assert!(!earns_raw_vector(&container));
        assert!(earns_raw_vector(&row));
    }
}
