# Delegation packet — tree-sitter AST extraction POC (Rust)
**From**: pij-instant-lynx (o-prime, flowspace3) · **To**: pij-likely-sailfish · **Date**: 2026-08-26
**Plan**: docs/plans/001-fs3-foundations · **Type**: Spike/POC (throwaway code, real captured output)

## Mission

Prove that direct tree-sitter-in-Rust AST extraction works across multiple file types and is
fast: build a small throwaway Rust program that parses source files into fs3-style elements
(functions, methods, classes/types, markdown heading sections), report timings, then parse a
large real repo and sanity-check accuracy. This de-risks base-prd reqs 2, 3, 21, 22 before
the plan pass.

## Context (read these first)

- `docs/plans/001-fs3-foundations/base-prd.md` — esp. reqs 2, 3, 21, 22
- `docs/plans/001-fs3-foundations/assets/workshops/001-architecture.md` — parser is CONCRETE
  (no trait); universal element-kind classification over raw tree-sitter kinds
- fs2 prior art (READ-ONLY): `/Users/jordanknight/substrate/fs2/flow_squared` — its
  `src/fs2/core/models/code_node.py` `classify_node()` shows the universal-kind mapping;
  its test fixtures / sample files are your multi-language corpus — find them (look under
  `tests/`, search for fixture/sample dirs) and note which you used.

## The work

1. **POC lives at** `docs/plans/001-fs3-foundations/assets/poc/treesitter/` — a standalone
   cargo project (bin). Throwaway rules: smallest code that answers the questions, no error
   handling polish, no tests, nothing outside this dir imports it.
2. Wire tree-sitter **directly** (crates: `tree-sitter` + per-language grammar crates — your
   choice which set; cover at least: Rust, Python, TypeScript/JavaScript, C#, Go, Markdown;
   more if cheap). Extract per file: elements with (kind [raw ts kind + universal category],
   qualified name where derivable, start/end lines). Markdown: heading sections with nesting.
3. **Fixture pass**: run over the fs2 sample files; print per-file element tables for a few
   representative files so accuracy is eyeballable; capture real output.
4. **Timing pass**: parse timings per file and aggregate (files/sec, MB/sec), single-thread
   AND parallel (rayon). Cheap harness: `std::time::Instant`, release build.
5. **Large-repo pass**: parse `/Users/jordanknight/substrate/harness-engineering` (READ-ONLY)
   — total files parsed, per-language counts, element counts, errors/panics/skips, wall time
   (single + parallel), and 3–5 spot-checks where you manually verify a known file's elements
   look right (paste the evidence).
6. **Results doc**: write `docs/plans/001-fs3-foundations/assets/poc/treesitter-results.md`
   using the Spike/POC shape: Question · What We Ran (real captured output) · Verdict
   (PROVEN/DISPROVEN/PARTIAL per claim, table) · Timings tables · Learnings to Promote
   (constraints that should land in the plan) · Discarded note.

## Allowed paths (create/modify ONLY here)

- `docs/plans/001-fs3-foundations/assets/poc/**` (cargo project + results doc)

## Read-only

- `/Users/jordanknight/substrate/fs2/flow_squared` · `/Users/jordanknight/substrate/harness-engineering`
- everything else in flowspace3

## Forbidden (never read or write)

- `.the-flow-state.json`, `the-flow.json`, `the-flow.md` (any location)
- `.harness/government/**` · `base-prd.md` (no edits) · any git commit/push

## Done-report (send to pij-instant-lynx, § C10: outcome first line)

```json
{ "outcome": "COMPLETE|PARTIAL|BLOCKED",
  "summary": "verdict headline + fastest/slowest numbers",
  "filesChanged": ["assets/poc/…"],
  "resultsDoc": "docs/plans/001-fs3-foundations/assets/poc/treesitter-results.md",
  "timingsHeadline": "harness-engineering: N files, X.Xs parallel (Y files/s)",
  "notes": "grammar-coverage gotchas / crate-choice learnings / accuracy misses" }
```
