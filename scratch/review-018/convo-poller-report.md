# 018-convo-poller — coder report

**REVIEW CLOSED — APPROVE at code head 3d35555 (F1–F4 + D1). Final review record and task receipts are committed docs-only; production acceptance remains o-prime-owned.**

- PR: https://github.com/AI-Substrate/flowspace3/pull/118
- Docs-only commit: `5331410a53582a4e42adbb869957628e60d07ff5`. Approved code remains `3d35555dd72386e3e4bc26fa9da404c279f95780`. One push, then stop; no code changed.
- Worktree: `/Users/jordanknight/substrate/flowspace/fs3-018-convo-poller`, branch `018-convo-poller`.
- Delta used `harness commit`: `direct-verified`, probe `connected`, verify `landed`; `refs/notes/ai` attribution confirmed. The optional conversation hook could not resolve session identity and ingested nothing.

## Delivered

1. Native Claude/OMP daemon polling keyed on files and durable per-file cursors; existing readers, normalization, ledger, GUID derivation and job execution reused.
2. Default 12 five-second ticks, 14-day cursorless lookback, cap 10; effective stagger `min(6s, cadence/cap)`. Live changes then post-boot new files outrank cold backlog, newest mtime first, with at least one reserved backlog slot. Truncation and detectable replacement trigger the existing reset/dedupe path.
3. Shared flowing/stalled/disabled state in status and doctor. Missing native files get `QUERY_CONVERSATION_NO_SESSION_FILE`/HTTP 404; existing-but-unindexed verify retains NOT-FOUND, while ingest remains accepted.
4. Native git-ai metrics path before legacy fallback; real read-only source resolution and repository scoping are checked before accepting a job.
5. Actual ingest-report counters in newest-ingest diagnostics, plus one INFO receipt per session file carrying address/subject, records_read, turns_new, deduped, summarized and rescanned. Ingest done subjects are populated. Quiet poll counts stay DEBUG.
6. Shared test harness HOME sealing and an actual-daemon negative proof: a valid-looking session outside sealed HOME creates zero ingest jobs.

## Verification

- Final D1 `harness checks`, **2026-09-06T11:28:39.199Z**: `harness checks exited 0`; envelope `"status": "ok"`, `"passed": 11`, `"doc_links_checked": 12`. Every gate: `"ok": true`, `"code": 0`. Full stdout/stderr: `.harness/temp/agent/convo-gate-full.log`. Sealed HOME/config; :5434 only.
- O-prime reported final reviewer APPROVE for F1–F4 and D1 at `3d35555`. No docs-only CI verdict is claimed; o-prime owns merge and production acceptance.
- Delta focused proof: poller **19 passed**, store indexed-ID/chunk/EXPLAIN **1 passed**, isolated verification **1 passed**, remaining conversation-query **15 passed**, spend **1 passed**, envelope goldens **2 passed**. Separate attempt-revision filter also passed. Public `IngestAccepted` and conversation goldens unchanged.
- Complete workspace gate passed via the test-suite orchestrator. It emits no aggregate Rust-test count; none is invented.
- D1 poller suite: **19 passed**. Pending 100-file backlog stays 100 while in-flight grows 10,20,…,100; healthy backlog falls 100,90,…,10,0 while in-flight stays capped. Both remain Flowing; fairness, priority and delay checks preserved.
- First D1 full gate hit `streaming.rs:207`. Three sequential sealed isolated runs passed (0.60s/0.64s/0.55s); o-prime filed the timing flake as backlog **200** and authorized one full rerun, which passed. `d1-streaming-{1,2,3}.log` and `d1-gate-streaming-failure.log` retained. No streaming/runner edits.
- Actual CLI smoke proved automatic Claude/OMP ingest and append, quiet subsequent polls, disabled status/doctor, and missing-file HTTP 404. Corrected fast-cadence append was observed in **1.246s**.

### Job-level spend proof

| Stage | Ingest jobs | Summarize jobs | Embed jobs |
|---|---:|---:|---:|
| Two turns fully ingested | 1 | 2 | 3 |
| Unchanged subsequent poll | 1 | 2 | 3 |
| Same-content identity rescan | 2 | 2 | 3 |
| One appended turn | 3 | 3 | 5 |

The rescan reports read=2, new=0, deduped=2, summarized=0. The append creates one summary plus raw/smart embedding jobs for that turn only. Restart adds no jobs and exposes no invented latest receipt.

Delta mutations all failed as intended: pending/running counted as terminal → Stalled vs Flowing (artifact://165); ingest ACK removed → pass-2 enqueues seven vs zero (artifact://167); negative-cache hit bypassed → six extra reads vs zero (artifact://169); ledger bypass → four summaries vs two (artifact://173). All restored; poller, spend and full gate green. Earlier baseline mutations and current named proofs remain in `docs/plans/018-convo-poller/assets/tasks/phase-1/execution.log.md`.

## Review-delta behavior

- Behind means eligible cursor/stamp/ACK backlog, not submitted work (D1 correction). Pending/running does not age the no-progress counter; reasons expose unique in-flight counts. Three observed terminal attempts without cursor/ACK progress stall; failed counts once. Missing rows provide no information. Equal timestamps do not collapse distinct attempts. Restart loses process-local ID/lag history and requires three fresh attempts.
- Retained IDs are looked up BEFORE enqueue can overwrite outcomes. SELECT-only lookup uses `jobs_pkey`, chunks of 256 IDs, and no migration. Queue ID returns through the crate-local submission seam; existing public response unchanged. Existing enqueue/retry/priority behavior is untouched.
- Stable zero-record actual reads acknowledge the file revision. Existing real headers checkpoint the exact reader cursor with the normal empty-ledger commit. Headerless reads use process-local ACKs: one job per unchanged file per poller lifetime, no invented header/timestamp or forced EOF. Seven mixed-harness fixtures prove flat jobs and behind=0/Flowing from pass 2, including a new-poller boundary; torn lines preserve the newline boundary.
- Stamped cwd misses warn once, then DEBUG without reopening; change invalidates them. Actual service consumes New priority; cap deferral preserves it. HOME-mutating verify has one synchronous test in its own binary, before runtime creation.
- Final cross-model review, including delta and D1 sections, archived byte-identically at `docs/plans/018-convo-poller/assets/reviews/cross-model-review.md`; source/archive MD5 `0fb0061b8e4121dc75181c8ec8c5cbdc`. Task receipts updated through ddocs; generated sibling check passed with drift=false and no findings.

## ASSUMPTIONS and preserved limits

- Durable `(harness, session-file id)` cursors remain authoritative. Metadata observations prioritize scheduling only; they never advance cursors. Sidecars keep their own cursors but are enqueued through their parent's existing file-set resolution.
- Folder discovery uses recorded cwd within the first 64 records / 8 MiB. Positive cache is path-based; negative cache is stamp-based and retries on change. No ambiguous slug inversion.
- The runner alone executes, retries and parks jobs. `runner.rs` changes only its existing subject-derivation function; claim order, SKIP LOCKED, concurrency and retry policy are untouched.
- Latest ingest receipts are process-local and explicitly unavailable until observed after restart. Their counters come from `IngestReport`, not cursor timestamps. `summarized` is queued summary work, not completed calls or currency. A cap of ten sessions is not a cap on the enrichment work a single large transcript can produce.
- Unix identity uses the unchanged device/inode helper. Non-Unix remains `(0,0)`, so same-size replacement detection is not claimed there; identity-rescan tests are Unix-gated.
- Rust LSP was rooted at the main clone. Its single authorized workspace-repair attempt failed; the approved fallback was semantic flowspace search/get plus exact-identifier and exact-byte worktree inspection. The final implementation is findable in flowspace without a manual add.

## Scope amendments and cleanup

All amendments were ruled by o-prime. F1–F4 scope remains as reviewed. D1 changes only `convo_poll.rs` gauge accounting and its assertions, plus execution evidence. No lag/stall, scheduling, store, public envelope, golden, reader/parser/normalizer/ledger/GUID or runner execution-policy change.

D1 failed cwd-probe and streaming fixtures were identified by label/run time, audited, and removed through `flowspace3-db-test`; the streaming residue contained only four seeded raw embed jobs, all done, and no other connections. After the final D1 gate, shared :5434 native-ingest count is **0**. No unrelated resources touched.

Proof logs and observations remain in `.harness/temp/agent/`. Observations were listed and reported, never cleared; o-prime owns the drain. Highest-leverage lesson is encoded: HOME is now part of the shared test seal, backed by a real enabled-daemon proof. Runtime/gate-output loss is captured separately; the supervised log wrapper retains results even when rendering truncates them.

## Remaining owner action

ac-0001..0007, spend/logging requirements, F1–F4 and D1 are implemented and review-approved. t8 records PR/review completion; only **o-prime merge, production bounce, and `assets/inputs/prod-after.md`/ac-0008** remain. No production acceptance invented and no merge performed. Retain logs and observations until o-prime drains them; final review record is committed before teardown.
