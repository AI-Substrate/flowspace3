# fresh-db-serialise stop-and-ask 001 — shared Postgres shutting down

At the review-delta listing-example proof, the first read-only pre-count failed:

```text
docker exec flowspace3-db psql -U flowspace3 -d postgres -Atc "select count(*) from pg_database where datname like 'fs3_%'"
psql: error: connection to server on socket "/var/run/postgresql/.s.PGSQL.5432" failed:
FATAL: the database system is shutting down
```

I stopped immediately. I did not retry, restart compose, run the example, run a sweep, or issue another database command. The new safety predicate, store-level lock, N=8 create-path mutation proof, bad-password classifier, and example source are in the worktree but uncommitted. Last known PR head remains `05d7d87cdfeec4c08ad1b6bdc02bcdbb88c927f6`.

Please rule when the shared `:5433` server is safe to use again, or whether remaining non-database edits should continue while runtime proofs stay blocked.
