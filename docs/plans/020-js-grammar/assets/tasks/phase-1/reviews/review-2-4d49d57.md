APPROVE 4d49d57a81876615ea65928f8da87b9ffc21253d

# Re-review — plan 020 Phase 1 fix round 1 (089cf2f..4d49d57)
Reviewer: pij-southern-bovid (claude-opus-5.5). Ran read-only in a detached worktree; fork head was verified as 4d49d57.

## Findings closed
- **F001 CLOSED.** `identifier_or_string` now returns None when the unquoted key contains `ADDRESS_SEGMENT`. The guard sits only on the binding path: `key`/`property`/member `property` via `binding_name`. TS `declare module` naming is unaffected.
  - Trap `"x::y": () => {}` was added to sample.js, and `x::y` was added to the invent-nothing list. The golden shows it spliced with no row.
- **F002 CLOSED.** `export default class { m() {} }` was added to the fixture. The nameless class is spliced, and `m` appears at file scope with the address `sample.js::m #4 16-16`.
- **F003/F004 CLOSED (documented).** The PR body now has a "Known limitations" section covering both.

## Dim-0 receipts (F001 guard)
- Baseline: lib 45 ok, fixture_elements 16 ok.
- Mutation (removed `&& !unquoted.contains(ADDRESS_SEGMENT)`): RED. Two tests failed, `javascript_fixture_yields_the_expected_tree` (:64) and `javascript_fixtures_invent_nothing_and_never_emit_blank_identity` (:269).
- Restored (git checkout): GREEN, 45 + 16.

## Rider and hygiene
- `git diff b7774df...4d49d57 -- crates/parsers/tests/fixture_elements.rs | grep -c '^-[^-]'` returns 0.
  - The delta's `-` lines are only within the new JS golden, which was renumbered for the 3 inserted lines.
- Fixture diff since b7774df touches only the new sample.js and sample.jsx.
- `cargo fmt --all --check` ok. `cargo clippy -p fs3-parsers --all-targets -D warnings` ok.

## Non-blocking notes (nothing new introduced)
- N1 (info): sample.js now has two `export default` statements. That is invalid as an ES module, but tree-sitter accepts it and the golden pins the behaviour. Acceptable for a fixture.
- N2 (pre-existing, out of scope): TS `declare module "a::b"` keeps the quoted name through `first_identifier_text`. It could carry `::` into an address. This was on main before plan 020 and is worth a follow-up issue, not this PR.
