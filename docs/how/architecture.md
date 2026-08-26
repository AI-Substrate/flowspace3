# Architecture — crate-enforced hexagonal, functional core

This is the standing answer to *"where does this code go, does it need a trait,
and how is it tested"*. It is the implemented form of
[workshop 001](../plans/001-fs3-foundations/assets/workshops/001-architecture.md),
which remains the authoritative source; this guide is what the workspace
actually does.

Read this before adding a file.

## The five rules

1. **The workspace is the architecture.** Boundaries are crates. Cargo makes an
   undeclared dependency a build error and forbids cycles. There are no
   `domain/` / `application/` / `infrastructure/` folders inside a crate, ever.
2. **Functional core, imperative shell.** `core` is data-in/data-out: no tokio,
   no sqlx, no HTTP. Effects live at the edges. Core's tests therefore need
   **zero doubles** — if a core test wants a fake, the code under test is in the
   wrong crate.
3. **A trait earns its existence only when a second real implementation exists
   or is firmly planned.** fs3 v1 has exactly **two** ports: `Embedder` and
   `Summarizer` (online API vs local model — PRD req 8). Everything else is
   concrete: the parser, git ops, the queue, the store. **A third port is
   stop-and-ask.**
4. **One composition root.** `crates/daemon/src/wiring.rs` reads config and wires
   concrete adapters into `Arc<dyn Port>`. That `match` *is* the entire IoC
   container. `dyn` for service seams (dispatch cost ≪ I/O); generics only in
   proven hot loops.
5. **Fakes over mocks, shipped as infrastructure.** Reusable fakes live in
   `testkit`, and `provider = "fake"` is a legal runtime config value — the whole
   stack runs offline with no API keys. Contract tests keep the fakes honest
   against the real implementations.

## The crate graph

```
flowspace3/                     (cargo workspace)
├── crates/core/       fs3-core       domain types (Element, BlobRef) + pure logic
│                              (classify, needs_summary, config types)
│                              + the two PORT traits. Depends on ~nothing.
├── crates/parsers/    fs3-parsers    tree-sitter grammars → core types.   → core
├── crates/providers/  fs3-providers  OpenAI impls of the ports.           → core
├── crates/store/      fs3-store      sqlx repos + migrations.             → core
├── crates/testkit/    fs3-testkit    fakes, contract harnesses, the       → core
│                              architecture check. A shipped crate.
├── crates/daemon/     fs3-daemon     axum HTTP + COMPOSITION ROOT         → all above
└── crates/cli/        fs3-cli        `flowspace3` binary, HTTP client.    → core
```

Directory names are short; package names are prefixed `fs3-` (`core` is a
reserved crate name). The CLI's binary is `flowspace3` (PRD req 28).

### Where does my code go?

| The code… | goes in |
|---|---|
| is pure logic over domain types | `core` |
| produces core types from the outside world | `parsers` |
| implements one of the two ports | `providers` |
| touches Postgres | `store` |
| wires things together, or serves them | `daemon` |
| is a fake, a fixture, or a contract harness | `testkit` |
| talks to the daemon over HTTP | `cli` |

If a change needs a *new* answer to that question, it needs a workshop, not a
new folder.

## The two ports

```rust
#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

#[async_trait]
pub trait Summarizer: Send + Sync {
    /// Returns summary text + 1–5 concept tags (PRD req 36).
    async fn summarize(&self, element: &Element) -> Result<Summary>;
}
```

Both use `#[async_trait]` rather than native `async fn` in traits: native async
fns are still not object-safe, and both seams are used as `Arc<dyn Port>`.
`crates/core/src/ports.rs` carries a doc-test that will stop compiling the day that
stops being true.

## The composition root

`crates/daemon/src/wiring.rs`, in full shape:

```rust
let embedder: Arc<dyn Embedder> = match &config.embedder {
    ProviderConfig::Fake => Arc::new(FakeEmbedder::default()),          // offline
    ProviderConfig::OpenAi { model, api_base, api_key_env } =>
        Arc::new(OpenAiEmbedder::new(model, api_base.clone(), api_key(api_key_env)?)),
};
```

There is no container framework, no registry, no service locator. When you want
one, add an arm.

`AppState` holds the two ports, a lazily-built `PgPool`, and the `Config` they
were wired from. The pool is lazy on purpose: the daemon starts and answers
`GET /health` without Postgres being up, so `flowspace3 ping` can distinguish
"daemon down" from "database down".

### Configuration

All configuration is files under `~/.config/flowspace3/` — never the database
(PRD reqs 28, 39). `FS3_CONFIG_DIR` overrides the directory, which is how tests
get an isolated config.

The *types* live in `fs3-core` (pure, serde only); the file reading lives in
`fs3-daemon` and `fs3-cli` separately, because reading a file is an effect. Both
read the same core types, so they cannot disagree about the endpoint.

A missing config file means defaults, and the defaults are a working offline
stack. A **malformed** config file is a loud error — silently falling back to
defaults hides a typo behind a running daemon.

## Testing strategy

| Crate | Strategy | Doubles |
|---|---|---|
| `core` | plain unit tests on pure functions | **none** |
| `parsers` | fixture files → an exact expected element table | none |
| `providers` | the same contract harness the fake runs, `#[ignore]`d | none |
| `store`, `daemon` | integration against real dockerized Postgres | testkit fakes, ports only |
| `cli` | the real binary, against a socket | none |

Every tier has exactly one exemplar in this repo. Copy the shape:

- core unit — `crates/core/src/classify.rs` (`declaration_gate_rejects_…`)
- parser fixture — `crates/parsers/tests/fixture_elements.rs`
- port contract — `crates/testkit/tests/fakes_contract.rs` + `crates/providers/tests/openai_contract.rs`
- PG integration — `crates/store/tests/pg_round_trip.rs`
- daemon integration — `crates/daemon/tests/health.rs`
- CLI end-to-end — `crates/cli/tests/ping.rs`

Integration tests **fail** when docker is absent; they do not skip. The failure
names `docker compose up -d`. A silently-skipped integration test is how a store
regression reaches main.

## Enforcement

Rules that only live in a document decay. These are mechanical:

| Rule | Enforced by |
|---|---|
| Dependency direction, undeclared imports, cycles | Cargo |
| Refused *declared* edges (`sqlx` in core, `axum` in parsers) | `crates/testkit/arch-allowlist.toml` + `fs3-arch-check` |
| No mocking frameworks, anywhere, in any table | the same check, with its own message |
| Formatting, lints | `cargo fmt --check`, `clippy -D warnings` |

`crates/testkit/arch-allowlist.toml` is an **allow-list**, not a deny-list: any direct
dependency edge nobody added deliberately fails the check. Adding a dependency
costs one considered line, which is the point.

Run it directly, or let `harness checks` do it:

```bash
cargo run -p fs3-testkit --bin fs3-arch-check
harness checks
```

The check's own failure mode is proved re-runnably, not by a violate-and-revert
ritual: `crates/testkit/fixtures/arch/drifted-metadata.json` is a committed manifest
with `sqlx` in the functional core, and `crates/testkit/tests/arch_drift.rs` asserts the
check goes red on it — on every `cargo test`.

## Refused anti-patterns

Name these in review; they are settled, not open:

- a trait for every service
- a repository trait over sqlx
- DTO mapping layers between crates (share `core` types)
- `domain/` `application/` `infrastructure/` folders inside a crate
- mocking frameworks
- abstracting the parser, or abstracting Postgres

## Classification, and why it is gated

`fs3_core::classify` is two stages, deliberately separate:

1. `category_hint` — the substring guess (`function` → callable, `struct` → type).
2. `is_declaration_shaped` — the gate: a declaration suffix (`_item`,
   `_declaration`, `_definition`, `_signature`, `_spec`) or an exact bare word
   (`method`, `class`, `module`, `atx_heading` — some grammars have no suffix).

Stage 1 alone invents elements. The POC measured a C++ file yielding 58 claimed
elements against 22 real ones, and a Ruby file yielding 68 against 18, because
`function_declarator` twins every definition and every `foo.method_call` matches
`method`. Stage 2 is PRD req 42, and it is why `parsers` needs no per-language
code: adding a language is adding a grammar crate and one line in
`Language::for_extension`.

## Open questions

- **Q1** — does conversation ingestion (PRD reqs 24–27) become a third port when
  the harness integration lands, or is it a concrete axum route? Rule 3 applies;
  decide when that plan starts.
- **Q2** — tag storage shape (PRD req 36): a column on the summary row, or a
  join table? Settle at schema design in plan 002. Migration 0001 here is an
  exemplar, not the schema.
