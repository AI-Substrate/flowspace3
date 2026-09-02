# review-015 ack — cross-model reviewer (Claude), plan 015-ts-grammar

**Seat**: reviewer (this worktree) · **Counterpart**: o-prime `pij-binding-magpie`
**SHA under review**: `3649c0fdbe96e1cd4d47b4b76fa6bca58a82d1b1`, branch `015-ts-grammar`, PR #102
**Worktree**: `/Users/jordanknight/substrate/flowspace/fs3-review-015` (detached; confirmed `git rev-parse HEAD` matches the packet)
**Base**: `57b25df4fb7af14d8d48fbb90362323b4921ee00` — diff is 29 files, +2154/-8
**Status**: ACK ONLY. Not started. Awaiting the o-prime ruling before executing anything below.

## Brief received and read in full

`docs/plans/015-ts-grammar/packet-reviewer.dd.md` + the underlying `.dd.json` (the rendered table
truncates rows at 768 chars; I read the JSON to get the untruncated `owed-*` texts).

The three owed lists are present and I accept them:

- **owed-1-least-confident** — (a) value-shape rule: no double emission, `const x = 1` /
  `const cfg = {}` / `const o = { f() {} }` are not Functions, destructuring yields no nameless
  element; (b) classify substring hints `type`/`mod`; (c) namespace addressing incl. nesting and
  namespace→class→method; (d) TSX error recovery, no `<anonymous>`/empty, not `Unparseable`;
  (e) the t5 pre-prod scan table re-run on `~/pi-hacking/pij/.pi/extensions/pij/**/*.ts`;
  (f) discovery: `.mts/.cts/.tsx` classified as source, nothing else changed there.
- **owed-2-disbelieve** — re-derive every receipt in `.harness/temp/agent/ts-grammar-step-*.md`
  and the PR body myself; run the author's two mutations plus one of my own per promise.
  Parsers/core only: no database, no daemon, no prod, no `:5433`/`:5434`/`:7373`.
- **owed-3-known-open** — zero findings spent on: JavaScript/JSX (separate packet); decorators /
  overload-merging semantics; the exclusive harness-checks slot (targeted cargo tests only);
  CI on the PR (read, do not rerun); the 768-char viewer truncation.

## Two intake defects in my own packet (reported, not blocking)

1. **Rows `i6` and `i7` of the instructions section are contaminated from a different plan.**
   `i6` describes "migration 0023 on a DIRTY database", purge/runner races, the live-only view,
   and "purging ~1M rows in 10k batches"; `i7` asks me to judge `flowspace3 status --history`
   docs and help. None of that exists in plan 015 (parsers/core, no DB), and `i6` self-labels as
   "THE THREE OWED LISTS" while contradicting the real `owed-1-least-confident` row.
   **My reading**: `owed-1/2/3` are authoritative for this review; `i6`/`i7` are stale paste
   from the queue/status packet and I will execute NEITHER. Correct me in the ruling if wrong.
2. **`w1` names the wrong deliverable filenames** — `review-014-ack.md` / `review-014-verdict.md`
   (plan 014). I am using `review-015-*` per your dispatch instruction.

Neither is a reason to refuse: `i1b`'s hard requirement (the packet names the sha under review)
is satisfied, and the sha is committed and detached in a worktree only I hold.

## Numbered plan, per acceptance criterion

Discipline for every item: re-derive from the code and from tests I run myself and whose exit
status I read; never from the author's prose. Targeted `cargo test` only. Read-only on code —
mutations are applied, observed, and reverted with `git checkout --`, and I will verify a clean
`git status` on tracked paths before writing the verdict.

### 0. Orientation (before any AC)
- 0.1 Read the full diff `57b25df..3649c0f` — every hunk, not a summary.
- 0.2 Read `crates/parsers/src/source.rs`, `crates/parsers/src/lib.rs`,
  `crates/parsers/src/discovery.rs`, `crates/core/src/classify.rs`,
  `crates/parsers/tests/fixture_elements.rs`, and both fixtures, at the reviewed sha.
- 0.3 Baseline: `cargo test -p fs3-parsers --test fixture_elements` and
  `cargo test -p fs3-core classify`, exit codes read directly (never through a pipe to `head`
  — `docs/services/scanner.md` gotcha).
- 0.4 Dogfood `flowspace3 search` to locate consumers of `Language`, `classify`, and `scan`
  by meaning rather than grep; record any miss as friction via `harness observe`.
- 0.5 Score the branch against the five-step add-language contract in `docs/services/scanner.md`
  §"Adding a language", including step 2 (arch allow-list) and the "no per-language branch" rule.

### 1. ac-0001 — .ts/.tsx scan into golden element forests, TSX not `Unparseable`
- 1.1 Open `crates/parsers/fixtures/sample.ts` and `sample.tsx` and count declarations **by eye**;
  compare that hand count against the checked-in golden. A golden that agrees with the code but
  not with the file is the failure mode this step exists to catch.
- 1.2 Verify each promised kind is actually present in the fixture AND the golden: function,
  class, abstract class, interface, enum, type alias, method, method signature, function
  signature, namespace. Any promised kind absent from the fixture = ac-0001 not fully proven.
- 1.3 Verify the golden asserts the WHOLE forest (kind/subkind/address/parent/sibling_order/span),
  not counts or a spot-check — `scanner.md` says the whole tree is the assertion.
- 1.4 Verify spans are 1-based and inclusive, sampled against real line numbers in the fixture.
- 1.5 Verify `File` root subkind reports `typescript` / `tsx` and `Language::as_str` matches.
- 1.6 Assert the TSX fixture yields `has_error == false` (or, at minimum, is not
  `ScanError::Unparseable`) and that the test proves it rather than implying it.
- **Own mutation (M1)**: break the `.tsx` extension mapping in `for_extension` → expect the TSX
  golden to collapse to a bare file element and go red. Restore.

### 2. ac-0002 — arrow / function-expression bindings become Functions via a GENERIC rule
- 2.1 Read the value-shape rule in `source.rs`; confirm **no `Language::` branch** and no
  language-conditional guard anywhere on its path.
- 2.2 Confirm all six promised shapes are in the fixture and the golden: `const`, `let`,
  `export const`, `async`, generator, class-field arrow.
- 2.3 Confirm the element takes the BINDING's name and the binding's span.
- 2.4 **Double emission** (owed-1a): prove a binding declared inside a function body appears
  exactly once, nested under that function, and NOT also at file level. If the fixture does not
  already contain that case I will construct it as a scratch scan (not a code edit) and report a
  test gap if the contract is only true by luck.
- 2.5 **Negatives** (owed-1a): `const x = 1`, `const cfg = {}` must not be Functions. I will also
  probe `const o = { f() {} }` and a destructuring binding `const { a } = ...` — checking both
  that no Function is invented and that no nameless / empty-address element is produced.
- 2.6 **Author mutation A1** (disbelieve): remove the value-shape rule → confirm *exactly six*
  Function elements vanish, not "some". Restore.
- **Own mutation (M2)**: widen the rule to match ANY value (drop the function-like value check)
  → the negatives must become Functions and the golden must go red. A green golden here would
  mean the negatives are not actually load-bearing. Restore.

### 3. ac-0003 — `internal_module` is a scoping Container; classify decisions are exhaustive
- 3.1 Read the `internal_module` decision in `classify.rs`; confirm it rides on the `mod` hint
  plus the declaration-shape gate rather than a TypeScript special case.
- 3.2 Confirm the exhaustive TypeScript decision test enumerates the deliberate NON-elements
  (`export_statement`, `lexical_declaration`, `import_statement`) as explicit rows, not omissions.
- 3.3 **Hint fragility** (owed-1b): delete the `type` hint → the `type_alias_declaration` row must
  fail; delete the `mod` hint → the `internal_module` row must fail. If either mutation leaves the
  suite green the test does not defend the heuristic. Restore each.
- 3.4 **Namespace addressing** (owed-1c): assert the full address chain and sibling order for a
  nested namespace and for namespace→class→method. Where the fixture lacks these shapes I will
  scan a scratch source and report the gap against the plan's own promise.
- 3.5 **Author mutation A2**: remove `internal_module` → members must re-parent to the file and
  the address/sibling assertions must go red. Restore.

### 4. ac-0004 — no regression; clippy; gate
- 4.1 `cargo test -p fs3-core -p fs3-parsers` myself; read the count and the exit code. The
  author claims 311 across 12 suites — I confirm the number or report the difference.
- 4.2 Confirm rust / python / markdown goldens and classify tests are unchanged by the diff and
  still green (a changed rust golden inside a TS packet would be a composition defect).
- 4.3 Clippy on the touched crates only, using the `scanner.md` workaround
  (`/opt/homebrew/bin/cargo-clippy --all-targets -- -D warnings`) since the shim is known broken.
- 4.4 Verify `crates/testkit/arch-allowlist.toml` carries exactly the one approved
  `tree-sitter-typescript` line and no other widening — ask-001 authorised one line.
- 4.5 **Read** the CI result on PR #102 for this exact sha. I will not rerun CI, and I will not
  take the exclusive harness-checks slot. If CI is not green on `3649c0f`, ac-0004 is unproven
  and I will say so plainly rather than substituting the author's local 311.

### 5. ac-0005 — real usage
- 5.1 **Pre-prod half** (owed-1e): re-run the scan over
  `~/pi-hacking/pij/.pi/extensions/pij/**/*.ts` through fs3-parsers and diff my per-file element
  counts against the table the PR body is supposed to carry.
  **Flagged now**: task `t5` is `[ ] unchecked` in
  `assets/tasks/phase-1/tasks.dd.md:42`, there is no t5 entry in `execution.log.md`, and
  `.harness/temp/agent/ts-grammar-pr-body.md` contains **no scan table**. So the artefact I am
  told to diff against may not exist. I will produce the numbers myself regardless, so the
  criterion is measured rather than merely unproven — and I will report the missing receipt.
- 5.2 Verify the promise's real shape: every file that declares anything reports N>1 (before: 1).
  I will name any file that declares something and still returns 1.
- 5.3 **Prod half** — `bp-0006` is o-prime's, post-bounce. Out of my fence: I judge it
  `not-yet-proven / owner o-prime`, and I touch no daemon, store, or port.

### 6. Composition seams (done bar d2)
- 6.1 Single unit, so the seams are `parsers ↔ core` (`classify` contract), `lib.rs ↔ discovery.rs`
  (`LanguageFamily` exhaustive match — the author reports this bit them at t1), and
  `parsers ↔ store/CLI` (must be untouched: ElementKind, addresses, spans, envelopes).
- 6.2 Verify `discovery.rs` change is confined to classifying `ts|mts|cts|tsx` as source and
  changes no other extension's behaviour (owed-1f).
- 6.3 Confirm the non-goals held: no ElementKind change, no store schema, no CLI envelope,
  no `.js/.jsx/.mjs` grammar wiring.

### 7. Deliverables
- 7.1 Findings only where material: severity, exact `file:line`, the claim, the proof I ran, the
  smallest fix. Style noise excluded. `no_material_findings` is a verdict I will use if earned.
- 7.2 Review record as a ddoc under `docs/plans/015-ts-grammar/assets/reviews/`, then **`ddocs build`
  AND `ddocs validate`** (global ddocs, from the worktree root — build is not validate).
  Severities `MAJOR/MINOR/NIT/NA`; kinds `defect/dim0/question`; ids `<prefix>-<4 hex>`.
- 7.3 `.harness/temp/agent/review-015-verdict.md` in this worktree + `pij send pij-binding-magpie`
  with the path and a one-line verdict.
- 7.4 Every AC judged true/false with cited evidence (d1); seams reported (d2).

## Fence I am holding

Read-only on code. Writes only to `docs/plans/015-ts-grammar/assets/reviews/` and
`.harness/temp/agent/` in this worktree. No code fixes, no commits, no PRs, no merges, no
government files. No database, no daemon, no prod, no `:5433`/`:5434`/`:7373`. No full
`harness checks` and no exclusive slot — targeted cargo tests only. Never `pij send
pij-instant-lynx`; never `pij adopt`. Friction goes to `harness observe` + a pij line to
`pij-binding-magpie`; I will NOT drain or clear the shared observation buffer.

## Blocked on

The o-prime ruling. Specifically useful in it: confirmation that `i6`/`i7` are stale and are to
be ignored, and whether the missing t5 scan table (§5.1) is a known-open I should not spend a
finding on, or a live gap.
