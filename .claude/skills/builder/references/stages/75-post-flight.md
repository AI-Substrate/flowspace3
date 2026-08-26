# post-flight

> Sub-skill — part of a verb library. Knows nothing about any flow:
> no stage ids, no successor/predecessor names, no flow commands.
> Composition is the bundling flow's job.

**Verb**: post-flight
**Purpose**: Close out a finished flight **before** any ship: verify every phase and review is done, write a short close-out note, then **archive the whole plan folder** to `docs/plans/archive/<ord>-<slug>/` — the durable home for completed plans. Shipping is optional and later; this close-out is not, and it runs whether or not the work ever ships.
**Consumes**: a plan folder (`--plan`, or auto-detect from cwd) whose phases and reviews are complete — task tables (`assets/tasks/*/tasks.md`, legacy root `tasks/`), execution log (`assets/execution.log.md`, legacy root), newest review verdict (Full: `assets/tasks/*/reviews/` · Simple: `assets/reviews/`; legacy roots). Open items are surfaced, never silently archived over.
**Flags**: `--plan "<abs path to docs/plans/<ordinal>-<slug>/>"` (optional; auto-detect from cwd)
**Produces**: `${PLAN_DIR}/assets/post-flight.md` (close-out note: completion evidence, open/deferred digest, archive record) · the plan folder relocated to `docs/plans/archive/<ord>-<slug>/` (layout intact, `git mv`).
**Side effects**: one repo-local `git mv` (in-tree and reversible — nothing outward-facing: no push, no PR, no network).

---

## Procedure

> Elegance: the close-out note is **output** — record the facts a later reader needs (what finished, what's open, where the folder went), not a play-by-play. Doctrine + the seven-function line test: `references/00-routing.md` § Shared conventions.

```md
User input:

$ARGUMENTS
# Optional flags:
# --plan "<abs path to docs/plans/<ordinal>-<slug>/>"   # plan folder (auto-detect if in plan dir)
```

1) **Input resolution**

   - `PLAN_DIR` = provided `--plan` OR auto-detect from cwd (look for `*-plan.md`, or a legacy `*-spec.md`; a dd-native plan has `plan.dd.json` and NO `*-plan.md`)
   - `ORD_SLUG` = the plan folder's basename (e.g. `035-flow-ship-stage`)
   - `DEST` = `docs/plans/archive/${ORD_SLUG}`
   - `FLOW_SLUG` = the flight plan driving this journey — `harness flow list --json` and take the flow whose `plan_dir` is `${PLAN_DIR}`. Empty (no dd-native flow) → skip the relocate in step 4 and say so.
   - Already under `docs/plans/archive/` → report "already archived", print the close-out note path if present, STOP (idempotent re-run, not an error).
   - `DEST` already exists with different content → STOP and report the collision; never overwrite an archived plan.

2) **Completion check (read-only)** — gather, don't gate:

   - every phase's task table complete (`assets/tasks/*/tasks.md`, legacy root `tasks/`; Simple mode: the plan's inline task table)
   - acceptance criteria met (plan `## Acceptance Criteria` vs execution log)
   - newest review verdict clean/APPROVE (Full mode: `assets/tasks/*/reviews/*.md` · Simple mode: `assets/reviews/*.md`; legacy root `tasks/*/reviews/` / `reviews/` as fallback)
   - leftover `Deferred` / `Noteworthy` rows, skipped/blocked tasks, unresolved review findings

   **All clean** → proceed. **Open items** → list them verbatim and ask: proceed (archive anyway — the digest is recorded in the close-out note) or stop to finish first. Never silently archive over open findings; never block a user who says go.

3) **Write the close-out note** to `${PLAN_DIR}/assets/post-flight.md`:

   ```markdown
   # Post-flight — ${ORD_SLUG}

   **Closed out**: <ISO timestamp>
   **Archived to**: docs/plans/archive/${ORD_SLUG}/
   **Shipped**: not yet — ship is optional and may run later from the archive path

   ## Completion

   | Check | Result |
   |-------|--------|
   | Phases / tasks | ${all complete | N open — listed below} |
   | Acceptance criteria | ${met | N unmet} |
   | Latest review | ${verdict} |

   ## Open / deferred items

   _${none — flight fully clean | carried into the archive on the user's explicit go-ahead}_

   | Kind | Item | Where | Note |
   |------|------|-------|------|
   ```

4) **Archive move — the LAST act of this verb** (any bookkeeping that writes into the folder must already be finished; the mover moves last):

   ```
   mkdir -p docs/plans/archive
   git mv "${PLAN_DIR}" "${DEST}"
   harness flow relocate --slug "${FLOW_SLUG}" --to "${DEST}"   # dd-native plans only
   ```

   Untracked files inside the folder ride along with a plain `mv` if `git mv` leaves them. The folder's internal layout is untouched, so every relative link inside it keeps working. Print `old → new` path.

   **The relocate is not optional on a dd-native plan.** A flow's `dd_link` gate addresses are anchored at the REPO ROOT, so the `git mv` strands every one of them — and nothing catches it: `dd doctor` sweeps `*.dd.json`, a flow is `.harness/flows/<slug>.json`, so the corpus reads perfectly clean while the gates point at a folder that no longer exists. On an archived plan there are no departures left to refuse, so the breakage has no discovery moment at all. `flow relocate` reads the folder the flow recorded at create time (`plan_dir`) and re-points only the addresses inside it, rebuilding the sibling in the same operation. Do **not** hand-edit the addresses: an address assembled by a model is a gate that fails the day the wording changes. A flow with no `plan_dir` refuses rather than guessing — say so in the report instead of patching it by hand.

5) **Report** (terminal summary):

   ```
   ✅ Post-flight complete: ${ORD_SLUG}
      Archived: docs/plans/archive/${ORD_SLUG}/
      Close-out note: docs/plans/archive/${ORD_SLUG}/assets/post-flight.md
      Open items: ${none | N — recorded in the note}
      Ship later (optional): resolve --plan at the archive path.
   ```

## Notes

- **Reversible**: `git mv` back restores the pre-archive state exactly.
- **Idempotent**: a folder already under `docs/plans/archive/` re-runs as a no-op report.
- **Read-only until step 4**: the only mutations are the close-out note and the move itself.

## Exit

Print the output-contract summary (✅ block above). Then STOP. Do not name a next stage. If invoked standalone, end with exactly: "Routing is the flow's job — run the parent flow bare to continue."
