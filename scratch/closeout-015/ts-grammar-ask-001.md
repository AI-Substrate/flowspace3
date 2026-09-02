# ts-grammar stop-and-ask 001 — architecture allow-list fence

## Decision needed

Amend the packet fence to permit the one required line in `crates/testkit/arch-allowlist.toml`: add `tree-sitter-typescript` to `[crates.fs3-parsers].external`.

## Evidence

- The loaded `add-language` skill makes this step mandatory: every grammar dependency must be mirrored in the parser architecture allow-list or the drift check fails.
- `docs/services/scanner.md` repeats the same five-step language-addition contract.
- The packet fence permits `crates/parsers/**`, `crates/core/src/classify.rs`, workspace Cargo files, plan files, and agent reports, but excludes `crates/testkit/arch-allowlist.toml`.

No edit has been made outside the fence. T1 is complete and green; t2 is paused pending the ruling.
