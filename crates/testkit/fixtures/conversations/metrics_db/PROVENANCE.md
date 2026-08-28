# metrics_db fixture provenance

```
source        /Users/agent/.git-ai/internal/metrics-db  ·  table `metrics`, event_kind=5
              (read-only URI; source is live, 4.2 GB + 47 MB uncheckpointed WAL at harvest time)
harvested     2026-08-28 by HarvestMetricsDb (pij-team fixture harvest, tk-c103)
records       100 rows (100 kept of 17250 event_kind=5 rows in source id range 929028..948627)
covers        coverage-table `metrics_db` row, in full:
              · BOTH dialects — claude mirror (72 rows, tool='claude') AND copilot
                event-stream (28 rows, tool='github-copilot-cli'; event name under
                `v."0".type`, model at `v."0".data.modelCall.model`)
              · 6 distinct external_session_id (>=2 required):
                a5a5588f-0979-439f-a1bf-ddf185a089c7 (claude main, 56 rows)
                agent-a01869bcb5e09448b            (claude subagent, 15 rows)
                222c2c9d-5798-48cf-9dbd-cd4a52324c53 (copilot, 26 rows)
                c5967bc2-f25c-438e-a23f-a61c15de973e / c800c9ff-86e7-4a5f-bdc3-f63517243af6 /
                1fe494c6-e5c5-4e46-a9b4-4691b9411c3c (foreign-repo negatives, 1 row each)
              · foreign-repo negative — ids 943197, 943232, 948060 name
                github.com/AI-Substrate/pij; `where event_json like '%flowspace3%'`
                returns 97 of 100, so repo-scoping is provable by exclusion
              recipe gotchas:
              1  one-line-per-content-block, merge by message.id — 7 shared-id groups,
                 e.g. ids 945058/945059/945060 share msg_011CeSucxDsJp3csFZeCfATW and
                 ids 929034/929035/929036 share msg_011CeQuu2bEwY2nrdkKgdoTa
              3  copilot dialect — ids 936664/936665/936666 carry
                 `v."0".data.modelCall.model` = "gpt-5.4-nano" while id 948627
                 (`v."0".type` = "session.shutdown", same session 222c2c9d)
                 reports `data.currentModel` = "gemini-3.7-flash": the seat label lies
              5  compaction — id 945255, `v."0".type` = "user",
                 `isCompactSummary` = true, body opens "This session is being continued
                 from a previous conversation that ran out of context."
              6  subagent linkage — the 15 `agent-a01869bcb5e09448b` rows carry
                 external_parent_session_id = a5a5588f-0979-439f-a1bf-ddf185a089c7
              8  machine-wide store, repo-scope every query — see foreign-repo negative above
              cursor behaviour (recipe §3): rowid == id for all 100 rows, strictly
              increasing 929028..948627; event_ts is second-grain and NON-unique
              (17 timestamps carry >1 row), so only rowid is a safe cursor
              also present: 11 tool_use<->tool_result pairs joinable on
              external_tool_use_id without parsing JSON; 12 bookkeeping record types
              ingestion must skip by allowlist (mode, permission-mode, queue-operation,
              attachment, file-history-delta, file-history-snapshot, last-prompt,
              custom-title, agent-name, atis-latch, pr-link, system)
sanitised     home-path rewrite: 175 · body caps: 19 · credential redactions: 0
notes         · Rows were copied column-for-column; the `metrics` and `schema_metadata`
                DDL and all 5 `metrics` indexes are byte-identical to the source
                sqlite_master text. VACUUMed, journal_mode=DELETE: one file, no -wal/-shm.
                `pragma integrity_check` = ok. 319488 bytes, 217483 bytes of event_json.
              · event_json was NEVER parsed and re-serialised. The home rewrite and the
                body cap are in-place substitutions on the raw record text; every one of
                the 100 records still parses (checked) with original key order and spacing.
              · Body cap: 14 rows contain 19 capped string values, each cut at a whole
                character / whole escape-sequence boundary at <= 2048 bytes and suffixed
                `…[fixture-truncated]`. The store writes raw UTF-8 (no \u escapes, no \/
                escapes), so the suffix is raw UTF-8 too.
              · Credential grep (spec rule 4, all 14 patterns) over the finished fixture:
                428 raw hits — 427 for `token`, 1 for `secret`. Every one was classified:
                the 427 are token-COUNT telemetry identifiers (input_tokens,
                cache_read_input_tokens, tokenCount, token_type, tokenizer, …) and the
                one `secret` is the English word inside prose about repo hygiene
                (id 945066). Zero string-valued credential keys exist in the fixture
                (checked with `"…token…":"` ). Redactions applied: 0. Checked, clean.
              · Extra rewrite beyond spec rule 2: after the two path rewrites, 10
                occurrences of the bare unix owner name (the leaf of the old home path)
                remained inside `ls -l` tool output (ids 929037, 929041) and were
                rewritten to `agent`. Nothing in the coverage depends on that name; a
                grep of this whole directory for the old owner name now returns zero.
              · Retained, NOT redacted, because they match none of rule 4's patterns and
                are real store shape: claude thinking-block `signature` blobs (7, up to
                2068 chars, so some are capped) and copilot
                `v."0".data.modelCall.api_id` (424 chars, 3 rows). They are opaque
                per-call identifiers, not authentication material — flagged here so a
                reader can rule otherwise without re-deriving them.
              · Foreign-repo rows are the spec rule 5 exception and carry no prose from
                that project: 943232 is a claude `mode` record (282 bytes, mode + sessionId),
                943197 a copilot `hook.end` (556 bytes, ids + hookType), 948060 a copilot
                `session.permissions_changed` (521 bytes, mode + previousMode).
              · schema_metadata is included because the source has it and it pins the
                store's schema `version` = 5 (its other row, metrics_last_prune_ts, is
                the store's own prune watermark — this db self-prunes).
```

## Row counts per dialect

```
claude              72   (claude mirror dialect: native record at v."0", type user /
                          assistant / bookkeeping; 56 main session + 15 subagent +
                          1 foreign-repo negative)
github-copilot-cli  28   (copilot event-stream dialect: event name under v."0".type,
                          per-call model at v."0".data.modelCall.model;
                          26 flowspace3 + 2 foreign-repo negatives)
--------------------------
total              100   all event_kind = 5; rowid range 929028..948627, rowid == id
                          for every row (no renumbering — rowid IS the reader's cursor)
```

## `.schema metrics` of this fixture file

Byte-identical to the source store's `sqlite_master` text for the table and all five
indexes; reproduced here so a reader can diff without opening the file.

```sql
CREATE TABLE metrics (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        event_json TEXT NOT NULL
    , delivered_ts INTEGER, attempts INTEGER NOT NULL DEFAULT 0, last_sync_error TEXT, last_sync_at INTEGER, next_retry_at INTEGER NOT NULL DEFAULT 0, processing_started_at INTEGER, event_ts INTEGER DEFAULT NULL, event_kind INTEGER DEFAULT NULL, trace_id TEXT DEFAULT NULL, session_id TEXT DEFAULT NULL, parent_session_id TEXT DEFAULT NULL, tool TEXT DEFAULT NULL, external_session_id TEXT DEFAULT NULL, external_parent_session_id TEXT DEFAULT NULL, external_event_id TEXT DEFAULT NULL, external_parent_event_id TEXT DEFAULT NULL, external_tool_use_id TEXT DEFAULT NULL);
CREATE INDEX metrics_processing_started_at
        ON metrics (processing_started_at)
        WHERE delivered_ts IS NULL AND processing_started_at IS NOT NULL;
CREATE INDEX metrics_event_ts_kind
        ON metrics (event_ts, event_kind, id)
        WHERE event_ts IS NOT NULL AND event_kind IS NOT NULL;
CREATE INDEX metrics_session_kind_ts
        ON metrics (session_id, event_kind, event_ts, id)
        WHERE session_id IS NOT NULL
            AND event_kind IS NOT NULL
            AND event_ts IS NOT NULL;
CREATE INDEX metrics_parent_session_kind_ts
        ON metrics (parent_session_id, event_kind, event_ts, id)
        WHERE parent_session_id IS NOT NULL
            AND event_kind IS NOT NULL
            AND event_ts IS NOT NULL;
CREATE INDEX metrics_retryable
        ON metrics (next_retry_at ASC, id DESC)
        WHERE delivered_ts IS NULL
            AND processing_started_at IS NULL
            AND attempts < 6;
```

`id` is `INTEGER PRIMARY KEY`, therefore an alias for `rowid`: the recipe's cursor query
`where rowid > :cursor and external_session_id = … order by rowid` runs unchanged, and
`metrics_event_ts_kind` / `metrics_session_kind_ts` / `metrics_parent_session_kind_ts`
are the indexes a reader can rely on. `schema_metadata` (version = 5) is also present.
