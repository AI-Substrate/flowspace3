# flowspace3

Semantic code search over a central index. A codebase is split into
elements — functions, types, markdown sections — each summarized, embedded, and
searchable by meaning, text, or regex, across every repo on the machine at once.

This is `fs3`: a Rust workspace, a Postgres+pgvector store, a background daemon,
and the `flowspace3` CLI.

> **Status: foundations.** Plan 001 built the mold, not the features — the crate
> graph, the two ports, the composition root, the drift check, and one exemplar
> test at every tier. Indexing, search, and summarization land in later plans.

## Quick start

Prerequisites: a Rust toolchain (1.95+, edition 2024) and Docker.

```bash
# 1. Bring up Postgres + pgvector (host port 5433, deliberately off 5432)
docker compose up -d

# 2. Build and prove the workspace
cargo build --workspace
cargo test --workspace

# 3. Run the daemon — no config file needed, no API keys needed
cargo run -p fs3-daemon

# 4. In another shell, ask it how it is
cargo run -p fs3-cli --bin flowspace3 -- ping
# healthy - fs3 daemon 0.1.0 at http://127.0.0.1:7373 (embedder: fake, summarizer: fake)
```

Step 3 works with no configuration because `provider = "fake"` is the default
and is a **legal runtime provider**, not a test hook: the whole stack runs
offline, with no keys, deterministically.

Stop the stack with `docker compose down` (add `-v` to delete the data volume).

### Configuration

All configuration lives in `~/.config/flowspace3/config.toml`. fs3 writes
nothing into the repos it indexes. Override the directory with `FS3_CONFIG_DIR`.

Every value below is a default — an empty or absent file is a working system:

```toml
[daemon]
url = "http://127.0.0.1:7373"

[database]
url = "postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3"

[embedder]
provider = "fake"          # or "openai"

[summarizer]
provider = "fake"

[indexing]
summary_min_lines = 10     # size floor for per-element summaries
debounce_seconds = 10      # how long a dirty file must settle
```

To use a real provider, name the model and export a key:

```toml
[embedder]
provider = "openai"
model = "text-embedding-3-small"
# api_key_env = "OPENAI_API_KEY"   # the default; keys never live in config
```

## Architecture

**Read [`docs/how/architecture.md`](docs/how/architecture.md) before adding a
file.** It is the standing answer to "where does this code go, does it need a
trait, how is it tested", and it is enforced mechanically rather than by review.

The short version — seven crates, dependencies pointing inward:

```
core/       domain types + pure logic + the two PORT traits.  → nothing
parsers/    tree-sitter grammars → core types.                → core
providers/  OpenAI implementations of the ports.              → core
store/      sqlx repositories + migrations.                   → core
testkit/    fakes, contract harnesses, the arch check.        → core
daemon/     axum HTTP + the composition root.                 → all above
cli/        the `flowspace3` binary, a thin HTTP client.      → core
```

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
