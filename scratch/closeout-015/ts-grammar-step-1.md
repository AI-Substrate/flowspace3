# ts-grammar step 1 report

## Result

T1 complete. `tree-sitter-typescript 0.23.2` resolves and compiles with workspace `tree-sitter 0.26`.

## Changed

- `Cargo.toml`: workspace grammar dependency.
- `Cargo.lock`: resolved grammar package.
- `crates/parsers/Cargo.toml`: parser dependency.
- `crates/parsers/src/lib.rs`: TypeScript/TSX variants, extensions, names, grammar constants, source walk, focused tests.
- `crates/parsers/src/discovery.rs`: both new grammars classify as source files.
- `docs/plans/015-ts-grammar/assets/tasks/phase-1/tasks.dd.{json,md}`: t1 receipt and checked state via global `ddocs`.
- `docs/plans/015-ts-grammar/assets/tasks/phase-1/execution.log.md`: execution evidence.

## Proof

- `cargo build -p fs3-parsers`: PASS; exact resolved crate `tree-sitter-typescript v0.23.2`.
- `cargo test -p fs3-parsers --lib`: PASS, 29 tests.
- Initial build failed only on the newly exhaustive `LanguageFamily` match; after mapping TypeScript/TSX to `Source`, the real build passed. No dependency or ABI stop condition fired.
