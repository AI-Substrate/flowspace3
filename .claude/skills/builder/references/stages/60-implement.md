# implement

> Sub-skill — part of a verb library. Knows nothing about any flow:
> no stage ids, no successor/predecessor names, no flow commands.
> Composition is the bundling flow's job.

**Verb**: implement
**Purpose**: Implement exactly one approved phase or subtask using the testing approach from the plan, with domain placement rules; keep the task table + execution log live; update domain.md files after implementation.
**Consumes**: plan (`**Status**: READY`); Full Mode needs the phase's tasks dossier (`assets/tasks/<phase-slug>/tasks.md`; legacy root `tasks/` fallback) with human GO given; Simple Mode uses inline plan tasks. Reads the plan's Testing Strategy, the task table, Context Brief / Key Findings, and domain context.
**Flags**: `--plan "<abs path to plan.md>"` · `--phase "<Phase N: Title>"` (Full Mode) or omitted (Simple Mode) · `[--subtask "<ORD-subtask-slug>"]`
**Produces**: Code changes + tests per the plan's testing approach; `execution.log.md` with per-task entries; task table + Architecture Map kept current per task; domain.md/registry/domain-map updates (domain mode ON only); terminal report = unified diffs, evidence, domain files updated (when ON), final status vs acceptance criteria, suggested commit message.
**Side effects**: updates `domain.md` / `registry.md` / `domain-map.md` after implementation (§4); keeps the task table + execution log live per task.
**Delegates**: progress — per-task protocol after each completed task; resolved via the Registry.

---

## Procedure

Implement **exactly one** approved phase or subtask using the **testing approach from the plan**. Apply domain placement rules. Update domain.md files after implementation.

> Elegance: code + the execution log are **output**. Log facts and evidence — what changed, why, the proof (diffs, test results, the task table carry the detail) — not a play-by-play monologue; prose only where it changes a decision or records a discovery. Doctrine + the seven-function line test: `references/00-routing.md` § Shared conventions.


## 📝 LOG DISCOVERIES AS YOU GO

Throughout implementation, capture discoveries in:
1. **Execution Log** (`execution.log.md`) — detailed narrative
2. **Discoveries Table** (`## Discoveries & Learnings` in tasks.md or plan) — structured record

Log when you encounter: something unexpected, needed research, hit a trouble spot, found a gotcha, made a decision, introduced debt, or gained an insight.

**Environment friction is work, not an apology (builder `SKILL.md` invariant #14).** When the environment fails or misleads you, fix it if it's small and reversible; otherwise capture it the moment it bites — `harness observe "<what>" --kind difficulty|confusion` when the harness is present, or an execution-log Discoveries row tagged `Noteworthy` as the fallback when it isn't — then keep moving. Own mistakes you fix silently; it's the hard walls and proof-gaps you pay forward, so the next agent never re-hits them.

**Tag what a human must see before it ships.** Mark a Discoveries row `Deferred` when something was *consciously punted* — a skipped/blocked task, an unmet acceptance criterion, scope cut to a follow-up, a `TODO`/`FIXME`/`HACK` left in the code — and `Noteworthy` when you made a call a human might have made differently — an autonomous decision, a workaround, a new dependency, or a file touched outside the plan's Domain Manifest. The tag is the only new thing (no new file); the phase-end summary (step 5) and the downstream ship rollup both read these rows. It **never blocks** — it surfaces, so the go-decision is informed.


## 🛑 MANDATORY: UPDATE PROGRESS AFTER EVERY TASK — NO EXCEPTIONS

The user watches the task table and execution log for live progress. Keeping them current is **highest priority**.

After EACH task you MUST update these locations before proceeding to the next task:

┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ Per-Task Progress Checklist — use this EVERY time, NO EXCEPTIONS    ┃
┃                                                                     ┃
┃ STARTING T00X:                                                      ┃
┃ [ ] Tasks Table: [ ] → [~]                                          ┃
┃ [ ] Architecture Map: T00X node → :::inprogress (orange)            ┃
┃                                                                     ┃
┃ COMPLETING T00X:                                                    ┃
┃ [ ] Tasks Table: [~] → [x]                                          ┃
┃ [ ] Architecture Map: T00X node → :::completed (green)              ┃
┃ [ ] Architecture Map: File nodes touched → :::completed             ┃
┃ [ ] Execution Log: append task entry with evidence                  ┃
┃ [ ] Discoveries table: add any gotchas/insights found               ┃
┃                                                                     ┃
┃ IF BLOCKED:                                                         ┃
┃ [ ] Tasks Table: mark task [!]; note the reason in the Execution Log┃
┃ [ ] (When unblocked: change back to [~] and continue)               ┃
┃                                                                     ┃
┃ ALL TASKS COMPLETE:                                                 ┃
┃ [ ] Tasks Table: confirm every phase task is [x]                    ┃
┃ [ ] Execution Log: append a phase-complete summary                  ┃
┃ [ ] PLAN progress section (if present): mark the phase complete     ┃
┃                                                                     ┃
┃ ✓ ALL UPDATES DONE → Proceed to next task                           ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

Progress lives in the **task file** (`assets/tasks/phase-N/tasks.dd.json`) and the **execution log** — there is no separate flight-plan file. The journey-level view (`the-flow.md`) is regenerated by `/the-flow` itself; this skill never writes it.

### Flipping state — through the dd verbs, never an editor (plan 071, ac-7111)

The task file is a DETERMINISTIC DOCUMENT. Its `.dd.md` sibling is GENERATED, and the phase node's departure gate reads the `.dd.json`. So:

```bash
TASKS="${PLAN_DIR}/assets/tasks/phase-N/tasks.dd.json"

node_modules/.bin/dd set "${TASKS}#done_when/tk-XXXX/dw-XXXX/state" checked   # an assertion holds
node_modules/.bin/dd set "${TASKS}#tasks/tk-XXXX/state" checked               # the task itself
node_modules/.bin/dd set "${TASKS}#tasks/tk-XXXX/state" blocked               # blocked ≠ not done: say which
node_modules/.bin/dd set "${TASKS}#tasks/tk-XXXX/receipt" "<command + result>"
node_modules/.bin/dd get "${TASKS}#tasks/tk-XXXX"                              # read it back
```

Each `dd set` validates against the schema BEFORE writing, rebuilds `tasks.dd.md` in the same operation, and refuses (writing nothing) if the value is not one the schema allows. **Never hand-edit `tasks.dd.json`, and never edit `tasks.dd.md` at all** — it is derived, and `node_modules/.bin/dd build --check` reports a hand-edit as drift that will be blamed on you.

Rehearse the gate before you try to leave the phase — it is cheaper than a refusal:

```bash
harness plan validate "${PLAN_DIR}/plan.dd.json" --address "${TASKS}#tasks"
```

`gate-terminal` states are `checked`, `human-skipped` and `na`. `unchecked` and `blocked` hold the gate — which is the point: state the truth on the row and the gate stops being an obstacle.

DO NOT start the next task until ALL updates above are done.


```md
User input:

$ARGUMENTS
# Expected flags:
# --phase "<Phase N: Title>" (Full Mode) or omitted (Simple Mode)
# --plan "<abs path to plan.md>"
# --subtask "<ORD-subtask-slug>" (optional)

1) Resolve paths:
   PLAN = provided --plan
   PLAN_DIR = dirname(PLAN)

   **Mode Detection**: Read PLAN for `**Mode**: Simple` or `**Mode**: Full`

   **Full Mode**:
   - PHASE_DIR = PLAN_DIR/assets/tasks/${PHASE_SLUG} (legacy root `tasks/` fallback for pre-assets plans — § Plan-folder layout, `references/00-routing.md`; write new files where the dossier already lives)
   - PHASE_DOC = ${PHASE_DIR}/tasks.md
   - EXEC_LOG = ${PHASE_DIR}/execution.log.md
   - If --subtask: PHASE_DOC = ${PHASE_DIR}/${SUBTASK_KEY}.md

   **Simple Mode**:
   - Check for optional dossier: ${PLAN_DIR}/assets/tasks/implementation/tasks.md (legacy root fallback)
   - If exists → PHASE_DOC = that file
   - If not → PHASE_DOC = PLAN itself (inline tasks from § Implementation)
   - EXEC_LOG = ${PLAN_DIR}/assets/execution.log.md (legacy root fallback; create assets/ if missing)

2) Load context:
   - Read Testing Strategy from plan (approach + mock usage)
   - Read task table from PHASE_DOC
   - Read Context Brief / Key Findings for hazards to watch for
   - **Load domain context** per `references/00-routing.md` § Domain mode & context loading (OFF by default — when OFF, skip domain context, domain placement rules, and domain-file updates)

3) Execute tasks:
   Follow task order. Apply testing approach from plan:

   ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
   ┃ 🛑 STOP! RE-READ "MANDATORY: UPDATE PROGRESS" SECTION ABOVE 🛑       ┃
   ┃                                                                    ┃
   ┃ After EACH task you MUST update ALL locations before proceeding:   ┃
   ┃   1. Tasks Table checkbox                                          ┃
   ┃   2. Architecture Map diagram nodes                                ┃
   ┃   3. Execution log entry                                           ┃
   ┃                                                                    ┃
   ┃ The user is watching the task table + log — update them FIRST.     ┃
   ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

   **Full TDD**: RED-GREEN-REFACTOR loop per task
   **Lightweight**: Minimal validation tests for core functionality
   **Manual**: Document verification steps, execute manually
   **Hybrid**: Apply approach per task annotation

   ### Domain Placement Rules

   1. Every new file MUST go under its declared domain's source directory
   2. Contract files (public interfaces) go in the domain's contracts/ directory
   3. Cross-domain imports MUST use the target domain's public contracts only
      (never import from another domain's internals)
   4. Dependency direction:
      - business → infrastructure: ✅ allowed
      - infrastructure → business: ❌ never
      - business → business: ⚠️ contracts only
   5. When creating a NEW domain (domain setup task):
      - Create `docs/domains/<slug>/domain.md` using format from /extract-domain
      - Create source directory structure
      - Update `docs/domains/registry.md`

4) After ALL tasks complete — update domain files *(domain mode ON only — when OFF, skip this step entirely)*:

   For each domain touched by this phase:

   a) **Update domain.md § History**:
      ```markdown
      | [plan-ordinal-slug] | [What changed — 1 line summary] | [today] |
      ```

   b) **Update domain.md § Composition** (if new services/adapters/repos created):
      Add new rows to the composition table.

   c) **Update domain.md § Contracts** (if public interface changed):
      Add/modify contract entries.

   d) **Update domain.md § Dependencies** (if new domain relationships formed):
      Add to "This Domain Depends On" or "Domains That Depend On This".

   e) **Update domain.md § Source Location** (if new files added):
      Add file paths to source location listing.

   f) **Update docs/domains/registry.md** if domain status changed.

   g) **Update docs/domains/domain-map.md** if:
      - New domain was created → add node with exposed contracts
      - New contracts were added to existing domain → update node label
      - New cross-domain dependency formed → add labeled edge
      - Domain contracts changed → update the Health Summary table

   h) **Update domain.md § Concepts** (if contracts changed or new domain):

      For NEW domains:
        - Create Concepts table from implemented contracts
        - Group related contracts into named concepts (verb phrases)
        - Add narrative + code example per concept (base on actual implemented code)

      For CHANGED contracts:
        - Add new concepts to table if new capabilities introduced
        - Update existing concept narratives if entry points changed
        - Update code examples to match new signatures

      For UNCHANGED contracts: no Concepts updates needed.

5) Output:
   - Execution Log with per-task entries (write incrementally)
   - Unified diffs for all touched files
   - Evidence (test output, verification results)
   - Domain files updated (domain.md changes listed)
   - Final status mapped to acceptance criteria
   - **Deferred & Noteworthy (this phase)** — a short, scannable digest of the `Deferred`/`Noteworthy` Discoveries rows + any skipped/blocked tasks, unmet ACs, and new `TODO`/`FIXME`/`HACK` markers in this phase's diff. Empty → omit it (silence is the all-clear). Emit it as a structured block so a reviewer can lift it verbatim; never a gate.
   - Suggested commit message

6) For each completed task, follow the **progress** sub-skill's protocol (the
   delegation declared in this verb's **Delegates** field — resolve its module via
   the Registry; auto-run, same flags per task: `--plan "<PLAN_PATH>"
   --phase "<Phase N: Title>" --task "<task-id>" --status completed`).

   For the **final task of the phase**, follow the **progress** sub-skill
   with:
   ```bash
   --plan "<PLAN_PATH>" \
   --phase "<Phase N: Title>" \
   --task "<final-task-id>" \
   --status completed
   ```

STOP: Report phase complete. Routing is the flow's job.
```

## Exit

Print the output-contract summary (✅ block: what was produced, where, key fields). Then STOP. Do not name a next stage. If invoked standalone, end with exactly: "Routing is the flow's job — run the parent flow bare to continue."
