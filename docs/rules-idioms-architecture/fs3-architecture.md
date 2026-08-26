<!-- PROMOTED from docs/plans/001-fs3-foundations/assets/workshops/001-architecture.md on 2026-08-26 by o-prime.
Promotion gate: conditions 1-3 met at s001 review; condition 4 (recorded real-provider contract run) satisfied by the Azure OpenAI keyed contract run (crates/providers/tests/azure_openai_contract.rs, green 3x on 2026-08-26, verified independently by o-prime). This copy is now the repo-wide authority; the plan copy is the historical original. -->
# Workshop: fs3 Architecture — crate-enforced hexagonal, functional core

**Type**: Architecture / Integration Pattern
**Plan**: 001-fs3-foundations
**Spec**: ../../base-prd.md (37 requirements) · ../fs3-overview.md
**Created**: 2026-08-26
**Status**: Draft — **promote to repo-wide doctrine (docs/rules-idioms-architecture/) after the initial implementation validates it** (Jordan, 2026-08-26)

**Value Thesis**: Every future plan and agent inherits one settled answer to "where does this code go, what gets a trait, how is it tested" — killing fs2's 40-file adapter/fake matrix while keeping full DI/IoC and fake-driven testing.
**Target Proof Level**: Contract Ready
**Current Proof Level**: Preferred Direction (becomes Validated when the initial impl ships against it; that's the promotion gate)

**Selected Value Axes**:
- **Safety to Change**: crate boundaries make dependency-rule violations a compile error, not a review catch.
- **Agent Readiness**: an agent placing new code has a deterministic decision table, not a judgment call.
- **Proof Quality / Cost Reduction**: fakes are shipped, reusable infrastructure (offline dev mode + deterministic tests), not per-test throwaways.

**Related Documents**: `base-prd.md` (esp. reqs 2, 4, 8, 20, 33) · fs2 prior art `/Users/jordanknight/substrate/fs2/flow_squared`

---

## Purpose

Settle fs3's architecture: how hexagonal/DI/IoC is expressed in Rust, which seams get traits, where code lives, and how everything is tested. This is the reference to keep open while building — and the doctrine to promote repo-wide once proven.

## Key Questions Addressed

1. How do we get full DI/IoC in Rust without a container framework?
2. How is hexagonal/ports-and-adapters expressed idiomatically (and what do we refuse)?
3. Fakes vs mocks — how do reusable fake implementations work here?
4. What is the crate layout and its dependency direction?

---

## Decision Space

| Option | Description | Decision |
|---|---|---|
| Runtime DI container (shaku/teloc) | Java/C#-style container in Rust | **Rejected** — fights the language; reflection-free Rust never adopted it |
| Single crate + layer folders (`domain/`, `infra/`) | Clean-architecture-by-convention | **Rejected** — discipline-enforced only; folders don't stop imports |
| **Workspace crates + composition root + trait ports** | Cargo enforces the dependency rule; `main` wires by hand | **Selected** |
| Trait-per-service + fake-per-adapter (fs2 style) | Everything abstracted | **Rejected** — 40-file matrix; a trait must earn its existence |
| Mocking framework (mockall) | Per-test doubles | **Rejected** — reusable fakes chosen (below) |

## The five rules (the doctrine to promote)

1. **The workspace is the architecture.** Boundaries are crates; Cargo makes an undeclared dependency a build error and forbids cycles. No layer folders inside crates.
2. **Functional core, imperative shell.** `core` is pure data-in/data-out (no tokio, no sqlx, no HTTP); effects live at the edges. Core tests need zero mocks.
3. **A trait earns its existence only when a second real implementation exists or is firmly planned.** fs3 v1 has exactly two ports: `Embedder` and `Summarizer` (online vs local — req 8). Everything else is concrete: parser (tree-sitter direct IS the point), git ops, queue, store (PG is a requirement — req 4 — not a variable).
4. **One composition root.** `crates/daemon/main.rs` reads config and wires concrete adapters into `Arc<dyn Port>`; the config `match` is the entire IoC container. `dyn` for service seams (dispatch cost ≪ I/O), generics only in proven hot loops.
5. **Fakes over mocks, shipped as infrastructure.** Rich reusable fakes live in a `testkit` crate; `provider = "fake"` is a legal config value (whole stack runs offline, no keys); contract tests keep fakes honest against real impls.

## Crate layout & dependency direction

```
flowspace3/                     (cargo workspace)
├── crates/core/       domain types (Element, Turn, BlobRef) + pure logic
│               (classify, chunk, tree-diff planning, ranking, needs_summary)
│               + the two PORT traits: Embedder, Summarizer. Depends on ~nothing.
├── crates/parsers/    tree-sitter grammars → core types. Concrete.        → core
├── crates/providers/  OpenAI / Azure / ort-local impls of the ports.      → core
├── crates/store/      sqlx repos, migrations, queue table. Concrete.      → core
├── crates/testkit/    FakeEmbedder, FakeSummarizer, fixtures, contract    → core
│               test harness. A real shipped crate.
├── crates/daemon/     axum HTTP, watcher, queue workers, COMPOSITION ROOT → all above
└── crates/cli/        `flowspace3` binary — thin HTTP client of daemon.   → (core for types)
```

Rule of thumb for placement: **pure logic → core · produces core types from the world → parsers · implements a port → providers · touches PG → store · wires or serves → daemon**.

## Contracts (the shapes to build against)

```rust
// core — the only two ports in v1
#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}
#[async_trait]
pub trait Summarizer: Send + Sync {
    /// Returns summary text + 1–5 concept tags (req 36).
    async fn summarize(&self, el: &Element) -> Result<Summary>;
}

// daemon — the composition root (this match IS the IoC container)
let embedder: Arc<dyn Embedder> = match cfg.embedder {
    EmbedderCfg::OpenAi { model } => Arc::new(OpenAiEmbedder::new(model)),
    EmbedderCfg::Local  { model } => Arc::new(OrtEmbedder::load(model)?),
    EmbedderCfg::Fake             => Arc::new(FakeEmbedder::default()),  // offline mode
};
let state = AppState { embedder, summarizer, db: PgPool::connect(&cfg.pg_url).await? };
```

```rust
// testkit — deterministic fake: hash-based vectors so same text → same vector,
// similarity search behaves MEANINGFULLY in tests. Records calls; injects failures.
pub struct FakeEmbedder { pub calls: Mutex<Vec<Vec<String>>>, pub fail_after: Option<usize> }

// contract test, written once, run over every impl (fake in CI; real on demand)
async fn embedder_contract<E: Embedder>(e: &E) { /* dimensionality, determinism, batch */ }
```

## Testing strategy (per crate)

| Crate | Strategy | Doubles needed |
|---|---|---|
| core | plain unit tests on pure functions | **none** |
| parsers | fixture files → expected elements | none |
| providers | contract tests (`#[ignore]`d live runs) | none |
| store, daemon | integration vs real dockerized PG (req 33) | testkit fakes for ports only |

## Refused anti-patterns (name them so reviews can cite them)

- A trait for every service · repository-trait over sqlx · DTO mapping layers between crates (share `core` types) · `domain/application/infrastructure` folders inside a crate · mocking frameworks · abstracting the parser or PG.

## Attention Reduction

| Future loop | Before | After |
|---|---|---|
| Implementation | "where does this go / does it need an interface?" per PR | placement table + rule 3 answer it |
| Review | architecture policed by convention | Cargo enforces it; reviews cite refused-list by name |
| Testing | invent doubles per test | import testkit; core needs none |
| Agent execution | re-derive architecture each session | this doc is the standing reference |

## Validation / Acceptance (promotion gate)

This workshop reaches **Validated** — and gets promoted to `docs/rules-idioms-architecture/` as repo-wide doctrine — when the initial implementation ships with: (1) the workspace compiling with this crate graph and no cross-boundary imports beyond it; (2) exactly two trait ports; (3) `provider=fake` running the stack end-to-end offline; (4) contract tests green over fake + at least one real provider.

## Open Questions

- **Q1 (OPEN)**: does `TurnSource`/conversation ingestion (reqs 24–27) become a third port when the harness integration lands, or is it a concrete axum route? Decide when that plan starts — rule 3 applies.
- **Q2 (OPEN)**: tag storage/search shape (req 36) — column on summary row vs join table. Settle at schema design.
