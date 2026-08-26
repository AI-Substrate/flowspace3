<!-- GENERATED from `fs3_core::catalog` — do not hand-edit.
     Regenerate with `FS3_UPDATE_DOCS=1 cargo test -p fs3-core error_codes`. -->
# fs3 error codes

Every failure fs3 reports carries one of these codes and the `fix` beside it. The
registry is `crates/core/src/catalog.rs`; this page is emitted from it.

`retryable` means repeating the same request could succeed without a change — the
daemon's job runner reads it to choose between re-queueing and failing a row.

`status` is the HTTP status a daemon endpoint answers with, derived mechanically
from the code's own spelling (workshop 004 D4).

## CONFIG

### `FS3-E-CONFIG-INVALID`

config.toml or an FS3_* override names a key, value or provider that cannot work.

**Fix**: run `flowspace3 config show` to see the effective values and the layer each came from, then correct the field the message names.

| retryable | status |
| --- | --- |
| false | 400 |
### `FS3-E-CONFIG-PROVIDER-UNKNOWN`

A port or repo selected a provider instance that is not in the registry.

**Fix**: add the instance to config.toml (`[providers.<name>]` with a `kind`), or point the selection at one that exists — `flowspace3 config show` lists the configured names.

| retryable | status |
| --- | --- |
| false | 500 |

## STORE

### `FS3-E-STORE-UNAVAILABLE`

The Postgres + pgvector store did not answer.

**Fix**: if the stack is not running: `docker compose up -d` — then re-run. `flowspace3 doctor` diagnoses further.

| retryable | status |
| --- | --- |
| true | 503 |
### `FS3-E-STORE-DATABASE-MISSING`

The server is up but the configured database has never been created.

**Fix**: run `flowspace3 doctor` — it creates the database and applies every migration.

| retryable | status |
| --- | --- |
| false | 500 |
### `FS3-E-STORE-SCHEMA-STALE`

The database schema is older than the migrations embedded in this binary.

**Fix**: run `flowspace3 doctor` — it applies the missing migrations and reports the result.

| retryable | status |
| --- | --- |
| false | 500 |
### `FS3-E-STORE-QUERY-FAILED`

A store statement failed.

**Fix**: re-run once; if it repeats, `flowspace3 doctor` reports the store's state and the daemon log carries the statement that failed.

| retryable | status |
| --- | --- |
| true | 500 |

## GIT

### `FS3-E-GIT-NOT-A-WORKTREE`

The path is not inside a git worktree (a bare repository has nothing on disk to index).

**Fix**: point the command at a checkout — the directory containing the files you want indexed.

| retryable | status |
| --- | --- |
| false | 500 |

## SCAN

### `FS3-E-SCAN-ROOT-NOT-FOUND`

The root path does not exist, or is not a directory.

**Fix**: check the path — `flowspace3 add <path>` takes an existing directory, and a relative path is resolved against the daemon's working directory, so an absolute path is safer.

| retryable | status |
| --- | --- |
| false | 404 |
### `FS3-E-SCAN-ROOT-NOT-REGISTERED`

The worktree is not registered, so there is nothing to re-scan.

**Fix**: run `flowspace3 add <path>` first — `flowspace3 status` lists the roots that are registered.

| retryable | status |
| --- | --- |
| false | 500 |
### `FS3-E-SCAN-DISCOVERY-FAILED`

The discovery walk could not start.

**Fix**: check the `[scan]` section: a `force_include` or `exclude` glob that does not compile stops the walk before it begins.

| retryable | status |
| --- | --- |
| false | 500 |
### `FS3-E-SCAN-UNPARSEABLE`

tree-sitter could not produce a tree for a file it has a grammar for.

**Fix**: this is a defect, not a configuration problem — report the path and language; the file is skipped and the rest of the scan continues.

| retryable | status |
| --- | --- |
| false | 500 |

## PROVIDER

### `FS3-E-PROVIDER-FAILED`

The configured embedder or summarizer refused the call.

**Fix**: check credentials and deployment names for the selected instance (`flowspace3 config show`); `[providers.<name>] kind = "fake"` runs the whole stack offline while you fix it.

| retryable | status |
| --- | --- |
| true | 500 |
### `FS3-E-PROVIDER-DIMENSIONS`

The embedder returned vectors of a width no embeddings table holds.

**Fix**: select a model whose width matches the configured table, or add an `embeddings_<width>` migration for the new model before selecting it.

| retryable | status |
| --- | --- |
| false | 500 |

## QUEUE

### `FS3-E-QUEUE-JOB-FAILED`

A job failed every attempt and is now terminal.

**Fix**: `flowspace3 status` reports failed jobs with their last error; fix the cause and re-run `flowspace3 scan <path>` to re-enqueue the work.

| retryable | status |
| --- | --- |
| false | 500 |

## QUERY

### `FS3-E-QUERY-INVALID`

The search request is not valid — an empty query, or a filter outside its range.

**Fix**: check the flag the message names; `flowspace3 search --help` lists the accepted values.

| retryable | status |
| --- | --- |
| false | 400 |
### `FS3-E-QUERY-NO-INDEX`

No embeddings exist for the active model, so a semantic search has nothing to rank.

**Fix**: run `flowspace3 add <path>` and wait for `flowspace3 status` to report an empty queue, then search again.

| retryable | status |
| --- | --- |
| false | 500 |

## DAEMON

### `FS3-E-DAEMON-UNAVAILABLE`

The fs3 daemon did not answer on its configured URL.

**Fix**: start it with `fs3-daemon`, or run `flowspace3 doctor` to diagnose the stack. The CLI never starts infrastructure itself (PRD req 37).

| retryable | status |
| --- | --- |
| true | 503 |

## USAGE

### `FS3-E-USAGE-INVALID`

The command was called with arguments it cannot act on.

**Fix**: run the command with `--help`; the CLI exits 2 for usage problems, 1 for real failures.

| retryable | status |
| --- | --- |
| false | 400 |
