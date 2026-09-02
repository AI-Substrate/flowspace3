# search-admission STOP-and-ask 005 — test postmaster blocks per-run database isolation

The cleared `:5434/flowspace3_test` endpoint is reachable, but the repository's isolation contract derives a maintenance URL on database `postgres` before creating a per-run child database. The focused test failed there before DDL/query execution:

```text
SQLSTATE 28000: no pg_hba.conf entry for host "192.168.97.1",
user "flowspace3", database "postgres", no encryption
```

Command used only the ruled environment (`:5434`, `CONTAINER=flowspace3-db-test`). No fallback, retry, container change, DDL, or EXPLAIN occurred.

Please choose:

1. **Recommended:** allow the `flowspace3` test user to connect to the `postgres` maintenance database on `flowspace3-db-test`, then prove CREATE/DROP of a scratch database; this makes existing `FreshDatabase` helpers and the later full suite work unchanged.
2. Provide an approved maintenance URL/database for per-run child creation.
3. Explicitly rule that this test may create a child while connected to `flowspace3_test`; that works for this helper but does not repair existing suite helpers using `maintenance_url`.

No further test/DDL work until the isolation path is ruled ready.
