# Enrichment: summaries, vectors, and the size cliffs

What turns a parsed element into something searchable: an LLM summary, a
vector of the element's own text, and a vector of that summary. Two job kinds
(`summarize`, `embed`), one handler module (`crates/daemon/src/enrich.rs`), two
provider ports (`fs3_core::Summarizer`, `fs3_core::Embedder`).

This page is about the two parts that are easy to get wrong: **whether to buy
at all**, and **size**.

## What it is

- `scan_file` parses a file and enqueues enrichment for the tree
  (`enrich::enqueue_for_tree`). Every element earns a raw vector except a file
  element whose children already cover it; elements past the configured line
  floor also earn a summary.
- `summarize` calls the repo's summarizer, stores the answer in
  `smart_content`, and enqueues the summary's OWN embed job.
- `embed` carries a BATCH of texts. Jobs claimed together are merged by
  `crates/daemon/src/batch.rs` into as few provider calls as a token budget
  allows.
- Everything is keyed by content (`raw_hash`, or the summary's `text_hash`),
  never by element id. The same function body on forty branches is one summary
  and one pair of vectors.

## The spend guards: is this still worth buying?

Before either handler asks what FITS, it asks whether the content is still
worth paying for at all. Two different questions get confused here, and only
one of them is about money:

| guard | question | where |
| --- | --- | --- |
| dedupe | "has this text already been bought?" | `existing_embedding_hashes`, top of `embed_items` |
| reference | "does anything still hold this content?" | `raw_hash_is_referenced` (summarize) · `referenced_source_hashes` (embed) |

The dedupe filter LOOKS like a spend guard and is not. It asks whether a hash
is already in `embeddings_1024`, never whether the content behind it is still
mapped by a live root — so a NEW hash for dead content sails straight through
it to the provider.

`summarize` has had the reference guard since roots became removable (req-0057);
`embed` did not, and the asymmetry was invisible for exactly that reason.
Measured when the watcher pulled a gitignored tree into the index (DL-035/036):
**4,436 raw vectors bought for content the next full walk unreferenced**, with
the ~26,000 summaries beside them saved purely because the other handler
already had the guard.

Both legs ask the same predicate GC uses at level two —
`held_by_a_live_root!` in `crates/store/src/roots.rs`, one macro, so the point
of spend and the collector cannot disagree about what "referenced" means.

### Two hash spaces, two questions

`referenced_source_hashes` takes a `SourceKind` because raw and smart hashes
live in different spaces. A `raw` hash IS an element's `raw_hash`. A `smart`
hash is a summary's `text_hash`, which reaches an element only through the
`smart_content` row — so asking the raw question of a smart batch would answer
"unreferenced" for every summary vector still waiting to be bought, and the
guard would quietly delete the index instead of protecting the bill.

### Why it is a batch query

`raw_hash_is_referenced` answers one text at a time, which is the right shape
for `summarize` — one job, one text, one guard. An `embed` job carries a batch,
so the per-item shape would be sixteen round trips to decide one provider call.
`referenced_source_hashes` asks once with `ANY($1)` and hands back the
survivors: one round trip per job, whatever the batch size.

The order inside `embed_items` is deliberate — dedupe first (cheapest, purely
local to the model key), then the reference query, then `embedder_for`. The
provider is reached only by items that survive both.

### Seeing it work

Money not spent leaves no rows, so the only evidence it exists is the log:

```
INFO skipping embeds for content no registered root holds dropped=3 kept=0 kind=raw
```

`dropped` is counted after the filter rather than as `offered - live.len()`,
because a merged batch may carry one hash twice and a count that assumed
otherwise would report spending that never happened. Pinned by
`streaming.rs::the_embed_spend_guard_says_what_it_refused_to_buy`, which reads
the line a human would have read.

**Not yet done, named rather than hidden**: there is no JSON log format in this
repo — `logging.rs` builds `tracing_subscriber::fmt::layer()` for both stdout
and the file — so the count is structured fields on a text line, not a `--json`
record. Making it one means introducing a JSON subscriber for the whole daemon,
which is a logging decision, not an enrichment one.

## The size cliffs, and the guard

There are two independent limits:

| limit | what it bounds | response |
| --- | --- | --- |
| `batch::TOKEN_BUDGET` (200k) | the sum of one request | split the provider call |
| `Embedder::max_input_tokens` (8192 hosted) | one input | chunk first; heal a typed cap rejection |

The original defect was measured on a live index: 59 of roughly 4,000 elements
exceeded the hosted model's per-input cap. Azure returned
`400 Invalid 'input[0]': maximum input length is 8192 tokens`; retrying identical
bytes exhausted the job and left the content unsearchable.

`fs3_core::tokens` owns the byte estimate and its safety margin. The estimate is
bytes ÷ 3, while `input_budget_bytes` fills two thirds of a declared cap: an
effective two bytes per token. `chunk_plan` uses that same helper, so its 7,500
token window is 15,000 bytes, with 600 bytes of overlap. No tokenizer dependency
is needed, and every byte remains represented by overlapping chunks.

Content can still tokenize denser than the estimate. OpenAI, Azure, and
OpenAI-compatible embedding adapters classify a 400 whose body reports
`maximum input length is N` as `Error::InputTooLong`, carrying the reported cap.
When the body names `input[N]`, the daemon re-splits that member. When the index
is absent or invalid, it bisects the rejected call until the member is isolated.
One bounded heal round tightens the member from the two-byte estimate to one
byte per token, the provable floor. Exhaustion is terminal and names the source
hash, original byte length, and the actual window-bytes/token-cap ratio.

Provider calls write nothing incrementally. After every sub-call succeeds,
chunks receive contiguous per-source numbers and the complete vector set is
stored in one transaction; a failed or re-planned call cannot leave duplicate or
partial rows.

### The caps each provider declares

`max_input_tokens` is required because only the provider knows the deployed
model:

| provider | cap | why |
| --- | --- | --- |
| OpenAI / Azure embeddings | 8192 | `text-embedding-3-*` input cap |
| openai-compat embeddings | 8192 | current adapter declaration; provider rejection still carries its reported cap |
| OpenAI / Azure chat | 32,000 | bounded share of the context window |
| openai-compat chat | 6,000 | smallest supported self-hosted context |
| local (`fastembed`) | `usize::MAX` | tokenizer truncates internally instead of rejecting |

Local truncation remains unmarked because `fastembed` does not expose the model
window through this adapter.

## How vector coverage is represented

An oversized source keeps its original `source_hash`; `chunk_no` distinguishes
its overlapping vectors. Hosted-provider chunks are stored with
`truncated=false` because splitting preserves the complete source. The legacy
`embeddings_1024.truncated` column remains queryable for rows produced by the
older prefix guard. Summary truncation remains
`smart_content.extras.truncated_input`.

## The path back for work that already died

`jobs.terminal` distinguishes a defect that cannot succeed from work that merely
ran out of attempts. At daemon boot, `recover_enrichment_jobs` calls
`requeue_failed` for non-terminal `summarize` and `embed` rows before the runner
starts. The update resets attempts and parks, schedules the rows immediately,
and skips a dedupe key already held by a live job.

This is the recovery mechanism for cap failures created by older binaries: the
bounce that installs the fix also returns them to `pending`; no repair SQL is
required. A heal that reaches the one-byte floor returns a non-retryable failure,
so its job is terminal and the same boot sweep does not revive it.

`scan_file` is deliberately not swept: touching or rescanning its file is the
ordinary recovery path.

## How to verify it works

```bash
# chunking, dense-token healing, terminal exhaustion, and boot recovery
FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_test \
  cargo test -p fs3-daemon --test oversize

# exact hosted-adapter classification and unrelated-400 controls
cargo test -p fs3-providers cap_rejection

# token budgeting and the ruled alignment measurement
cargo test -p fs3-core --lib tokens
cargo test -p fs3-daemon chunk_plan -- --nocapture

# the reference guard: unheld content never reaches the provider
FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_test \
  cargo test -p fs3-daemon --test embed_dedupe

# and the log line that is the only evidence it fired
FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_test \
  cargo test -p fs3-daemon --test streaming
```

The oversize tests assert on provider inputs and stored chunks. Their dense fake
counts at one byte per token, so deleting the heal arm reproduces the permanent
cap failure instead of merely checking the caller's plan.

## Code pointers

- `crates/core/src/tokens.rs` — the estimate and shared FILL-aligned byte budget
- `crates/core/src/error.rs` — the typed `InputTooLong` signal
- `crates/core/src/ports.rs` — `max_input_tokens` on both ports
- `crates/providers/src/{openai,azure_openai}.rs` — exact cap-400 classification
- `crates/daemon/src/enrich.rs` — chunk planning, call budgeting, bounded healing,
  and atomic vector storage
- `crates/store/src/roots.rs` — `held_by_a_live_root!`,
  `raw_hash_is_referenced`, `referenced_source_hashes`
- `crates/daemon/src/batch.rs` — the request budget and the merge rules
- `crates/daemon/src/boot.rs` — the requeue sweep
- `crates/store/src/jobs.rs` — `fail_job`, `requeue_failed`
- `crates/store/migrations/0010_embedding_truncation.sql`,
  `0011_job_terminal.sql`

## Gotchas discovered

- **A budget for the sum is not a guard for the member.** They fail differently
  and only one of them can be fixed by retrying, which is why the retry ladder
  turned a size problem into permanent data loss.
- **`fail_job` was one verb for two endings.** Until they were told apart,
  nothing could revive the recoverable ones without also reviving the defects —
  which would be an unbounded trickle of claims that can only fail again.
- **The fake has to enforce what it declares.** An earlier `FakeEmbedder`
  accepted anything, so no test in the repo could have caught this class at all.
- **A dedupe filter is not a spend guard.** "Already bought" and "still worth
  buying" are different questions, and the first one passes every new hash
  through — which is how a watcher defect two crates away turned into 4,436
  paid vectors.
- **A test that enqueues bare synthetic hashes is now testing the guard.** With
  the reference guard in place, an embed test whose content nothing holds never
  reaches the provider, so every assertion about the call is really asserting on
  an empty list. `support::hold` registers a root that holds the items;
  `lanes.rs` and `oversize.rs` both had to take it up, and `lanes` is the
  cautionary one — an unheld lane records a peak of ZERO concurrent calls and
  reads exactly like a lane that does not work.
