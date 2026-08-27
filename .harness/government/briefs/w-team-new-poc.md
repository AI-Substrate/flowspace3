# Worker brief — team-new scaffold POC · (seat at canary)

**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-28 · one bounded POC task.

## The job

Jordan's ruling (2026-08-28, verbatim intent): "We need a new harness extension
that creates a new worktree and then the latest ordinal plan id folder -
<slug> in the docs/plans folder for it. It will scan all worktrees and main
for the highest one and create ext. param of the slug. It then creates empty
plan.dd.json etc ready for editing. It will create a new impl-suggestion.dd
which we will iterate on now." — "get a coder to POC the new worktree code and
validate its working, then we can move this work to our new worktree when its
done in to a real extension too."

This packet is the POC ONLY — prove the mechanics work, as a standalone script,
NOT yet a harness extension.

Deliverables (numbered):

1. A POC script at `scratch/team-new-poc/team-new.sh` (or `.mjs` — your call,
   but the real thing will be a harness extension in TypeScript, so note
   portability) taking one arg `<slug>`, which:
   a. Scans `git worktree list` (every worktree's `docs/plans/`) AND main's
      `docs/plans/` for the highest `NNN-` ordinal folder; next = max+1,
      zero-padded to 3 (current main has 001..004 plus non-ordinal folders —
      those and `archive/` variants must not break the scan).
   b. Creates `git worktree add ../fs3-<slug> -b <ord>-<slug>` from the main
      clone (refuse politely if the worktree or branch already exists).
   c. In the new worktree runs `harness plan new <slug> --ordinal <ord>` to get
      `docs/plans/<ord>-<slug>/` with empty plan ddocs.
   d. Copies the four templates from `.agents/skills/pij-team/templates/`
      (impl-guide + 3 packets) into that plan folder, renaming
      `impl-guide.dd.json` as-is, and runs `ddocs build` on each so the .dd.md
      siblings exist.
   e. Prints a JSON envelope: worktree path, branch, ordinal, plan folder,
      next_action ("prime: write the plan, then the impl-guide").
2. A validation transcript at `scratch/team-new-poc/validation.md`: at least
   two runs — a happy path (then tidy: `git worktree remove`, delete branch)
   and one collision/refusal case — with real command output pasted.
3. A short "extension notes" section in validation.md: what the real
   `.harness/extensions/team/` port needs (harness `new` scaffolding shape,
   anything the shell POC fudged).

Out of scope / DEFERRED: the real extension, any change under
`.harness/extensions/`, any edit to the pij-team skill or templates, the
conversations/ingestion work.

## Rules & fence

- Fence: `scratch/team-new-poc/**` only, PLUS the transient test worktree(s)
  `../fs3-<test-slug>` you create and MUST remove before done. Nothing else.
- Do not touch `.harness/government/**`, `.claude/**`, `docs/plans/**` in the
  MAIN tree (your test plan folders live only inside your test worktree, which
  you delete).
- Test slugs: use `poctest-<something>` so nothing looks like a real plan.
- No commits needed for the POC (scratch is fine uncommitted); if you commit,
  use `harness commit`.
- Doctrine: `.agents/skills/pij-team/TENETS.md` — this POC is tenet-13 work
  (building the machine that builds the product).
- `harness observe` every friction the moment it bites; list, never clear.

## Report back

claim · script path · validation.md with pasted output · the computed next
ordinal on this repo right now · observations. Deviations = stop-and-ask.
Ack with your read + numbered plan before coding.
