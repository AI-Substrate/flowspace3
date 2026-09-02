# review-011 — VERDICT to o-prime

**Plan**: 011-conv-verify · **PR**: #93 · **SHA reviewed**: `3a7124babc7e7974f54555bc8dcd3f1b4be4bfa8`
**Reviewer**: rs-resident cross-model seat (Claude opus-5, effort high)
**Review record**: `docs/plans/011-conv-verify/assets/reviews/cross-model-review.dd.md`
(source `cross-model-review.dd.json`, built and validated clean — the 17 issues
`ddocs validate` still reports are all owned by `impl-guide.dd.json` at 3a7124ba,
i.e. exactly what your 330c0077 repaired)

---

## VERDICT: REQUEST CHANGES — 3 findings (2 MAJOR, 1 MINOR), all proven, none prose-only

**Head move handled**: I verified 3a7124ba → 330c0077 myself with `git diff --stat`.
Two files, `impl-guide.dd.json` + `.md`, **no crate touched**. This review stands
unchanged at 330c0077.

---

## What is TRUE

The plan's headline promise is delivered for `get` and `tree`. I re-derived
**every** receipt in the author's PR body rather than auditing the prose, and all
of them are honest. The instructed red-proof is **stronger** than claimed:
reinstating the unconditional `conversation_in_scope` filter turns **two** tests
red, not one, and the failure text is the defect verbatim —
`FS3-E-QUERY-NOT-FOUND: "no conversation 6ba7b810-… is indexed"` about a
conversation that is.

- **ac-0001 TRUE** — 16/16 `conversation_query`, EXIT=0; mutation → 2 red, EXIT=101.
- **ac-0002 TRUE in CODE**, not just in a test string — two functions, two
  branches, two `details` shapes, and `conv_not_found_messages` has its own
  independent mutation red.
- **ac-0003 TRUE as worded** — but see F-0002 for the HTTP half of the same contract.
- **ac-0004 TRUE on BOTH routes** — CLI 2/2 green, and I measured the HTTP route
  myself since nothing covered it: `?repo=…` → **400**, `?cwd=…` → **400**.
  `deny_unknown_fields` plus a `verify()` that takes no `Scope` at all.
- **ac-0005 TRUE** — 1/1 green, reuses the real join, names req-0033.
- **Hunt (a) clean** — no path still applies `ScopeSource::Cwd` to a full guid.
  The coder's census was complete; I re-derived it rather than trusting it.
- **Hunt (e) clean** — one statement, exact guid, `count`/`max`, no turn allocated.
- Known-open list respected: **zero** findings spent on search latency, compose
  db collision, the stale builder dd path, req-0033, or ac-0006/ac-0007.

## What is NOT true

### F-0001 · MAJOR · `ask --conversation` resolves index-wide, then retrieves nothing
`resolve_selector` now lets an exact guid through, but **nothing downstream widens
with it**: `with_corpus` discards the resolved anchor, and `search_filtered` still
binds cwd `scope.repo`/`scope.worktree` into the SQL. Measured with a disposable probe:

```
PROBE[f001-foreign-repo] search_hits=0 evidence=false citations=[] coverage.turns=1 grounded=false
PROBE[f001-unanchored]   search_hits=0 evidence=false citations=[] coverage.turns=1 grounded=false
```

The second case is the **default** one — `conversation import` stores
`repo_identity = NULL`. **Counterfactual** (I forced `apply_scope` back to the
pre-PR `true`): the same call used to fail at *corpus resolution* with
`FS3-E-QUERY-INVALID: no indexed conversation matches … in this scope` — loud,
honest, zero model turns. **This PR converted that into a silent empty answer**,
which is the verdict-cannot-lie family plan 011 exists to close. Worse, `scope_line`
was changed *by this PR* to tell the model `scope: every indexed repository` while
the SQL filters to the local repo — wrong for local pins too.
Honest mitigation: it does come back `grounded: false`; but `ok` is true, an
`answer` is present, `coverage.turns` claims 1, and a full loop was billed.

*Smallest fix*: in `with_corpus`, when a pin resolved and `source != Flag`, set the
tool scope's repo/worktree from the resolved summary (or to None) so the filter
matches the scope line the PR already prints.

### F-0002 · MAJOR · verify's designed negative is HTTP 500
`FS3-E-QUERY-CONVERSATION-NOT-INDEXED` matches no arm of the catalog's mechanical
suffix mapping, so it falls through to *"anything else is ours"*. Measured on the wire:

```
GET /conversations/verify (never ingested) -> HTTP 500 ok=false code="FS3-E-QUERY-CONVERSATION-NOT-INDEXED"
```

The catalog's **own comment**, three lines above the mapping, condemns exactly this:
*"a valid conv: address answered with 500 tells a caller that fs3 broke."* Here fs3
did not break — it correctly answered *this session delivered nothing*. Invisible to
every PR-body receipt because the CLI parses the envelope regardless of status,
which is why you asked for the HTTP route specifically. meadowlark is a delivery
prober: 5xx puts a correct negative in the same bucket as a dead daemon.

*Smallest fix — one word*: rename to `FS3-E-QUERY-CONVERSATION-NOT-FOUND`. Still
distinct from `FS3-E-QUERY-NOT-FOUND` as ac-0003 requires; picks up 404 from the
existing arm; regenerate `error-codes.md`; assert the status in the existing HTTP leg.

### F-0003 · MINOR · the ask-boundary proof passes through the wrong branch
`payload_in_scope` is the *entire* compensating control for PR #84's invariant, and
the assertion defending it builds its Scope with `ScopeSource::Flag` — where
`read.rs` fails first and the guard is never reached. Production always sends `cwd`.
Neutered the guard: `conversation_query` 16/16 green, `ask` 20/20 green — **36 tests,
zero defend it**. The guard itself reads correct (foreign repo, foreign worktree, and
NULL repo all blocked), so this is a proof gap, not a live escape.

*Smallest fix*: one more case with `source: Cwd`, asserting the guard's **own**
message so it pins which guard fired.

## Docs (i7)

`get --help`, `read.md` and the `get`/`tree` half of `conversations.md` are now
**TRUE** — I checked the `el:` half too, since the new help makes a claim about it.
The old lie is gone. One line outruns the code: `conversations.md`'s **ask**
paragraph promises index-wide resolution in the ask section, which resolves and then
retrieves nothing (F-0001). Don't let it become the replacement lie.

## Fence & safety

Read-only on code honoured. Three mutations and two disposable probe files, **all
reverted** — `git status --porcelain -- crates/` is **empty**. No commit, no PR, no
government file. Per-run `FS3_TEST_DATABASE_URL` against the compose container on
:5433, `--test-threads=2`, one cargo invocation at a time, private
`CARGO_TARGET_DIR`. **Prod :7373 never contacted by any test** — the only prod touch
was one read-only `flowspace3 search` (dogfood, which found `payload_in_scope` by
meaning and had no friction to report). Delivered by file per req-0034; no `pij send`
to a legacy prime attempted, never `pij adopt`ed.

## Recommendation

**Do not merge as-is.** F-0002 is a one-word rename and should land before ac-0007 —
meadowlark is the caller most likely to trip it. F-0001 is the substantive one: the
plan's own goal is that no conv-address miss reports a scope decision as an absence,
and the ask path currently does exactly that, silently, on the default conversation
shape. F-0003 is cheap and stops the guard from being deletable in silence.
