# flowspace3

[![ci](https://github.com/AI-Substrate/flowspace3/actions/workflows/ci.yml/badge.svg)](https://github.com/AI-Substrate/flowspace3/actions/workflows/ci.yml)
Semantic code search over a central index. A codebase is split into
elements — functions, types, markdown sections — each summarized, embedded, and
searchable by meaning, text, or regex, across every repo on the machine at once.

This is `fs3`: a Rust workspace, a Postgres+pgvector store, a background daemon,
and the `flowspace3` CLI.

> **Status: first light.** The pipeline is wired end to end — `add` a repo, the
> daemon scans it, summarises and embeds it, and `search` answers by meaning.
> Offline by default, with no keys. The file watcher, `get`/`tree`, text and
> regex modes, and conversations land in later plans.

## Install

**Option A — convenience script** (macOS or Linux):

```bash
curl -fsSL https://raw.githubusercontent.com/AI-Substrate/flowspace3/main/install.sh | sh
```

Installs to `/usr/local/bin` when permitted, otherwise `~/.local/bin`.

**Option B — direct download**: take `flowspace3-<your-triple>` from the
[latest release](https://github.com/AI-Substrate/flowspace3/releases/latest),
`chmod +x` it, and put it on your `PATH`. Published triples:
`aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`,
`x86_64-unknown-linux-gnu`.

Windows: `install.ps1` ships in the repo but is **not yet validated**.

**Releases**: a rolling release PR (`chore(main): release x.y.z`) is kept up
to date automatically; merging it tags the semver version and publishes.
Conventional commit subjects (`feat:`, `fix:`, …) are binding on main — they
are what drives the version.

Building from source instead: `cargo build --release -p fs3-cli` (Rust 1.95+,
edition 2024).

## First run

One binary does everything: `flowspace3`. No config file, no API keys — the
default providers are `fake`, which is a **legal offline runtime**, not a
stub: real embeddings-shaped work, deterministic answers.

```bash
# 1. Set up the world. `doctor` walks engine -> stack -> database -> schema
#    and REPAIRS as it goes: it starts the Docker compose stack, creates the
#    database, applies migrations. There is no second setup command.
flowspace3 doctor

# 2. Start the background daemon (indexing worker + HTTP API).
flowspace3 daemon &

# 3. Index something. Any directory: a git repo, a worktree, a plain folder.
flowspace3 add .

# 4. Watch the queue drain.
flowspace3 status

# 5. Ask a question.
flowspace3 search "how does the queue avoid two workers taking the same job"
```

Every command answers one JSON envelope: `{"ok": true, …}` or `{"ok": false,
"error": {"code": "FS3-E-…", "fix": "…"}}`. `ok` is the only discriminator, and
every error carries the command or config change that resolves it — the codes
are documented in [`docs/reference/error-codes.md`](docs/reference/error-codes.md),
generated from the registry so they cannot drift.

**Troubleshooting**: run `flowspace3 doctor` again — it diagnoses and repairs
the full chain. Error codes and their fixes:
[`docs/reference/error-codes.md`](docs/reference/error-codes.md).

Providers are **unconfigured out of the box** — everything routes to the
built-in `fake` until you name real ones. When you are ready:
`flowspace3 docs get providers`.

**Agent docs ship inside the binary** — the same guidance, offline:
`flowspace3 docs list` and `flowspace3 docs get <topic>` (topics include
`install`, `doctor`, `daemon`, `search`, `config`, `agents`).

Re-running `add` on an unchanged tree costs nothing: enrichment is keyed by the
hash of the text it describes, so a re-scan enqueues zero LLM work. That is not
a cache — it is the same answer by construction.

Stop the stack with `docker compose down` (add `-v` to delete the data volume).

How the pipeline fits together:
[`docs/services/first-light.md`](docs/services/first-light.md).


### Configuration

All configuration lives in `~/.config/flowspace3/config.toml`. fs3 writes
nothing into the repos it indexes. Override the directory with `FS3_CONFIG_DIR`.

Every value below is a default — an empty or absent file is a working system:

```toml
[daemon]
url = "http://127.0.0.1:7373"

[database]
url = "postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3"

# The registry: any number of named provider instances. Declaring one costs
# nothing — only the instances a port or a repo actually names are constructed.
[providers.fake]
kind = "fake"

# The ports name one of them.
[embedder]
active = "fake"

[summarizer]
active = "fake"

[indexing]
summary_min_lines = 10     # size floor for per-element summaries
debounce_seconds = 10      # how long a dirty file must settle
worker_concurrency = 4     # jobs claimed at once — the QUEUE's concurrency

[scan]
max_file_bytes = 2000000   # generated bundles teach the index nothing
respect_gitignore = true
```

To use real providers, add instances and point the ports at them. Keys are named,
never stored:

```toml
[providers.small]
kind = "openai"
model = "text-embedding-3-small"
# api_key_env = "OPENAI_API_KEY"   # the default; keys never live in config

[providers.azure-embed]
kind = "azure_openai"
endpoint = "https://NAME.openai.azure.com"
deployment = "text-embedding-3-small"   # the DEPLOYMENT, not the model
api_version = "2024-02-01"
dimensions = 1024
# no api_key_env => Entra (managed identity, then `az login`)

[embedder]
active = "azure-embed"
```

A repo may override either port, so a monorepo of Rust and a repo of prose can
use different models without a second config file:

```toml
[repos."git:github.com/AI-Substrate/flowspace3"]
summarizer = "fake"
```

## Docker

The paved docker surface — stack up/down, cross-platform builds of the
daemon binary, and in-container workspace tests against the compose db:

```bash
harness docker up      # postgres+pgvector on 127.0.0.1:5433 (db-only by design)
harness docker run     # cargo test --workspace inside the build container
```

See [`docs/how/docker.md`](docs/how/docker.md).

## Architecture

**Read [`docs/how/architecture.md`](docs/how/architecture.md) before adding a
file.** It is the standing answer to "where does this code go, does it need a
trait, how is it tested", and it is enforced mechanically rather than by review.

The short version — seven crates, dependencies pointing inward:

```
crates/core/       domain types + pure logic + the two PORT traits.  → nothing
crates/parsers/    tree-sitter grammars + discovery → core types.    → core
crates/providers/  OpenAI + Azure implementations of the ports.      → core
crates/store/      sqlx repositories + migrations + the job queue.   → core
crates/git/        gitoxide: repo identity, blob-keyed snapshots.    → core
crates/testkit/    fakes, contract harnesses, the arch check.        → core
crates/daemon/     axum HTTP, the worker loop, composition root.     → all above
crates/cli/        the `flowspace3` binary, a thin HTTP client.      → core (+store for doctor)

Exactly two trait ports exist (`Embedder`, `Summarizer`); a third is
stop-and-ask. There are no mocking frameworks — reusable fakes ship in
`testkit`, and the same contract harness runs over the fake in CI and the real
provider on demand.

## Development

This repo has an engineering harness — the deterministic front door:

```bash
harness boot      # prove the environment before changing it
harness checks    # the mandated gate: fmt, clippy, tests, architecture drift
```

Or the underlying commands:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo run -p fs3-testkit --bin fs3-arch-check   # crate-graph drift
```

**`--all-targets` and `--all` are load-bearing.** `cargo check` and
`cargo build` do **not** build `examples/`, so an example can land both
unformatted and uncompiled with no local signal at all. Only
`cargo fmt --all --check` and `cargo clippy --all-targets` see them — and
because fmt is the FIRST gate, one unformatted example stops clippy and the
test suite from running at all, which makes a red board say nothing about the
code. (Found the hard way, 2026-08-26.)

`cargo test` needs the compose stack up. The store and daemon integration tests
run against real Postgres and **fail** — naming `docker compose up -d` — rather
than skipping, because a silently-skipped integration test is how a store
regression reaches main.

Real-provider contract tests are `#[ignore]`d and cost money:

```bash
OPENAI_API_KEY=sk-… cargo test -p fs3-providers -- --ignored
```

See [`AGENTS.md`](AGENTS.md) for the agent-facing briefing.

## License

MIT — see [`LICENSE`](LICENSE).
