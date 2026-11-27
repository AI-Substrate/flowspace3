# Code Review: Phase 1: JavaScript + JSX grammar

**Plan**: docs/plans/020-js-grammar/plan.dd.md (plan.dd.json)
**Phase**: Phase 1: JavaScript + JSX grammar
**Head**: 089cf2f3f2c0c9cc2b1d16f3722b93d90d6cd7c4 (branch s020/js-grammar; `git ls-remote fork` matches) · **Base**: b7774df
**Date**: 2026-11-14 · **Reviewer**: pij-southern-bovid (claude-opus-5.5, cold cross-model; coder gpt-5.6-sol)
**Testing Approach**: TDD with whole-tree goldens + mutation receipts

## A) Verdict

**APPROVE** (zero HIGH/CRITICAL; one MEDIUM and three LOW notes, none blocking)

## B) Summary

Clean, minimal, language-agnostic change. JavaScript is wired through dep, allowlist, extension map, grammar, discovery family and scan dispatch with no `Language::` branch in extraction. The widened value-shape rule is correctly guarded: the binding-only name fields (`key`/`property`/`left`) only answer for FUNCTION_BINDINGS nodes, and FUNCTION_VALUES holds only JS/TS function kinds, so Python `pair`/lambda and Rust `assignment_expression`/closures are provably unaffected (probe below). The goldens match the AC-0001..0003 contract row-for-row; they are not snapshot-shaped bug encodings. `scan` stays pure (no IO added). The fence is respected. The PR body names the TS gain and the #180 reparse. The probe artefacts are real.

## C) Checklist

- [x] Goldens derived from contract (AC-0001/0002/0003 constructs each present as a row or a refused trap)
- [x] RED→GREEN mutation receipts reproduced independently (Dim-0 below)
- [x] Rider: no removed lines in fixture_elements.rs; five existing fixtures byte-unchanged
- [x] No cross-grammar drift beyond the documented TS gain (probe)
- [x] `scan` purity unchanged; arch allowlist +1 line
- [x] PR body honest (TS gain, #180 reparse, non-goals)
- [x] Probe receipt real (dodo-head.tsv 84 rows; main.js 1→21, init/update/draw true; probe path-deps crates/parsers)
- N/A Domain compliance (no docs/domains); N/A project-rules (none)

## D) Findings

| id | sev | file:line | issue | fix |
|---|---|---|---|---|
| F001 | MEDIUM | crates/parsers/src/source.rs:333-347 (`identifier_or_string`) | An unquoted string key may contain the address separator: `{"x::y": () => {}}` yields address `b.js::x::y`, indistinguishable from an element `y` nested under `x`. The new unquoting is the first path by which a name can carry `::`. Today's only downstream address split (daemon/src/search.rs:937) takes the file prefix only, so impact is limited to ambiguous `find`/display. | In `identifier_or_string`, refuse (return None → spliced) when `unquoted.contains(ADDRESS_SEGMENT)`; add `'a::b': () => {}` to the invents-nothing traps. Can be a follow-up. |
| F002 | LOW | crates/parsers/fixtures/sample.js | AC-0003 lists an "anonymous default-export class" trap, but the fixture has no `export default class {}` (only `const AnonymousExpression = class {}`). Behaviour is correct (my probe: no element, no blank name); only the fixture coverage is missing. | Add `export default class {}` in a second fixture or a unit test (one file can hold only one default export). |
| F003 | LOW | crates/parsers/src/source.rs:340-346 | Escaped quotes stay raw: `{'it\'s': ...}` names `it\'s`. Cosmetic. | Optional: leave as is, or reject keys containing `\`. |
| F004 | LOW | crates/parsers/fixtures/sample.js:45 golden | `module.exports = function () {}` is named `exports` (contract-correct per AC-0002 member→property). Noted so search consumers expect many `::exports` elements in CommonJS repos. | None required; consider mentioning in scanner.md. |

## E) Dim-0 receipts (mutation, in detached review worktree, restored after each)

Command: `perl -e 'alarm shift @ARGV; exec @ARGV' 1200 nice -n 19 cargo test -p fs3-parsers -j 4 --no-fail-fast --lib --test fixture_elements -- --test-threads 8`

| mutation | result | failing assertions |
|---|---|---|
| (a) left-kind guard: `_ => None` → `_ => first_identifier_text(node, source)` (source.rs:325) | RED exit 101 | `javascript_fixture_yields_the_expected_tree` (fixture_elements.rs:64, extra row); `javascript_fixtures_invent_nothing_and_never_emit_blank_identity` (:275) "subscript binding handlers[computed] must not become an element" |
| (b1) drop `"pair"` from FUNCTION_BINDINGS | RED exit 101 | `typescript_uses_the_same_pair_and_assignment_binding_rule` (source.rs:534); `javascript_fixture_yields_the_expected_tree` |
| (b2) drop `"assignment_expression"` from FUNCTION_BINDINGS | RED exit 101 | `typescript_uses_the_same_pair_and_assignment_binding_rule` (source.rs:538); `javascript_fixture_yields_the_expected_tree` |
| (c) drop `"js" \| "mjs" \| "cjs" \| "jsx"` arm (lib.rs:72) | RED exit 101 | `extensions_map_to_grammars` (lib.rs:227 left None right Some(JavaScript)); `jsx_fixture_yields_the_expected_tree` + `javascript_fixture_yields_the_expected_tree` (left `["file unknown …"]`) |
| restored | GREEN exit 0 | lib 45 passed; fixture_elements 16 passed; `git status --porcelain` clean |

Full packet command (`cargo test -p fs3-parsers`, no target filter) baseline: lib 45 ok, cross_filter 1 ok, **ddoc 2 FAILED**: `discovery_admits_only_the_ddoc_source` and `watcher_subtree_discovery_uses_the_same_ddoc_admission`. These are environmental, not the change: the mandated review worktree sits under `.harness/temp/`, which the parent repo ignores (`.gitignore:45`), and discovery honours that, so the fixture directory admits 0 files. Observed as harness DL-008. The coder's isolated full gate (harness checks 11/11) ran outside that path.

## F) Cross-grammar probe (temporary test, deleted)

- `d.py` `{"k": lambda: 1}`, `x = lambda: 2` → file only (unchanged).
- `e.rs` closure assignment → only `m` (unchanged).
- `c.ts` `declare module "x"` → still `"x"` with quotes (M3 option (a) holds); `{ k: () => 1 }` → `c.ts::k` (documented TS gain).
- JS: `export default class {}` / `export default () => {}` → nothing; `obj.a.b = fn` → `b`; constructor `this.handler = fn` → `S::constructor::handler`; `obj?.c = fn` → `c`; numeric key and `''` key → nothing; `'a b'` → `a b`; `"x::y"` → `x::y` (F001).

## G) Rider / fence / docs

- `git diff b7774df...089cf2f -- crates/parsers/tests/fixture_elements.rs | grep -c '^-[^-]'` → 0; `git diff --stat … sample.{rs,py,ts,tsx,md}` → empty.
- Fence (`--name-only`, non-plan): SKILL.md (ruling 5), Cargo.toml/lock, daemon/src/scan.rs (ruling 1), parsers/{Cargo.toml,fixtures/sample.js,sample.jsx,src/discovery.rs,lib.rs,source.rs,tests/fixture_elements.rs}, testkit/arch-allowlist.toml, docs/services/scanner.md. All in scope.
- PARSER_VERSION @5 with history line naming plan 020. There are no stale `@4` references in code.

## H) Handover

APPROVE 089cf2f. F001 is worth a one-line guard before merge if the PM wants it; otherwise it can be a follow-up. Nothing else blocks.
