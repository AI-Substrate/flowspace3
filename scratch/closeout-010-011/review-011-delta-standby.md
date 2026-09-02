# review-011 — delta re-review standby

**Status**: STANDING BY for the new sha. Read-only, no code touched.
**Round 1 outcome**: verdict ACCEPTED by o-prime; all three findings ruled FIX in
PR #93, smallest-fixes adopted verbatim.

## Retained state (nothing to re-establish when the sha lands)

- Worktree: `/Users/jordanknight/substrate/flowspace/fs3-review-011`
- Scratch DB retained: `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3`
  (compose container `flowspace3-db`, healthy). Prod :7373 stays untouched.
- `CARGO_TARGET_DIR=/Users/jordanknight/substrate/flowspace/fs3-review-011/target-review011`
  — warm, so the delta runs cost seconds not minutes.
- Discipline unchanged: one cargo invocation at a time, `--test-threads=2`,
  per-run `FreshDatabase` names.
- `git status --porcelain -- crates/` is EMPTY. Round-1 mutations and both
  disposable probes are gone.

## Delta scope — ONLY the three fixes plus the ask docs paragraph

Nothing already-confirmed gets re-litigated. ac-0001/0002/0004/0005, the seam
census, hunt (a)/(c)/(e), and the `get`/`tree` docs were judged TRUE at 3a7124ba
and are not reopened unless the fixes touched them.

### f-0001 — `with_corpus` takes scope from the resolved anchor
1. Re-create both round-1 probe cases and require them GREEN this time:
   foreign-repo anchor AND the unanchored (`repo_identity = NULL`) default —
   `search_hits > 0`, `citations` non-empty, `grounded: true`.
2. **Individual mutation**: revert only the `with_corpus` widening; both cases
   must go RED. A fix whose own test cannot fail is not proven.
3. Check the widening did **not** reopen F-0003's hole: an UNPINNED
   model-proposed `get conv:<foreign guid>#t1` under `ScopeSource::Cwd` must
   still be refused. The two fixes pull in opposite directions — this is the
   seam I will hunt hardest.
4. Confirm `scope_line` now agrees with the filter actually applied, including
   for a LOCAL pin (where round 1 found it newly wrong).
5. Confirm an explicit `--repo` still narrows a pin (ScopeSource::Flag), so the
   fix widened cwd-derived scope only.

### f-0002 — rename to `FS3-E-QUERY-CONVERSATION-NOT-FOUND`
1. Wire proof: `GET /conversations/verify` for a never-ingested session must be
   **404**, not 500, with the renamed code on the envelope.
2. The code must remain DISTINCT from `FS3-E-QUERY-NOT-FOUND` (ac-0003's
   requirement) — assert both codes and both statuses in the same run.
3. `docs/reference/error-codes.md` regenerated: the row must read 404, and
   `crates/core/tests/error_codes.rs` drift test must pass.
4. The zero-turn branch and `details.turns = 0` must survive the rename.
5. **Individual mutation**: rename back; the newly-added status assertion must
   go RED. If it does not, the assertion is decorative.

### f-0003 — `ScopeSource::Cwd` case pinning the guard's own message
1. New case present with `source: Cwd` and asserting the guard's OWN message
   (`get resolved outside the caller's immutable repository scope`), not
   read.rs's `outside the explicitly requested scope`.
2. **Individual mutation**: neuter `payload_in_scope` again. Round 1 measured 36
   tests green under that mutation; the new case must now go RED. That single
   red is the whole point of the finding.

### ask docs paragraph
Judge the rewritten `conversations.md` ask paragraph against the shipped fix:
it must describe what retrieval can actually reach, not only what resolves.
TRUE, not aspirational — the same bar the `get` paragraph passed.

## Gate

`harness checks` verdict on the fix head is o-prime's call, not mine; I report
what I ran and its exit status. Delivery stays BY FILE (req-0034): updated
`cross-model-review.dd.json` (round 2 rows appended, round 1 rows preserved) and
a refreshed `review-011-verdict.md`. No `pij send` to a legacy prime, no
`pij adopt`.
