# Phase 1 execution log

## t1 — Grammar wiring

Added `tree-sitter-typescript 0.23.2` to the workspace and parser crate; registered `Language::TypeScript` and `Language::Tsx`, extension mappings, stable language names, grammar constants, source extraction, and source-family discovery.

Evidence:
- `cargo build -p fs3-parsers` resolved and compiled `tree-sitter-typescript 0.23.2` against workspace `tree-sitter 0.26`.
- `cargo test -p fs3-parsers --lib`: 29 passed.
- The first build exposed an exhaustive `LanguageFamily` match in `discovery.rs`; TypeScript and TSX were added as source languages, then the build passed.

Noteworthy: the add-language contract required `crates/testkit/arch-allowlist.toml` outside the original fence. O-prime approved the single-line amendment in ask-001; the dependency edge is now explicit.

## t2 — TypeScript and TSX fixture contract

Added nested TypeScript and JSX-heavy TSX fixtures plus exact whole-tree goldens. The TypeScript fixture covers declarations, namespace parenting, six function-valued binding shapes, `const x = 1` / `const cfg = {}` negatives, wrapper/heritage grep traps, and non-empty name/address invariants.

RED evidence: `cargo test -p fs3-parsers --test fixture_elements` ran 13 tests: 11 passed, and only the two new goldens failed. Their diffs contain exactly the planned t3 gaps: `internal_module`, function-valued `variable_declarator` / `public_field_definition`, and the resulting namespace parenting. Existing declaration rows and all spans matched the checked-in expectations.

## t3 — Generic function bindings and TypeScript classification

`source.rs` now promotes only `variable_declarator` / `public_field_definition` nodes whose `value` is `arrow_function`, `function_expression`, or `generator_function`. The rule has no `Language` branch and retains the binding node's name and span. `classify.rs` recognizes bare `internal_module` containers and has an explicit table test for every TypeScript declaration, wrapper, binding, and function-value node in scope.

Evidence:
- `cargo test -p fs3-parsers --test fixture_elements`: 13 passed.
- `cargo test -p fs3-core classify`: 10 passed across 3 suites.
- Mutation 1: replacing the value-shape composition with raw `classify` removed exactly six binding Functions (class field plus five variable bindings); the TypeScript golden failed.
- Mutation 2: removing `internal_module` flattened `inside` and `Nested` onto the file and shifted binding sibling order; the namespace golden failed.
- Both rules were restored and both focused suites returned green.

## t4 — Regression and gate

`cargo fmt --all` completed. `cargo test -p fs3-core -p fs3-parsers` passed 311 tests across 12 suites. The first local `harness checks` attempt correctly refused without the test database URL; after o-prime ruled the DB-backed gate in scope, the full gate ran with the dedicated `:5434/flowspace3_test` URL and timed out after 734.93 seconds (`fs3-test-suite` exit 124) under host load, with no assertion failure in the captured output. O-prime ruled this an environment ceiling and selected CI green on the exact PR SHA as the gate; no local retry.
