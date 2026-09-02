# search-admission STOP-and-ask 004 — new test postmaster unreachable

The first focused run after CLEARED used only the ruled separate postmaster:

```text
FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test
CONTAINER=flowspace3-db-test
cargo test -p fs3-store search_plan_shape -- --nocapture
```

It failed before any DDL or EXPLAIN at `shape_database()` while connecting to the derived maintenance URL `postgres://flowspace3:flowspace3@127.0.0.1:5434/postgres`: `StoreError::Unreachable { source: PoolTimedOut }` after 5.02 seconds.

I did not retry, inspect/touch the container, or fall back to `:5433`. Please confirm when `flowspace3-db-test` is ready at the ruled URL, or provide corrected credentials/URL. The bounded test redesign is formatted and retained locally; no query ran on either postmaster in this attempt.
