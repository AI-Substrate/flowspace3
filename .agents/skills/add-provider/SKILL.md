---
name: add-provider
description: Build one fs3 provider adapter (Embedder/Summarizer) end-to-end as an independent worker packet — module, stub tests, keyed contract leg, snap-in recipe. Use when adding any new LLM/embedding provider to fs3-providers.
---

# add-provider — build one fs3 provider adapter

This skill is a **worker packet recipe**: one `/pij` worker builds one adapter end-to-end, in parallel with others, no shared state beyond the repo. Each adapter is an independent module: two workers never touch the same file except `crates/providers/src/lib.rs` (one `pub mod` + re-export line each — trivial merge).

Pick your adapter from the roster: `docs/plans/prd/providers-roster.md`. Flip its row when done.

## The frozen contract (do not negotiate these)

- Exactly two ports exist: `Embedder` and `Summarizer` in `fs3-core::ports` (`#[async_trait]`, object-safe, `Arc<dyn>`). **A third port is stop-and-ask.** Your adapter implements one or both — nothing else changes shape.
- `Summary` = text + 1–5 concept tags (PRD req 36); `has_valid_tags()` must hold for real outputs.
- Errors map into `fs3_core::Error::Provider` with actionable messages (say which env var / endpoint / model was wrong and what to do).
- Batch semantics: `embed()` returns one vector per input, **in input order**.
- No mocking crates (the arch check refuses them by name). Offline tests use a local stub HTTP server (a fake) — `axum` bound to `127.0.0.1:0` in a test is the pattern.
- Architecture: your code lives ONLY in `crates/providers/`; deps you add must pass the arch allowlist (`crates/testkit/arch-allowlist.toml`) — extend it only for your adapter's real needs, never for core.
- **Use existing libraries — never reinvent plumbing**: the same HTTP client stack the existing adapters use; official SDK crates for auth (e.g. `azure_identity` for Entra tokens — never hand-rolled OAuth); established crates for protocol/serialization. Hand-roll only what has no credible crate.

## Files you create/touch

```text
crates/providers/src/<name>.rs             # the adapter (struct + config + port impls)
crates/providers/src/lib.rs                # + pub mod <name>; pub use ...  (one line each)
crates/providers/Cargo.toml + root Cargo.toml/Cargo.lock  # only if you add deps — shared-merge files, same one-line-each discipline; commit promptly
crates/providers/tests/<name>_stub.rs      # offline: request shape, auth headers, error mapping vs stub server
crates/providers/tests/<name>_contract.rs  # #[ignore]d keyed run: shared contract suite vs the real service
docs/plans/prd/providers-roster.md         # flip your row's status when done
```

## The steps

1. **Read prior art**: `crates/providers/src/openai.rs` + its tests (the exemplar); `crates/testkit/src/contract.rs` (the done-bar you must satisfy); for Azure/credential shapes, fs2 read-only at `/Users/jordanknight/substrate/fs2/flow_squared`.
2. **Adapter struct**: constructor takes model/endpoint/credential config resolved by the caller. NORMATIVE credential shape: config carries *env var names*; values are read once at construction (openai.rs's raw-value constructor is legacy — offer both if you must, env-name is the standard). Never hardcode, never log secrets; redaction rules from s001 rev-0002 apply. **Acquired credentials (OAuth/CLI tokens) MUST be cached with an expiry-refresh skew** — an uncached CLI credential is one process spawn per batch; pin reuse + refresh in stub tests.
3. **Offline stub tests** (run keyless in CI): assert the exact request path/headers/payload your adapter sends, response parsing, and error mapping (bad status → `Error::Provider` naming the fix).
4. **Contract leg**: wire the shared suite from `fs3-testkit::contract` as `#[ignore]`d tests, one per port, with a doc comment naming the exact env vars, the run command, **and the auth-precedence order** (which credential wins when several are present — a stale exported key silently beating a CLI login is a real trap; make the error name both fixes). Cross-call vector comparisons in the suite use similarity tolerance — do not tighten them back to bit-exact.
5. **Snap-in recipe, not snap-in**: add a short `## Snap-in` doc comment on the module: the `ProviderConfig` variant + composition-root match arm this adapter needs. **Do NOT edit `fs3-core::config` or the daemon wiring** — wiring happens at adoption, by the integrating stream, so parallel workers never collide there.
6. **Gates**: `harness checks` green (fmt, clippy -D warnings, tests, arch drift) + `cargo test -p fs3-providers` green. Keyed contract run: check your packet for credentials, THEN the machine (existing provider configs like `~/.config/fs2/`, `az login`, ambient env) before declaring it not-run; if found, run it and report actual output.
7. **Report**: claim · files · gate outputs · keyed-run status · any deviation from this recipe (deviations are stop-and-ask, not improvisation). Flip your roster row, and write your service page `docs/services/<name>.md` (convention: `docs/services/README.md` — what it is, decisions, gotchas, verify commands, code pointers).

## Test tiers (repo convention)
Default `cargo test` stays FAST and OFFLINE. Anything else is `#[ignore = "<tier>: <reason>"]` with the reason string mandatory: `keyed: <env vars>` for real endpoints (missing var fails BY NAME when run), `slow: <why>` for expensive-but-free work (model loads/downloads). Opt-in via `-- --ignored`.

## Done means

- [ ] Builds in-workspace; arch check green; no mocking crates; no new ports
- [ ] Offline stub tests prove request shape + auth + error mapping keylessly
- [ ] `#[ignore]`d keyed contract tests exist with documented env vars + command
- [ ] Snap-in recipe documented on the module; core/daemon untouched
- [ ] Roster row updated; report filed

## Known debts (integrating stream owns these — do not fix them from a worker fence)

- A shared `StubServer` belongs in `fs3-testkit` (promoted from azure_openai_stub.rs's ~110-line recorder) — until it lands, copy that pattern.
- Shared OpenAI wire-shape helpers (`order_embeddings`, summary parsing) belong in `crates/providers/src/wire.rs` — until it lands, reference PUBLIC items from openai.rs where possible; copy privately only as a last resort and say so in your report.

## Improving this skill

This recipe is expected to improve as adapters land. When a worker hits a gap or a better pattern, they report it; the o-prime folds it in here. Keep the frozen-contract section stable — everything else is fair game.
