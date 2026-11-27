# Phase 1 execution log

## T001 — JavaScript grammar wiring

Added `tree-sitter-javascript 0.25` to the workspace and parser crate, mirrored
the dependency in the architecture allow-list, and registered one
`Language::JavaScript` for `js`, `mjs`, `cjs`, and `jsx`. JavaScript now reports
the stable language name `javascript`, uses the dedicated grammar, enters the
generic source walk, and remains in the source discovery family.

Evidence:
- `cargo build -p fs3-parsers -j 4`: resolved
  `tree-sitter-javascript 0.25.0` with `tree-sitter 0.26.13`; build passed.
- `cargo test -p fs3-parsers --lib extensions_map_to_grammars -j 4 --
  --test-threads 8`: 1 passed.
- `cargo run -p fs3-testkit --bin fs3-arch-check -j 4`: 8 crates, 98 direct
  edges, 0 violations.

## T002 — JavaScript and JSX fixture contract

Added nested JavaScript and JSX fixtures plus exact whole-tree expectations.
The JavaScript fixture covers declarations, generators, async functions,
classes and class expressions, class members, function-valued bindings,
object-literal methods and pairs, member assignments, the ImpactJS
`.defines(...extend({...}))` shape, and the planned grep/anonymous/computed
traps. The JSX fixture covers function, arrow, and class components with a
JSX-heavy body and a class-field arrow.

RED evidence:
- `cargo test -p fs3-parsers --test fixture_elements -j 4 --
  --test-threads 8`: 16 tests run, 14 passed, 2 failed.
- Only `javascript_fixture_yields_the_expected_tree` and
  `jsx_fixture_yields_the_expected_tree` failed.
- The JavaScript diff was exactly the planned missing `field_definition`,
  `pair`, and `assignment_expression` rows. The JSX diff was exactly the
  missing `field_definition` row.
- `javascript_fixtures_invent_nothing_and_never_emit_blank_identity` passed,
  and both golden tests reached their equality assertion after proving
  `has_error == false`.

## T003 — Generic value-shape widening

Widened the language-agnostic source walk so function-valued
`field_definition`, `pair`, and `assignment_expression` bindings become
Function elements. Values are read from ordered fields `value`, then `right`.
Binding names use guarded `key`, `property`, and `left` fields: identifiers are
accepted, string keys are unquoted locally, member-expression left sides use
their property, and subscript/computed/pattern left sides remain nameless and
are spliced through. `type` remains the last generic name field.

Evidence:
- `cargo test -p fs3-parsers -j 4 -- --test-threads 8`: all parser tests passed
  (45 unit tests plus 61 integration tests).
- `javascript_classification_decisions_use_the_generic_classifier` pins the JS
  classify decisions without a language branch or core classifier change.
- `typescript_uses_the_same_pair_and_assignment_binding_rule` proves a `.ts`
  pair and member assignment use the same generic rule.

Mutation receipts (each mutation was restored and the same targeted command
returned GREEN):

| Widening removed | Command | RED assertion |
| --- | --- | --- |
| `field_definition` binding | `cargo test -p fs3-parsers --test fixture_elements jsx_fixture_yields_the_expected_tree` | `ClassComponent::handleClick` vanished and `render` shifted to sibling 0. |
| `pair` binding | `cargo test -p fs3-parsers --test fixture_elements javascript_fixture_yields_the_expected_tree` | `key`, `quoted`, `init`, `update`, and `draw` vanished. |
| `assignment_expression` binding | same JavaScript golden command | `h`, `bar`, and `exports` vanished. |
| `right` value field | same JavaScript golden command | all three assignment rows vanished while pair rows remained. |
| member-expression → `property` naming | same JavaScript golden command | all three member-assignment rows vanished. |
| string-key unquoting | same JavaScript golden command | the row became `sample.js::'quoted'` instead of `sample.js::quoted`. |
| `left` kind guard | `cargo test -p fs3-parsers --test fixture_elements javascript_fixtures_invent_nothing_and_never_emit_blank_identity` | failed: `subscript binding handlers[computed] must not become an element`. |

## T004 — Cross-language non-regression

Evidence:
- `git diff b7774df -- crates/parsers/tests/fixture_elements.rs` contained no
  removed source lines; the JavaScript coverage is additive.
- `git diff --exit-code b7774df --
  crates/parsers/fixtures/sample.{rs,py,ts,tsx,md}` was empty.
- `git diff --name-only b7774df -- crates/core/src/classify.rs` was empty.
- `cargo test -p fs3-parsers -j 4 -- --test-threads 8` remained fully GREEN:
  45 unit tests and 61 integration tests passed.

## T005 — Parser identity and documentation

Bumped `PARSER_VERSION` from `fs3-parsers@4` to `fs3-parsers@5` with the plan
020 history line naming the JavaScript grammar set and widened value-shape
rule. Updated the scanner service contract to list JavaScript/JSX and explain
the language-agnostic binding rule. Updated the parser module documentation to
six grammars and corrected the add-language skill to include the parser-version
step.

Evidence:
- `grep -n 'fs3-parsers@5' crates/daemon/src/scan.rs`: constant found at line
  283.
- `rg -l 'fs3-parsers@5' crates --glob '*.rs'`: exactly one Rust source file
  contains the literal.
- `cargo fmt --all --check`: passed.
- Documentation search found JavaScript, the value-shape rule, and the six-step
  add-language recipe in their required artifacts.

## T006 — dodo-gogo real-usage probe

Created a detached `b7774df` worktree and a read-only-source copy of
`~/GitHub/dodo-gogo` under `.harness/temp/s020-js-grammar/`. A throwaway Cargo
probe scanned the same 84 `.js` files once with the base parser and once with
the current parser, using separate target directories and no daemon or
database.

Totals:
- Base: 84 files accepted, language `unknown`, 84 elements, 0 scan errors.
- Current: 84 files accepted, language `javascript`, 1,170 elements, 0 scan
  errors.
- Syntax census: 1,100 callable nodes, 14 nameless callables; every file met
  `after >= 1 + callable_nodes - nameless`.
- `game/impact/lib/game/main.js`: 1 -> 21 elements for 20 callable nodes; the
  resulting names include `init`, `update`, and `draw`.
- Scratch evidence:
  `.harness/temp/s020-js-grammar/dodo-{base,head,combined}.tsv`,
  `.harness/temp/s020-js-grammar/dodo-{base,head}.stderr`, and
  `.harness/temp/s020-js-grammar/probe/`.

The 14 nameless callables are all in `game/js/jquery.1.7.1.js`: function
expressions assigned through `subscript_expression` left sides at lines 3827,
5520, 6143, 7093, 7099, 7879, 7894, 8564, 8838, 9125, 9178, 9188, and 9197;
and line 4312, whose pair key is the empty string. Both shapes deliberately
lack a safe non-empty short name.

| File | Language | Elements | Callable nodes | Nameless |
| --- | --- | ---: | ---: | ---: |
| `game/impact/lib/game/classes/branch-pool.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/classes/particle-emitter.js` | unknown -> javascript | 1 -> 35 | 34 | 0 |
| `game/impact/lib/game/classes/particle.js` | unknown -> javascript | 1 -> 53 | 52 | 0 |
| `game/impact/lib/game/classes/random-distribution.js` | unknown -> javascript | 1 -> 5 | 4 | 0 |
| `game/impact/lib/game/classes/random.js` | unknown -> javascript | 1 -> 7 | 6 | 0 |
| `game/impact/lib/game/classes/score-manager.js` | unknown -> javascript | 1 -> 10 | 9 | 0 |
| `game/impact/lib/game/classes/settings.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/classes/state-handler.js` | unknown -> javascript | 1 -> 5 | 4 | 0 |
| `game/impact/lib/game/classes/store.js` | unknown -> javascript | 1 -> 8 | 7 | 0 |
| `game/impact/lib/game/classes/water.js` | unknown -> javascript | 1 -> 7 | 6 | 0 |
| `game/impact/lib/game/entities/activity-bounce.js` | unknown -> javascript | 1 -> 16 | 15 | 0 |
| `game/impact/lib/game/entities/activity-high-score.js` | unknown -> javascript | 1 -> 9 | 8 | 0 |
| `game/impact/lib/game/entities/activity-splash.js` | unknown -> javascript | 1 -> 10 | 9 | 0 |
| `game/impact/lib/game/entities/big-splash-dude.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/entities/bindable.js` | unknown -> javascript | 1 -> 6 | 5 | 0 |
| `game/impact/lib/game/entities/bird-menu.js` | unknown -> javascript | 1 -> 8 | 7 | 0 |
| `game/impact/lib/game/entities/blast.js` | unknown -> javascript | 1 -> 5 | 4 | 0 |
| `game/impact/lib/game/entities/bonus-angel-active.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/entities/bonus-fly-active.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/entities/bonus.js` | unknown -> javascript | 1 -> 14 | 13 | 0 |
| `game/impact/lib/game/entities/boulder-large.js` | unknown -> javascript | 1 -> 6 | 5 | 0 |
| `game/impact/lib/game/entities/boulder-small.js` | unknown -> javascript | 1 -> 5 | 4 | 0 |
| `game/impact/lib/game/entities/bounce-player.js` | unknown -> javascript | 1 -> 18 | 17 | 0 |
| `game/impact/lib/game/entities/branch.js` | unknown -> javascript | 1 -> 17 | 16 | 0 |
| `game/impact/lib/game/entities/button-direction.js` | unknown -> javascript | 1 -> 9 | 8 | 0 |
| `game/impact/lib/game/entities/button-music.js` | unknown -> javascript | 1 -> 11 | 10 | 0 |
| `game/impact/lib/game/entities/button.js` | unknown -> javascript | 1 -> 8 | 7 | 0 |
| `game/impact/lib/game/entities/controller.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/entities/dead-bird.js` | unknown -> javascript | 1 -> 6 | 5 | 0 |
| `game/impact/lib/game/entities/death-bird.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/entities/dialog.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/entities/hi-score.js` | unknown -> javascript | 1 -> 5 | 4 | 0 |
| `game/impact/lib/game/entities/image.js` | unknown -> javascript | 1 -> 6 | 5 | 0 |
| `game/impact/lib/game/entities/label.js` | unknown -> javascript | 1 -> 7 | 6 | 0 |
| `game/impact/lib/game/entities/logo.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/entities/margin-manager.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/entities/next-level.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/entities/particle.js` | unknown -> javascript | 1 -> 5 | 4 | 0 |
| `game/impact/lib/game/entities/pause-button.js` | unknown -> javascript | 1 -> 8 | 7 | 0 |
| `game/impact/lib/game/entities/play-button.js` | unknown -> javascript | 1 -> 5 | 4 | 0 |
| `game/impact/lib/game/entities/pointer.js` | unknown -> javascript | 1 -> 17 | 16 | 0 |
| `game/impact/lib/game/entities/score.js` | unknown -> javascript | 1 -> 7 | 6 | 0 |
| `game/impact/lib/game/entities/smoke.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/entities/target-position.js` | unknown -> javascript | 1 -> 2 | 1 | 0 |
| `game/impact/lib/game/entities/trigger.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/game/entities/zoom-activity.js` | unknown -> javascript | 1 -> 5 | 4 | 0 |
| `game/impact/lib/game/levels/bounce.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/levels/glidinggame.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/levels/highscore.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/levels/level01.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/levels/level02.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/levels/level03.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/levels/menu.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/levels/ricochet.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/levels/splash.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/levels/testmenu.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/levels/towerdefence.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/levels/untitled.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/game/main.js` | unknown -> javascript | 1 -> 21 | 20 | 0 |
| `game/impact/lib/impact/animation.js` | unknown -> javascript | 1 -> 8 | 7 | 0 |
| `game/impact/lib/impact/background-map.js` | unknown -> javascript | 1 -> 9 | 8 | 0 |
| `game/impact/lib/impact/collision-map.js` | unknown -> javascript | 1 -> 5 | 4 | 0 |
| `game/impact/lib/impact/debug/debug.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/impact/lib/impact/debug/entities-panel.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/impact/debug/graph-panel.js` | unknown -> javascript | 1 -> 12 | 11 | 0 |
| `game/impact/lib/impact/debug/maps-panel.js` | unknown -> javascript | 1 -> 6 | 5 | 0 |
| `game/impact/lib/impact/debug/menu.js` | unknown -> javascript | 1 -> 21 | 20 | 0 |
| `game/impact/lib/impact/entity.js` | unknown -> javascript | 1 -> 19 | 18 | 0 |
| `game/impact/lib/impact/font.js` | unknown -> javascript | 1 -> 6 | 5 | 0 |
| `game/impact/lib/impact/game.js` | unknown -> javascript | 1 -> 17 | 16 | 0 |
| `game/impact/lib/impact/image.js` | unknown -> javascript | 1 -> 11 | 10 | 0 |
| `game/impact/lib/impact/impact.js` | unknown -> javascript | 1 -> 36 | 35 | 0 |
| `game/impact/lib/impact/input.js` | unknown -> javascript | 1 -> 43 | 42 | 0 |
| `game/impact/lib/impact/loader.js` | unknown -> javascript | 1 -> 9 | 8 | 0 |
| `game/impact/lib/impact/map.js` | unknown -> javascript | 1 -> 4 | 3 | 0 |
| `game/impact/lib/impact/sound.js` | unknown -> javascript | 1 -> 36 | 35 | 0 |
| `game/impact/lib/impact/system.js` | unknown -> javascript | 1 -> 11 | 10 | 0 |
| `game/impact/lib/impact/timer.js` | unknown -> javascript | 1 -> 7 | 6 | 0 |
| `game/js/Messenger.js` | unknown -> javascript | 1 -> 6 | 5 | 0 |
| `game/js/jquery.1.7.1.js` | unknown -> javascript | 1 -> 390 | 403 | 14 |
| `game/js/name-generator.js` | unknown -> javascript | 1 -> 22 | 21 | 0 |
| `game/js/simpleinheritance.js` | unknown -> javascript | 1 -> 1 | 0 | 0 |
| `game/js/tween.js` | unknown -> javascript | 1 -> 54 | 53 | 0 |
| `game/js/utility.js` | unknown -> javascript | 1 -> 2 | 1 | 0 |

## T007 — Gate and handoff

The full quality gate ran against an isolated `fs3-s020` compose project with
`db-test` bound only to `127.0.0.1:5444`. The worktree-local override stayed
under `.harness/temp/`; after the gate, `docker compose -p fs3-s020 down -v`
removed its container, network, and volume and released port 5444.

The first isolated run exposed one environment dependency:
`ddoc_notice_distinguishes_unprobed_absent_and_healthy_after_restart` could not
find `ddocs` on PATH. The pinned binary already existed at
`~/GitHub/harness-engineering/node_modules/.bin/ddocs`; adding that directory
to the gate process PATH made the targeted test pass.

Pre-close gate verdict:
- `harness checks`: `status=ok`, 11/11 gates passed at
  `2026-11-14T03:36:43.924Z`.
- Passed: docs links, locked metadata, test-DB guard, harness contract, daemon
  bounce contract, fmt, clippy, production guard before, full test suite,
  production guard after, and architecture drift.
- Feature commit: `5bb486167f8582c9da59eb2b843804a258d904c8`
  (`feat(parsers): add JavaScript and JSX grammar`), collector attribution
  verified.
- Final exact-HEAD gate output:
  `.harness/temp/s020-js-grammar/harness-checks-final.json`.
- PR body draft: `docs/plans/020-js-grammar/assets/pr-body.md`.

## Fix round 1 — reviewer findings

F001 now refuses quoted binding keys whose unquoted value contains
`fs3_core::ADDRESS_SEGMENT`. The fixture trap `"x::y": () => {}` is therefore
nameless and spliced instead of forging the address
`parsers/fixtures/sample.js::x::y`.

Evidence:
- Before the guard, both the JavaScript whole-tree golden and
  `javascript_fixtures_invent_nothing_and_never_emit_blank_identity` failed on
  the invented `function pair ...::x::y` row.
- With the guard, both targeted tests passed.
- Mutation receipt: removing only the separator guard made
  `javascript_fixtures_invent_nothing_and_never_emit_blank_identity` fail with
  `trap x::y must not become an element`; restoring it returned GREEN.

F002 adds `export default class { m() {} }`. The nameless class is spliced and
its method appears at file scope as
`function method_definition parsers/fixtures/sample.js::m #4 16-16`.

F003/F004 are documented as known limitations in the PR body: escaped quotes
in string keys remain raw, and `module.exports = fn` is contract-correctly
named `exports` under the non-unique-address contract.

Proof:
- `cargo test -p fs3-parsers -j 4 -- --test-threads 8`: 45 unit tests and 61
  integration tests passed.
- `cargo run -p fs3-testkit --bin fs3-arch-check -j 4`: 8 crates, 98 direct
  edges, 0 violations.
- The fixture-test diff still removes no lines relative to `b7774df`; existing
  Rust, Python, TypeScript, TSX, and Markdown fixtures remain byte-identical.
- `harness checks`: `status=ok`, 11/11 gates passed at
  `2026-11-14T04:02:41.587Z` against the isolated `fs3-s020` database on
  `127.0.0.1:5444`.
- Gate output:
  `.harness/temp/s020-js-grammar/harness-checks-fix-1-precommit.json`.
- `docker compose -p fs3-s020 down -v` removed the isolated container,
  network, and volume and released port 5444.

## Phase-complete summary

T001-T007 delivered the dedicated JavaScript/JSX grammar, additive whole-tree
goldens, the guarded generic binding widening with seven mutation receipts,
cross-language non-regression, parser identity/docs updates, the 84-file
dodo-gogo probe, scoped commits, the isolated full gate, and fork-branch
handoff. No live daemon or database was intentionally used for test work after
the corrected port ruling; no PR or issue was opened.

## Discoveries & learnings

| Tag | Discovery | Disposition |
| --- | --- | --- |
| Noteworthy | `harness boot` prescribes starting the live `:5433` stack, while this packet explicitly forbids touching it. | Captured as `DL-003`; retained the degraded boot verdict and confined proof to parser-only/test-compose surfaces. |
| Noteworthy | The current worktree is not indexed by flowspace3; semantic search warned that it answered from unrelated repositories, while the packet forbids `flowspace3 add` because it writes the live DB. | Captured as `DL-004`; used the warned result as dogfood evidence and continued with exact local reads. |
| Noteworthy | Builder implement requires an in-progress task state, but the deterministic schema permits only `unchecked`, `checked`, `blocked`, `human-skipped`, or `na`. | Captured as `CONF-001`; tasks stay `unchecked` until evidence permits `checked`. |
| Noteworthy | This worktree's default `db-test` compose service could not bind fixed host port `5434`. | Captured as `DL-005`; o-prime corrected both `5433` and `5434` as live ports, so the final gate used an untracked override and isolated `fs3-s020` project on `127.0.0.1:5444`. |
| Noteworthy | The original packet's live/test port ownership was stale; a test-db check and harness gate had begun against live `:5434` before the correction arrived. | Captured as `COORD-001`; stopped immediately, disclosed exact timing/URL, and o-prime verified no test DBs, migrations, or damage. |
| Noteworthy | The full gate's `read_surface` test requires `ddocs` on PATH, while this repo intentionally has no local install. | Captured as `DL-007`; added the existing pinned `~/GitHub/harness-engineering/node_modules/.bin` to the gate process PATH, proved the failing test, then reran the full gate. |
