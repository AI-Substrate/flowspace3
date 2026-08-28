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
forces both providers to the offline fake regardless of ambient configuration,
reserves an ephemeral loopback port, and uses process-owned credential and log
directories. Its first line names the complete posture:
The configured database itself need not exist; only its Postgres server and
credentials are reused to create the child through the maintenance database.

```text
sandbox=true embedder=fake summarizer=fake db=fs3_sandbox_<unique> port=<n> config=<dir>
```

Ctrl-C is a clean shutdown and drops the database. Point a client at the
printed port and set `FS3_CONFIG_DIR` to the printed directory so it reads this
sandbox's bearer key. A killed process cannot run async cleanup, so the same
boot line deliberately leaves the unique database name visible for manual
removal.

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

1. Reads configuration from the same directory the CLI uses.
2. Generates a fresh 256-bit bearer key and atomically publishes it as
   `daemon.key` in that directory with mode `0600`, before binding any socket.
3. Refuses any `daemon.url` that is not loopback. Authentication is still
   defense in depth for a local surface that fronts every indexed repo; a typo
   must never publish it to another interface.
4. Migrates the store, and refuses to serve if it cannot — a writer that cannot
   reach its own schema has nothing useful to do.
5. Requeues any job left `running` by a previous process that died holding it.
6. Starts the workers, then listens.

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

Every request, including `/health`, must send the current file as
`Authorization: Bearer <key>`. The CLI does this automatically. A missing or
stale key receives a `401` `FS3-E-DAEMON-UNAUTHORIZED` envelope whose
`next_action` names the resolved key path and daemon restart.

Every route answers the same envelope the CLI prints, and the HTTP status is
derived from the error code, so an endpoint never chooses one. `/health` is the
only route that works against a stale schema — it is how a client decides the
daemon exists at all.
