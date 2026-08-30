# Worker brief — ignore-aware file discovery · (seat at canary, pane %39)
**From**: pij-instant-lynx (o-prime) · 2026-08-26 · one bounded task

## The job
The pure discovery walker (PRD reqs 41, 43, 12): given a root path + injected settings, return the list of scannable files. The POC proved discovery filtering is THE perf lever (13.8× — see `docs/plans/001-fs3-foundations/assets/poc/treesitter-results.md`).

1. **Module in `fs3-core`** (pure types/logic) or `fs3-parsers` if it needs the walker dep — your call, justify; the IO walk itself may live behind a thin function taking `&Path`. Use the `ignore` crate (ripgrep's — gitignore semantics for free, cross-platform, parallel walker) rather than reinventing (lib-reuse rule).
2. Semantics: respects `.gitignore`/git tracking; **per-repo config can force-include ignored dirs** (PRD 41 — take an injected `DiscoverySettings` with include/exclude globs); **config/data formats are EXCLUDED by default** (PRD 43 — yaml/json/toml/hcl etc., PII ruling); size ceiling + binary sniff; returns relative paths + file size + detected language family.
3. Fixture-tested like the scanner: committed fixture tree(s) covering gitignored dirs, forced includes, config-file exclusion, binary files, size caps; assert the exact returned set (grep-trap negative included).
4. `docs/services/discovery.md` per the convention when done.

## Rules & fence
- Architecture authority: `docs/rules-idioms-architecture/fs3-architecture.md`. No new ports; no mocks; arch allowlist extended only for the `ignore` crate (+justify).
- Fence: the new module + its tests + fixtures, `crates/testkit/arch-allowlist.toml` (one row), `docs/services/discovery.md`. Nothing else. NOTE: sibling mollusk is mid-refactor of `crates/core/src/element.rs` — do not touch element/classify; if the tree doesn't compile when you start, build/test only your package (`cargo test -p <your-crate>`) until mollusk's cutover lands.
- Commit+push per unit, scoped adds only, push-first (`.harness/government/rulings/2026-08-26-commit-push-as-you-go.md`).
- Gates: your package's tests green now; `harness checks` green once the tree compiles. Report to pij-instant-lynx. Deviations = stop-and-ask.
