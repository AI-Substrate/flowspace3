# Table maintenance

The big tables (`elements`, `embeddings_1024`, `turns`, `smart_content`, `worktree_files`, `jobs`) are kept vacuumed and analyzed.

**Ruling:** Jordan, 2026-10-14, relayed by o-prime.

## Why

Postgres's default autovacuum thresholds fire at 20% dead rows and 10% changed rows. On a million-row table that never happened. On 2026-10-14, prod's `elements` table had:
- 245k dead rows;
- never been vacuumed;
- last been analyzed in the middle of a backfill.

Its stale row estimates did two kinds of damage:
- **Backfill probe:** they turned a one-row queue probe into a sequential scan every five seconds. It became the costliest statement in the database, with 1.2 billion blocks read, until #159.
- **Keyword leg:** they set the keyword leg's estimate to 8 rows against 10,623 real ones.

## What

1. **Migration 0031:** per-table autovacuum reloptions on the watched tables. Autovacuum fires at 2% dead and 1% changed, and at 5% inserted on the insert-only tables.
   - `embeddings_1024` is the exception: it vacuums at 10%, because its vacuum also cleans the 13 GB HNSW index (the one-off prod vacuum took 50 minutes). It still analyzes at 1%.
   - `embeddings_1024` and `elements` vacuum with a 10 ms cost delay, so a long vacuum trickles beside live searches instead of spiking.
2. **`table-analyze` reconciler:** on the shared 5 s cadence, while the job queue is idle, it ANALYZEs any watched table where at least 1% of rows (and at least 10,000 rows) changed since its last ANALYZE. So a backfill, a restatement or a big import is followed by fresh statistics as soon as it finishes, without waiting for an autovacuum worker.
3. **Doctor `tables` row:** it warns when a table with at least 100k rows has more than 10% dead rows, has had more than 10% of its rows changed since its last ANALYZE, or has never been analyzed. It names the `VACUUM (ANALYZE)` to run in a quiet moment, and reports without repairing.
4. **Query shapes that hold whatever the statistics say:** for example, #159's `ORDER BY id` keeps the backfill probe on its partial index.

## Verify

```bash
flowspace3 doctor                                   # the `tables` row
cargo test -p fs3-store --test pg_maintenance       # reloptions applied; health read; ANALYZE guarded
```

## Code

- `crates/store/migrations/0031_autovacuum_thresholds.sql`
- `crates/store/src/maintenance.rs` (`table_health`, `analyze_table`, `WATCHED_TABLES`)
- `crates/daemon/src/table_analyze.rs`
- `crates/cli/src/doctor.rs` (`tables_row`)
