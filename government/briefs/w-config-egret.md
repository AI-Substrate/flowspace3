# Worker brief — global config system · pij-technological-egret
**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-26 · one bounded task

## The job

Build fs3's centralized GLOBAL configuration system. Jordan's scope ruling: **global only** — `~/.config/flowspace3/` — NO per-repo/per-folder overrides yet (keep the loader layered-shaped so overrides can slot in later without redesign). Design distilled from fs2 research (what to copy/drop is settled — do not re-litigate):

### Layers (precedence low→high)
1. serde defaults on the config types (every field has one where sensible)
2. `~/.config/flowspace3/config.toml` (`FS3_CONFIG_DIR` overrides the dir — existing daemon behaviour, keep it)
3. `FS3_*` env var overrides with `__` nesting (`FS3_DATABASE__URL`) — strings, coerced by serde; this is how containers/tests override without files

### Secrets
Separate chain, never in config.toml: process env, then `~/.config/flowspace3/secrets.env` loaded INTO env at startup (daemon + cli). Config files reference env-var NAMES (the `api_key_env` shape already normative in providers) — no `${VAR}` templating. Never log secret values (s001 redaction rules).

### Types & injection (the DI story — compile-time, no registry)
- Config TYPES live in `fs3-core` (pure — serde only, no IO); the LOADER (file read, env merge, secrets load) lives in `fs3-daemon` config module (existing split, tk-0009 ruling; extend, don't relocate).
- One root `Config` struct of typed sections: `daemon`, `database`, `embedder`, `summarizer`, `scan` (new — carries the scanner-policy knobs like min-size thresholds; coordinate shape with the scanner worker mollusk ONLY via me if a conflict appears, likely none — you define the section, it consumes later).
- Injection pattern (document it, exemplify it): composition root loads once → `AppState` carries it → each service is CONSTRUCTED with the narrow section it needs (`&EmbedderConfig`), never a god-object, never a lookup. Services receive everything, go looking for nothing.
- Validation: each section validates on load and reports ALL problems at once (collect, don't fail-first), with actionable messages naming the file path + key + an example line. Missing file = defaults + INFO log naming where to create it (not an error).
- Rustdoc on every section struct includes its example config.toml block (fs2's best habit).

### Deliverables
1. Types + loader + env-override + secrets chain, wired through the existing composition root (`AppState::from_config` path stays the single entry).
2. `flowspace3 config show` CLI verb (or extend an existing surface if one fits): prints the EFFECTIVE merged config with secrets redacted + which layer each section came from — the debuggability anchor.
3. Tests: fixture config dirs via `FS3_CONFIG_DIR` (existing pattern in daemon tests); env-override precedence proven; all-errors-at-once validation proven; secrets-never-logged guard.
4. **Parked debt (fold in — you're in the daemon anyway)**: an automated test of the fail-fast boot contract — spawn fs3-daemon with an unreachable `database.url`, assert nonzero exit and stderr naming the url + `docker compose up -d` (cicada's observation; ~25 lines, `crates/daemon/tests/`).
5. `docs/how/configuration.md` — one page: where config lives, how to change it, env overrides, secrets; PLUS the missing repo-wide walkthrough section **"Adding a new injected service"** (constructor injection, narrow sections, the composition root is the only chooser, drift check enforces it — cite docs/rules-idioms-architecture/fs3-architecture.md).

## Rules & fence

- Architecture binds (docs/rules-idioms-architecture/fs3-architecture.md — the promoted authority): no new ports, core stays pure, no mocking crates, arch check green.
- Fence: `crates/core/src/config.rs` (+ tests), `crates/daemon/src/config.rs`/`main.rs`/`wiring.rs` (config-loading concerns only), `crates/cli/**` (the config show verb), `crates/daemon/tests/**`, `docs/how/configuration.md`. Scratch `.harness/temp/w-config/**`.
- Excluded: providers, parsers, store (except compile fixes), `.harness/government/**`, `.claude/**`, `docs/plans/**`.
- Commit + push per coherent unit: scoped `git add <paths>` ONLY (siblings mollusk + gibbon work this tree), push-first, never pull --rebase over unstaged sibling work (ruling `.harness/government/rulings/2026-08-26-commit-push-as-you-go.md`).
- Gates: `harness checks` + `cargo test --workspace` green (compose PG on 5433 up).
- Report to pij-instant-lynx: claim · files · gate output · config-show sample output · observations. Deviations = stop-and-ask.

Ack by pij message, then go.
