# Worker brief — harness team tidy (teardown verb) · pij-involved-planarian

**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-28 · answers your own
question: teardown was NOT in #37's scope (mint + --propose only) and is OPEN —
and yes, it is where the disk lesson encodes. The packet is yours: you hold the
census context, and the create side's own E_BRANCH_EXISTS scar tissue is your
evidence that the missing verb is already costing us.

## The job

Build `harness team tidy <slug>` in the existing team extension
(.harness/extensions/team/), the teardown counterpart to `team new`:

1. Removes the worktree `../fs3-<slug>` (refusing with a named error if it has
   uncommitted changes or unpushed commits, unless `--force`; ALWAYS printing
   what would be lost) and prunes the registry.
2. Deletes the local plan branch (and remote with `--remote`, only if merged
   or --force; never silently).
3. Clears that worktree's `target/` as part of removal (it goes with the tree).
4. Drops docker volumes namespaced to that slug's compose project
   (`fs3-<slug>_*`), zero-link only, BY NAME, never a global prune.
5. OBSERVATION-BUFFER RESCUE FIRST, mechanically: if
   `<worktree>/.harness/temp/agent/session-buffer.md` exists and is non-empty,
   copy it to the main clone as `<slug>-observations.md` (sha-verify the copy)
   BEFORE removal, and say so in the envelope — DL-027 encoded at last.
6. `--dry-run` (mirror `team new --propose`: same envelope shape,
   `would_remove` block, nothing touched) and an audited envelope on the real
   run: what was removed, rescued, refused, and why.
7. instructions.md updated: new-vs-tidy lifecycle, the refusal table, and the
   E_BRANCH_EXISTS note on the create side updated to point at tidy.

Out of scope: the zero-link dind-var-lib-docker-* orphans and chainglass
volumes from OTHER projects (name them in instructions.md as a
`docker volume rm` list for Jordan, do not touch); the fs3-cargo-target
cross-build cache (DL-047 — separate packet); any change to `team new` beyond
the doc note.

## Rules & fence

- Worktree `../fs3-team-tidy`, branch `w-team-tidy` off main. `harness commit`.
- Fence: `.harness/extensions/team/**` only (+ your worktree's plan-free tree).
- ABSOLUTE PATHS for every file read/edit (DL-007/008); PIJ_SESSION_ID export
  for sends from the worktree; CARGO_INCREMENTAL=0; no docker compose up.
- Validation: real runs against throwaway `poctest-` scaffolds you mint with
  `team new` and then tidy — happy path, refusal (dirty tree), --dry-run,
  buffer-rescue (plant a fake buffer) — transcripts in the PR body.
- Gate green in your worktree; PR into main, DO NOT MERGE (Telegram precedes).

## Report back

claim · envelope samples · validation transcripts · PR number · observations
(list, never clear). Ack with your read + numbered plan via pij send first.
