# Scope cache

A scoped vector search reads its scope's sources from a cache instead of collecting them on every pass.

**Ruling:** Jordan, 2026-10-15 (exact ranking up to 150k sources, then the scope cache to win the speed back). Approved by o-prime under binding conditions (`fix7-review-oprime.md`).

## Why

Before ranking, a scoped search collects every source its scope admits:
- the scope's file blobs and their elements;
- its conversations' turns;
- the summaries of both.

On prod (2026-10-15) that took 0.36 s for a 13k-source code scope, 0.48 s for a 13k all-kinds scope, and 2.5 s cold for a 220k one. It was repeated on every candidate pass of every search, for a set that only changes when the index does.

## What

- **`scope_sources` / `scope_cache`** (migration 0032, UNLOGGED) hold each scope's collected sources, keyed on a hash of (repository, worktree, path, kinds).
- **`cached_scope`** runs before the first pass. It reads a key that is at most 10 minutes old, or rebuilds it. A search that finds another search building the key collects in full rather than wait.
- **The search statement** reads the cached set plus everything stamped after the key's cutoff:
  - paths added or re-pointed;
  - blobs that gained elements;
  - conversations with new turns or a new repository or worktree;
  - summaries written for a cached source.
- **Bypassed** for unscoped searches, conversation scopes, and ddoc filters (`id_kinds`, `ddoc_schema`), which read element fields rewritten in place.
- **Kill switch:** `[search] scope_cache = false` (or `FS3_SEARCH__SCOPE_CACHE=false`) collects every scope in full, as before.
- **Telemetry:** `meta.timing.scope_cache` (`hit` / `built` / `busy` / `bypassed`) and `scope_cache_ms`, also in the `search timing` log line.

## Never missing a source

A source missing from a scope is content the search can't find. So the cache is only ever a superset of the scope, and growth is read at once.

1. **Change stamps come from triggers, not from each writer.** Every write that can grow a scope stamps `changed_xid = pg_current_xact_id()` (the writing transaction, which assigns its xid if it has none yet). That covers writers not written yet too:
   - `elements`: insert, or a change of `kind`, `raw_hash` or `blob_sha`;
   - `worktree_files`: insert, or a `blob_sha` change. The watcher's every-row `last_seen` sync stamps nothing;
   - `smart_content`: insert, or a `text_hash` change;
   - `conversations`: insert, or a `repo_identity`/`worktree` change;
   - `turns`: insert, or a `blob_sha` change, which stamps the conversation.
2. **The cutoff is MVCC's own visibility rule.**
   - A key's cutoff is the xmin of the snapshot it was built from. Its delta reads every row stamped by that transaction or a later one, which is every transaction the build couldn't see.
   - There's no clock, so a write in flight, or not yet begun, is always read.
3. **Shrinking is bounded, not immediate.**
   - Sources leave a scope when a path is removed or re-pointed, a turn is restated or deleted, a conversation moves, or an element or summary is rewritten in place. Each departure is counted per repository in `scope_shrinks`, as an upper bound: two sources per element of the departing blob, two per turn.
   - A key rebuilds before its repositories' count reaches `SCOPE_SHRINK_REBUILD` (2,000) since its build, or after 10 minutes.
   - Admission re-checks every candidate against live provenance. Because a cached count can overstate the live scope by up to that bound, a cached scope within 2,000 above an exact-ranking limit is ranked exactly, which is never worse than the live scope's own path.
4. **Boot empties the cache after migrating**, so no cached scope outlives a schema change.

## Gotchas

- **A deduped turn reuses an existing element**, so an element-id delta misses it. The conversation stamp is what catches it.
- **Time stamps miss writes.**
  - A BEFORE trigger runs before the row gets an xid, so a first write isn't yet visible in `pg_stat_activity`.
  - `clock_timestamp()` steps back when the VM's clock does.
  - xid stamps against the snapshot xmin have neither problem. The independent review of #174 caught this.
- **Update-side triggers must be row triggers guarded by the columns that matter.** A statement trigger with transition tables copies every updated row, which would be all 175k paths of each watcher sync.
- **`upsert_conversation` anchors a conversation to whichever repository owns its worktree root.** A test fixture that gave every conversation one root silently put every repository's turns in one scope.
- **The migration waits at most 5 s for a lock** (`lock_timeout`). A boot behind a pg_dump or an open psql fails with that error instead of hanging.

## Verify

```bash
cargo test -p fs3-store --test pg_scope_cache   # parity after 10 kinds of change, fresh turn, in-flight and first writes, near-limit ranking, shrink bound, kill switch
cargo test -p fs3-daemon --test search_timing   # the setting reaches a scoped search
flowspace3 search "<question>" --json | jq .meta.timing.scope_cache
```

Each safeguard was proven necessary by mutation: removing any delta, stamp, the cutoff, the shrink count or the near-limit band fails a named step.

## Code

- `crates/store/migrations/0032_scope_cache.sql`
- `crates/store/src/scope_cache.rs` (`cached_scope`, `scope_key`, the build)
- `crates/store/src/embeddings.rs` (`scope_ctes!`, shared by the search and the build)
- `crates/core/src/config.rs` (`SearchConfig::scope_cache`)
