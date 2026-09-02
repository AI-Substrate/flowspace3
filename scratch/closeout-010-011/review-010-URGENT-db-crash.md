# URGENT — reviewer (pij-fiscal-tick) to o-prime: shared Postgres crashed into recovery at 22:48:35 UTC

**Read this before you finish characterising prod.** This is evidence for your
FREEZE, not a review finding. Volunteering it unasked because it lands inside
your investigation window.

## What happened

At **2026-09-01 22:48:35 UTC** the shared `flowspace3-db` container (host port
5433) **crashed and auto-recovered**. It was NOT shut down cleanly. Verbatim
from `docker logs flowspace3-db`:

```
2026-09-01 22:48:26.667 UTC [351853] LOG:  checkpoint starting: immediate force wait
2026-09-01 22:48:35.532 UTC [1] LOG:  server process (PID 3147563) exited with exit code 2
2026-09-01 22:48:35.532 UTC [1] LOG:  terminating any other active server processes
2026-09-01 22:48:35.709 UTC [1] LOG:  all server processes terminated; reinitializing
2026-09-01 22:48:51.578 UTC [3147586] LOG:  database system was not properly shut down; automatic recovery in progress
2026-09-01 22:48:55.387 UTC [3147587] LOG:  checkpoint complete: wrote 16183 buffers (98.8%) ... redo lsn=56/EF61498
2026-09-01 22:48:55.392 UTC [1] LOG:  database system is ready to accept connections
```

`postgres` was in recovery mode for roughly 20 seconds and every connection in
that window got `FATAL: the database system is in recovery mode` or
`expected to read 5 bytes, got 0 bytes at EOF`.

## My hand in it — full disclosure

I ran ONE `cargo test -p fs3-daemon --test oversize` at ~22:48:2x against my own
scratch DB (`FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/fs3_review_010`,
created by me). That test binary has 12 `#[tokio::test]`s and `FreshDatabase`
issues a `CREATE DATABASE` per test, so it fired ~12 concurrent
`CREATE DATABASE` at the shared server on a heavily loaded host. It crashed
mid-checkpoint. **I never pointed anything at prod and never at :7373.**

This is not a new failure mode for this box. The same crash shape is already in
the container's history against `CREATE DATABASE fs3_migrations_*`:

```
2026-08-27 10:03:29.426 UTC [1] LOG:  server process (PID 354230) was terminated by signal 6: Aborted
2026-08-27 10:03:29.426 UTC [1] DETAIL:  Failed process was running: CREATE DATABASE fs3_migrations_000000030001723818cfa15fa858a8e0
2026-08-28 07:09:09.448 UTC [1] LOG:  server process (PID 344971) exited with exit code 2
2026-08-28 07:12:37.310 UTC [1] LOG:  server process (PID 351762) exited with exit code 2
```

So: concurrent scratch-DB creation against this single shared instance has taken
it down at least four times, twice before tonight and once in front of me.

## Why it matters to YOUR freeze

Your sibling's guard tripped on **"prod migration version 22 -> absent"**. A
crash-and-recover of the instance that HOSTS prod is a candidate explanation for
a migration-version read coming back absent — a read taken during the ~20s
recovery window returns a connection error or an unqueryable server, which a
guard may render as "absent" rather than as "could not read". Before you
conclude a sibling's `cargo test --all` wrote to prod, please check whether the
guard's probe simply landed inside 22:48:35–22:48:55 UTC.

I have not inspected the prod database (out of my fence, and I would not while
you are characterising it). I can re-read container logs on request — that is
read-only and unaffected by the freeze.

## Freeze compliance

- No further `cargo test` and no `harness checks` until you clear it.
- Everything below was ALREADY COMPLETE before your freeze arrived, so no
  re-runs are needed for it:
  - `cargo test -p fs3-daemon --lib chunk_plan -- --nocapture` → **exit 0**,
    printed `oversized 7→10, request_whale 33→50, prod_20_872 1→2, total 41→62`.
  - `cargo test -p fs3-providers cap_rejection` → **exit 0**, 4 passed
    (2 in `openai_stub.rs`, 2 in `azure_openai_stub.rs`).
  - `cargo test -p fs3-daemon --test oversize -- --test-threads=2` → **exit 0**,
    **12 passed, 0 failed** (the serial re-run after recovery).
- **HELD, needs the freeze lifted**: the packet's i6(2) mutation check (remove
  the `Error::InputTooLong` arm from `embed_items`, prove the heal test goes RED,
  restore). It needs one `cargo test` invocation. Say the word and it is a
  ~2-minute job; I will run it with `--test-threads=2` against
  `fs3_review_010`, never the default parallelism that took the server down.
- I am continuing with read-only analysis and drafting the review ddoc. My
  verdict will ship with the mutation check explicitly marked
  UNVERIFIED-PENDING-FREEZE if you do not clear it in time.

## Recommendation for after the freeze (harness, not this PR)

Recorded as `DL-001` in `.harness/temp/agent/session-buffer.md` (captured, NOT
drained — the drain is yours):

1. `FreshDatabase` should serialise `CREATE`/`DROP DATABASE` behind a
   process-wide lock, or the daemon test binaries should cap `--test-threads`.
   A test helper that can kill the fleet's database is a harness defect.
2. `crates/testkit/src/fresh_database.rs:46`'s panic tells the agent
   "Start it with: docker compose up -d". That is the WRONG next action when the
   server is up and in recovery — and `docker compose up` on this box is the
   known container-name collision (row 110 family). It should distinguish
   "server closed the connection / in recovery" from "no server configured".

— pij-fiscal-tick (reviewer, plan 010), worktree
`/Users/jordanknight/substrate/flowspace/fs3-review-010`, detached at
`6377a1fe4b14bc27b7894bd3a997724a87763b7f`.
