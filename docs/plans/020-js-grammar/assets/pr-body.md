## Problem

JavaScript and JSX files currently scan as one `unknown` file element, so real
repositories such as dodo-gogo expose no function- or class-level search
surface.

## Change

- Add dedicated `tree-sitter-javascript 0.25` support for
  `.js`, `.mjs`, `.cjs`, and `.jsx`, reporting language `javascript`.
- Add JavaScript and JSX whole-tree fixtures covering declarations, classes,
  methods, function-valued bindings, ImpactJS object pairs, and negative traps.
- Widen the generic value-shape rule to promote function-valued
  `field_definition`, `pair`, and `assignment_expression` nodes using guarded
  binding names.
- Bump `PARSER_VERSION` to `fs3-parsers@5`.
- Update the scanner contract and add-language recipe.

## Design calls

JavaScript uses its dedicated grammar rather than the TypeScript grammar.
JavaScript syntax has ambiguities the TypeScript grammar interprets
differently, and reporting `.js` files as `typescript` would make the language
contract false.

The value-shape widening is deliberately language-agnostic. TypeScript files
therefore gain the same function-valued object-pair and assignment callables.
String unquoting is confined to binding keys; existing generic name extraction
is unchanged.

## Upgrade consequence

Since #180, changing `PARSER_VERSION` causes every registered root to be
reparsed once when the upgraded daemon next boots. Existing element text keeps
its enrichment through the `raw_hash` key, but newly discovered JavaScript
elements on JS repositories require new summaries and embeddings.

## Evidence

- `cargo build -p fs3-parsers`: `tree-sitter-javascript 0.25.0` compiled with
  workspace `tree-sitter 0.26.13`.
- Parser suite: 45 unit and 61 integration tests passed.
- Mutation receipts: removing each of `field_definition`, `pair`,
  `assignment_expression`, `right`, member-property naming, string unquoting,
  and the left-kind guard produced its named RED; each restoration returned
  GREEN.
- Existing Rust, Python, TypeScript, TSX, and Markdown fixtures are byte
  unchanged from `b7774df`; the fixture-test diff removes no lines.
- dodo-gogo probe: 84/84 JavaScript files accepted, 84 `unknown` file elements
  before and 1,170 `javascript` elements after, 1,100 callable syntax nodes,
  14 deliberately nameless shapes, and 0 scan errors.
- `game/impact/lib/game/main.js`: 1 -> 21 elements and includes `init`,
  `update`, and `draw`.

The complete per-file table and mutation assertions are in
`docs/plans/020-js-grammar/assets/tasks/phase-1/execution.log.md`.

## Non-goals

- No `.extend()` framework-call heuristic or synthetic container.
- No object-literal name scoping; pair methods attach to the nearest real
  declaration.
- No Flow/JSDoc type semantics, module resolution, store schema, or live
  re-ingest.

Closes nothing - no issue opened.
