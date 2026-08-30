# w-conv-scope — conversation rows must be visible to default scoped search

**From**: pij-instant-lynx · 2026-08-30 · Jordan ruled dispatch ("yep").
Closes backlog row 79; touches row 74's kin (get content:null probe).

## The defect (o-prime proof, 2026-08-30)

7,788 conversation elements stored with raw_text and vectors (joined by
raw_hash) for conversation f3a6f4d9… whose `conversations.repo` names THIS
repo — yet default cwd-scoped search returns 0 of them; `--repo all
--source conversation` returns them at 0.75. Mechanism: 006's checkout
scoping resolves scope through `worktree_files`, which conversation
elements are never in, so 005's content silently fell out of the default
view (tenet-15/16 composition shape — the scoping rule was never walked to
the conversation surface).

## The job

1. The scope filter ADMITS conversation elements when the conversation's
   recorded repo matches the scope's repo identity (and worktree when the
   conversation records one and the scope pins one) — via
   `conversations.repo`/`worktree`, not `worktree_files`.
2. `--source conversation` composes with the default scope: standing in a
   checkout, it returns that repo's conversations without `--repo all`.
3. Walk the rule to the NEXT surfaces (tenet 16): `ask`'s retrieval and
   `get`'s neighbourhood must see conversation rows under the same scope
   law — verify, and fix if they share the join.
4. Regression: fixture with a conversation attributed to repo A and one to
   repo B — scoped search in A returns only A's turns; `--repo all`
   returns both; mutation-checked (remove the admit clause, test fails).
5. While in there: reproduce `get conv:<guid>#t100` returning ok:true with
   content:null (row 74 kin — t100 may be an items-only turn); make the
   envelope honest for body-less turns (say WHY it is empty), don't
   invent content.

6. **Result-composition facet (Jordan, 2026-08-30)**: the search envelope
   (and human render) reports totals BY SOURCE within the score threshold,
   beyond what the limit returned — e.g. `composition: {code: 41, doc: 12,
   conversation: 3}` plus a human line "top 10 shown; 3 conversation and
   46 more file matches within threshold" — so an all-files top-10 never
   reads as "no conversation touched this". Counts come from the same
   scored set the limit truncates (no second scan); channel/source
   vocabulary stays consistent with #74's channel tags. Test: a corpus
   where conversation hits exist below the top-k proves the facet names
   them.

## Fence

IN: store search scope SQL, daemon search/ask/get read paths, tests.
OUT: lexical-channel internals (merged #74 — compose, don't rework),
ranking, ingest/write paths. Standard rules: worktree fs3-conv-scope,
plan-ack before code, per-seat CARGO_TARGET_DIR, base FS3_TEST_DATABASE_URL
as server selector (runner mints children), never prod :7373, harness
checks/commit, PR into main.
