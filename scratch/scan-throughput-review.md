# Scan-throughput architecture review: flowspace3 vs flowspace2

## Executive verdict

**Headline: fs3 has a wide embedding API and a capable merge planner, but its live scheduling defeats batching for summary embeddings.** In the isolated 959-file run, raw embeddings averaged **17.90 texts/call** (median 14, max 141), while smart/summary embeddings averaged **1.06 texts/call** (median and p95 both 1): **11,232 of 11,727 smart calls carried exactly one text**. flowspace2 collects all raw and smart chunks after its summary stage and cuts fixed **16-text** calls by default. At the observed fs3 workload, merely reaching fs2's 16-text shape would reduce 12,416 smart inputs from 11,727 calls to about 776 calls: **93.4% fewer HTTP calls / 15.1x call compression**.

The cause is architectural, not provider capability. Each fs3 summary stores its result and enqueues a one-item smart embed (`crates/daemon/src/enrich.rs:486-500`); the general drain invokes `drain_embed` before each claim/fill cycle (`crates/daemon/src/runner.rs:229-238`), so it usually claims the one smart job that just appeared instead of waiting for a batch. The planner can merge up to 64 job rows and cut at 200,000 estimated tokens (`crates/daemon/src/runner.rs:62-67`; `crates/daemon/src/batch.rs:27-40,83-139`), and the provider sends the whole `texts` slice as one HTTP `input` array (`crates/providers/src/azure_openai.rs:378-412`; `crates/providers/src/openai.rs:144-179`). The batch mechanism exists; its trigger cadence is wrong.

A second, independent bottleneck is queue observability implemented in the hot settlement path. Every completed general job performs `jobs_remaining`, then a full grouped `queue_depth` aggregate (`crates/daemon/src/runner.rs:781-804`); embed settlement does the same per original job row (`crates/daemon/src/runner.rs:595-617`). On the production-shaped table (**544,721 rows, 1.516 GB total / 473 MB heap**), the grouped aggregate took **80.84 ms** and read 60,574 buffers. On the isolated completed run (**27,469 rows, 50 MB**), it took **17.59 ms**. This work is repeated after every job despite progress logging already having a five-second cadence (`crates/daemon/src/runner.rs:693-730`).

## Measurement protocol and limits

### Isolation

No production daemon endpoint or production row was mutated.

1. **Full fs3 pipeline run**: dedicated daemon `127.0.0.1:60945`, dedicated database `fs3_scan_throughput_review`, fake summarizer/embedder, linked-worktree discovery disabled (`worktree_reconcile_ticks = 0`), and four general workers. Corpus: `/Users/jordanknight/substrate/fs2/flow_squared`, 959 accepted files. Database was empty before the run.
2. **Stage comparator**: dedicated daemon `127.0.0.1:60946`, dedicated database `fs3_scan_parse_review`, fake embedder, summaries disabled by an unreachable line threshold, linked-worktree discovery disabled, and four general workers. Same 959 accepted files. Raw embeddings cannot be disabled in fs3, so this is **scan + mandatory raw embedding**, not a pure parser-only run.
3. **flowspace2 comparator**: `uv run fs2 scan --scan-path ... --no-smart-content --no-embeddings --no-cross-refs --no-progress`; 1,098 discovered files, 11,454 nodes, **50.65 s** wall. The CLI has no graph-output override and wrote the configured gitignored/regenerable `.fs2/graph.pickle`; o-prime verified no tracked state changed. Do not interpret the differing 959/1,098 file counts as identical discovery policy.
4. **Production queue evidence**: SQL `SELECT` and `EXPLAIN (ANALYZE)` only against the production database. No UPDATE/DDL/add/scan was issued. `flowspace3 config show` established the effective production width as **32 general workers**, with `summarize_lane = 32` and `embed_lane = 10`.
5. The full fs3 run uses fake providers, so provider network latency is deliberately absent. It isolates orchestration, queue, parsing, and database cost. A real hosted provider would make one-text smart calls materially worse.

An initial `flowspace3 daemon --sandbox` trial on port 60944 / DB `fs3_sandbox_00000000000029aa18d01ca6a7c4a470` was rejected as contaminated: after the requested 959-file root was added, automatic linked-worktree discovery registered nine sibling fs2 checkouts and expanded the backlog to 7,873 scans and 11,256 pending summaries. That trial is excluded from throughput numbers.

### Measured runs

| Run | Discovery/enqueue | Drain wall | Final jobs | Content rows | Key distribution |
|---|---:|---:|---:|---:|---|
| fs3 full fake pipeline, 959 files | 16.21 s | **721.87 s** from first progress to idle | 959 scan + 12,416 summarize + 14,094 embed | 18,835 elements; 12,161 summaries; 29,833 vectors | scan median 108 ms, p95 473 ms; summarize median 16 ms, p95 72 ms |
| fs3 summaries-disabled comparator, 959 files | 25.05 s | **150.47 s** from first progress to idle | 959 scan + 1,678 embed | 18,874 elements; 17,672 vectors | scan median 44 ms, p95 202 ms; 929 embed calls, median 14 texts |
| fs2 parser/storage comparator, 1,098 files | included | **50.65 s total** | no DB queue | 11,454 in-memory nodes | sequential parse; repeated courtesy saves + final pickle save included |

For the full fs3 run, logged handler sums were 155.66 worker-seconds for scan and 322.10 worker-seconds for summarize, yet wall was 721.87 s with four general workers. The difference establishes substantial time outside the logged handler timers. Per-settlement `emit_queue` is a strong measured contributor because it performs the 17.59 ms post-run full-history aggregate after every settlement, but cache state and concurrent database work prevent assigning the entire gap to that query from this run alone.

## 1. Per-call embedding batch shape

### fs3

- Scan emits raw embed **jobs** in groups of 16 texts (`crates/daemon/src/enrich.rs:41-46,165-204`).
- The embed drain claims up to **64 jobs** at once (`crates/daemon/src/runner.rs:62-67,460-478`).
- The planner merges same `(identity, source)` jobs and cuts calls at **200,000 estimated tokens**, not at 16 items (`crates/daemon/src/batch.rs:11-20,27-40,83-180`). A first-attempt call may therefore contain hundreds of texts; retrying jobs travel alone.
- Hosted adapters serialize the complete slice as one request-array (`crates/providers/src/azure_openai.rs:378-412`; `crates/providers/src/openai.rs:144-179`).
- Full-run observed raw calls: **1,015 calls / 18,164 texts**, mean 17.90, median 14, p95 52, max 141; 127 one-item calls.
- Full-run observed smart calls: **11,727 calls / 12,416 texts**, mean 1.06, median 1, p95 1, max 4; **11,232 one-item calls (95.8%)**.

### flowspace2

- Default `batch_size = 16`, configurable 1–2,048; `max_concurrent_batches = 1` (`src/fs2/config/objects.py:699-769,807-835`).
- The service gathers chunks across all nodes, including both raw and smart content, then slices the complete list by `batch_size` (`src/fs2/core/services/embedding/embedding_service.py:651-699`).
- Each slice becomes exactly one `embed_batch(texts)` call (`src/fs2/core/services/embedding/embedding_service.py:703-735`).
- Azure and OpenAI adapters pass the complete list as `input` in one SDK request (`src/fs2/core/adapters/embedding_adapter_azure.py:156-201`; `src/fs2/core/adapters/embedding_adapter_openai.py:134-186`).

**Finding:** fs3 is wider than fs2 for accumulated raw work, but substantially narrower for the summary-derived half of the corpus because it drains each newly-created smart job immediately. “64 jobs per claim” does not describe the observed provider call shape.

## 2. Scan parallelism and where 1,000-file wall time goes

### fs3

- Effective production general worker width is **32** (`flowspace3 config show`, captured in the evidence appendix); the code default is **4** (`crates/core/src/config.rs:919-926,1058-1068`). Both controlled runs used 4.
- `scan_file` and `summarize` share the same general task pool (`crates/daemon/src/runner.rs:53-60,208-265`). In production, `summarize_lane = 32` can occupy all 32 general slots; in the controlled runs it was capped by the four-worker pool. The semaphore never reserves capacity for scans.
- Parsing itself is decoupled from provider calls: scan persists a tree, enqueues enrichment, and returns (`crates/daemon/src/scan.rs:173-223`).
- However, scan progress competes with newly-enqueued LIFO summaries and with embed draining inside the same loop. In the full run, 959 scans took 721.87 s end-to-end even though their recorded handler sum was 155.66 worker-seconds.
- The summaries-disabled comparator still took **150.47 s** for 959 scans plus mandatory raw embeddings, versus fs2's **50.65 s** for 1,098 parser/storage files. fs3's scan handler median fell from 108 ms to 44 ms when summaries were removed, showing shared-lane/DB pressure affects even the measured scan handler.
- fs3 persists each file during scan; its tree writer issues one `INSERT ... RETURNING id` per element inside a transaction because parent IDs are generated row by row (`crates/store/src/elements.rs:32-45,49-104`).

### flowspace2

- Pipeline stages are globally sequential: Discovery → Parsing → CrossFileRels → SmartContent → Embedding → Storage (`src/fs2/core/services/scan_pipeline.py:224-237,297-306`).
- Parsing loops files synchronously and calls `ast_parser.parse` once per file; there is no file/process pool (`src/fs2/core/services/stages/parsing_stage.py:37-96`).
- The measured no-enrichment run completed 1,098 files / 11,454 nodes in 50.65 s despite sequential parsing. It proves the current fs2 bulk path is faster end-to-end than the closest available fs3 comparator; because fs3 still performs mandatory raw embeddings and the systems use different discovery/parser/storage policies, it does **not** isolate database protocol as the sole cause.
- Storage is in-memory NetworkX node/edge addition followed by one atomic pickle write (`src/fs2/core/services/stages/storage_stage.py:60-111`; `src/fs2/core/repos/graph_store_impl.py:139-176,300-339`). The pipeline also courtesy-saves after three slow-stage slots, even when disabled (`src/fs2/core/services/scan_pipeline.py:297-306`), so the 50.65 s result includes redundant full-graph saves.

**Finding:** fs3's workers are not dedicated scan workers; they form a shared scan+summary pool, and each scan performs many synchronous SQL round trips. The matched-enough end-to-end result plus the measured queue queries make queue/DB overhead a high-confidence remediation target, but a future parser-only instrumented fs3 mode is required to apportion the 3x comparator gap exactly.

## 3. Summarize coupling

### fs3

The provider call is correctly out-of-line from scan (`crates/daemon/src/scan.rs:218-221`), but scheduling is only partially decoupled. `GENERAL_KINDS = [SCAN_FILE, SUMMARIZE]` (`crates/daemon/src/runner.rs:53-60`), and a single `JoinSet` is bounded by `worker_concurrency` (`crates/daemon/src/runner.rs:208-265`). Since new summaries have higher IDs and the queue is LIFO within priority, they can jump older scans. The separate `summarize_lane` semaphore does not help unless `worker_concurrency` is at least as wide.

### flowspace2

Summarization is explicitly inline as a whole pipeline barrier. The parsing stage must finish, then SmartContentStage runs, then embedding, then storage (`src/fs2/core/services/scan_pipeline.py:125-137,297-306`). Inside that barrier, smart content uses an asyncio queue and up to **50 workers by default** (`src/fs2/core/services/smart_content/smart_content_service.py:295-410`; `src/fs2/config/objects.py:398-425`). Only after all workers complete does the embedding stage collect all chunks, which is why it naturally produces full batches. The tradeoff: no parse/embed streaming, but much better batch fill.

**Finding:** fs3 has the better intended pipeline shape (streamed, durable, resumable), but its shared admission pool and immediate embed drain combine the disadvantages: scan contention plus unfilled calls. Keep decoupling; fix the scheduler rather than reverting to fs2's global barrier.

## 4. Database write batching and queue round trips

### fs3

- Root discovery hashes accepted files, fetches known/current blobs in batched reads, syncs the path map once, then enqueues **one SQL upsert per changed file** (`crates/daemon/src/roots.rs:187-250`).
- General work claims **one row per SQL statement** (`crates/store/src/jobs.rs:141-185`). Embed is the exception: up to 64 rows in one claim (`crates/store/src/jobs.rs:187-246`).
- Elements are inserted **row-at-a-time** in one transaction (`crates/store/src/elements.rs:49-104`). Parse comparator: 18,874 element rows for 959 files.
- Embeddings are also inserted **row-at-a-time** in one transaction; each vector is cloned into a new `Vec` for `pgvector` binding (`crates/store/src/embeddings.rs:176-222`). Parse comparator: 17,672 vector rows.
- Every job completion is a settlement UPDATE, a live-count query, then a grouped full-history queue query (`crates/daemon/src/runner.rs:781-804`). Thus a scan job with N elements is not “one transaction”: it is N element inserts + enrichment enqueue upserts + settlement + observability scans.

### flowspace2

Nodes and edges are added to an in-memory graph (`src/fs2/core/services/stages/storage_stage.py:60-103`) and persisted as one pickle payload plus atomic rename (`src/fs2/core/repos/graph_store_impl.py:315-339`). There is no durable per-file job claim/settle protocol and no per-row database latency. This is less crash-resilient and less centrally queryable, but much cheaper for bulk bootstrap.

**Finding:** fs3 pays durability at the finest grains. Transactions preserve correctness but do not eliminate round trips; batching statements inside a transaction is still required for throughput.

## 5. Queue overhead at 40k+ rows

Schema and access path:

- History and live work share one append-only `jobs` table (`crates/store/migrations/0005_job_backlog.sql:17-46`).
- Live dedupe is a partial unique index on `dedupe_key`; the claim index is partial pending `(priority DESC, id DESC) INCLUDE (not_before)` and **does not lead with `kind`** (`crates/store/migrations/0016_job_lifo.sql:8-12`).
- Claim filters `kind` after walking that index (`crates/store/src/jobs.rs:156-172,211-232`).

Measured production table: **544,721 total rows; 29,836 pending; 1.516 GB total; 473 MB heap**.

- Ready general `LIMIT 1`: **0.041 ms**, 24 buffers. The newest pending row matched a general kind.
- Empty embed `LIMIT 64`: **14.759 ms**, 29,902 buffers, **29,838 rows removed by filter**. This query occurs at the start of each general drain cycle (`crates/daemon/src/runner.rs:229-238,460-468`).
- Grouped queue depth: **80.840 ms**, parallel sequential scan, 60,574 buffers.
- Isolated 27,469-row grouped queue depth after drain: **17.592 ms**, 3,519 buffers.
- Live-count query is comparatively indexed: **0.241 ms** on the isolated idle table.

**Finding:** the jobs table is not inherently too slow to claim a matching top row. Two query shapes are bottlenecks with different scaling: a missing `kind` prefix makes an empty lane probe walk the **live pending backlog of other kinds**, while full-history `queue_depth` aggregation grows with **lifetime settled history**.

## 6. Live churn finding (cited, not re-investigated)

Source: `scratch/w-scan-churn-root-cause.md` (re-homed to the main clone after the fs3-scan-churn worktree was tidied; original path no longer resolves).

- Queue drained 10,335 → 10,136 with max pending ID flat, then auto-registration of `fs3-copilot-provider` minted **748 priority-1 scans** and drove pending 10,136 → 10,733.
- Oldest pending ID stayed fixed and aged while new high IDs arrived.
- Linked-worktree full-tree batches are the producer. `priority DESC, id DESC` amplifies starvation but is not the producer.
- Deleted roots had zero live scans; the old failed embed was not hot.
- The smallest producer-side fix is to skip scan jobs for reusable current blobs on a newly registered identical checkout while preserving worktree-specific ddoc/tooling cases.

My rejected sandbox measurement independently exposed the same mechanism: registering one fs2 root automatically imported nine linked checkouts into an otherwise isolated database. That run is evidence of harness friction, not included performance data.

## Ranked remediation list — smallest fix first

1. **Remove full queue aggregation from per-job settlement; emit deltas and refresh snapshots on the existing five-second cadence.** Keep one settlement UPDATE and, if needed, the cheap live count; do not call `queue_depth` from `emit_queue` for every job. Expected effect: eliminates up to one 17.6–80.8 ms full-history scan per settled row in measured environments; makes cost depend on reporting cadence, not job count. Smallest/highest-confidence fix.
2. **Give smart embeds a microbatch window or independent drain cadence.** Do not call `drain_embed` immediately after every completed summary; wake an embed lane on threshold (for example 16 items), a short maximum wait, or when general work goes idle. Expected effect on the measured corpus: smart calls **11,727 → ~776** at fs2-equivalent batch 16 (93.4% fewer); hosted-provider wall/cost improvement should be large. Preserve token-budget splitting and retry isolation.
3. **Add `kind` to the pending claim access path (or one partial claim index per lane).** Target shape: `(kind, priority DESC, id DESC) INCLUDE (not_before) WHERE state='pending'`, validated against eligibility and SKIP LOCKED. Expected effect: empty embed claims stop walking all 29,838 pending non-embed rows; measured 14.759 ms empty probe should become an index-edge lookup.
4. **Suppress reusable linked-worktree scan production.** Apply the churn report's enqueue-predicate fix: a newly registered checkout whose blobs already have current parse rows should map paths without minting a corpus-sized scan batch, except where worktree-specific ddoc state requires work. Expected effect observed in production: avoids 748-job and ~1,300-job promoted bursts per checkout and stops hidden bootstrap multiplication.
5. **Split scan and summarize admission pools.** A dedicated scan pool (CPU/DB width) and summarize pool (provider width), each clamped independently, prevents summaries from consuming every general permit. Expected effect: bootstrap reaches “all files parsed/searchable raw” earlier while enrichment continues.
6. **Batch vector inserts with `UNNEST`/array binding.** Replace one execute per vector with one statement per provider batch; avoid `row.vector.to_vec()` where sqlx/pgvector binding permits. Expected effect: parse comparator's 17,672 vector INSERT round trips become about 929 statements at the observed call plan, and far fewer after remediation 2.
7. **Batch element-tree persistence without losing parent links.** Preassign stable local ordinals and insert via a CTE/staging table, then resolve generated IDs/parents set-wise; alternatively persist a path/parent ordinal key. Expected effect: 18,874 element INSERT round trips become roughly one/few statements per file. This is the largest schema/code change in the list.
8. **Add fairness/aging after producer fixes.** Retain fresh-work preference, but cap consecutive LIFO/promoted claims or age old rows upward. Expected effect: bounds oldest-job latency under continued worktree/watcher input. This does not reduce work and therefore must follow producer and hot-query fixes.
9. **Only then tune worker counts.** Raising `worker_concurrency` now multiplies row-at-a-time writes and per-settlement aggregates; it may worsen database contention. After items 1–7, benchmark scan-only widths 4/8/16/32 against the same 959-file fixture and select the knee. CPU parser POC evidence shows parallel extraction can scale, but the live architecture currently mixes that lever with database and enrichment work.

## Bottom line

Evidence points away from Rust parsing speed and provider array capability as the primary explanation. The measured losses are repeated whole-queue observation, row-at-a-time persistence, immediate one-item smart-embed drains, and hidden linked-worktree production. flowspace2's current parser is sequential, yet its bulk in-memory graph path scans 1.1k files in ~51 s. Preserve fs3's durability and stage decoupling; move observation off the hot path, make the embed lane accumulate, suppress unnecessary work, then batch writes and separate admission lanes.

## Evidence appendix

### Run receipts

- Full-run log: `/tmp/fs3-scan-throughput-review-logs/flowspace3.log`, SHA-256 `e8377cef1147c95e809b08fdd1541204b4b323e9e49dbd14881bc871bf76b5ff`. First progress `2026-08-28T23:49:08.598474Z`; idle `2026-08-29T00:01:10.466666Z`; delta 721.868192 s.
- Parse-comparator log: `/tmp/fs3-scan-parse-review-logs/flowspace3.log`, SHA-256 `1bea991ddba13929fd1f520656e9b773e0d765d81f0823d2152a570fdb9cdad1`. First progress `2026-08-29T00:05:18.892345Z`; idle `2026-08-29T00:07:49.364559Z`; delta 150.472214 s.
- Full-run batch extraction matched `batch kind="embed" subject=<items> x <raw|smart> jobs=<jobs> ms=<ms>` over every log line. Complete smart item-count histogram: `{1: 11232, 2: 314, 3: 168, 4: 13}`; sum 11,727 calls / 12,416 items. Raw totals: 1,015 calls / 18,164 items; median 14, p95 52, max 141.
- Handler distributions matched `done kind=scan_file ... ms=<ms>` and `done kind=summarize ... ms=<ms>` over every log line. Counts equal final completed-job counts: 959 scan and 12,416 summarize.
- fs2 command receipt: `/usr/bin/time -p uv run fs2 scan --scan-path /Users/jordanknight/substrate/fs2/flow_squared --no-smart-content --no-embeddings --no-cross-refs --no-progress`; output: 1,098 files, 11,454 nodes, `real 50.65`, `user 4.04`, `sys 1.14`.

### SQL receipts

Production query basis: `SELECT count(*), count(*) FILTER (WHERE state='pending') FROM jobs` returned `544721 / 29836`; `pg_total_relation_size('jobs') / pg_relation_size('jobs')` returned `1516 MB / 473 MB`.

- General probe: `EXPLAIN (ANALYZE, BUFFERS, TIMING) SELECT id FROM jobs WHERE state='pending' AND not_before <= now() AND kind = ANY(ARRAY['scan_file','summarize']) ORDER BY priority DESC,id DESC LIMIT 1` → 1 row, 24 shared hits, **0.041 ms execution**.
- Embed probe: same predicate with `kind='embed' LIMIT 64` → 0 rows, **29,838 rows removed by filter**, 29,902 shared hits, **14.759 ms execution**.
- Production queue depth: `SELECT kind,state,count(*) FROM jobs GROUP BY kind,state ORDER BY kind,state` → parallel sequential scan, 60,574 buffers, **80.840 ms execution**.
- Isolated queue depth used the production function's full shape, including `count(*) FILTER (WHERE last_error IS NOT NULL)` → 27,469 rows, 3,519 buffers, **17.592 ms execution**.
- Isolated live count: `SELECT count(*) FROM jobs WHERE state IN ('pending','running')` → partial-index-only scan, 64 buffers, **0.241 ms execution**.

The two dedicated databases were force-dropped after measurement. The log files remain read-only receipts on this workstation; hashes above detect any later change.
