# daemon

```bash
flowspace3 daemon
```

Runs the indexer in the foreground: HTTP on `daemon.url`, the job queue, and
the workers that drain it. Stop it with Ctrl-C.

It lives inside the `flowspace3` binary rather than shipping separately — one
file to install, one version, and no way for a CLI and a daemon of different
vintages to meet.

## What it does at boot

1. Reads configuration from the same directory the CLI uses.
2. Refuses any `daemon.url` that is not loopback. The HTTP surface is local
   and unauthenticated, and it fronts an index of every repo on the machine, so
   binding `0.0.0.0` would publish that to the network. A typo is a startup
   failure, not a silent exposure.
3. Migrates the store, and refuses to serve if it cannot — a writer that cannot
   reach its own schema has nothing useful to do.
4. Requeues any job left `running` by a previous process that died holding it.
5. Starts the workers, then listens.

## Watching it work

At the default log filter it streams one line per completed job and a progress
summary every five seconds while work is in flight:

```text
INFO fs3_daemon::runner: done kind=scan_file subject=src/auth.rs ms=91
INFO fs3_daemon::runner: done kind=summarize subject=src/auth.rs::validate ms=612
INFO fs3_daemon::runner: done kind=embed subject=16 x raw ms=104
INFO fs3_daemon::runner: progress phase="working" scanned=18 scan_left=0 \
     summarized=54 summarize_left=44 embedded=61 embed_left=57 failed=0
```

Raise or lower it with `RUST_LOG` (`RUST_LOG=fs3_daemon=debug`). Payloads are
never logged: an embed job carries the text being embedded, and logging it
would put your source code in the log at volume.

## The queue

Three job kinds, all in one Postgres table: `scan_file`, `summarize`, `embed`.
Workers claim with `SKIP LOCKED`, so N workers take N different jobs and an LLM
call and an embedding call run at the same time.

`indexing.worker_concurrency` (default 4) sets how many are claimed at once.

A failed job is retried up to three times with backoff, but **only if the error
is retryable** — re-running a job whose cause is a missing API key costs three
times as much and fails three times. `flowspace3 status` reports failed jobs
with their last error.

## HTTP surface

`GET /health` · `POST /roots` · `GET /status` · `POST /scan` · `GET /search`.

Every route answers the same envelope the CLI prints, and the HTTP status is
derived from the error code, so an endpoint never chooses one. `/health` is the
only route that works against a stale schema — it is how a client decides the
daemon exists at all.
