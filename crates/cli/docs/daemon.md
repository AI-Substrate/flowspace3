# daemon

```bash
flowspace3 daemon
```

Runs the indexer in the foreground: HTTP on `daemon.url`, the job queue, and
the workers that drain it. Stop it with Ctrl-C.

It lives inside the `flowspace3` binary rather than shipping separately — one
file to install, one version, and no way for a CLI and a daemon of different
vintages to meet.

## Safe hand-run isolation

Use the built-in sandbox for development, demonstrations, and manual checks:

```bash
flowspace3 daemon --sandbox
```

It creates and migrates a uniquely named database beside the configured one,
loads daemon configuration only from its new process-owned directory, forces
embedder, summarizer, and agent surfaces to the offline fake, reserves an
ephemeral loopback port, and owns its credential and log directories. Ambient
per-surface selections never reach wiring. Its ready line is emitted only after
provider wiring, store migration, listener bind, and atomic key publication all
succeed:

```text
sandbox=true embedder=fake summarizer=fake db=fs3_sandbox_<unique> port=<n> config=<dir>
```

SIGINT and SIGTERM both stop dequeueing immediately, finish only jobs already
in flight, then drop the database. A second signal cancels the remaining
in-flight jobs but still unwinds through database cleanup. Every exit reports
whether the database was dropped; a failed drop names it and prints the
`docker exec flowspace3-db psql ...` cleanup command. Point a client at the
printed port and set `FS3_CONFIG_DIR` to the printed directory so it reads this
sandbox's bearer key.

Real providers over a real, read-only index are a separate capability: the
daemon must mechanically disable add, scan, enrichment, reconciliation, and
every other write path before such a mode can honestly claim safety. That
read-live posture will use a sibling sandbox flag; bare `--sandbox` keeps this
complete fake-provider meaning.

### Appendix: manual overrides

For diagnosis of the sandbox implementation itself, the old manual recipe is:
an empty `FS3_CONFIG_DIR`, a disposable `FS3_DATABASE__URL`, a unique
`FS3_DAEMON__URL`, and both provider selections forced to `fake`. The caller
must create/drop the database and choose the port; normal manual work should
not reproduce these steps—use `--sandbox`.

## What it does at boot

1. Reads configuration from the directory the CLI uses.
2. Stages a fresh 256-bit bearer key beside `daemon.key` with mode `0600`; the
   published key remains unchanged at this point.
3. Refuses any `daemon.url` that is not loopback. Authentication is still
   defense in depth for a local surface that fronts every indexed repo; a typo
   must never publish it to another interface.
4. Wires providers, migrates the store, and performs boot recovery.
5. Binds the listener. A bind failure discards the staged key without touching
   the credential used by an existing daemon.
6. Atomically publishes the staged key, starts workers, then starts the accept
   loop. Publication is after bind but before the first request can be served.

## Watching it work

At the default log filter it streams one line per completed job and a progress
summary every five seconds while work is in flight:

```text
INFO fs3_daemon::runner: done kind=scan_file subject=src/auth.rs ms=91
INFO fs3_daemon::runner: done kind=summarize subject=src/auth.rs::validate ms=612
INFO fs3_daemon::runner: done kind=embed subject=16 x raw ms=104
INFO fs3_daemon::runner: progress phase="working" scan_left=0 \
     summarize_left=44 embed_left=57 failed=0
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
times as much and fails three times. Ordinary `flowspace3 status` counts only
failed non-terminal work in the live queue; terminal failure counts appear only
with `--history`. The most recent failure remains visible as `last error` in
either mode.

Completed jobs are retained for `indexing.job_retention_days` (default 1), then
the daemon purges them in bounded batches at boot and hourly. Ordinary
`flowspace3 status` reads only live rows; `flowspace3 status --history` requests
the bounded completed history explicitly.

## HTTP surface

`GET /health` · `POST /roots` · `GET /status` · `POST /scan` · `GET /search`.

Every request, including `/health`, must send the current file as
`Authorization: Bearer <key>`. The CLI does this automatically. A missing or
stale key receives a `401` `FS3-E-DAEMON-UNAUTHORIZED` envelope whose
`next_action` names the resolved key path and daemon restart.

Every route answers the same envelope the CLI prints, and the HTTP status is
derived from the error code, so an endpoint never chooses one. `/health` is the
only route that works against a stale schema — it is how a client decides the
daemon exists at all.
