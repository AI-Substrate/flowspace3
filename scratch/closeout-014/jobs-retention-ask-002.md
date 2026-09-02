# jobs-retention stop-and-ask 002 — dedicated test postmaster host auth rejected

The first focused retry after o-prime's health receipt used the exact binding URL:

```text
FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test
cargo test -p fs3-store --test pg_jobs_retention -- --nocapture
```

All three tests reached Postgres but were rejected:

```text
no pg_hba.conf entry for host "192.168.97.1", user "flowspace3",
database "flowspace3_test", no encryption
```

No container command, `:5433` access, cleanup, or retry was attempted. DB-backed tests are stopped until the exact host URL authenticates. Failed run: `artifact://62`.
