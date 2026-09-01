<!-- GENERATED from `fs3_core::catalog` — do not hand-edit.
     Regenerate with `FS3_UPDATE_DOCS=1 cargo test -p fs3-core --test error_codes`. -->
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

### `FS3-E-PROVIDER-CANNOT-ANSWER`

The agent port is configured with a provider that cannot answer questions.

**Fix**: point `[agent] active` at a real chat deployment (`flowspace3 config show` names the current one, `flowspace3 docs get providers` sets one up). The offline `fake` runs the rest of the stack without keys, but it cannot answer a question.

| retryable | status |
| --- | --- |
| false | 500 |
### `FS3-E-PROVIDER-FAILED`

The configured embedder or summarizer refused the call.

**Fix**: check credentials and deployment names for the selected instance (`flowspace3 config show`); `[providers.<name>] kind = "fake"` runs the whole stack offline while you fix it.

| retryable | status |
| --- | --- |
| true | 500 |
### `FS3-E-PROVIDER-RATE-LIMITED`

The provider is rate limiting us.

**Fix**: nothing, usually — the job is parked and retried on the service's own schedule. If it persists, lower `worker_concurrency` or raise the deployment's quota; `flowspace3 doctor` names the active instance.

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
### `FS3-E-QUERY-ASK-ITERATION-LIMIT`

Ask reached its iteration limit before producing an answer.

**Fix**: ask a narrower question or raise `[agent] max_iterations` in config.toml.

| retryable | status |
| --- | --- |
| false | 500 |
### `FS3-E-QUERY-ASK-TOKEN-BUDGET`

Ask exhausted its token budget before producing an answer.

**Fix**: ask a narrower question or raise `[agent] token_budget` in config.toml.

| retryable | status |
| --- | --- |
| false | 500 |
### `FS3-E-QUERY-NO-INDEX`

No embeddings exist for the active model, so a semantic search has nothing to rank.

**Fix**: run `flowspace3 add <path>` and wait for `flowspace3 status` to report an empty queue, then search again.

| retryable | status |
| --- | --- |
| false | 500 |
### `FS3-E-QUERY-CONVERSATION-NOT-INDEXED`

The derived conversation is absent from the index or has not delivered any turns.

**Fix**: run `flowspace3 conversation ingest` for the session, then wait for the queue to drain and verify again.

| retryable | status |
| --- | --- |
| false | 500 |
### `FS3-E-QUERY-NOT-FOUND`

No repository, file or element in the index answers to the address that was asked for.

**Fix**: check the address against a search hit — `flowspace3 search "<question>"` prints the address of everything it returns, and `flowspace3 tree <repo-or-path>` lists what is actually indexed under a path.

| retryable | status |
| --- | --- |
| false | 404 |
### `FS3-E-QUERY-INVALID-ADDRESS`

The address does not parse: it must be `el:<repo>/<path>::<name>` or `conv:<guid>`.

**Fix**: copy the `address` field from a search hit rather than composing one by hand; `flowspace3 search "<question>"` prints it for every result.

| retryable | status |
| --- | --- |
| false | 400 |
### `FS3-E-QUERY-INVALID-AMBIGUOUS`

The address matches more than one element or repository, so there is no single answer.

**Fix**: narrow it: `--span <line>` picks one of several elements sharing an address (the candidates are listed in `details`), and `--repo <identity>` picks one repository.

| retryable | status |
| --- | --- |
| false | 400 |
### `FS3-E-QUERY-NOT-IMPLEMENTED`

The request is valid but names something this build does not implement yet.

**Fix**: nothing to fix in your request — the message names what is missing. `flowspace3 docs list` describes what this version does answer, and `flowspace3 doctor upgrade` installs a newer one.

| retryable | status |
| --- | --- |
| false | 501 |

## DAEMON

### `FS3-E-DAEMON-UNAVAILABLE`

The fs3 daemon did not answer on its configured URL.

**Fix**: start it with `flowspace3 daemon &`, or run `flowspace3 doctor` to diagnose the stack. Doctor reports the daemon but never starts one — a diagnostic command must not leave a process running that you did not ask for.

| retryable | status |
| --- | --- |
| true | 503 |
### `FS3-E-DAEMON-UNAUTHORIZED`

The request did not present the bearer key generated by the running fs3 daemon.

**Fix**: read daemon.key from the resolved fs3 config directory and send it as `Authorization: Bearer <key>`; if the file is missing or stale, restart the daemon so it republishes it.

| retryable | status |
| --- | --- |
| false | 401 |

## UPDATE

### `FS3-E-UPDATE-UNREACHABLE`

The published release list could not be read, so there is nothing to compare against.

**Fix**: check network access to https://github.com/AI-Substrate/flowspace3/releases and try again; the daemon retries on its own schedule, so a transient outage needs no action.

| retryable | status |
| --- | --- |
| true | 500 |
### `FS3-E-UPDATE-NO-INSTALL-PATH`

This process cannot resolve its own executable, so there is no binary to replace.

**Fix**: reinstall instead: `curl -fsSL https://raw.githubusercontent.com/AI-Substrate/flowspace3/main/install.sh | sh`.

| retryable | status |
| --- | --- |
| false | 500 |

## USAGE

### `FS3-E-USAGE-INVALID`

The command was called with arguments it cannot act on.

**Fix**: run the command with `--help`; the CLI exits 2 for usage problems, 1 for real failures.

| retryable | status |
| --- | --- |
| false | 400 |
### `FS3-E-USAGE-TOPIC-NOT-FOUND`

The requested documentation topic is not bundled in this binary.

**Fix**: run `flowspace3 docs list` to see the topics this binary carries; the set is fixed at build time, so a topic that is not listed does not exist in this version.

| retryable | status |
| --- | --- |
| false | 404 |
