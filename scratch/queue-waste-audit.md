# PROD queue waste audit — summarize + embed

**Verdict: MIXED — 137,410 repeat summarize-key generations reached `done`, but the content cache makes their expected duplicate LLM charge $0. The number to remember is 137,410: real queue churn, not evidenced duplicate model spend.**

Audit snapshots: 2026-08-30 04:41–04:46Z. Database: PROD `flowspace3` at `127.0.0.1:5433`, every query inside `BEGIN TRANSACTION [ISOLATION LEVEL REPEATABLE READ] READ ONLY`. Logs: live daemon tmux pane `%50` and `~/.local/state/flowspace3/logs/flowspace3.log{,.1,.2,.3,.4}`. No queue, daemon, root, or database mutation was performed.

**Cost model (assumption):** Azure `gpt-5.6-luna` at $0.20/M input tokens + $1.20/M output tokens; token estimate is repository convention `bytes / 3`; prompt/schema/tag overhead and deleted historical cache rows are excluded. Embeddings use `text-embedding-3-small` and are ranked as near-free, per steer.

## Ranked findings by provider cost

### 1. Genuine summarize work dominates spend; repeat rows do not show repeat spend

At 04:46:21Z, completed summarize history contained **247,450 executions**, **110,040 distinct `(identity, raw_hash)` keys**, and **104,385 distinct raw hashes**. Therefore:

- **104,385 first-seen raw hashes** versus **143,065 repeated raw-hash executions** (57.8% of completed executions).
- At the queue-key generation level: **110,040 first generations** versus **137,410 repeated generations** (55.5%).
- Maximum observed generations for one key: **52**. The largest offenders were unchanged `pij` hashes repeatedly re-emitted over 3–28 hours.

SQL receipt:

```sql
SELECT count(*) summarize_done,
       count(DISTINCT dedupe_key) keys,
       count(DISTINCT payload->>'raw_hash') raw_hashes,
       count(*)-count(DISTINCT dedupe_key) repeat_key_generations,
       count(*)-count(DISTINCT payload->>'raw_hash') repeat_raw_executions
FROM jobs WHERE kind='summarize' AND state='done';
-- 247450 | 110040 | 104385 | 137410 | 143065

SELECT max(n) FROM (
  SELECT dedupe_key,count(*) n
  FROM jobs WHERE kind='summarize' AND state='done'
  GROUP BY dedupe_key
) g;
-- 52
```

This is substantial queue/DB churn, but it is not evidence of repeated LLM billing. `summarize()` checks `(raw_hash, model_key)` in `smart_content` before obtaining the provider and returns after only re-emitting the smart-vector job when the row exists (`crates/daemon/src/enrich.rs:397-424`). The live unique index also prevents concurrent generations of the same queue key.

The retained paid-content estimate is **84,288 Luna summaries**, about **67.01M input + 10.28M output tokens = $25.74**. This is the paid baseline, not waste:

```sql
WITH one AS (
  SELECT DISTINCT ON (payload->>'raw_hash')
         payload->>'raw_hash' raw_hash,
         octet_length(payload->'element'->>'raw_text') input_bytes
  FROM jobs
  WHERE kind='summarize' AND state='done'
  ORDER BY payload->>'raw_hash',id
), paid AS (
  SELECT one.input_bytes,octet_length(s.text) output_bytes
  FROM one JOIN smart_content s USING(raw_hash)
  WHERE s.model_key='gpt-5.6-luna@1'
)
SELECT count(*),sum(input_bytes)/3,sum(output_bytes)/3,
       round((sum(input_bytes)/3.0*.20 + sum(output_bytes)/3.0*1.20)/1e6,2)
FROM paid;
-- 84288 | 67010557 | 10280372 | $25.74
```

The gap between 104,385 first-seen hashes and 84,288 retained Luna rows is not automatically loss: the point-of-spend reference guard skips content no live root holds, and GC may remove old unreferenced cache rows.

**Cross-identity race bound.** The queue key includes identity while the cache key does not, so two identities carrying one raw hash could theoretically pass the pre-check concurrently. History contains 8,298 cross-identity successor rows, but only **15 settled within one second** of their predecessor (25 within 5s). Where the current `smart_content.created_at` survived, the second row settled 36–706ms *after* the cache row existed, consistent with a cache hit. No duplicate provider call is logged for summarize, so exact proof is unavailable; the worst-case cost of all 15 one-second candidates at the observed average is under **$0.005**. This is a telemetry ceiling, not a found bill.

### 2. Conversation re-ingest produces repeat jobs, but the paid-content guard is holding

The pre-whale recovery identity has **8,062 done summary jobs over 5,011 hashes: 3,051 repeat jobs**, with only 8 rows needing a second attempt and no failed summaries.

```sql
SELECT count(*),count(DISTINCT payload->>'raw_hash'),
       count(*)-count(DISTINCT payload->>'raw_hash'),
       count(*) FILTER (WHERE attempts>1)
FROM jobs
WHERE kind='summarize' AND state='done'
  AND payload->>'identity'='conv:recovery';
-- 8062 | 5011 | 3051 | 8
```

Same-identity generations cannot overlap because `jobs_live_dedupe_idx` is unique over live keys. Later generations therefore encounter an existing `smart_content` row and avoid Luna. Result: **observable queue waste, expected duplicate provider cost $0**.

### 3. Empty-string embeds are a bounded boot loop; cheap, but definitely wasteful

There are now **8 failed embed jobs containing one empty string each**, 82 payload items total. All are `attempts=3`, `terminal=false`; six predate the whale and two arrived during it. Because the boot sweep requeues every non-terminal failed summarize/embed job, these poisons wake only at daemon boot, then spend the normal 3-attempt ladder. They do **not** retry continuously while parked.

```sql
SELECT count(*) poison_jobs,
       sum(jsonb_array_length(payload->'items')) payload_items,
       min(updated_at),max(updated_at)
FROM jobs j
WHERE kind='embed' AND state='failed'
  AND EXISTS (
    SELECT 1 FROM jsonb_array_elements(j.payload->'items') e
    WHERE e->>1=''
  );
-- 8 | 82 | 04:22:16Z | 04:37:58Z
```

Boot/log receipts:

```text
23:09:26Z swept=1
23:47:04Z swept=2
23:48:40Z swept=3
00:31:29Z swept=4
04:21:49Z swept=5
```

After the two boots covered by structured batch logging, the provider log records **11 error calls at 00:31Z** and **14 at 04:21–04:22Z**. The current whale added **7 more error calls**. These are rejected HTTP 400 requests, so token spend is likely negligible, but request/retry work is real.

The larger operational cost is collateral batching. One new empty item caused merged first attempts of **309** and **368** raw texts to be rejected; the retry planner then isolated suspect jobs as designed. Relevant log receipt:

```text
04:36:43.998Z embed: sent batch of 309 texts ... outcome="error"
04:36:51.311Z embed: sent batch of 15 texts ... outcome="error"
04:36:58.910Z embed: sent batch of 15 texts ... outcome="error"
04:37:35.810Z embed: sent batch of 368 texts ... outcome="error"
04:37:45.815Z embed: sent batch of 15 texts ... outcome="error"
04:37:56.220Z embed: sent batch of 15 texts ... outcome="error"
```

The 7–10 second observed spacing is consistent with 2s/4s backoff plus lane work; it is not a hot sub-second loop. The defect is that empty input remains non-terminal and is revived on every boot.

### 4. Embed history is highly repetitive, but the vector cache makes repeats free

At 04:46:32Z:

| source/state | jobs | item occurrences | distinct hashes | repeats | empty |
|---|---:|---:|---:|---:|---:|
| raw/done | 134,346 | 1,112,904 | 167,534 | 945,370 | 0 |
| smart/done | 245,187 | 258,399 | 103,530 | 154,869 | 0 |
| raw/failed | 8 | 82 | 70 | 12 | 8 |

SQL receipt:

```sql
WITH items AS (
  SELECT j.state,j.id,j.payload->>'source' source,
         x.elem->>0 hash,x.elem->>1 text
  FROM jobs j
  CROSS JOIN LATERAL jsonb_array_elements(j.payload->'items') x(elem)
  WHERE j.kind='embed'
)
SELECT source,state,count(DISTINCT id),count(*),count(DISTINCT hash),
       count(*)-count(DISTINCT hash),count(*) FILTER (WHERE text='')
FROM items GROUP BY source,state;
```

`embed_items()` runs one `existing_embedding_hashes` batch query and returns before the provider when every hash exists (`crates/daemon/src/enrich.rs:539-563`). Thus **1,100,239 repeated done-item occurrences** are queue work but expected zero additional embedding purchase. The current history has 749 successful embeds with `attempts>1`, one successful attempt-3 row, and the 8 poison failures; no parks were present in the snapshot.

## Looping and oscillation verdict

- **Summarize:** 247,450 done; 227 (0.092%) needed attempt 2; zero attempt 3, failed, or parked. No retry loop.
- **Embed:** 379,389 done at 04:46:21Z; 749 (0.197%) needed multiple attempts; 8 failed poison rows at attempt 3. No hot retry loop; poison revival is boot-triggered.
- **Duplicate-root scans:** 37 current failed rows across 6 bad blobs. The largest blob accounts for 24 rows and the next for 6. Every failed row has a distinct `(worktree,path)` dedupe key; **zero same-key failed generations**. Scan jobs are not included in the boot `requeue_failed([summarize, embed])` sweep, so these are terminally parked in practice rather than looping. New worktrees create new keys for the same bad blob, explaining the population growth.
- **Oscillation proof ceiling:** `jobs` stores final state and `updated_at`, not a transition ledger or `created_at`. Pending→running→pending cycles cannot be reconstructed exactly. Attempts plus structured retry timestamps show the bounded ladders above; repeated `done` generations reflect re-enqueue after completion, not state oscillation of one row.

## Post-#72/#73/#75 operating shape

Deployment receipts:

```text
#72 commit 7174c1f 2026-08-29T23:42:16Z; PROD boot 23:45:01Z
#73 commit c881277 2026-08-30T00:12:02Z
#75 commit 89cd0dc 2026-08-30T00:28:26Z; PROD boot 00:29:41Z
migration 0019 installed 00:30:15Z
```

### Whale: 14,575-turn conversation

Conversation `5eb0e424-68de-8830-a747-adc897c32cde`, anchored to `https://github.com/AI-Substrate/harness-engineering`, was created at **04:35:57.844Z** with **14,575 turns**. It minted 10,331 summarize jobs (70.9% of turns met the summary floor) and 912 raw embed jobs (910 done, 2 failed poison batches).

At 04:46:21Z: **5,943 summaries done, 4,364 pending, 24 running**. From 04:38–04:44 full-minute buckets, 4,772 summaries settled in 420s: **11.36 summaries/s (682/min)**. Log aggregation independently measured 5,811 completions over 522.9s: **11.11/s**, with no retries or failures.

```sql
-- Full-minute done counts for the whale:
-- 04:38 721; 04:39 673; 04:40 683; 04:41 716;
-- 04:42 539; 04:43 733; 04:44 707
```

### Embed provider batches are wide

From whale creation through 04:44:57Z, structured provider-call logs show:

| source | successful calls | texts | mean | median | max | single-text calls | texts carried in calls >=16 |
|---|---:|---:|---:|---:|---:|---:|---:|
| raw | 165 | 14,570 | 88.3 | 15 | 751 | 3 | 92.3% |
| smart | 332 | 5,022 | 15.13 | 16 | 32 | 11 | 72.7% |

Raw batches are often hundreds of texts because up to 64 sixteen-item jobs merge under the 200k-token budget. Smart embeds arrive one per completed summary; the post-#73 microbatch produces the intended median of 16. The summarize progress logger emitted 349 full 16-item groups out of 368 groups (94.8%). Small tails are expected from the one-second flush and idle drain.

### Settlement no longer performs one queue census per item

After the 00:31:29Z post-#72/#73/#75 production startup, the rolling logs contain **0** `done kind=summarize ... left=` lines and **2,137** grouped `summarize: dispatched group ...` lines. During the whale, progress censuses occurred at median **5.37s** spacing (p90 6.65s), not once per settlement. Code receipt: `SummaryReport::flush` logs groups; `report_progress` owns the grouped queue census. This is consistent with #72's removal of `jobs_remaining()` from the summarize settlement hot path.

### LIFO is visible in history

The claim SQL is explicitly `ORDER BY priority DESC, id DESC` for both single and batched claims, backed by lane-specific partial indexes from migration 0019. Whale completion order is noisy because 32 calls run concurrently and ingestion was still enqueueing, but it has the expected descending-id trend:

- Whale summarize `corr(id, settled_at) = -0.859`.
- Whale raw embed `corr(id, settled_at) = -0.726`.
- Once enqueue finished, mean settled summarize id descended by minute: **1,161,392 (04:38) → 1,160,488 → 1,159,735 → 1,158,948 → 1,158,200 → 1,157,494 → 1,156,700 (04:44)**.

That is strong historical confirmation of LIFO. The earliest 04:36 rows include low ids because the consumer was active while the producer was still appending; that is expected, not FIFO regression.

## Bottom line

- **Money:** about **$25.74** of retained genuine Luna work under the stated model; **$0 evidenced duplicate summarize spend**. Exact provider-call proof is blocked because summarize cache hits and provider calls share the same `done`/group log shape.
- **Waste:** **137,410 repeat summarize-key generations** and **1.10M repeated embed-item occurrences** consumed queue/SQL work but should not call providers.
- **Loop:** no hot loop. One bounded defect remains: 8 empty-string embed jobs are non-terminal and revive only on boot, generating repeated 400s and occasionally poisoning large first-attempt batches.
- **Design:** post-#72/#73/#75 behavior is present: grouped settlement/progress, smart embed median 16, wide raw batches, ~11.3 summaries/s, and clear LIFO correlations.
