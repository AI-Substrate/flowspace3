# w-ask-budget-honesty — budget exhaustion must be an honest terminal

**From**: pij-instant-lynx · 2026-08-30 · Jordan ruled: "yes fix the llm
token runout issue thanks". Closes backlog row 71 (three sightings in one
day: sylac 8-iter, camel 6-citations, bedbug 4-citations — all
ok:true/grounded:true/answer:null).

## The defect

An ask that exhausts its token budget (DEFAULT_TOKEN_BUDGET = 80_000,
crates/core/src/config.rs:584) returns a SUCCESS-shaped envelope with
answer:null — grounded:true describing an answer that does not exist, and
ok:true forcing every consumer to null-check. Same law as #67/rows 42/62/74:
the envelope must not claim more than it measured.

## The job

1. Budget exhaustion becomes its own honest terminal: the envelope says
   stopped:token_budget WITH ok/grounded semantics that cannot be read as
   success — grounded absent/false when no answer was synthesized; the
   next_action names the two real moves (narrow the question / raise the
   budget) as today, unchanged.
2. SALVAGE, don't discard: when the loop dies with citations gathered,
   return them as `evidence` (the citations + one-line finding per
   iteration it completed) labelled explicitly as partial — three seats
   today had 4-6 useful citations thrown away behind a null.
3. Walk the law to the other bounded terminals (tenet 16): iteration-limit
   exhaustion and provider failure mid-loop get the same honest shape.
4. Eval fixture: a question forced over budget proves the envelope shape
   (no success-shaped null) and the salvage payload; mutation-checked.
5. Do NOT raise the default budget — Jordan asked for honesty, not a
   bigger ceiling; note in the PR that budget is config-overridable.

## Fence

IN: ask loop terminals (crates/daemon ask module), envelope fields, human
render of the partial shape, eval fixture, docs. OUT: retrieval, ranking,
provider adapters, the budget default. Standard rules: worktree
fs3-ask-budget-honesty, plan-ack before code, per-seat CARGO_TARGET_DIR,
never prod :7373 for tests, harness checks/commit, PR into main.
