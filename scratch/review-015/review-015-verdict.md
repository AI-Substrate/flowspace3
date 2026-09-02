# review-015 verdict — cross-model reviewer (Claude), plan 015-ts-grammar

**Record**: `docs/plans/015-ts-grammar/assets/reviews/review-015.dd.json` (built + validated; `.dd.md` sibling rendered)
**SHA under review**: `3649c0fdbe96e1cd4d47b4b76fa6bca58a82d1b1` — PR #102 head (`gh pr view 102` headRefOid matches), base `57b25df`, detached in a worktree only this seat holds.

## VERDICT: APPROVE — no blocking findings

All five acceptance criteria are TRUE. Every author receipt was re-derived, not audited; all held, including the "exactly six" count. Five findings, none blocking: two real TypeScript extraction gaps, two deferred notes, one docs-hygiene defect.

## Per-AC judgement

| AC | Verdict | Proof I ran |
| --- | --- | --- |
| ac-0001 | **TRUE** | 22-row golden hand-counted against `sample.ts` line by line; all ten promised kinds present; spans 1-based inclusive and correct; TSX asserts `!has_error` (stronger than "not `Unparseable`") |
| ac-0002 | **TRUE** | Rule at `source.rs:93-101`, reached from `:55` via `classify(..).or_else(..)` — no `Language::` branch on its path; six shapes named by binding; **no double emission** (proved on my own source); removing → red; widening → red |
| ac-0003 | **TRUE** | `internal_module` scopes members; proved beyond the fixture on nested namespaces (`Outer::Inner::deep`) and namespace→class→method; classify test enumerates all three deliberate non-elements |
| ac-0004 | **TRUE** | 311 passed / 0 failed (matches author exactly); workspace clippy `-D warnings` exit 0; CI check `gate` COMPLETED/SUCCESS on the exact sha |
| ac-0005 | **TRUE** (pre-prod) | Re-derived the table independently: 327 / 327 / 3452 / 284 / 43 / 1 recovery / 0 errors — **identical to the PR body**. Prod half (bp-0006) is o-prime's, out of my fence |

## Mutation battery — 6 controls, 6 confirmed

| Mutation | Source | Result |
| --- | --- | --- |
| Remove value-shape rule | author's | **Exactly six** Functions vanish: `Service::field`, `plain`, `assigned`, `exported`, `asyncTask`, `generated`. Count is exact |
| Widen rule to ANY value | **mine** | Golden **and** `invents_nothing` both red — `const x = 1` / `const cfg = {}` are load-bearing controls, not decoration |
| Remove `internal_module` | author's | `Tools` disappears, members flatten to `sample.ts::inside` / `::Nested`; red in **both** parsers and core |
| Delete `"type"` hint | **mine** | 1+1 red |
| Delete `"mod"` hint | **mine** | 3+3 red — impl-guide risk-2 discharged |
| Remove `.tsx` mapping | **mine** | TSX collapses to `file unknown`, zero elements |

Everything restored; `git status` on `crates/` clean.

## Findings

- **f-2b01 · MINOR · defect** — object-literal and class-expression members get two *different* wrong answers. `export const api = { get(){}, post(){}, put: () => {} }` emits `p.ts::get` / `p.ts::post` at **file** scope (the `api` segment is lost, so two modules exporting `get` collide), while `put: () => {}` is an object `pair` and produces **no element at all** — a real exported callable invisible to search. Same split hits `const C = class { m(){} }`. Follows the documented splice rule, so not a contract violation, but unfixtured, so either behaviour can change silently. *Smallest fix now: add both shapes to `sample.ts` and lock today's behaviour. Behaviour change belongs in the JavaScript packet, where the same rule must serve object literals anyway.*
- **f-2b02 · MINOR · defect** — `declare module "pkg/sub"` yields `p.ts::"pkg/sub"`: quote characters carried into name and address, inherited by every member. Newly reachable because `.ts` never parsed before. *Fix: strip one matched pair of surrounding quotes in `first_identifier_text` — one condition, still language-agnostic.* Low urgency: 0 of the 327 measured files hit it.
- **f-2b03 · NIT · question (deferred)** — anonymous default exports produce no element for the exported thing. Correct-by-design nameless-skip, but TS makes the idiom routine in a way Rust/Python never do. Worth a deliberate decision, not a fix here.
- **f-2b04 · NIT · question (deferred)** — ac-0005 says "every file that declares anything"; 43 files remain at 1. I checked all 43: every one declares **only value bindings** (e.g. `core/name-corpus.ts`, 1,720 lines → 1 element). The implemented contract is "declares a callable or container", exactly consistent with Rust's `const_item`/`static_item` exclusion. Flagged only so the prod receipt is not misread as an extraction miss.
- **f-2b05 · MINOR · defect** — the plan's own docs do not pass `ddocs validate`, and both offenders are new on this branch: `impl-guide.dd.json` has 2 schema errors (`units[0].paths` and `review.inputs` are arrays where a string is required), and `packet-coder.dd.json` has a duplicate instruction id (`i10` at both `[9]` and `[14]`, so a substantive instruction in the **coder's own brief** is not uniquely addressable). `plan`/`backpressure`/`tasks`/`packet-reviewer` are clean. Invisible in the rendered `.dd.md` — build is not validate. *Fix: join the arrays, renumber the second `i10`.* Pairs with the t5 checkbox gap already ruled known-open — one docs pass closes all of it.

## Composition seams (done-bar d2)

Nothing outside `crates/{parsers,core,testkit}`. `crates/core/src/element.rs` untouched → `ElementKind` unchanged. Arch allow-list gained exactly the one approved line. All four exhaustive `Language` matches updated (`as_str`, `grammar`, `scan` dispatch, `LanguageFamily`); exhaustiveness forecloses a missed callsite — which is precisely what bit the author at t1, i.e. the correct failure mode. Non-goals hold: `.js`/`.jsx`/`.mjs` still scan to `unknown` with zero elements. `.mts`/`.cts` newly become discoverable `Source` via the grammar-first branch (they are absent from `SOURCE_EXTENSIONS`) — intended goal, not drift. The five-step add-language contract in `docs/services/scanner.md` is followed in full, and the snap-in doc comment was updated to match.

## Fence honoured

Read-only on code (every mutation reverted, verified clean). Targeted `cargo test` only — no full `harness checks`, no exclusive slot. No database, daemon, or prod; `:5433`/`:5434`/`:7373` untouched. CI read, never rerun. Zero findings spent on the known-open list. Shared observation buffer not drained. Wrote only `assets/reviews/` and `.harness/temp/agent/`.

## Dogfooding

`flowspace3 search` located the grammar-selection and classification seams by meaning on the first query both times (`Language::for_extension`, `LanguageFamily::for_extension`, `classify`, `element_at`, plus `docs/services/scanner.md` sections). No search miss to report as friction.

## Recommendation

**Merge.** f-2b01 and f-2b02 are worth a small follow-up packet — ideally folded into the JavaScript/JSX packet, which has to solve the object-literal case regardless. f-2b05 is a docs pass to run before this plan is archived.
