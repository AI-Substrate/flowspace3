# Worker brief — scanner v1: element tree + pure scan · (seat assigned at canary)
**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-26 · one bounded task

## The job

Build fs3's scanner: a **pure function** that takes a file's path+bytes and returns the element tree. Jordan (verbatim): "this scanner should just take a file and return a data object. We don't want it to know anything about databases or anything like that."

### 1. Element model (in `fs3-core`, extend `element.rs`)
The agreed hierarchy — one node type, self-parented tree:
- `kind`: small CLOSED enum — `File | Container | Function | Section` (language-specific detail like class/impl/trait/mod goes in `subkind: String`).
- Per node: `name`, `address` (`src/foo.rs::Indexer::scan` style — stable across re-parses), `span` (start/end line), `raw_text` (the element's exact slice), `raw_hash` (sha256 of raw_text — THE dirtiness key), `sibling_order`, children (tree shape — parent links/ids are the STORE's concern, not yours; return an owned tree).
- Keep whatever the existing `Element` already has that s001 consumers use (testkit fakes derive tags from element address; store round-trip uses spans) — evolve, don't break: `cargo test --workspace` must stay green, adapting existing call-sites is in-fence.

### 2. Scanner (in `fs3-parsers`)
- `pub fn scan(path: &Path, bytes: &[u8]) -> Result<ElementTree>` — pure: no IO beyond the given bytes, no DB, no async, no tokio/sqlx (the arch drift check enforces this — it must stay green).
- tree-sitter direct (PRD req 2). Exemplar languages this pass: **Rust** (fns, impls, mods, structs w/ methods properly parented), **Markdown** (heading-section hierarchy — fs2 has special md handling worth mining), **Python** (classes/defs, nesting).
- Unparseable/unknown files degrade gracefully: a single File element, never an error for "no grammar".
- Mine the POC: `docs/plans/002-docker-daemon-base/../001-fs3-foundations/assets/poc/treesitter-results.md` — sailfish proved tree-sitter across many types (4,914 files/s) with 11 learnings; `docs/plans/001-fs3-foundations/assets/poc/treesitter/` has working harness code. fs2 read-only at `/Users/jordanknight/substrate/fs2/flow_squared` (md element shapes, what fs2 calls things).

### 3. Tests (fixture-driven — the repo's exemplar pattern)
- Follow `crates/parsers/` existing fixture tests: committed fixture files per language + tests asserting the WHOLE element tree (kinds, subkinds, addresses, spans, parenting, order), including a grep-trap negative. Add fixtures for all three languages incl. nesting edge cases (nested fns, impl-in-mod, md deep headings).
- Hash determinism test: same bytes → identical raw_hashes; one-char change → only touched elements' hashes change.

## Rules & fence

- Architecture binds: no new ports; parsers stays pure (arch check green); no mocking crates.
- Fence: `crates/parsers/**`, `crates/core/src/element.rs` (+ its test), minimal adaptations in existing call-sites broken by the model change (`crates/testkit/`, `crates/store/` compile fixes only — flag anything bigger), fixtures dirs. Scratch `.harness/temp/w-scanner/**`.
- Excluded: daemon/cli behaviour changes, `.harness/government/**`, `.claude/**`, `docs/plans/**` content, providers.
- **Commit and push as you go** (ruling `.harness/government/rulings/2026-08-26-commit-push-as-you-go.md`): scoped `git add <paths>` only — NEVER `-A` (siblings work this tree); pull --rebase then push main at each coherent unit.
- Gates: `harness checks` + `cargo test --workspace` green (docker compose db on 5433 must be up for store tests: `docker compose up -d`).
- Report to pij-instant-lynx: claim · files · gate output · fixture inventory · observations (esp. anything that should become the `add-language` skill). Deviations = stop-and-ask.

Ack by pij message, then go.
