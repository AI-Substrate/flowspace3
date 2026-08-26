---
name: add-language
description: Add one tree-sitter language to the fs3 scanner — grammar wiring, fixture, whole-tree test. Use when extending fs3-parsers language coverage.
---

# add-language — extend the fs3 scanner by one language

Extraction is generic — **there is no step 6**. The full write-up with worked examples lives at the end of `docs/services/scanner.md` (read it first); this is the packet form. Authored from the scanner build (worker mollusk, 2026-08-26).

## The 5 steps

1. Grammar crate → workspace deps + `crates/parsers/Cargo.toml`.
2. ONE line in `[crates.fs3-parsers].external` of `crates/testkit/arch-allowlist.toml` — or the drift check bites.
3. Extension + grammar registered in `Language::for_extension` / `grammar` (crates/parsers).
4. A fixture file with real NESTING **and** a construct a naive substring classifier would wrongly turn into an element (the grep-trap).
5. A whole-tree expectation test (kind/subkind/address/sibling_order/span as one table) plus an `invents_nothing` negative.

Touch `core/src/classify.rs` ONLY when the grammar's declaration kinds are shaped unlike every other's: a bare-word declaration earns a `BARE_DECLS` entry, a new suffix earns a `DECL_SUFFIXES` entry — **a per-language branch never**.

## The four traps (each cost real time once — don't pay again)

1. Hint substrings are near-miss magnets (`'mod'` vs Java/C# `modifiers`) — safe only behind the declaration-shape gate; pin near-misses with a test.
2. Wrapper nodes (e.g. Python `decorated_definition`) must be spliced through, or every decorated function twins.
3. tree-sitter error recovery yields name fields that are PRESENT AND EMPTY — refuse `Some("")`.
4. Point-node grammars (markdown) need ranges synthesised in fs3 code.

## Rules (inherited, non-negotiable)

- `scan` stays pure — no IO/DB/async; unknown/binary degrade to a one-element tree, never an error.
- Addresses are non-unique by design (`struct X` + `impl X` share one); `(address, span.start_line)` identifies a node — never invent `X#2`.
- Gates: `cargo test -p fs3-parsers` + arch drift green. Fixture + expectation land in the SAME commit as the wiring.
- Shared-file staging discipline: ruling `.harness/government/rulings/2026-08-26-commit-push-as-you-go.md`.
- Report per repo convention; update `docs/services/scanner.md`'s language table.
