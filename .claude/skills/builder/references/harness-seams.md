# the-flow · harness seams — the flow-owned home for engineering-harness orchestration

The **single owner** of *where & when* the engineering harness is touched across the SDD flow. Loaded **lazily** by guided mode (`coach.md` + `00-routing.md`) only when the flow reaches a harness edge — progressive disclosure, the same way a stage's sub-skill is loaded only when its step is accepted. Direct-jump never loads it (direct-jump goes harness-less by design — `SKILL.md` § Two load paths).

> **The inversion (why this file exists).** Harness seams used to fire as **side-effects buried inside the stage sub-skills**, so the guided **engine** — which owns the rail, the narration, and the flight-plan nodes — never saw them: invisible, untracked, silently skipped. This file pulls every "should the harness run here, and how" decision **up into the flow**. The sub-skills under `references/stages/` are now pure **flow-blind *and* orchestration-blind verbs** — they describe only their own verb and carry **zero** harness *orchestration* knowledge (no `/eng-harness-flow` literal, no boot/backpressure/retro seam concept). The one thing they *may* still do is `harness observe` a friction they hit at the moment it bites (with an execution-log note as the fallback when harness-less) — capturing friction is not orchestration (builder `SKILL.md` invariant #14). All harness knowledge lives in exactly three places: **this file** (where/when/whether), the **Graph** in `00-routing.md` (the edges that carry the seams), and the external **`/eng-harness-flow` router** (what actually happens behind the door). Nothing else.

> **One door, never its children.** Every harness touchpoint is the single entry point **`/eng-harness-flow`**. Its child skills are private and may move or rename — **never name or invoke them**. The only stable surface is `/eng-harness-flow` + its `--hook` vocabulary (and the permanent `--event` alias).

---

## Why the harness seams exist

the-flow drives the product change. eng-harness-flow improves the environment used to make that change.

Agents tend to race toward "done". The harness seams deliberately interrupt that blinkered path at useful moments:

- before work, prove the environment and available deterministic backpressure;
- during work, capture friction and missing proof;
- after work, review the process that produced the code;
- at closeout, offer to encode the best lesson so the next run does not pay the same inference cost.

Code review asks whether the code is acceptable. Harness review asks whether the process could be made easier, safer, or more provable next time. The seams are **declinable by the user — never by the agent**: a real attempt at each one is part of getting to done, and only an explicit human decline (the ordered two-call decline — verbatim-words comment, then skipped — exact commands in the doctrine block below) waives one — a missing or unavailable harness resolves a seam as a receipted attempt, never a skip.

**Pay difficulties forward.** This is the seam-level face of builder `SKILL.md` invariant #14 ("don't apologise — fix"): "us" is agents and humans together, each inheriting the environment the last one left. A friction you hit and route around silently is one the next agent re-hits from scratch; a friction you `harness observe` (or fix small-and-reversible on the spot) is one they never pay for. The seams exist so that every difficulty becomes a durable signal instead of evaporating as remembered prose — the whole point is compounding, not confession.
---

## Flight-plan harness seams — creation, lifecycle & execution discipline

<!-- doctrine-parity:039 v2 — this block is mirrored byte-identically in the-flow `references/harness-seams.md`; edit BOTH copies and keep them identical (the parity guard diffs them). -->

This is the **canonical** statement of how harness seams are laid onto an SDD flight plan, how they live, and how an agent satisfies them; the-flow's `harness-seams.md` mirrors this block verbatim. *(Supersedes the v1 reconcile-on-read mechanism and the the-flow `f9a86f1` prose-cadence step — no spawn-on-read, no `provenance.reconcile_hook`, no remembered prose trigger.)*

**Creation is two parts — no gate** *(D1 reversed the old three-part "skeleton + conditional chore apply + expander" model: the full seed is now baked **unconditionally**, with **no create-time gate and no expansion gate**)*:

1. **The full pre-authored template seed.** A fixed, hand-authored JSON template (`references/flight-plan.template.json`, BYO via `harness flow create --template`) ships the **complete 11-node starter** — the `research → plan → phase-1 → review-1 → post-flight → ship` spine **plus the 5 harness chores** (phase-1's own **boot + observe + drain**, plus the two globals **backpressure** anchored off `plan` and the **post-flight harvest** anchored off `post-flight`) **plus pre-authored per-node `instructions[]`**. The CLI instantiates it **verbatim** in **one `harness flow create --template` call** and stamps root identity (provenance / created_at / events / nav). This is **unconditional — there is no router-installed / repo-provisioned gate**: the chores seed at `status: known` (designed, definite work — never speculative) and are **mandatory for the agent, declinable only by the human** (`importance` grades advisory strength for the human, never agent licence), and an un-harnessed repo carries them **un-run, not absent** — provisioning gates only whether a chore is ever *run*, **never** whether its node *exists*. A **Simple 1-phase flow is therefore complete at `create`** — the spine **and** its five baked chores, no follow-up apply.
2. **A plan-complete additive expander.** When the plan locks N>1 phases, **one additive `apply` batch** splices `phase-2..N` after `phase-1`, **each new phase carrying its own spine `review-N` (`zone: flight`, interleaved `phase-N → review-N → next`) plus its boot+observe+drain trio** spliced from **this same doctrine**. Expansion is **purely additive — nothing relocates, no `mv`** — **byte-stable idempotent**, and **likewise un-gated** (a no-harness repo still gets the full per-phase chores, un-run). It is re-fired at every structural entry (plan-complete + adopt + resume-mismatch + manual sync) and **no-ops on a complete spine**. Expansion is triggered **structurally by the `plan` node completing** — no remembered prose trigger, and **no `expand` nodeType** (the schema is not extended for it).

**The per-phase additive chore model.** **Every phase** carries its own three chores — **boot** (`harness-boot`, `pre-flight`), **observe** (`observe`, `kind: command`, `importance: recommended`, command `harness observe "<what>" --kind <kind>`), and **drain** (`harness-retro` `(drain)`, `post-coding`) — plus **two global** chores: **backpressure** (`pre-coding`, anchored off `plan`) and the **post-flight harvest** (`harness-retro` `(harvest)`, hook `post-flight`, anchored off the `post-flight` close-out stage — a pre-existing flow that anchored it off `ship` keeps that anchor: dedup on the `--hook` token, never re-parent). `observe` **gets a chore** — the inversion of the old "the `coding` seam gets no chore" rule: the continuous mid-phase capture seam now has a structural anchor, surfacing in `due_chores` the whole time `nav.now` sits on its phase, with its lifecycle terminal at that phase's **drain**. Only **`improve`** gets no chore. **`(drain)` vs `(harvest)` is one `harness-retro` type**, disambiguated by hook — `post-coding` drains that phase's observe buffer; `post-flight` is the terminal long-horizon harvest — plus the label. **No new retro nodeType.**

**D5 — never resurrect a terminal node.** No mutation (`upsert`/`set`/`mv`/`remove`, single or batched) silently flips a `done`/`skipped` node back to `todo`; the guard is **batch-wide** — a `remove`-then-re-`add`/`upsert` of the same terminal id within one batch cannot launder it.

**Chore/seam execution discipline (the anti-fake rule).** A runnable harness chore/seam is satisfied **only by actually invoking the `/eng-harness-flow` skill** through the host's skill mechanism — **the Skill tool in Claude Code**, the equivalent slash-command invocation elsewhere — with the node's **exact `--hook`**. **Never** narrate a plausible router envelope, reimplement the check inline, or flip the node to `done` without a real invocation; the envelope is narrated **verbatim from that real call** (never fabricate an insight). For `observe`, the equivalent is **actually running `harness observe "<what>" …`** — a real capture, never a narrated one. **Execution of a seam *attempt* is mandatory for the agent — declining belongs to the human alone.** Every chore is resolved by a **real attempt**, whatever the environment: router **installed and routing** → the real invocation above; router **missing** (Layer-1 miss) → record a **detection receipt** — `harness flow comment --path <flow-path> --node <chore-id> --kind validation --source agent --text "decision:unavailable reason:<…> time:<…>"` — **then** `harness flow status --path <flow-path> --node <chore-id> --to done`; router installed but the envelope says `noop`/`UNAVAILABLE` → the same two calls with that **real envelope** as the comment text. All three are **completed attempts, never skips** — a chore never sits outstanding forever blocking `nav` in an un-harnessed repo. `skipped` is reserved for the human's discretionary decline, which is **two real CLI calls in the same turn, ordered**: `harness flow comment --path <flow-path> --node <chore-id> --kind decision --source user --text "<the human's verbatim words>"`, **then** `harness flow status --path <flow-path> --node <chore-id> --to skipped` — the receipt lands before the due entry clears; a status flip alone records nothing. Never a narrated skip, never a fake `done`, and never the agent's own judgment that a chore is "optional" (no wording — "advisory", "recommended", `importance` — is ever agent licence to skip). **Receipts — comment first, status second, always:** every terminalization is the append-only validation comment above (decision/verdict/reason + time from the real envelope or detection, and for `pre-coding` also `basis_sha256:<full SHA-256 of the plan file surveyed>`) followed by the status flip — never `set-node --note` for a receipt (comments are append-only, auditable history; a note is overwritable) — and a `done` harness chore with no receipt comment is treated as unsatisfied. **Never advance past a due or unreceipted chore:** `nav set --now` may depart a node only when `due_chores` is empty **and** — since terminal nodes leave that read — a read-only inspection of the node's anchored chores in `the-flow.json` shows every terminal chore carrying its receipt comment. Prior or manual evidence never substitutes for the seam attempt (a bare `harness boot` does not satisfy a `pre-flight` chore). For `observe`, terminalize at phase end **only after** at least one real capture — its validation comment carrying the real capture's record pointer — or a human decline; **then** the phase's drain runs. **Print-then-offer is untouched:** a routed action with side effects runs only on the human's explicit acceptance — **silence is neither acceptance nor decline** (the chore stays open until a real attempt outcome or a human decline resolves it). **Recurrence — latest-plan basis:** on each guided entry recompute the plan file's SHA-256; a mismatch with (or absence of) the terminal `backpressure` chore's recorded `basis_sha256` re-opens the seam with a **deterministic node id** — `backpressure-<first 12 hex of that SHA-256>` (full hash in the receipt; D5 — never resurrect the terminal one): same bytes re-use the same node and its valid receipt (a byte-stable no-op), changed bytes create exactly one new node, fired against the latest plan. Toward the **user** the posture is unchanged: chores never gate, never score, never block the human.

<!-- /doctrine-parity:039 v2 -->

> The sections below are the-flow's detailed seam *mechanics* (how the engine fires the call, the seam map, node emission, lifecycle) — they implement the canonical block above; where they once said "`coding`/observe is unwired," 039 wires `observe` as the per-phase chore named above.

---

## How the engine presents a seam — auto-fire the call, then offer the action

A harness command is **not** a `/the-flow` command: it has no Registry row, so it is **never** rendered through the dispatch's § Command grammar or a `{{render-edge}}` slot. Two distinct things happen at a seam, and they gate **differently**:

**1. The router call auto-fires (mandatory, mechanical, read-only).** The `/eng-harness-flow --hook … --json` probe is **fired automatically** the moment the flow lands on a seam edge — the engine does **not** wait for the user to accept it, and does **not** depend on the instruction to fire it still being in context. The call is advisory and read-only: it returns a routing *decision* in a `--json` envelope; a bare `--hook … --json` call never mutates the repo, never runs anything irreversible, and never blocks. So firing it costs nothing and gates nothing. **This is the load-bearing rule: the call is driven by where the flow *is* (the durable Graph/nav position), not by whether anyone remembered to offer it** — so a long or compacted conversation can never silently skip a seam (§ Compaction-robust firing below).

**2. The routed action is print-then-offer (call-only depth).** Whatever the envelope routes *to* — a boot command, a backpressure survey, a retro drain, an adoption step — is surfaced as a **literal** copyable line (the convention `coach.md` already uses for `/eng-harness-flow`) and **offered**, under dispatch invariants **#1 (print-then-offer)** and **#4 (never gate)**. The engine narrates the envelope **verbatim**, advances *through* the corresponding flight-plan node (sets its status), then continues. The user accepts or declines the *action* — **explicitly; silence is neither acceptance nor decline** — but the *call that surfaced it already happened automatically*. Nothing with side effects runs without a go-ahead.

This is what makes a seam **first-class and un-forgettable**: the engine *always fires the advisory call* at the edge (the beat is never missed), and *always offers* what it routes to (nothing irreversible runs unprompted).

### Mid-flow fix excursion — do the small fix now, tracked (plan 056)

The `post-coding` drain (eng-harness-flow `retro.md` § Step 2) presents its observations **recommendation-led** — numbered paragraphs, the highest-value one named with its reasoning, save-all the default — and records a `disposition` for every entry it presents (declines included). When the top recommendation is a **small, reversible fix** (a justfile line, a one-line error message, a fixture), the drain may offer to **make it right now** instead of filing it for later. That in-flight fix is not a silent side-quest: it is tracked as a **flight-plan excursion node** off the current phase — `harness flow insert-node --branch-of <phase-node> --type chore …`, capturing the **intent** ("what we're fixing and why") up front and the **outcome** ("what changed + proof") once done, both as node notes. No mini-plan ceremony; the excursion node *is* the record, and the drained entry's disposition becomes `fixed-now`. This is the operational face of builder `SKILL.md` invariant #14 — fix the small stuff on the spot, but leave a durable trace so the next agent inherits the improvement, not a mystery.

---

## Compaction-robust firing — the call is positional, not remembered

The failure this prevents: as a session gets long or is `/compact`-ed, the instruction to call the harness gets diluted out of context and the flow **forgets to fire it at the proper moments**. The fix is to make firing a **mechanical consequence of where the flow is**, re-derived from durable substrate every guided turn — never a soft "remember to offer this" sitting in the conversation:

- **The trigger is position, not memory.** Every guided turn the engine reads `harness flow nav show` and checks whether `nav.now` sits on a seam edge (per the § seam map). If it does and that seam hasn't fired for the current node, the engine **fires the router call now** — whether or not anything about the harness is still in context. The flight-plan node + `nav` position *are* the durable record (KISS — no new state file).
- **`due_chores` is the backstop.** `harness flow nav show` returns `due_chores` (harness hooks anchored at `nav.now`, status still outstanding — ∉ `done`/`skipped`). A pending seam surfaces there even after a compaction wiped the narration that would have offered it — the engine fires its call and narrates it as the "due here" beat (§ Per-phase retro lifecycle; § CLI-driven cadence step 3 in [`00-routing.md`](./00-routing.md)).
- **Re-derive, then fire.** After a `/compact` the engine reloads `00-routing.md` + `coach.md` + (at a harness edge) **this file**, re-reads `nav`, and fires any seam due at the current position. Nothing about "call the harness" needs to have survived the compaction — the position does the remembering.

So "the flow forgot to run it" cannot come from context dilution: the advisory call is recomputed from `nav.now` each turn and fired the moment the position lands on a seam. (Layer-1/Layer-2 detection still applies — a *missing* or *unprovisioned* router short-circuits to the silent paths below, which *resolve* the due chore with a detection receipt + `done` rather than stranding it; firing only ever means firing the read-only `--json` call, never auto-running the action.)

---

## Two-layer detection (relocated here — load-bearing)

**Layer 1 — is the router installed?** Probe once per flow: `test -f ~/.agents/skills/eng-harness-flow/SKILL.md` (fallback `~/.claude/skills/eng-harness-flow/SKILL.md`). On a **miss**, print exactly once, verbatim:

> ⚠️ No engineering harness detected — the eng-harness skills aren't installed. Continuing without one: standard testing applies, nothing else changes. (To add the harness loop: `harness skills install --target claude-code --global`.)

…then **fire no harness beat for the rest of this session** — no router call ever runs and no routed action is ever offered. The per-phase nodes (already baked into the seed at `create`) are **resolved, not stranded**: as `nav` reaches each chore's anchor, record a detection receipt — `harness flow comment --path <flow-path> --node <chore-id> --kind validation --source agent --text "decision:unavailable reason:<…> time:<…>"` — then `harness flow status --path <flow-path> --node <chore-id> --to done` — a completed attempt, never an agent skip, so navigation never deadlocks. Never stripped, never a nag (record the detection outcome once; never re-warn **within the session**). Re-derive detection on a fresh session or after a `/compact` — a harness installed mid-flow should be picked up next session, not suppressed forever by one early miss. A repo without a harness is fully supported — never nag.

**Layer 2 — route the seam.** Router installed → call the seam with `--json` and act on the envelope (`decision: route | redirect | noop | ambiguous`):
- `route` → print-then-offer the returned command.
- setup-routing / `noop` (router installed but the repo is **unprovisioned** — no `.harness/`, no governance doc) → one calm line the first time (*"No engineering harness in this repo — proceeding without one; say 'set up a harness' anytime."*), then pass `--prompt-optional=false` on later seam calls so the user is never nagged per phase.
- Verdicts/labels are narrated **verbatim from the envelope** — never reimplement the router's checks. Boot vocabulary: `healthy / SLOW / UNHEALTHY / UNAVAILABLE`.

---

## The seam map — every edge, its hook, its node, its literal command

The flow wires **four fire-hooks** as edge-anchored seams **plus** the `coding`/`observe` hook as a **per-phase chore** (039 — the prior "skip `coding`" decision is reversed; observe now has a structural anchor, see the canonical block above). `pre-flight` appears at **two** edges (flow entry and before each phase) — same hook, different context flags, different outcome.

| Graph edge / state | `--hook` (+ context flags) | `--event` alias | Emitted node | Literal call the engine **auto-fires** at this edge (then offers the routed action) |
|---|---|---|---|---|
| **flow entry** (`start`) | `pre-flight` | `session-start` | *(usually none — detection + one calm line only)* | `/eng-harness-flow --hook pre-flight --json` |
| **post-plan refinement** off `awaiting-1b` | `pre-coding` `--spec <plan path>` | `post-spec` | `backpressure` (`branch_of: "plan"`) | `/eng-harness-flow --hook pre-coding --spec "<plan path>" --json` |
| **before each phase** (into a `phase`) | `pre-flight` `--phase "<Phase N>" --plan-dir <p>` | `pre-implement` | `harness-boot` (`branch_of: "<phase-id>"`) | `/eng-harness-flow --hook pre-flight --phase "<Phase N: Title>" --plan-dir "<plan dir>" --json` |
| **each phase end** (out of a `phase`) | `post-coding` `--plan-dir <p>` | `phase-end` | `harness-retro` (`branch_of: "<phase-id>"`) | `/eng-harness-flow --hook post-coding --plan-dir "<plan dir>" --json` |
| **at post-flight** (all phases + reviews done — the pre-ship close-out stage, **before** the plan folder archives) | `post-flight` `--plan-dir <p>` | `plan-complete` | `harness-retro` (`branch_of: "post-flight"`; legacy flows: `branch_of: "ship"`) | `/eng-harness-flow --hook post-flight --plan-dir "<plan dir>" --json` |
| **in-phase** (per-phase chore, the whole time `nav.now` sits on a phase) | `coding` | `task-pause` | `observe` (`branch_of: "<phase-id>"`, chore `kind: command` `importance: recommended`) | `harness observe "<what>" --kind <kind>` (the **real capture**, not a router call; surfaces in `due_chores` until its phase's `post-coding` **drain** terminates it) |

**What each beat is *for* (narration source):**
- **`pre-flight` @ entry** — auto-fire the call to detect + position the router (read-only); narrate one calm line; no node. Fired automatically at flow entry so a fresh or just-compacted session always re-establishes the harness.
- **`pre-coding` @ post-plan** — the backpressure survey: *can the planned work be proven by deterministic sensors, or only eyeballed?* Produces `backpressure-coverage.md` — **advisory output**: re-run the **plan** verb *informed by* it (the harness-blind plan verb does **not** auto-read the file; you fold what you learned into the re-plan intent). The *re-plan it informs* is an optional refinement; the `pre-coding` router *call* itself auto-fires at this edge (mandatory, read-only) — only the survey output and any re-plan are declinable, and only by the human. **Recurrence — latest-plan basis:** the chore's receipt records `basis_sha256:<full SHA-256 of the plan file surveyed>`; a plan re-written after the chore went terminal (hash mismatch, or no recorded basis) re-opens the seam — insert a deterministic new chore off `plan` — id `backpressure-<first 12 hex of the latest plan's SHA-256>`, full hash in its receipt (same bytes re-use the same node + receipt; changed bytes create exactly one new node; D5) — and fire it against the latest plan.
- **`pre-flight` @ phase** — the router proves the system **boots** before a line of code; verdict narrated verbatim (`healthy → build` · `SLOW → build with a note` · `UNHEALTHY → stop and ask the human: Retry / Continue without harness / Abort` · `UNAVAILABLE → standard testing`). Only the real router call satisfies this seam — a prior manual `harness boot` (or any other earlier evidence) never does.
- **`post-coding` @ phase end** — drain **this phase's** friction notes → `.retro.md` (the router owns drain-vs-harvest; the harness may offer a few environment improvements it noticed, presented conversationally).
- **`post-flight` @ post-flight** — the long-horizon reflection: harvest + present improvements + encode, fired at the close-out stage (all phases + reviews done) **before** the plan folder archives and before any ship — so the terminal harvest runs even on work that never ships. Fire the seam and land its flight-plan receipts **while the flow file is still at its pre-archive path**; the archive move is the stage's last act.
- **`coding`/`observe` @ in-phase (039)** — the per-phase capture chore: while `nav.now` sits on a phase, the agent **actually runs** `harness observe "<what>" --kind <kind>` to record decisions/surprises/dead-ends as they happen (a real capture, never narrated). It surfaces in `due_chores` for the whole phase and its lifecycle terminal is that phase's `post-coding` **drain** — the producer/consumer pair that stops observations evaporating as remembered prose.

---

## Node emission — per-phase nodes are always baked; provisioning gates only the *run*

**Per-phase harness nodes (`harness-boot`, `harness-retro`, and the `backpressure` excursion) are *always emitted* — baked into the seed at `create` (phase-1) and laid by the un-gated additive expander (phases 2..N), unconditionally (D1, no gate).** Installation and provisioning affect only whether the agent **runs** a chore — plus the session-level `pre-flight` **detection** beat — **never** whether the node *exists*. The chores are mandatory for the agent and declinable only by the human (the doctrine block above), so an un-harnessed or unprovisioned repo carries them **un-run, not absent** — and the Layer-1/Layer-2 silent paths *resolve* each chore as `nav` reaches it (detection receipt / `noop` envelope + `done`, a completed attempt): they suppress the *routed actions*, never grant the agent a skip, and never strand a chore as a permanent `nav` blocker. One rule, so the three surfaces agree:

- Router **not installed** (Layer-1 miss) → the baked per-phase nodes **still exist** in the seed and **no router call ever fires**; as `nav` reaches each anchor the engine records a detection receipt (`decision:unavailable`) and marks the chore `done` — a completed attempt, not a skip, so navigation never deadlocks. One verbatim warning at entry, then silence (§ Two-layer detection).
- Router **installed, repo provisioned** → the baked per-phase chores run their normal lifecycle (`known → done`), styled `:::harness` (violet); the agent actually invokes the router at each seam.
- Router **installed, repo unprovisioned** → the **same baked nodes** stay; the seam call fires, the `noop` envelope is recorded on the chore as its receipt and the chore is marked `done` (a completed attempt) — one calm session line (Layer 2 `noop`), then `--prompt-optional=false` so no per-phase nagging. Provisioning gates the *routed action*, not the node or the attempt.

This makes the **three emission surfaces** agree:
1. **Flight-plan node** (`the-flow.json`) — the per-phase chore nodes are **always present** (baked at `create` / laid by the expander); installation and provisioning gate only whether they *run*, never whether they appear.
2. **Rail companion line** (`coach.md`) — the **live router-reported `⚙` companion line** (distinct from the baked chore *pips*, which render from `create`) is shown only when the router actually **reported this session** (its envelope carried a `rail`/`now`/`next`); omit it until then (no empty scaffolding).
3. **Narration beat** — the print-then-offer at the edge, governed by Layer 2.

---

## Per-phase retro lifecycle — one node per phase, "owed" re-derived

The `post-coding` (phase-end) seam is a **first-class beat per phase**, not a buried side-effect: one `harness-retro` node per `phase`, `branch_of` that phase, on a `known → done` lifecycle.

- **"Drain owed" is re-derived, not stored** — no new state file. A phase whose node is `done` while its `harness-retro` sibling is still `known`/absent ⇒ that phase's drain is owed. The flight-plan node **is** the durable record (existing `status` + `branch_of` fields suffice — KISS, no rollup).
- **Observe terminalizes before the drain.** At phase end the phase's `observe` chore is terminalized first — `done` only on at least one real capture (receipt/pointer noted on the node), or `skipped` on the human's two-call decline — and only then does the `post-coding` drain fire (the producer closes before the consumer runs).
- **Drain before harvest.** At phase/session end the router drains the non-empty buffer first; harvest (the `post-flight` beat at the post-flight close-out stage) reads `.retro.md`. The router owns that ordering — the flow just offers the beat at the edge.

### Chore-flag ownership — the-flow lays it (Route A); R-1 governs only coexistence

**Route A (plan 039 · D1): the-flow is the *sole CLI writer* of the flight plan, so it owns the chore node + flag + status.** The-flow **bakes phase-1's chores into the seed at `create`** and lays **phases 2..N via the un-gated additive expander** — **unconditionally, no gate** (the chores are human-declinable, so an un-harnessed/unprovisioned repo carries them un-run, not absent; provisioning gates only the *run*). `eng-harness-flow` is **stateless** — it owns the *routing decision* and is the *invoked action* at a seam, but it **writes nothing** to `the-flow.json`. (This is *more* correct than the original R-1 framing: a stateless router never physically owned a flag; the sole CLI writer does.)

The **pre-039 R-1 path** — where `eng-harness-flow` flags an existing seam node — governs **only the narrow back-compat coexistence case**: the loop running alongside a `the-flow.json` that a **pre-039** the-flow built as **bare seam nodes** (no chore flag). In that case, to avoid double-placement:

- Dedup on the **`--hook <X>` token** inside a node's `command` (one chore per hook).
- If a bare seam node already emitted (`harness-boot` / `backpressure` / `harness-retro` / `observe`) carries that hook's command, **flag that existing node** (`harness flow set-node --chore-kind command --importance …`) — never add a duplicate; the node keeps its type and gains a chore pip.
- Only when no such node exists is a fresh chore node added.

So in that back-compat case the two paths **converge via D5 + byte-stability with no double-placement** — a re-flag of an already-flagged node is a byte-stable no-op. (The full chore shape lives in the harness skill's `flight-plan-ops.md`.)

**Anchored, and surfaced as "due here" (harness plan 033).** Each injected chore is `branch_of` its spine node (the hook→anchor map in the harness skill's `flight-plan-ops.md`: `pre-flight`→first phase, `pre-coding`→`plan`, `post-coding`→last phase, `post-flight`→`post-flight` — fallback `ship` on a flow with no post-flight node), so it renders as a **connected dotted excursion**, never a floating orphan. The engine surfaces the outstanding ones at the current node **every guided turn**: `harness flow nav show` returns a **`due_chores`** array — the chores whose anchor is `nav.now`, status still outstanding (∉ `done`/`skipped`) — which the coach narrates as the **"due here" beat** (§ CLI-driven cadence step 3 in [`00-routing.md`](./00-routing.md)). So the right hook is shown at the right node and stops getting missed, even across a `/compact`. Never a gate for the human — for the agent, each due chore is work to resolve before departing the node. (Older CLIs without `due_chores` simply omit the beat — a graceful no-op.)

---

## Reconcile pre-anchors seam nodes (the every-entry pass)

the-flow's **§ Reconcile the spine** pass ([`00-routing.md`](./00-routing.md), the `sync` verb) keeps the harness loop *visible up front* instead of appearing only at the edge you reach. On every guided entry — **unconditionally (D1, no install/provision gate)** — reconcile ensures the per-phase seam nodes exist across **all known phases** — `harness-boot` before each phase, `harness-retro` after each phase, `backpressure` off `plan`, the `harness-retro` harvest off `post-flight` (a legacy flow whose harvest is anchored off `ship` keeps it — dedup on the `--hook` token, never re-parent) (the § seam map's nodes, for the whole roster, not just the current edge). The nodes are already baked at `create`; reconcile re-asserts them so a flight plan that predates a phase split still converges on the full roster.

**Ownership (Route A): the-flow lays node + flag together; the byte-stable expander re-asserts both.** Under Route A reconcile re-asserts the per-phase chores via the **byte-stable idempotent expander** (the create-time/plan-complete `apply` batch) — node **and** flag **and** status, **un-gated (D1 — the seed is baked unconditionally; the expander is purely additive)**, no-op on a complete spine (§ Chore-flag ownership above; [`00-routing.md`](./00-routing.md) § Reconcile the spine item 3). It never adds a `chore`-type twin. The **pre-039 R-1 path** — reconcile emitting a bare seam node and `eng-harness-flow` later flagging it — governs **only the narrow back-compat coexistence case** (a `the-flow.json` built by a pre-039 the-flow); D5 + byte-stability converge the two with no double-placement. Either way reconcile makes every flight plan converge on the one canonical shape (no standalone `chore` twins; no missing `harness-boot`/`observe`/`harness-retro` at any phase; no missing `backpressure` or post-flight harvest `harness-retro`).

**Installed-but-unprovisioned / not-installed → the baked nodes stay** (§ Node emission, § Two-layer detection): the per-phase chores are already in the seed (baked unconditionally at `create`), so reconcile finds the spine complete and **no-ops** — it never strips them; each resolves to a receipted `done` (detection receipt / `noop` envelope) as `nav` reaches its anchor, with **no per-phase nagging** (chores never nag the human; the silent-path resolution is a completed attempt, not an agent skip).

---

## Honored, not forced — the action, not the call

Draw the line precisely — it runs between the **human** and the **agent**. The **router call** auto-fires (it's read-only — § How the engine presents a seam, § Compaction-robust firing); the **action it routes to** is a **print-then-offer beat** (invariants #1/#4) that the **user** explicitly accepts or declines (silence is neither) — actions with side effects run only on acceptance, and a decline is two real CLI calls, ordered: `harness flow comment --path <flow-path> --node <chore-id> --kind decision --source user --text "<their verbatim words>"`, then `harness flow status --path <flow-path> --node <chore-id> --to skipped`. So seams **never** gate, score, or block the *human*, and nothing with side effects runs without a go-ahead — while the agent holds **no skip licence at all**: "advisory", "recommended", and `importance` grade the offer's strength for the human, never agent permission. Best-effort remains the posture toward the environment: a router that's missing, a repo that's unprovisioned, or an `UNAVAILABLE` verdict all fall back to standard testing with at most one calm line. The user can still opt out conversationally — "don't use the harness" stops the calls; there is **no** sentinel file.

---

## Not-installed & unprovisioned — the silent paths (summary)

| Situation | What the flow does |
|---|---|
| Router **not installed** | one verbatim warning at flow entry, then **fire no harness beat**; the baked per-phase nodes **remain, never stripped** — each resolved to `done` with a detection receipt (`decision:unavailable`) as `nav` reaches it, silence otherwise |
| Router installed, repo **unprovisioned** | one calm session line, `--prompt-optional=false` thereafter; the per-phase nodes **remain (baked)** — each resolved to `done` with its real `noop` envelope as receipt, **no nagging** |
| Verdict `UNAVAILABLE` at a seam | note it, proceed with standard testing (not an error) |
| Verdict `UNHEALTHY` at the boot seam | stop and ask the human: Retry / Continue without harness / Abort |
| User says "don't use the harness" | conversational opt-out — stop calling the router (there is **no** sentinel file) |

---

## Seam contract (mirrors eng-harness-flow)

> **`harness_seam_contract: v2`** — this is the durable mirror of the small slice of the external harness contract that the flow depends on. It is the **one place to edit** when the upstream contract changes. This block is the in-skill copy of the plan's `## Maintenance & resync` record (`docs/plans/033-flow-owned-harness-seams/flow-owned-harness-seams-plan.md`).

**What we mirror (the dependency surface):**

| Facet | Value (v1, 2026-06-17) |
|---|---|
| `harness_seam_contract` | `v2` (2026-06-28, plan 039) — **bumped from v1**: the `coding` hook moved from *not-wired* to **wired as a per-phase `observe` chore**, and `observe` was added to the mapped node types — both mirrored facts that changed meaning. **Bump** when any mirrored fact changes meaning (a hook renamed or its wired-state changed, an alias remapped, a verdict added, a mapped node type added). *(v1 history: the 021 five-hook contract; the 2026-06-19 ship remap did **not** bump — moving the `post-flight` retro from the `merge` edge to `ship` changed only which the-flow Graph edge the hook rides, an internal-flow fact, not a mirrored upstream fact.)* |
| Hooks we **wire** (emit `--hook`) | `pre-flight` (flow entry **and** before each phase) · `pre-coding` (post-plan) · `post-coding` (each phase end) · `post-flight` (at the **post-flight** close-out stage — all phases + reviews done, pre-ship; remapped from `ship` 2026-08-04, earlier from `merge` 2026-06-19 — both internal-flow edge moves, no contract bump) |
| Hook we wire as a **per-phase chore** (039) | `coding` — the `harness observe "<what>" --kind <kind>` capture (one buffer entry per call), ridden as a per-phase `observe` chore `branch_of` the phase, terminal at that phase's `post-coding` drain. *(Reverses the prior `task-pause`/`coding` skip — observe now has a structural anchor in `due_chores` so it stops getting forgotten; per the canonical block above.)* |
| `--event` alias (permanent) | `session-start`→`pre-flight` · `pre-implement`→`pre-flight` · `post-spec`→`pre-coding` · `phase-end`→`post-coding` · `plan-complete`→`post-flight` · `task-pause`→`coding`. The flow **emits `--hook`**; `--event` is retained only for **back-compat understanding** — mapping/reading an older router's envelope, never a command the flow emits or down-emits (an older router is a runtime-dependency gap → reinstall, not an automatic fallback) |
| Envelope `decision` | `route` · `redirect` · `noop` · `ambiguous` (+ an additive `hook` field on every routing envelope) |
| Boot verdicts | `healthy` · `SLOW` · `UNHEALTHY` · `UNAVAILABLE` |
| Node types this maps to | `backpressure` · `harness-boot` · `harness-retro` · `observe` (039 — the per-phase capture chore) (defined in `flight-plan.schema.json`) |
| Slug rule | friendly name → installed slug; **never append a guessed version suffix** |

**Upstream source-of-truth (authority, in priority order):**

1. **Live manifest (preferred resync anchor)** — `/eng-harness-flow --hooks --json` → Shape A `{ manifest_version, hooks[5] }`, nine fields per hook (`hook`, `intent`, `run_at`, `kind`, `invoke`, `aliases`, `produces`, `needs`, `preconditions`). Machine-readable and version-stamped — **re-read it, don't hand-diff prose.**
2. **Source SKILL.md** (the `AI-Substrate/harness-engineering` repo) — `skills/eng-harness-flow/SKILL.md` §§ **Lifecycle hooks** (the five hooks + the `--event`→`--hook` alias map), **The `--hooks` discovery manifest** (the nine-field contract), **The `--json` routing envelope** (`decision` enum + additive `hook` + boot verdicts), **Parameter contract** (`--hook` / `--event` / flag surface).
3. **Runtime probe** (detection, not contract) — installed at `~/.agents/skills/eng-harness-flow/SKILL.md` (fallback `~/.claude/skills/…`); what Layer 1 tests.

**Resync procedure — when the harness family or its hooks change:**

1. Run `/eng-harness-flow --hooks --json` and read the 5-entry manifest (or, if the router is uninstalled, read the source SKILL.md §§ above).
2. Diff its `hook` / `aliases` / `kind` / `produces` / `decision` / verdict tokens against the mirror table above.
3. Reconcile the **seam map** and the **mirror table** in this file; if any mirrored fact changed meaning, **bump `harness_seam_contract`** and note what changed.
4. `just check-flow` (L1–L6) + redeploy `just install-skills-from-source`.

**Why no script.** A drift-check script was considered and **declined** — the `--hooks --json` manifest + this procedure are the contract; this file is the one file to edit.

**Runtime dependency (honesty).** The flow **emits `--hook`**, which assumes the **021 hook-aware router** is installed (≥478-line SKILL.md). An older `--event`-only router cannot parse `--hook`, and the flow does **not** silently down-emit `--event` — an older router is a **runtime-dependency gap**, surfaced as a reinstall prompt, not an automatic fallback. The `--event` alias column above documents the hook→event mapping for back-compat *understanding* (and for reading an older router's envelope), not a command the flow emits. To make `--hook` live, install or update the packaged harness skills (`harness skills update --target claude-code --global`).

---

> **Keep in sync, single-source.** The hook-woven flow tree in `docs/how/the-flow-harness-seams.md` (Appendix A of the plan) is a **rendered view** of the seam map above — when the seam map changes, regenerate that tree; never let it become a second source.
