# Enrichment: summaries, vectors, and the size cliffs

What turns a parsed element into something searchable: an LLM summary, a
vector of the element's own text, and a vector of that summary. Two job kinds
(`summarize`, `embed`), one handler module (`crates/daemon/src/enrich.rs`), two
provider ports (`fs3_core::Summarizer`, `fs3_core::Embedder`).

This page is about the part that is easy to get wrong: **size**.

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

## The size cliffs, and the guard

There are two independent limits and they are NOT the same thing:

| limit | what it bounds | what breaks it |
| --- | --- | --- |
| `batch::TOKEN_BUDGET` (200k) | the SUM of one request | fixed by splitting the batch |
| `Embedder::max_input_tokens` (8192) | the largest SINGLE input | **cannot** be fixed by splitting |

fs3 had the first and not the second until 2026-08-27. The consequence, measured
on a live index: **59 of ~4,000 elements were permanently unsearchable.** Each
exceeded the embedding model's per-input cap, Azure answered
`400 Invalid 'input[0]': maximum input length is 8192 tokens`, the runner
retried three times into the same answer, and the job failed for good. Worse
than the loss was the silence — the queue recorded the work as finished
business, and nothing in the index said those elements had no vector.

The batch planner made this reachable on purpose: `split_to_budget` lets an item
bigger than the whole budget ride ALONE rather than dropping it, on the
reasoning that "one oversized request the provider may well accept" beats a
silent hole. That reasoning was right about the hole and wrong about the
acceptance.

**The ruling (Jordan, 2026-08-27): truncate now, split later.** An oversized
input is shortened to fit rather than skipped — the head of a large element is
most of what a search wants from it, and a vector of the head beats no vector at
all. Splitting one element into several vectors is the better answer and is
deliberately not built; it slots in at the same loop in `enrich::embed_items`,
and it changes what a `source_hash` addresses, which is why it is its own
packet.

### How much gets kept

`fs3_core::tokens` is the ONE place fs3 counts tokens — the batch budget and the
per-input guard read the same function, deliberately.

- `estimate_tokens` = bytes ÷ 3. Pessimistic on purpose: code tokenizes worse
  than prose.
- `fit_to_cap` fills only **two thirds** of the declared cap, i.e. it assumes
  **2 bytes per token**. That is the same headroom `TOKEN_BUDGET` takes against
  Azure's 300k request ceiling, and for the same reason: the number is an
  estimate, and overshooting costs a whole call.
- No tokenizer dependency. `cl100k_base` would be exact for Azure and OpenAI and
  wrong for the local embedder (`fastembed`, WordPiece), so "exact" would mean
  two counting mechanisms — one of them applied to models it does not describe.

**The honest limit**: content denser than 2 bytes per token — minified bundles,
base64 blobs, punctuation soup — can still exceed the cap. It fails visibly in
the queue, exactly as it does today. Buying safety against it means truncating
at 1 byte per token (the only provable bound: a token is never fewer than one
byte), which would throw away three quarters of what every ordinary element
could contribute.

### The caps each provider declares

`max_input_tokens` is a **declaration**, like `concurrency_ceiling`: required,
no default, because only the provider knows which model is deployed behind it.

| provider | cap | why |
| --- | --- | --- |
| OpenAI / Azure embeddings | 8192 | the `text-embedding-3-*` family's real cap |
| OpenAI / Azure chat (summaries) | 32,000 | a quarter of a 128k window: the reply shares it, and not every deployment has 128k |
| openai-compat (self-hosted) | 6,000 | must fit the SMALLEST plausible box (8k context) — the endpoint reports a model id, never a window |
| local (`fastembed`) | `usize::MAX` | see below |

The local embedder declares no cap **because it never rejects**: `fastembed`
configures its tokenizer with `TruncationParams`, so an over-long text is
silently cut at load time. Declaring a smaller number would make the caller cut
BELOW what the model would have read, trading real content for a marker — and
the number would be a guess, since `fastembed`'s catalogue reports a model's
width and its files but never its context window. **Consequence, stated
plainly: a local index does not mark its truncations.** Fixing that needs the
window, which needs `fastembed` to expose it or this adapter to read
`tokenizer_config.json` itself.

## Honesty: how a partial vector says so

- **Vectors**: `embeddings_1024.truncated` (migration 0010). A column rather
  than JSONB because it must be aggregable — "how many of my vectors are
  partial" is `count(*) FILTER (WHERE truncated)`.
- **Summaries**: `smart_content.extras.truncated_input`, the staging area
  migration 0006 created for facts that have not earned a column.
- Both are logged at WARN the moment they happen, naming the hash or address
  and the before/after size.

The row stays keyed by the **original** hash. A truncated embedding IS the
embedding for that content: re-keying on the prefix would make the dedupe
pre-check answer "not embedded" for ever and re-buy the same vector on every
scan. The consequence is that raising the cap does not automatically re-embed
what was truncated under the old one — which is what the column is for; the rows
to redo are exactly `WHERE truncated`.

```sql
-- what is partial, by model
SELECT model_key, count(*) FILTER (WHERE truncated) AS partial, count(*) AS total
  FROM embeddings_1024 GROUP BY model_key;
```

## The path back for work that already died

A failed enrichment job had no way back. `summarize` and `embed` jobs are minted
by a scan, and `add_root` enqueues nothing for a file whose blob is unchanged —
so a failed one is the end of the line for that element. `fail_job`'s own
documentation pointed at the decision-D6 reconciler sweep, which does not exist.

Two pieces now close that:

1. **`jobs.terminal`** (migration 0011) records WHICH kind of ending a failure
   was. `true` = no run will ever succeed (unreadable payload, unknown kind,
   wrong vector width). `false` = the attempts simply ran out. The runner
   already knows this — a `Failure` carries `retryable`.
2. **`requeue_failed`**, called at daemon boot before the runner starts, returns
   non-terminal failed `summarize`/`embed` rows to `pending`. Rows whose dedupe
   key is held by a live job are skipped (the live-dedupe index is unique over
   `pending`/`running`, so waking a duplicate would abort the statement).

So the fix arriving as a new binary is enough: no repair command to discover, no
SQL to write. It is cheap by construction — a requeued job whose vectors already
exist settles on its own pre-check without a provider call.

`scan_file` is deliberately NOT swept: a failed scan has an ordinary way back,
because the file is on disk and touching it enqueues a new job.

## How to verify it works

```bash
# the guard, the marker, the batch interaction, and the recovery
FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_test \
  cargo test -p fs3-daemon --test oversize

# the counting convention itself (no database needed)
cargo test -p fs3-core --lib tokens
```

The tests assert on what ARRIVED at the provider (`FakeEmbedder::received`),
not on what the caller believed it sent — `FakeEmbedder::capped` and
`FakeSummarizer::capped` refuse an oversized input the way a hosted API does, so
a missing guard is a failing test rather than a hopeful comment.

## Code pointers

- `crates/core/src/tokens.rs` — the counting convention and `fit_to_cap`
- `crates/core/src/ports.rs` — `max_input_tokens` on both ports
- `crates/daemon/src/enrich.rs` — both guards, at the two points of spend
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
