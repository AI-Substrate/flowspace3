# ts-grammar coder acknowledgement

Worktree root: `/Users/jordanknight/substrate/flowspace/fs3-ts-grammar`

## Evidence read

- `assets/inputs/evidence.md` establishes the user-visible failure: prod has only file elements for TypeScript—chainglass 11,286/0, harness-engineering 7,237/0, and pij 5,893/0 file/non-file elements—so `tree` and `refs` return honest-looking emptiness rather than TypeScript symbols.
- The same evidence identifies the source boundary: discovery already admits `ts`/`tsx`, while `Language::for_extension` wires only Rust, Python, and Markdown. TypeScript is indexed but parsed as an unsupported bare file.
- Existing classification already recognizes declaration-suffixed TypeScript functions, classes, interfaces, enums, aliases, methods, and signatures. It misses `internal_module`, `variable_declarator`, and `public_field_definition`; wrappers such as `export_statement` must remain deliberate non-elements.
- The dependency evidence is provisional: `tree-sitter-typescript 0.23.2` exposes both `LANGUAGE_TYPESCRIPT` and `LANGUAGE_TSX`, and only a dry-run has resolved it beside tree-sitter 0.26. The packet's first tripwire is therefore a real parsers build before any fixture or classifier work.

## Numbered implementation plan

1. **t1 only:** add the workspace/parser dependency and `Language::{TypeScript, Tsx}` mappings for `ts|mts|cts|tsx`, `as_str`, and `grammar()`. Immediately run a real `fs3-parsers` build against workspace tree-sitter 0.26. A dependency/ABI failure is a stop-and-ask with the exact Cargo error; no t2 work starts.
2. Add nested `.ts` and JSX-heavy `.tsx` fixtures plus full-forest golden assertions covering every named declaration, exact address, sibling order, and 1-based span; include deliberate non-elements and empty/anonymous-name negatives.
3. Add the language-agnostic value-shape rule for function-like binding values and the `internal_module` classification decision. Prove six binding shapes, non-function negatives, no double emission, namespace scoping, and every TypeScript classify/non-classify decision; record remove-rule mutation failures.
4. Run the parsers/core regression set and clippy, then request the exclusive `harness checks` slot from o-prime. Update deterministic task state and execution receipts only through the global `ddocs` CLI.
5. Scan `~/pi-hacking/pij/.pi/extensions/pij/**/*.ts` through parsers only, produce the reproducible per-file before/after element table and o-prime's exact post-bounce prod commands, commit with `harness commit`, open the PR, and send the durable done report. No database, daemon, store, CLI, prod mutation, or merge.

## Operating checks

- `flowspace3 search` found `Language::grammar` and the plan architecture from the worktree index; I will continue semantic-first dogfooding and report misses/confusing envelopes immediately.
- rust-analyzer is configured and available (`configured, not started` before first symbol request); no LSP availability exception is needed.
- Harness 0.14.0 is installed. `harness doctor --json` is degraded at its extensions/convention layer. `harness boot --json` completed after acknowledgement: toolchain/crate/build passed (`cargo build --all-targets`); only compose was degraded because the database is intentionally not running for this parsers/core-only packet.
