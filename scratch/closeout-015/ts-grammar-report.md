# ts-grammar DONE report

## Delivery

- Commit: `3649c0fdbe96e1cd4d47b4b76fa6bca58a82d1b1`
- PR: https://github.com/AI-Substrate/flowspace3/pull/102
- Branch: `015-ts-grammar`
- Exclusive local gate slot: **released**.
- Hold: no further pushes while the detached Claude reviewer evaluates `3649c0f`; CI passed on that exact SHA.

## What shipped

- `tree-sitter-typescript 0.23.2` wired against workspace tree-sitter 0.26.
- `.ts`, `.mts`, `.cts` → `Language::TypeScript`; `.tsx` → `Language::Tsx`.
- TypeScript declarations, namespace containers, methods/signatures, and nested addresses extracted through the existing generic walk.
- Function-valued `variable_declarator` / `public_field_definition` nodes promoted as Function elements only when their `value` is `arrow_function`, `function_expression`, or `generator_function`; no language branch.
- Full TypeScript/TSX golden forests, JSX-heavy clean parse, deliberate non-elements, non-function binding negatives, and non-empty identity assertions.
- Architecture allow-list edge and exact grammar snap-in recipe.

## Evidence

- `cargo build -p fs3-parsers`: PASS; `tree-sitter-typescript v0.23.2` compiled with tree-sitter 0.26.
- `cargo test -p fs3-parsers --test fixture_elements`: 13 passed.
- `cargo test -p fs3-core classify`: 10 passed across 3 suites.
- `cargo test -p fs3-core -p fs3-parsers`: 311 passed across 12 suites.
- Value-shape mutation: removing the rule removed exactly six Function elements and failed the golden; restored.
- Namespace mutation: removing `internal_module` flattened members and failed addresses/sibling order; restored.
- Local full gate: timed out under host load after 734.93s (`fs3-test-suite` exit 124), with no assertion failure in captured output. O-prime ruled CI green on the exact PR SHA as the gate; no local retry.
- GitHub Actions `gate`: PASS in 4m59s on exact head `3649c0fdbe96e1cd4d47b4b76fa6bca58a82d1b1` (run 33594520431, job 100135163848).

## Pre-production scan

The complete 327-row table is in the PR body and `.harness/temp/agent/ts-grammar-scan-table.md`.

- Files scanned: 327
- Before: 327 elements (one bare file per path)
- After: 3,452 elements
- Files with extracted symbols: 284
- File-only files: 43 (data-only or anonymous-callback/test shapes remain deliberate non-elements)
- Tree-sitter recovery: 1 file (`core/memorable-id.test.ts`), still 2 elements
- Scan errors: 0

## O-prime production commands

```bash
flowspace3 remove /Users/jordanknight/pi-hacking/pij --json
flowspace3 add /Users/jordanknight/pi-hacking/pij --json
flowspace3 status --json
flowspace3 search "where does the pij extension register the seat at boot" --path '.pi/extensions/pij/**/*.ts' --json
```

Expected: the search returns a Function element from `.pi/extensions/pij/**/*.ts`; TypeScript paths have non-file elements after re-ingest.

## Deviations and noteworthy facts

- `crates/testkit/arch-allowlist.toml` was outside the original fence; ask-001 approved its single required line.
- The DB-backed local gate exceeded the harness suite timeout; ask-003 selected CI exact-SHA proof.
- `docs/services/scanner.md` has no language table to update. The in-scope `Language::for_extension` doc comment now carries the exact five-step snap-in recipe, and the module grammar list names TypeScript/TSX.
- `harness commit` created the commit while collector ingress was connected, but the `refs/notes/ai` verification was missing and no replay buffer existed (`DL-003`).
- The parser-only scan probe was temporary and removed before commit; no diagnostic binary shipped.
- PR body was updated via `gh pr edit` only; no post-review push occurred.

## Assumptions

- O-prime owns deployment, daemon bounce, prod remove/add, re-ingest completion, and the final live search receipt.
- CI evaluates the exact current head SHA `3649c0f`; a later push invalidates that gate and requires a fresh exact-SHA verdict.
- File-only TypeScript inputs intentionally contain no supported named element shape; anonymous callbacks and data-valued bindings remain non-elements by contract.
- Store schema, ElementKind, daemon, CLI, and production envelope behavior remain unchanged.

## Observation buffer — listed, not cleared

- `CONF-001`: rs-resident pushed seats reject `pij inbox --wait`; wait by pushed turn.
- `DL-001`: harness gate database prerequisite conflicted with initial packet wording; o-prime clarified the full DB-backed gate is in scope.
- `DL-002`: full gate exit 124 omitted the active timed-out command and targeted rerun.
- `DL-003`: connected `harness commit` produced no verified AI note and no replay buffer.

Buffer: `.harness/temp/agent/session-buffer.md`. It was listed with `harness observe --list` and **not cleared**.
