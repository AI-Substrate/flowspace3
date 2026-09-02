# w-db-cpu-profile — is the flowspace3 Postgres eating this machine?

**Seat:** `pij-purring-orangutan` (rs, spawn `s1788309908670-61353`, model `github-copilot/claude-opus-5`)
**Date:** 2026-09-02, ~00:45–01:05 UTC · **Mode:** READ-ONLY. Nothing was restarted, reconfigured, vacuumed, installed, or killed. Only `.harness/temp/agent/` was written.
**Raw data:** `.harness/temp/agent/dbprof/` — `timeseries.csv` (120 rows), `act_raw.txt` (2,432 samples), `explain160.out`, `pglog6h.txt`.

---

## TL;DR — the answer to Jordan's question

**No. The database is not what is eating your machine — the fleet is.** Over a measured 742-second window `flowspace3-db` consumed **380 CPU-seconds = 0.51 cores average = 3.2% of a 16-core machine** (peak 208.9% ≈ 2.1 cores). During the same window host load average ran **28.3 → 76.3** with total host CPU utilisation around **13.7%**. Load that high against that little CPU is not a CPU-bound database; it is **1,309 processes / 210 `node` processes** (agent seats, bun, tmux, rustc, pij daemons) queueing. The OrbStack Helper measured **0.5–1.1%** while I watched it, and the seven idle `voska/hass-mcp` containers plus `fs3-linuxtest` and buildkit measured **0.00% CPU each** — they cost nothing.

**But the DB is burning 5–10× more CPU than its workload justifies, and one query is 69% of it.** The single highest-leverage change is **the search admission filter in `crates/store/src/embeddings.rs`** — it executes a `smart_content` index probe **962,792–1,698,017 times per search**, for a query whose actual vector work costs **12 ms**. That is a code fix, needs no restart, and is worth more than every config change on the table.

**Row 122's headline hypothesis is falsified.** `shared_buffers=128MB` is *not* what makes search slow. Measured: the HNSW index scan touches **1,078 buffers and 12.4 ms**; the whole search touches **3,853,170 buffer HITS against 11,378 reads (99.7% hit rate)**. Search is **CPU-bound on a bad plan**, not I/O-bound on a cold index. (Row 122 *is* right about `shared_buffers` for a different query — see cause #2.)

---

## 1. Time series — 742 s @ 6.24 s cadence (`dbprof/timeseries.csv`, 120 rows)

`docker stats flowspace3-db` + `pg_stat_activity` + host `uptime`, 2026-09-02T00:46:53Z → 00:59:15Z.

| metric | value |
|---|---|
| container CPU% | min 0.7 · **median 25.4** · mean 50.8 · p90 110.9 · **max 208.9** |
| CPU-seconds consumed | **380 s of 11,872 available = 3.2% of the machine** |
| mean cores | **0.51** |
| host load1 over window | 28.3 → 76.3 |
| active client backends | mean 1.08 when CPU<50%, **2.07 when CPU≥50%** |
| container memory | 394 → 535 MiB of 31.36 GiB (1.2–1.7%) |

**When CPU spikes, what is running** — the oldest active statement at each sample, correlated against container CPU:

| oldest active statement | n | mean CPU% | max CPU% | CPU-seconds | share of DB CPU |
|---|---|---|---|---|---|
| **search CTE** (`WITH candidate_vectors AS MATERIALIZED …`) | 38 | **110.2%** | **208.9%** | **261.2** | **68.7%** |
| *(no client backend active — background floor)* | 71 | 17.8% | 120.6% | 74.7 | 19.7% |
| `UPDATE jobs …` | 4 | 79.2% | 90.8% | 19.8 | 5.2% |
| other | 3 | 79.1% | 131.3% | 14.8 | 3.9% |
| `INSERT …` | 4 | 38.3% | 101.5% | 9.6 | 2.5% |

Of the 55 samples at CPU ≥ 50%, **38 (69%) had the search CTE as the oldest active statement**. Background floor is 0.17 cores. **Search-attributable excess over that floor: 219 CPU-seconds of 380.**

Memory is a non-issue: 535 MiB used of a 31 GiB limit. The container is not under memory pressure; it is under *plan* pressure.

---

## 2. Poor man's profile — `pg_stat_activity` @ 1 s for 5 min (`dbprof/act_raw.txt`)

315 ticks, 2,432 backend samples, **191 active** (0.61 active backends/tick — the DB is *not* saturated on average; it is bursty).

**Top query shapes by active-sample count** (literals and `$n` normalised):

| # | samples | share | max age | shape |
|---|---|---|---|---|
| 1 | 61 | **31.9%** | **15.0 s** | `WITH candidate_vectors AS MATERIALIZED (SELECT source_hash, source_kind, chunk_no, vector <=> …` |
| 2 | 33 | **17.3%** | 0.2 s | `SELECT kind, state, count(*) AS depth, count(*) FILTER (WHERE last_error IS NOT NULL) …` |
| 3 | 21 | 11.0% | 0.1 s | `UPDATE jobs SET state=?, last_error=?, terminal=?, updated_at=now() WHERE id=?` |
| 4 | 19 | 9.9% | 0.0 s | `SELECT w.id, r.identity, w.root_path, w.ref_name, (SELECT count(*) FROM worktree_files f …` |
| 5 | 15 | 7.9% | 0.0 s | `UPDATE jobs SET state=?, attempts=attempts+N … WHERE id = (SELECT id FROM jobs …)` (the claim) |
| 6 | 14 | 7.3% | 0.0 s | `INSERT INTO jobs (kind, dedupe_key, payload, not_before, priority) … ON CONFLICT` |
| 7 | 11 | 5.8% | 0.0 s | `INSERT INTO embeddings_1024 (source_hash, source_kind, chunk_no, model_key, vector, truncated)` |
| 8 | 3 | 1.6% | 0.0 s | `INSERT INTO smart_content …` |
| 9 | 3 | 1.6% | 2.8 s | `autovacuum: VACUUM pg_toast.pg_toast_29885` |
| 10 | 3 | 1.6% | 0.2 s | `WITH file_groups AS (SELECT DISTINCT blob_sha, parser_version FROM elements …)` |
| 11 | 3 | 1.6% | 0.0 s | `SELECT count(*) AS left FROM jobs WHERE state IN (?, ?)` |
| 12 | 2 | 1.0% | 0.0 s | `INSERT INTO worktree_files … ON CONFLICT` |

**Wait-event histogram (active backends only):**

| wait | samples | share |
|---|---|---|
| **none — running on CPU** | 123 | **64.4%** |
| `IO / WALSync` | 40 | **20.9%** |
| `IO / DataFileRead` | 14 | 7.3% |
| `LWLock / WALWrite` | 9 | 4.7% |
| `Timeout / VacuumDelay` | 3 | 1.6% |
| `Client / ClientRead`, `IO / DataFileWrite` | 2 | 1.0% |

Read that as: **two thirds of active DB time is pure CPU** (cause #1), and **a quarter is stalled on WAL** (`WALSync` + `WALWrite` = 25.6%, cause #3). Only 7.3% is data-file read — which again is why "the HNSW index doesn't fit in shared_buffers" is not the story.

Shapes #2 and #4 together are **27.2% of active samples** and are both issued by a single `GET /status` call (`crates/daemon/src/status.rs:30` and `:19`).

---

## 3. The jobs hot path

**Poll shape** (`crates/store/src/jobs.rs:156` `claim_job`, `:211` `claim_jobs`):
```sql
UPDATE jobs SET state='running', attempts=attempts+1, updated_at=now()
 WHERE id = (SELECT id FROM jobs
              WHERE state='pending' AND not_before<=now() AND kind = ANY($1)
              ORDER BY priority DESC, id DESC
              FOR UPDATE SKIP LOCKED LIMIT 1)
 RETURNING …
```
**Cadence:** `IDLE_POLL = 250 ms` (`crates/daemon/src/runner.rs:129`) — but *only when the queue came back empty*. While work is in flight the drain loops spin with **no sleep at all**, at `indexing.worker_concurrency = 32` across three lanes (general / ingest / embed). Measured claim rate: `jobs_claim_general_idx` 9 scans/s, `jobs_claim_embed_idx` 4 scans/s.

**Indexes on `jobs`** (60 MB total against an 884 MB heap):

| index | size | definition | idx_scan (2.2 h) |
|---|---|---|---|
| `jobs_pkey` | 36 MB | `btree (id)` | 197,678 |
| `jobs_live_dedupe_idx` | 18 MB | `UNIQUE btree (dedupe_key) WHERE state IN ('pending','running')` | 228,749 |
| `jobs_claim_general_idx` | 4,336 kB | `btree (priority DESC, id DESC) INCLUDE (not_before) WHERE state='pending' AND kind IN ('scan_file','summarize')` | 23,461 |
| `jobs_claim_embed_idx` | 2,512 kB | `btree (priority DESC, id DESC) INCLUDE (not_before) WHERE state='pending' AND kind='embed'` | 3,867 |
| `jobs_revivable_idx` | 16 kB | `btree (kind) WHERE state='failed' AND NOT terminal` | 11 |

**No index covers `state` alone, and there is no covering index for `GROUP BY kind, state`.**

**`pg_stat_user_tables` for `jobs`:**

| seq_scan | seq_tup_read | idx_scan | n_live_tup | n_dead_tup | last_autovacuum | autovacuum_count |
|---|---|---|---|---|---|---|
| 2,428 | **785,097,415** | 453,766 | 1,016,092 | 196,719 (16.2%) | **never** | **0** |

**Measured 65-second counter delta (the number that matters):**
```
jobs  seq_scan=+30      seq_tup_read=+10,162,106  (155,931 tuples/s)
      idx_scan=+4,152   n_tup_upd=+1,032          n_dead_tup=+1,032
db    blks_read=+1,664,323 (25,538 blocks/s ≈ 200 MB/s from disk)
      blks_hit=+35,376,398 (542,829/s)
```

**Row counts:** `done` **1,009,934** · `pending` 1,267 · `failed` 34 · `running` 33. **99.4% of the table is completed history that is never purged** — I found no `DELETE FROM jobs` or retention path anywhere in `crates/store/src/jobs.rs` or `crates/daemon/`. The table has grown from the brief's 495k `done` rows to 1.01M **during today**.

**Autovacuum status:** `jobs` needs `0.2 × 1,016,092 + 50 = 203,268` dead tuples to trigger; it currently sits at **196,719 — 6,549 away**. It has never run. When it does fire it will vacuum a **2,150 MB** relation with `maintenance_work_mem = 64 MB`, forcing **multiple index-cleanup passes over five indexes**. That is a large latent spike, arriving soon.

---

## 4. Checkpoint / WAL churn — this is our own test suite

**Container log, last 6 h (`docker logs --since 6h flowspace3-db`), checkpoint start reasons:**

| reason | count |
|---|---|
| **`immediate force wait`** | **917** |
| `time` | 54 |
| `wal` | 4 |
| `end` | 1 |

**917 forced immediate checkpoints in 6 hours = one every 23.6 s.** They arrive in bursts — **65 in the single minute 23:32**, 63 at 23:29, 55 at 23:33 — across only 51 distinct minutes. `immediate force wait` is the signature of **`DROP DATABASE`**, which calls `RequestCheckpoint(IMMEDIATE|FORCE|WAIT)`. The test suite mints and drops a database per test (`crates/store/tests/support/mod.rs:91`, `crates/store/src/admin.rs:203/219`) **against the shared production container**.

**`pg_stat_bgwriter`** (window: 7,956 s since the 22:48:51 crash-induced stats reset):

| metric | value | reading |
|---|---|---|
| `checkpoints_timed` | **11** | the 5-min timer barely gets to fire |
| `checkpoints_req` | **836** | **one checkpoint every 9.4 s** |
| `checkpoint_write_time` | 2,227 s | **28.0% of wall-clock spent writing checkpoints** |
| `checkpoint_sync_time` | 207 s | |
| `buffers_checkpoint` / `buffers_clean` / `buffers_backend` | 623,980 / 442,141 / **2,095,127** | **backends do 66.3% of their own evictions** |
| `buffers_alloc` | 26,138,474 = **3,285/s** | **the 16,384-buffer pool turns over completely every 5.0 s** |

**`pg_stat_wal`:** `wal_bytes` **12.67 GB in 2.21 h** (1.59 MB/s), `wal_records` 22,229,172, **`wal_fpi` 2,034,020**, `wal_buffers_full` 191,029. WAL directory: **928 MB / 58 files**, i.e. pinned at the `max_wal_size = 1024MB` ceiling.

**The death spiral, named:** `DROP DATABASE` forces an immediate checkpoint → every checkpoint resets the full-page-image window, so the *next* write to every page emits a full 8 KB FPI → **2.03M FPIs ≈ 16.7 GB of would-be WAL against 12.67 GB actual** → WAL blows through `max_wal_size` → **836 *requested* checkpoints** on top of the forced ones → repeat. Meanwhile each `CREATE DATABASE` under PG16's default `WAL_LOG` strategy WAL-logs the entire template (~8.7 MB × ~900 creates ≈ 8 GB), which is most of that 12.67 GB.

**Correction to a stated fact:** the brief's "`checkpoint complete … write=135–158 s` every ~5 min, checkpoints taking most of the interval" is **not** pathological on its own. That is `checkpoint_completion_target = 0.9` deliberately spreading a *timed* checkpoint over 270 s of a 300 s interval — working as designed. The pathology is the **917 forced ones that bypass spreading entirely**, and they are ours.

**Leaked test databases: 60 databases exist, 56 of them `fs3_*` at ~8.7 MB each (~490 MB).** Directly row 110 ("orphan test-DB accumulation"), and the same shared-container problem row 126 is about. `flowspace3` itself is 8,190 MB.

---

## 5. The search CTE — `EXPLAIN (ANALYZE, BUFFERS)` on prod

Query taken from `crates/store/src/embeddings.rs:557` (verified as the shape actually running: prod samples show `FROM embeddings_1024 e`, the filtered variant). Binds: real prod vector, `model_key='text-embedding-3-small-no-rate@1024'`, `limit=40`, all filters NULL, `candidate_limit=160`. Full output: `dbprof/explain160.out`.

```
Limit  (actual time=1654.374..1654.382 rows=35 loops=1)
  Buffers: shared hit=3853170 read=11378, temp read=8413 written=790
  CTE candidate_vectors
    -> Limit (cost=3082.92..478931854.03 rows=160) (actual time=74.076..1367.160 rows=40)
        -> Nested Loop Semi Join (actual time=74.071..1367.139 rows=40)
             Join Filter: ((e.source_kind='raw' AND admitted.raw_hash=e.source_hash)
                        OR (e.source_kind='smart' AND (SubPlan 1)))
             Rows Removed by Join Filter: 1133036
             -> Index Scan using embeddings_1024_vector_idx on embeddings_1024 e
                  (actual time=12.131..12.464 rows=40)      Buffers: shared hit=119 read=959
             -> Materialize (actual rows=28327 loops=40)
                  Buffers: shared read=9974, temp read=8413 written=790
                  -> Seq Scan on elements admitted (actual rows=86191 loops=1)
             SubPlan 1
               -> Index Scan using smart_content_text_hash_idx on smart_content candidate
                    (actual time=0.001..0.001 rows=0 loops=962792)
                    Buffers: shared hit=3851137 read=31
Planning Time: 3.133 ms
JIT: Functions: 88 … Optimization 132.814 ms, Emission 109.080 ms, Total 281.374 ms
Execution Time: 1667.423 ms
```

**What this says, in order of importance:**

1. **`loops=962792`.** The admission `EXISTS` fires a `smart_content` index probe **962,792 times to return 40 rows**, costing **3,851,137 of the query's 3,853,170 buffer hits (99.95%)**. A second run at `candidate_limit=160` measured **1,698,017 loops / 6,793,986 buffers / 2,725 ms**; at 640 it measured 1,486,658 / 5,950,186 / 2,594 ms. **Cost is ~constant in `candidate_limit` and enormous in absolute terms.**
2. **The HNSW index is innocent.** `Index Scan using embeddings_1024_vector_idx` = **12.4 ms, 1,078 buffers**. The 2,286 MB index is doing its job. **Row 122's premise — "the index cannot live in 128MB of buffers, so every query walks it from disk" — is not what the plan shows.**
3. **`hit=3,853,170` vs `read=11,378` — a 99.7% buffer hit rate.** This query is **CPU-bound on buffer lookups and tuple comparisons, not I/O-bound**. Raising `shared_buffers` will barely move it.
4. **`Materialize` over a `Seq Scan on elements`** (86,191 rows) **spills to disk** (`temp read=8413 written=790`) because `work_mem = 4 MB`, and is rescanned 40 times → **1,133,036 rows removed by join filter**.
5. **JIT burns 281 ms (17% of the query)** compiling 88 functions, triggered by a nonsense cost estimate of **478,936,610**. Fixing the plan removes this for free.
6. The `MATERIALIZED` CTE does pull full 1024-float vectors into a temp set (row 122's hypothesis (b)) — real, but second-order next to the 962k probes.

**Amplifier:** `MAX_CANDIDATE_EXPANSIONS = 8` with `CANDIDATE_GROWTH_FACTOR = 2` (`embeddings.rs:52-58`). When a pass under-fills, `candidate_limit` doubles and **the entire query re-runs, up to 9 times** (160 → 40,960). This is how row 122's 13.4 s / 60 s / 120 s searches happen. My own dogfooded `flowspace3 search` took **15.6 s**, and prod sampling caught this statement at **age 15.0 s**.

**Live rate confirming the plan:** `smart_content_text_hash_idx` measured at **124,551 index scans per second** sustained over 65 s (8,117,038 scans). Nothing else in the codebase can produce that.

---

## 6. Autovacuum / bloat

`pg_stat_progress_vacuum`: **empty** — no vacuum in progress. One `autovacuum: VACUUM pg_toast.pg_toast_29885` was caught in the activity sample. `pgstattuple` is not installed (not installed by me); figures below are the standard heap bloat estimate.

| table | real size | bloat | bloat % | n_live | n_dead | dead % | last_autovacuum | autovac count |
|---|---|---|---|---|---|---|---|---|
| **jobs** | 2,150 MB | **170 MB** | 7.9% | 1,016,092 | **196,719** | **16.2%** | **never** | **0** |
| elements | 484 MB | 72 MB | 14.9% | 264,882 | 4,289 | 1.6% | 00:12:38Z | 1 |
| embeddings_1024 | 1,503 MB | 69 MB | 4.6% | 325,886 | 9,385 | 2.8% | **never** | **0** |
| worktree_files | 26 MB | 8,392 kB | **31.5%** | 126,037 | 0 | 0.0% | 00:55:24Z | 7 |
| smart_content | 81 MB | 4,064 kB | 4.9% | 127,319 | 2,870 | 2.2% | never | 0 |
| turns | 72 MB | −808 kB | −1.1% | 73,129 | 2,192 | 2.9% | 00:11:16Z | 1 |

**Bloat is real but is *not* the CPU problem** — 170 MB on `jobs` is 7.9%. The `jobs` problem is **volume, not bloat**: 1.01M retained `done` rows. Autovacuum settings are all defaults (`scale_factor 0.2`, `naptime 60s`, `max_workers 3`, `cost_delay 2ms`, `maintenance_work_mem 64MB`).

Total relation sizes for context: `embeddings_1024` 4,503 MB (2,670 MB indexes), `jobs` 2,467 MB, `elements` 938 MB — **~8.2 GB of hot relations against a 128 MB buffer pool.**

---

## 7. OrbStack itself

`orb 2.2.3`. Measured per-container CPU, 6 passes:

| container | mean CPU% | max CPU% |
|---|---|---|
| **flowspace3-db** | **16.34%** | 53.93% |
| mfb-mcp | 0.11% | 0.12% |
| 7 × `voska/hass-mcp` | **0.00%** | **0.00%** |
| fs3-linuxtest | **0.00%** | **0.00%** |
| buildx_buildkit | **0.00%** | **0.00%** |

**The seven idle `hass-mcp` containers and `fs3-linuxtest` cost nothing in CPU.** They cost ~358 MB RSS combined and nothing else. They are not worth touching.

**Host side:** `OrbStack Helper` sampled at **0.5%, 1.0%, 1.1%** across three snapshots while host load1 was 69.12. Total host CPU utilisation ~13.7%; top host consumers were `rustc` 2.4%, `node` 2.3%, `pij_daemon` 1.2% ×2 — **no process dominating**. The brief's "Helper 30% steady, peaked 197%" is consistent with what I measured *only during DB bursts*: OrbStack Helper is the VM wrapper, so **postgres's own CPU and its virtio block I/O appear inside the Helper's number**. `flowspace3-db` has done **1.51 TB read / 793 GB written** of block I/O in 5 days; that traffic is billed to the Helper on the host side. **OrbStack's overhead independent of postgres is under ~1%.**

**Disk (not CPU, but worth naming):** `docker system df` — Local Volumes **128.8 GB, 99.97 GB (77%) reclaimable**; Images 13.39 GB with 8.7 GB reclaimable; Build Cache 4.97 GB.

---

## 8. Causes ranked by attributable CPU-seconds

CPU-seconds are from the 742 s window (380 CPU-s total for the container).

### #1 — Search admission `EXISTS` filter runs ~1M index probes per query
**261 CPU-s of 380 (68.7% of DB CPU); 219 CPU-s above background floor.**
`crates/store/src/embeddings.rs:557-620` — the `EXISTS (SELECT 1 FROM elements admitted … OR (source_kind='smart' AND EXISTS (SELECT 1 FROM smart_content candidate WHERE candidate.text_hash = e.source_hash AND candidate.raw_hash = admitted.raw_hash)))` inside the `candidate_vectors` CTE plans as a Nested Loop Semi Join over a spilling `Materialize` of a `Seq Scan on elements`, producing **962,792–1,698,017 `smart_content` probes and 3.8–6.8M buffer hits per search**, 1.7–2.7 s each, ×9 if the expansion loop runs. Measured live at **124,551 probes/s**.
**Fix:** restructure admission so it is not a per-candidate correlated `OR`-`EXISTS` — resolve `smart_content.text_hash → raw_hash` once into a CTE/join and let the planner hash it, instead of probing per candidate; then apply the element-admission predicate as a hash semi-join. Secondary: have the CTE carry `(source_hash, chunk_no, distance)` only and fetch vectors never (row 122(b)); JIT overhead (281 ms/query) disappears once the cost estimate is sane.
**Cost:** query rewrite in one function + plan verification. **Restart: NO.** **Row: 122** (and this *corrects* 122's stated cause).

### #2 — `queue_depth()` full-scans 1.01M jobs rows on 3 cores every ~6.5 s
**~27% of active samples (shapes #2 + #4, both from one `GET /status`); the dominant source of the 200 MB/s disk read.**
`crates/store/src/jobs.rs:569` — `SELECT kind, state, count(*), count(*) FILTER (…) FROM jobs GROUP BY kind, state` has **no `WHERE` clause**. Measured: `Parallel Seq Scan on jobs`, **Workers Launched: 2** (3 processes), **`Buffers: shared hit=17 read=114185`** — it reads **892 MB from disk, essentially none of it cached**, 134 ms wall ≈ 260 ms CPU, per call. Called by `report_progress` (`runner.rs:772`, every `PROGRESS_EVERY = 5 s` while working, plus every idle transition) **and** by `GET /status` (`status.rs:30`). Live delta: `seq_tup_read +10,162,106 in 65 s`.
This is also **the one place row 122's `shared_buffers` hypothesis is correct** — `read=114185` vs `hit=17` is a pure buffer-pool miss.
**Fix, cheapest first:** (a) purge/retain `done` jobs — 1,009,934 of 1,016,092 rows are settled history with no retention path anywhere; (b) make the progress path use a live-only count (`WHERE state IN ('pending','running')` already gets an index-only scan on `jobs_live_dedupe_idx` in **0.77 ms** — measured); (c) if full history counts are genuinely needed by `/status`, add a covering index or cache them.
**Cost:** small. **Restart: NO.** **Rows: 120** (status/jobs surface), **122**.
*Also fix the stale doc comment at `crates/store/src/jobs.rs:558-560` — it claims "`jobs_claim_idx` leads on `state`, so this is an index scan of the live rows and never touches the settled history." `jobs_claim_idx` no longer exists.*

### #3 — Test-DB `DROP DATABASE` forces a checkpoint every 23.6 s → FPI death spiral
**28.0% of wall-clock in `checkpoint_write_time`; 20.9% of active DB samples stalled in `IO/WALSync` + 4.7% in `LWLock/WALWrite` = 25.6% of active time.**
917 `immediate force wait` checkpoints in 6 h (bursts of 65/min) vs 54 timed; **836 requested vs 11 timed** in `pg_stat_bgwriter` = one every 9.4 s; **2,034,020 FPIs / 12.67 GB WAL in 2.21 h**; WAL pinned at the 928 MB/1 GB ceiling. Caused by the test suite creating and dropping databases **on the shared production container**. 56 leaked `fs3_*` databases remain.
**Fix:** give tests their own postmaster (row 124b/126c) — this removes the forced checkpoints from prod entirely. Interim: serialise CREATE/DROP behind a process-wide lock (row 126a, also fixes the crash), reap leaked `fs3_*` DBs, raise `max_wal_size`, enable `wal_compression`.
**Cost:** container/compose change. **Restart: YES for `max_wal_size`/`wal_compression`** (`max_wal_size` is actually SIGHUP-reloadable; `shared_buffers` is not). Separate test postmaster needs a new container, not a prod restart.
**Rows: 126, 110, 124.**

### #4 — Backends do 66.3% of their own page evictions; buffer pool turns over every 5.0 s
**Diffuse — inflates every one of the above rather than appearing as its own line.**
`shared_buffers = 128 MB` (16,384 buffers) against **~8.2 GB of hot relations**. `buffers_backend 2,095,127` vs `buffers_checkpoint 623,980` + `buffers_clean 442,141`. `buffers_alloc` 3,285/s. `work_mem = 4 MB` causes the search's `Materialize` to spill (`temp_bytes +42 MB in 65 s`).
**Fix:** `shared_buffers` 4 GB, `work_mem` 64 MB, `effective_cache_size` 16 GB, `maintenance_work_mem` 1 GB, `effective_io_concurrency` 200 (NVMe), `random_page_cost` 1.1. Container has 31.36 GiB available and uses 535 MiB.
**Cost:** trivial edit. **`shared_buffers` NEEDS RESTART** — o-prime to take to Jordan. Expect a solid win for #2 and for bulk ingest, and **only a modest one for search**, which is CPU-bound (see #1).
**Row: 122.**

### #5 — `jobs` has never been autovacuumed and is 6,549 dead tuples from triggering
**~0 CPU-s today; a latent spike.**
`n_dead_tup 196,719` vs a `203,268` threshold, `autovacuum_count = 0`, 170 MB bloat, and when it fires it will vacuum a **2,150 MB** relation with `maintenance_work_mem = 64 MB` across **five indexes** — multiple cleanup passes. Fixing #2(a) (purge `done` jobs) removes this problem rather than tuning around it.
**Cost:** covered by #2. **Restart: NO.** **Rows: 120, 122.**

### Not a cause — ruled out with evidence
- **Idle containers.** 7 × `hass-mcp` + `fs3-linuxtest` + buildkit: **0.00% CPU each**. Leave them.
- **OrbStack overhead itself.** Helper measured 0.5–1.1%; its larger historical numbers are postgres's own CPU and block I/O billed through the VM wrapper.
- **The HNSW index / `embeddings_1024_vector_idx`.** 12.4 ms and 1,078 buffers per search. It is fine.
- **`jobs_remaining()`.** 0.771 ms via index-only scan. Fine despite its stale comment.
- **Spread timed checkpoints (`write=135–158 s`).** That is `checkpoint_completion_target=0.9` working correctly, not a fault.

---

## NEEDS RESTART / NEEDS CHANGE — for o-prime to take to Jordan

| item | why | restart? |
|---|---|---|
| `shared_preload_libraries = 'pg_stat_statements'` + `CREATE EXTENSION` | I had to hand-roll a 1 s activity sampler to get section 2. Every future DB question pays this cost again. | **YES** |
| `shared_buffers` 128 MB → 4 GB | cause #4; container uses 535 MiB of 31 GiB | **YES** |
| `work_mem` 4 MB → 64 MB, `maintenance_work_mem` 64 MB → 1 GB, `effective_cache_size` → 16 GB, `effective_io_concurrency` → 200, `random_page_cost` → 1.1 | spills, vacuum passes, NVMe costing | reload/session |
| `max_wal_size` 1 GB → 8 GB, `wal_compression = on` | cause #3 FPI volume | SIGHUP reload |
| `track_io_timing = on` | `blk_read_time`/`blk_write_time` are both 0, so I could not attribute I/O time | SIGHUP reload |
| separate postmaster for tests | causes #3 and the row-126 crash both vanish | new container |

**I changed none of these.**

---

## One paragraph for Jordan

The Postgres in OrbStack is **not** what is eating your machine. Measured over twelve minutes, `flowspace3-db` used **0.51 cores on average and peaked at 2.1 of your 16** — about **3% of the box** — while host load sat between 28 and 76 with total CPU utilisation around 14%. Load that high with CPU that low means the load is **process-count and I/O-wait from the agent fleet**: 1,309 processes, 210 of them `node`, plus bun, tmux, rustc and the pij daemons. The idle containers you were suspicious of cost exactly **0.00%**. That said, the DB *is* burning far more CPU than its work justifies, and it is one query: **69% of all database CPU is the search admission filter**, which runs a `smart_content` index probe **962,792 times to return 40 rows** — while the HNSW vector index it is supposedly waiting on takes **12 milliseconds**. **The single highest-leverage change is rewriting that admission `EXISTS` in `crates/store/src/embeddings.rs` so it resolves `smart_content` once as a join instead of probing per candidate.** It is a code fix, it needs no restart, and it will do more for search latency than the `shared_buffers` bump in row 122 — which my `EXPLAIN` shows was aimed at the wrong target (99.7% buffer hit rate on the search path). Bump `shared_buffers` anyway, because it fixes the *second* problem: `GET /status` full-scans **892 MB of the jobs table off disk on three cores every six seconds**, because 1,009,934 of the table's 1,016,092 rows are completed jobs that nothing ever deletes. And the checkpoint storm in the log — one forced checkpoint every 23.6 seconds — is **our own test suite dropping databases on the production container**, which is exactly what rows 110 and 126 already say needs its own postmaster.

---

## Method notes / friction

Every number above cites the command that produced it; raw artefacts are in `.harness/temp/agent/dbprof/`. Dogfooded `flowspace3 search "how does the daemon poll the jobs table for work"` before grepping — it returned the right conversation turns **and took 15.6 s**, which became evidence for cause #1.

Friction captured with `harness observe` (**listed, not cleared** — the buffer is shared and the drain is o-prime-owned):
- **DL-003** — search plan pathology; no `pg_stat_statements` to see it, sampler hand-rolled.
- **DL-004** — `queue_depth` full scan + no `done`-job retention + stale doc comment.
- **DL-005** — forced-checkpoint storm from test-DB churn on the shared container.

Suggested encoding for DL-003: a `harness db profile` command that runs the activity sampler, the shape histogram, and the search `EXPLAIN` — so the next agent does not rebuild this from scratch.
