# w-ask-conv-scope — ask a question of one particular conversation

**From**: pij-instant-lynx · 2026-08-30 · Jordan: "get an agent on it then."
Closes backlog row 85. Natural follow-on to PR #80 (conversations in the
default scoped mixed corpus) — BUILD ON #80's merged main, do not fork its
semantics.

## The gap

`flowspace3 ask` takes only `--repo`. "Ask this question of THAT
conversation" — e.g. "what did we decide about X in yesterday's session?" —
is not expressible: after #80 the mixed default may surface conversation
turns, but nothing pins retrieval to one transcript.

## The job

1. `ask` gains the scope filters search already has: `--source
   code|doc|conversation|all` composing with the default repo scope
   (contract identical to search's — one scope law, shared plumbing where
   possible, tenet 16).
2. `ask --conversation <guid-or-conv:address>` pins the retrieval loop to
   that transcript's turns: every search the loop issues is filtered to
   that conversation; citations are conv:<guid>#t<n> only. Accepts the
   short guid, the full guid, or a conv: address; unknown guid = honest
   refusal naming `conversation list` (not an empty answer — row 63's
   law).
3. Coverage honesty (#67's field) reflects the narrowed corpus: the
   envelope says the loop searched ONE conversation of N turns, so a
   consumer cannot mistake a transcript answer for a repo-wide one.
4. Human affordance: `conversation list` output's next_action teaches the
   new flag.
5. Eval fixtures: (a) a question answerable only from a specific
   conversation, pinned — grounded answer with conv-only citations;
   (b) same question with a WRONG guid — honest refusal;
   (c) mutation check: unpinned ask on a multi-conversation corpus must
   not be restricted (the pin's absence is proven meaningful).

## Fence

IN: ask CLI flags + daemon ask loop retrieval filters, envelope coverage
wording, eval fixtures, docs/skill teaching line. OUT: search verb
internals (#80 owns them), ingest/write paths, ranking. Standard rules:
worktree fs3-ask-conv-scope, branch w-ask-conv-scope, plan-ack to
pij-instant-lynx before code, per-seat CARGO_TARGET_DIR, base
FS3_TEST_DATABASE_URL as server selector, never prod :7373 for tests
(read-only prod dogfood encouraged), harness checks/commit, PR into main.
NOTE: #80 may still be in the merge train — branch from origin/main AFTER
it lands (wait for it; check `gh pr view 80 --json state`).
