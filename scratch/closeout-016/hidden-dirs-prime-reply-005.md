# o-prime ruling — review 016 verdict: APPROVE WITH FINDINGS. Fix the two MEDIUMs on the branch.

Reviewer knobbler returned **APPROVE WITH FINDINGS**: all five ACs TRUE, every
receipt re-derived from primary sources (your ac-0005 numbers survived to the
byte), seams clean, no weakened assertions. Good work. Full verdict:
`/Users/jordanknight/substrate/flowspace/fs3-governance/scratch/review-016/review-016-verdict.md`

Both MEDIUMs are wrong output in an **agent-facing envelope** — the class of
defect this product exists to avoid — and you still hold the context, so they are
fixed here, not deferred. The three MINORs are backlog, not yours.

## Fix 1 — f-16a1: the prune ledger's `fix` names a command that does not work

The hidden check runs before the deny list (`discovery.rs:868` vs `:884`), so a
default add reports `.venv`/`.cache`/`.next` with reason `hidden` and fix
"…`--include-hidden`". Following that advice literally, the opt-in add reports the
same dir with reason `standard-ignore` and it is *still* absent from the map.

Do: **test the deny list before the hidden policy for directories**, so a
deny-listed dir reports `standard-ignore` (with the `standard_ignores = false`
fix) in BOTH modes and its reason no longer flips with the mode. Extend
`hidden_directory_prunes_name_the_effective_rule` to assert the **fix string**,
not just the reason — the current test stays green while the advice is wrong,
which is why this reached review.

## Fix 2 — f-16a2: `tree` reports the cwd root's policy for a file in another root

`tree_hidden_policy` (`read.rs:346-364`) resolves from `scope.worktree` while the
repo comes from the resolved address, so `tree el:<file in root B>` run from root A
prints root A's policy — confirmed symmetric in both directions, and the human
renderer printed `· hidden yes` for a root whose policy is off.

Do: resolve the policy from **the worktree that owns the resolved file**; if that
cannot be determined, return `None` (an absent label beats a wrong one — `--repo
<other>` already returns honest `null`). Add the cross-root case as a test, both
directions, plus the human-renderer line.

## Then

- Commit the **review record** onto this branch too:
  `docs/plans/016-hidden-dirs/assets/reviews/review-016.dd.json` + `.dd.md`
  (copies in `fs3-governance/scratch/review-016/`), so the PR carries its own review.
- Mutation receipt for each fix (red-then-restore), reported to me.
- **`harness checks`: cod holds the exclusive slot right now.** Run targeted
  `cargo test` for your crates while you wait; ask me for the slot before the full
  gate. `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test`
  only — never `:5433`, never `:7373`, never the default config dir.
- Push to `016-hidden-dirs` (PR #107 updates in place); tell me the new head sha.
  Do not merge.

Note for your transcript: knobbler diagnosed `tests/health.rs` as **not a 016
defect** — boot probes ddocs for every registered worktree before it binds, and the
shared test DB holds 19 roots (0 roots → key published in 1 s; 19 roots → 25 s
against a 10 s ceiling). That upholds the earlier ruling. Separately, your ac-0005
receipt left `/Users/jordanknight/pi-hacking/pij` registered as root 19 in the
SHARED test DB — do not clear it yourself (cod is mid-gate against that DB); I am
handling it, and the receipt recipe needs an unregister step.

---

## Addendum — the reviewer's delta gate, verbatim. Build against THIS.

knobbler will re-run exactly these on your delta sha; nothing else is in scope.

**f-16a1.** A default add of a fixture containing `.venv`/`.cache`/`.next` **plus**
a plain `.hidden` must report:
- the deny-listed dot-names with reason `standard-ignore` and the
  `standard_ignores` fix, and
- `.hidden` keeping reason `hidden` and the `--include-hidden` fix,
- and **the reason must not flip between modes**.

It will then *execute the advertised fix* and assert `.venv` stays pruned either way.

**f-16a2.** With two roots of differing policy registered at once,
`tree el:<repoB>/<file>` run from inside root A must return **repo B's** policy or
`null` — never A's — **in both directions**, and the human title must agree. Its
three passing controls must stay correct: plain `tree` in each root,
`tree <absolute path>`, and `tree --repo <other>`.

It will also re-run fmt, clippy and the targeted suites on the delta sha, and
re-read CI on that exact sha. So: **commit and push before you report**, and give
me the sha — a reviewer never reviews a moving tree (DL-011).
