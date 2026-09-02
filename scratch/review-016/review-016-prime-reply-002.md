# DELTA HANDOFF — review-016, delta sha `aa1abb61d9d427a1138fd8ce3622834d5b4468b1`

**Delta sha (committed, pushed, PR #107 head): `aa1abb6`**
Previous reviewed sha: `f9b6d07` (+ the docs-only drift to `fa4da2f` you already cleared).

Review **`fa4da2f..aa1abb6` and nothing else.** 10 files, +642/−76:

```
crates/parsers/src/discovery.rs                     (f-16a1)
crates/parsers/tests/discovery_standard_ignores.rs  (f-16a1 tests)
crates/daemon/src/read.rs                           (f-16a2)
crates/daemon/src/roots.rs                          (f-16a2)
crates/daemon/tests/read_surface.rs                 (f-16a2 tests)
crates/daemon/tests/first_light.rs
crates/cli/src/render/surfaces/read.rs              (f-16a2 renderer)
docs/plans/016-hidden-dirs/assets/reviews/review-016.dd.{json,md}   (your record, now committed)
docs/plans/016-hidden-dirs/assets/tasks/phase-1/execution.log.md
```

## What the author claims (verify or falsify — do not take on trust)

**f-16a1** — deny-listed dot dirs are classified *before* hidden policy: `.venv`,
`.cache`, `.next` report `standard-ignore` + the `standard_ignores = false` fix in
**both** modes; plain `.hidden` keeps `hidden` + `--include-hidden`; the fixture
executes the advertised fix. Mutation: gating the deny-list branch on
`include_hidden` made `hidden_directory_prunes_name_the_effective_rule` fail
`left: "hidden", right: "standard-ignore"`; restored → pass.

**f-16a2** — tree policy resolves from the selected `IndexedFile.root_path`;
ambiguous explicit repo/dir targets return **absent** rather than borrowing cwd
policy. Test `tree_policy_tracks_the_resolved_root_in_both_directions` covers both
cross-address directions plus your three controls; renderer test covers
`hidden yes` / `hidden no` / absent. Mutation: restoring the cwd-based lookup made
that test fail with root A's `true` for root B's expected `false`; restored → pass.

## Gate

Per the ruling `2026-09-02-ci-green-on-the-sha-is-the-gate.md`, **CI green on
`aa1abb6` is the gate** — the local exclusive slot is pre-PR proof and this is
already pushed. The author ran fmt + the targeted suites, not the full gate
(it did not hold the slot at push time; that is accepted, not a finding).
Re-read CI on `aa1abb6` itself as you planned; it was IN_PROGRESS at handoff.

Your delta gate as you defined it stands verbatim — build nothing new for me.
Verdict file: `review-016-delta-verdict.md`. If both MEDIUMs are closed and no new
defect entered, say so plainly; the three MINORs (f-16b1/b2/b3) are backlog rows
174/175/176 and are **out of scope** — do not re-litigate them.
