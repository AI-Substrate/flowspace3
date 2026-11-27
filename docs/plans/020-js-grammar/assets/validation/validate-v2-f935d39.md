✅ **VALIDATED** — all five findings (M1–M5) are closed and no material new issue was introduced. There is one cosmetic note (N1, below).

- **Target**: `docs/plans/020-js-grammar/plan.dd.json` (plus `tasks.dd.json` and `backpressure.dd.json`) at f935d39. The delta was read with `git diff 2c06fad f935d39 -- docs/plans/020-js-grammar` (word-diff of the JSON sources), and I spot-checked that the generated `.dd.md` views match.
- **Proof**: source reads only, no cargo. I re-checked the new wording against the tree-sitter-javascript 0.25 `node-types.json` field sets (the `assignment_expression.left` kinds; `pair.key` and `field_definition.property` kinds), the dodo-gogo tree, and `git remote -v`.

## Closure check

| Finding | Closed? | Evidence at f935d39 |
|---|---|---|
| M1 — no left-kind guard | ✅ | **AC-0002** now says:<br>• a `left` names only when it is an `identifier`, or a `member_expression` (named by its `property`)<br>• a `key`/`property` names only when it is an identifier-kind or a string<br>• subscript lefts and computed keys are not elements<br>• the left-kind guard has its own mutation<br>**T002** adds the traps `handlers[k] = function(){}` and `exports['x'] = () => {}`.<br>**T003** spells out the guard.<br>**bp-0002** adds the guard mutation ("drop guard → invents-nothing red").<br>The guard matches node-types: `number` and `computed_property_name` keys are excluded; `private_property_identifier` is an identifier-kind. |
| M2 — diff ran against the moving upstream `origin/main` | ✅ | AC-0004, bp-0004, T004 and the coverage map now use `git diff b7774df...HEAD`. They expect no removed lines in `fixture_elements.rs` and an empty diff for all five `sample.*` fixtures. The base is pinned to `b7774df`, the same base T006 uses. |
| M3 — unquote location | ✅ (option a) | The key finding and T003 say strings are unquoted "here only, never in `first_identifier_text`", so TS `declare module "pkg"` is unchanged. The decision is recorded as clarification 2026-11-14 #8. AC-0002 now says no other grammar's names change. |
| M4 — no PR-body artifact | ✅ | AC-0005 names `assets/pr-body.md` and its two required contents: the #180 reparse consequence and the TS pair/assignment gain. T007 now drafts that file, done-when dw-0702 checks it, coverage for AC-0005 is now T005 + T007, and the bp-0007 criterion was updated. The file sits inside the fence (`docs/plans/020-js-grammar/**`). |
| M5 — AC-0006 non-deterministic; JSX check weak | ✅ | **AC-0006** now defines the callable-node count by exact node kinds and parent kinds:<br>• acceptance = 0 scan errors, and after-count ≥ 1 + callable-node count, minus nameless nodes that are listed with a reason<br>• `main.js` must show `init`, `update` and `draw` (lines 77/173/218 exist)<br>**AC-0001** and **dw-0202** now require `has_error == false` for both `sample.js` and `sample.jsx`. |

The object-literal scoping note was also recorded, as a non-goal: `{ get(){}, put: () => {} }` gives `file::get` and `file::put`.

## New issues from the edits

None material.

- **N1 (cosmetic, not blocking)**: the Gate Matrix row G1 still reads "7 clarifications recorded" (`plan.dd.md:140`), but there are now 8 rows (`plan.dd.md:109-116`). Fix it with `ddocs set` and rebuild. I didn't edit it because this run is read-only.
- **Residual, by design**: AC-0006's "minus nameless nodes listed with a reason" leaves an escape hatch that a coder could use. It is still auditable, because each excluded node has to carry its reason in the execution log. I checked that the inequality is sound:
  - every counted node with a name emits exactly one element;
  - spliced nameless parents don't suppress counted children;
  - elements outside the count (named `class` expressions, nested declarations) can only push the after-count higher.

**Thesis**: advanced. The plan is ready to implement, and every acceptance criterion now has a deterministic or artifact-backed proof.
**Consumers**: unchanged. `daemon::scan` through `PARSER_VERSION`, and `discovery.rs:170`, which is now named in T001's file list.
