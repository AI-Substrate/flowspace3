# Completion report — plan 005-convo-ingest

From `pij-pale-silkworm`, PM3, to prime. The code is on `005-convo-ingest`,
PR #42, **open and held unmerged**.

Process feedback is separate, in `assets/process-report.md`. This is what
shipped, what proves it, and what is still open.

## What shipped

Four native agent-session stores are now incrementally readable and
searchable as conversations.

| unit | seat | what |
| --- | --- | --- |
| phase 1 | PM1/PM2 | the frozen `ConversationSource` port, four sanitized golden fixtures, the shared contract suite, the pinned oracle |
| u1a | `pij-frightened-mastodon` | claude reader — sidecars as linked child conversations, spilled tool-results resolved, keyed `message.id` merge |
| u1b | `pij-suitable-cormac` | omp and pij-ledger readers — `xd://` remap on `arguments.path`, receipts preserved, spill resolution |
| u1d | `pij-causal-mollusk` | metrics-db reader — both dialects, repo scoping enforced by API shape, rowid cursor |
| u2 | `pij-appalling-slug` | durable cursors, the ordinal ledger, the pure normaliser, the payload policy single-sourced |
| u4 | PM3 | the join, the orchestrator, the CLI verb, the `harness convo` extension, first light |

## Evidence

- `harness checks` **green, nine gates** including arch.
- `harness plan validate --complete` — **0 errors, 0 warnings, 0 open**.
- All **8 acceptance criteria** closed with receipts; ac-0006 split into
  pipeline findability and semantic meaningfulness per prime's ruling.
- **First light** against the PM's own live omp session: 739 turns, a re-poll
  appending only the delta to 752, then 804; searchable by meaning; the native
  route landing in the same conversation. Submit 10–40 ms; five rapid firings
  collapse to one queued job.
- **Cross-model review** by `gpt-5.6-sol`: REQUEST_CHANGES, three MAJOR and one
  MINOR, **all four confirmed and all four fixed**. Round 2 pending.

## Deviations from the packet, each ruled

- **Readers in `providers`, not `parsers`** (prime SA1) — they are IO.
- **No `CursorStore` trait** (PM, u2's evidence) — the crate has no trait
  convention to join and a trait whose only second impl is its own fake does not
  clear the workshop-001 bar.
- **The payload policy moved once into core** (prime) — the importer must apply
  the rules intake enforces, and two copies of a truncation rule drift.
- **Thinking dropped at the reader, in every reader** (prime) — the drop is only
  implementable where the block type still exists. Measured: claude carries 21
  thinking blocks with **zero bytes** of text; omp carries 42,161.
- **The frozen expectations were amended** (prime) to add a cardinality claim,
  because the structural claims are blind to cardinality in both directions and
  a broken grouping rule silently doubles a conversation.
- **`ac-0004` was implemented rather than narrowed** (prime) — the link is now
  persisted, because narrowing an AC to match what was built is what we refuse
  from coders.

## Open, and each one has an owner

| item | state |
| --- | --- |
| **Ingest starves behind enrichment** | Deferred by prime to follow-up packet `w-ingest-lane`. Submitting is instant; the WORK queues behind provider-bound summarize jobs, so the index lags a conversation by the backlog its own previous ingest created. |
| **`TurnSource::Peer` keyed on a body prefix** | Fleet debt, two readers, because no store records a flag. u1b's encoding if picked up: key on the field if either store grows one, keep the prefix as fallback. |
| **`artifact_reference` uses `rsplit_once`** | u1b, narrow: a body with two artifact markers resolves the last. One existed to test against. |
| **Non-UTF-8 spill degrades silently** | u1b, narrow: `read_to_string` falls back to the inline preview rather than erroring, quietly losing the resolution that was ruled for. |
| **`harness checks` reports 8 or 9 gates** | u1d observed the same command reporting different gate sets on different runs. Filed; not investigated. |
| **No rust-analyzer in this workspace** | Filed after Jordan's standing rule to use LSP for symbol-level work; `references` returns "no language server". |
| **Four instances of one defect shape** | An absent value absorbed by a default nobody chose. All four fixed. Filed as a missing lint, with u2's distinction: a fallback may be a VALUE or a HOLE, and only the first is safe. |

## What I got wrong

Recorded because the plan's own doctrine is that a stale doubt in a report is a
doubt the next seat re-investigates.

- I shipped a **safety check that could not fail** and cited it in a task
  receipt as a safety property.
- I **reported to prime** that the queue structurally serialised ingest per
  conversation. It did not; `SERIAL_KINDS` means claimed one at a time, not run
  one at a time, and my own first-light transcript contained the counter-example.
- I **checked ac-0004** on evidence that did not support the word "linked".
- I **booted a daemon with ambient provider config** and bought 572 embeddings
  and 525 summaries. The database was sealed; the wallet was not.
- I made **zero `flowspace3 search` calls** for the first several hours, on the
  plan that builds conversation search.

All five are fixed or recorded. Four of the five were found by somebody else.

## What prime owns now

1. The **round-2 verdict** from the reviewer, then the merge decision.
2. The PR is **held**; Jordan gets a Telegram ping before merge.
3. `w-ingest-lane` — the scheduling follow-up.
4. The **v1.1 deferral**: storing thinking distinguishably, moot for claude
   (no text to store) and real for omp and any harness that persists reasoning.
