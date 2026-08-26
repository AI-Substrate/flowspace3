# Workshop 002 — PG schema: content layer, ref layer, job backlog
**Type**: Storage Design + Data Model · **Date**: 2026-08-26 · **Author**: o-prime (from Jordan-agreed decisions in session) · **Status**: AUTHORITATIVE once Jordan signs off; feeds the daemon/store plan
**Home note**: lives in `docs/plans/prd/workshops/` (cross-plan home) because its consumer plan doesn't exist yet; deliberately deferred from plan 001.

## The frame

Three layers, already agreed in principle (this workshop pins the tables): **ref layer** (repos/worktrees → blob sets, cheap pointers) · **content layer** (blob-addressed elements + content-addressed enrichment, shared across branches/repos) · **job backlog** (the daemon's PG-backed work list — locked direction, `docs/plans/prd/daemon-worker-architecture.md`).

Config is NOT in the DB (files in `~/.config/flowspace3/` — DB is data only). Conversations (PRD 24–27) get their own workshop/plan — tables deliberately absent here.

## Tables (DDL sketch — lands as migrations 0002+)

```sql
-- ═══ REF LAYER ═══════════════════════════════════════════════
CREATE TABLE repos (
  id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  identity    TEXT NOT NULL UNIQUE,      -- git remote URL (req 35); fallback: path-derived id
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE worktrees (
  id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  repo_id     BIGINT NOT NULL REFERENCES repos(id),
  root_path   TEXT NOT NULL,             -- absolute host path
  ref_name    TEXT,                      -- branch/ref when known
  added_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (repo_id, root_path)
);

CREATE TABLE worktree_files (            -- the ref layer's heart: path → blob, per worktree
  worktree_id BIGINT NOT NULL REFERENCES worktrees(id) ON DELETE CASCADE,
  path        TEXT   NOT NULL,           -- relative to root_path
  blob_sha    TEXT   NOT NULL,           -- git blob id (untracked files hashed identically)
  last_seen   TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (worktree_id, path)
);
CREATE INDEX ON worktree_files (blob_sha);

-- ═══ CONTENT LAYER (shared, content-addressed) ═══════════════
-- elements: the parsed tree per blob. Parse is cheap → keyed by (blob_sha, parser_version).
CREATE TABLE elements (
  id             BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  blob_sha       TEXT NOT NULL,
  parser_version TEXT NOT NULL,
  parent_id      BIGINT REFERENCES elements(id) ON DELETE CASCADE,  -- NULL = file root
  kind           TEXT NOT NULL CHECK (kind IN ('file','container','function','section')),
  subkind        TEXT NOT NULL DEFAULT '',
  name           TEXT NOT NULL,
  address        TEXT NOT NULL,          -- src/foo.rs::Indexer::scan — stable identity
  span_start     INT  NOT NULL,
  span_end       INT  NOT NULL CHECK (span_end >= span_start),
  sibling_order  INT  NOT NULL,
  raw_text       TEXT NOT NULL,
  raw_hash       TEXT NOT NULL,          -- sha256(raw_text) — THE dirtiness/enrichment key
  enrich         BOOLEAN NOT NULL,       -- scanner's injected-policy verdict (size threshold etc.)
  UNIQUE (blob_sha, parser_version, address)
);
CREATE INDEX ON elements (raw_hash);
CREATE INDEX ON elements (blob_sha, parser_version);

-- smart content: fs2's LLM layer, content-addressed — dedupes across branches/repos/worktrees.
CREATE TABLE smart_content (
  raw_hash    TEXT NOT NULL,
  model_key   TEXT NOT NULL,             -- "<model>@<prompt_version>" from the config registry
  text        TEXT NOT NULL,
  text_hash   TEXT NOT NULL,             -- sha256(text) — joins embeddings(source_kind='smart') back to here (sylac fix, 2026-08-26)
  tags        TEXT[] NOT NULL CHECK (cardinality(tags) BETWEEN 1 AND 5),  -- req 36
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (raw_hash, model_key)
);

-- embeddings: one row per embedded text (raw OR smart), per model.
CREATE TABLE embeddings_1024 (           -- ⚠ dim-suffixed strategy — see Decision D3
  source_hash TEXT NOT NULL,             -- raw_hash, or sha256(smart text)
  source_kind TEXT NOT NULL CHECK (source_kind IN ('raw','smart')),
  model_key   TEXT NOT NULL,
  vector      vector(1024) NOT NULL,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (source_hash, source_kind, model_key)
);
CREATE INDEX ON embeddings_1024 USING hnsw (vector vector_cosine_ops);

-- ═══ JOB BACKLOG (locked direction) ══════════════════════════
CREATE TABLE jobs (
  id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  kind        TEXT NOT NULL,             -- 'scan_file' first; 'summarize','embed' siblings
  dedupe_key  TEXT NOT NULL,             -- e.g. 'scan:wt42:src/foo.rs' — idempotent enqueue
  payload     JSONB NOT NULL,
  state       TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','running','done','failed')),
  priority    INT  NOT NULL DEFAULT 0,
  not_before  TIMESTAMPTZ NOT NULL DEFAULT now(),   -- the 10s debounce lives HERE
  attempts    INT  NOT NULL DEFAULT 0,
  last_error  TEXT,
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX ON jobs (dedupe_key) WHERE state IN ('pending','running');
CREATE INDEX ON jobs (state, not_before, priority DESC);
```

## Decisions (with the road not taken)

| # | Decision | Rejected alternative | Why |
|---|---|---|---|
| D1 | Dirty-file tracking IS the jobs table (`scan_file` + `not_before` debounce + partial-unique dedupe) | separate `dirty_files` table | one mechanism, watcher re-fires just bump `not_before`; duplicate enqueues are no-ops via dedupe_key |
| D2 | Enrichment is content-addressed by `raw_hash`, never `element_id` | per-element enrichment rows | same method on 40 branches enriches ONCE; "dirty" = missing row, never a stored flag; model bump = new `model_key`, old rows untouched (instant rollback) |
| D3 | Per-dimension embedding tables (`embeddings_1024`, …), created by migration when config first names a model of that dim | single table, untyped `vector` | HNSW indexes require a typed dimension; per-dim tables keep index+query honest. Daemon (single writer) owns table creation via its migration path |
| D4 | Worker claims jobs with `FOR UPDATE SKIP LOCKED` | advisory locks / external queue | the boring, proven PG pattern; parallelizes workers for free (LLM + embedding jobs run concurrently — the fs2 property Jordan wants kept) |
| D5 | `elements.enrich` records the scanner's injected-policy verdict | policy re-evaluated at queue time | policy lives in ONE place (scanner settings); queue/backfill just reads the flag |
| D6 | Reconciler sweep derives missing work ("elements where enrich AND no smart_content row for current model_key") and enqueues it | trust the queue alone | self-healing: crashes, model changes, and policy changes all converge without manual replay |
| D7 | No FK from `smart_content`/`embeddings_*` to `elements` | strict FKs | content outlives any one parse (parser_version bumps re-mint element rows); GC is explicit (D8), not cascade |
| D9 | `smart_content.text_hash` column (indexed) makes smart-embedding hits resolvable back to their element | redefining smart `source_hash` as `raw_hash` | preserves source_hash = hash-of-embedded-text invariant; found by schema worker during implementation |
| D8 | GC = a `prune` job kind, later plan: blobs unreferenced by any `worktree_files` row → their elements; enrichment pruned only by explicit model retirement | ON DELETE CASCADE chains | enrichment is the expensive asset — never let a worktree removal cascade into re-payable LLM spend |

## Key flows (words, one line each)

- **Watcher fires** → upsert `jobs(scan:wt:path, not_before=now()+10s)` — re-fires push `not_before` out (debounce in SQL).
- **scan_file job** → git-blob layer resolves blob_sha → skip if `elements` already has (blob_sha, parser_version) → else pure scan → upsert element tree + `worktree_files` row → enqueue `summarize`/`embed` for enrich-marked elements missing rows.
- **Search** → embed query → HNSW over `embeddings_<dim>` → join `smart_content` + `elements` → resolve to live paths via `worktree_files`.

## Open questions (Jordan)

1. D3 accepted, or prefer single-model-per-deployment v1 (one `embeddings` table, dim fixed by config at init; multi-model = later migration)?
2. Raw-text storage: `elements.raw_text` inline (simple, DB grows with code size) vs fetch-from-git-on-demand (lean DB, needs repo access at query time). Sketch assumes inline — fs2 precedent.
3. GC timing: fine to defer the `prune` job to a later plan (sketch assumes yes)?
