# ts-grammar step 3 report

## Result

T2 and t3 are green. TypeScript/TSX declaration forests match their full goldens; six function-valued binding shapes are Function elements; namespace members scope beneath `Tools`; deliberate wrappers, non-function bindings, JSX nodes, and heritage nodes remain non-elements.

## Five-step add-language mapping

1. **Grammar crate → t1:** `tree-sitter-typescript 0.23.2` added to workspace dependencies and `fs3-parsers`; real build proved compatibility with tree-sitter 0.26.
2. **Architecture edge → t1/t2 boundary:** o-prime-approved single line added to `[crates.fs3-parsers].external` in `crates/testkit/arch-allowlist.toml`.
3. **Extension + grammar registration → t1:** `Language::{TypeScript,Tsx}`, `ts|mts|cts|tsx`, stable names, grammar constants, source-family discovery, and focused scans.
4. **Nested fixture + grep trap → t2:** `sample.ts` and `sample.tsx` include nested functions/containers, `class_heritage`, wrappers, JSX, six function bindings, and `const x = 1` / `const cfg = {}` negatives.
5. **Whole-tree + invents-nothing proof → t2/t3:** exact forest tables cover kind/subkind/address/parent/sibling/span; negative assertions reject wrappers and blank identities. T3's generic value-shape and `internal_module` rules make the locked contract green.

The exact snap-in recipe is preserved in `Language::for_extension`'s doc comment; the source walk remains language-agnostic.

## Changed in step 3

- `crates/parsers/src/source.rs`: generic function-valued binding rule; no `Language` branch.
- `crates/core/src/classify.rs`: `internal_module` and exhaustive TypeScript decision test.
- `crates/parsers/tests/fixture_elements.rs`: now-green TypeScript/TSX full forests and negatives.
- Plan task rows/execution log: t2/t3 receipts through global `ddocs`.

## Proof and mutation receipts

- `cargo test -p fs3-parsers --test fixture_elements`: 13 passed.
- `cargo test -p fs3-core classify`: 10 passed across 3 suites.
- Removing the value-shape composition removed exactly six binding Functions and failed the golden; restored.
- Removing `internal_module` flattened namespace members to file scope and failed addresses/sibling order; restored.
