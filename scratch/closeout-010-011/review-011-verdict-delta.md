# review-011 — DELTA VERDICT to o-prime

**Plan**: 011-conv-verify · **PR**: #93
**Delta sha**: `a80e9a57bc0be87e9ef7dda2a4f1134b76a45db0` ("fix: preserve conversation
verification contracts", on 330c0077) · **Round 1 sha**: `3a7124ba`
**Review record**: `docs/plans/011-conv-verify/assets/reviews/cross-model-review.dd.md`
(round 1 rows preserved unchanged; delta rows f-0010…f-0015 + v-0005 appended;
`ddocs validate` now **status: ok, zero errors** — the neighbourhood is clean too,
your 330c0077 repaired the impl-guide)

---

## DELTA VERDICT: APPROVE

All three findings **FIXED**, each defended by its **own** test, each proven by
mutating **that fix alone** — not by reading the diff.

| # | fix | its own test reds when the fix is reverted? |
|---|---|---|
| f-0001 | `with_corpus` clears repo/worktree for a resolved pin under cwd | ✅ coder's new fixture + both my probes |
| f-0002 | rename → `FS3-E-QUERY-CONVERSATION-NOT-FOUND` | ✅ **two** mutations — see below |
| f-0003 | `ScopeSource::Cwd` case asserting the guard's own message | ✅ exactly 1 red (round 1: 36 green) |

### f-0001 — measured, not read

My round-1 probes, re-run independently of the coder's fixture (mine drives
`resolve_corpus` + `ask` directly with **no** source narrowing; theirs posts to HTTP
with `source: conversation`):

```
PROBE[f0001d-foreign-repo] search_hits=1 evidence=true citations=["conv:…#t1"] coverage.turns=1 grounded=true
PROBE[f0001d-unanchored]   search_hits=1 evidence=true citations=["conv:…#t1"] coverage.turns=1 grounded=true
```

Round 1 measured `search_hits=0 … grounded=false` on both. The coverage envelope's
claim of 1 turn is now **honoured** rather than advertised.

Your third ask, the explicit-`--repo` mismatch, still refuses loudly:
`FS3-E-QUERY-INVALID: no indexed conversation matches conv:… in this scope`. The
widening did not swallow the explicit filter — it is correctly conditioned on
**both** `conversation.is_some()` and `source != Flag`.

### f-0002 — I needed TWO mutations, and the second is the one that mattered

Reverting the code string reds at `conversation_query.rs:683` — but that is the
*code* assertion, which fires **before** the status assertion and would have masked
it. My finding was about the **wire status**, so I mutated only the mapping
(name intact, this one code special-cased back to 500):

```
conversation_query.rs:731  left: 500  right: 404
```

The status assertion is load-bearing in its own right. `error-codes.md` regenerated
to `| false | 404 |`, drift test green, no stale spelling anywhere in source.

### f-0003 — the exact inversion of round 1

Re-neutering `payload_in_scope` now reds **exactly one** test, at the new
`ScopeSource::Cwd` case asserting the guard's own message. Round 1: **36 tests
green** under that identical mutation.

## The seam the fix CREATES — hunted, holds

No fix's own test covers this, so I went looking. In pinned mode the tool scope is
now **deliberately wide open**, which makes `guard_address` the *only* remaining
confinement — f-0001 and f-0003 pull in opposite directions, and a widening not
conditioned on a pin having resolved would have bought retrieval at the cost of
PR #84's boundary. Probed directly:

```
trace[0] get conv:<other>#t1  failed=true   ← foreign turn refused
trace[1] get conv:<other>      failed=true   ← bare foreign conversation refused
trace[2] get conv:<pinned>#t1 failed=false  citations=["conv:<pinned>#t1"]
```

Holds. Only the pinned transcript is readable or citable.

## Docs

Round-1 docs finding **closed**. The ask paragraph no longer stops at resolution —
"…and the tool search/read scope follows that resolved transcript even when it is
foreign or unanchored" — and every clause is now backed by a measurement rather
than aspiration. `docs_bundle` 5/5.

## One NIT — f-0014 — explicitly NOT a blocker, your call

For a pinned ask, `meta.scope` on the envelope still reports the **pre**-widening
scope (`http.rs` builds `meta` at :167-169, attaches at :222), so it can read
`scope.repo = <where I'm standing>, source = cwd` for a run that retrieved
index-wide inside a foreign transcript. The **model-facing** `scope_line` is now
correct — that was round-1 f-0001 and it is fixed; this is the consumer-facing
field only, and `coverage.corpus.conversation.guid` names the true corpus. I raise
it solely because scope.rs's own doc says that field exists "so the scope is never
something a consumer has to infer". Cheap close if you want it: build `meta` from
the widened scope, or omit `scope.repo` when a pin widened it. **Merge without it.**

## Final state at a80e9a57 — pristine

```
conversation_query   16/16  EXIT=0
ask                  21/21  EXIT=0   (was 20, +1 new fixture)
fs3-core error_codes  1/1   EXIT=0   (drift)
fs3-cli docs_bundle   5/5   EXIT=0
```

Every mutation reverted, both disposable probes deleted,
`git status --porcelain -- crates/` **empty**. Scratch DB on the compose container
at :5433 throughout; **prod :7373 never contacted by any test**. Delivered by file
per req-0034 — no `pij send` to a legacy prime, never `pij adopt`ed.

## Recommendation

**Merge.** The three fixes are minimal, correctly conditioned, and each one is now
the thing its own test would catch. `harness checks` on the fix head and the merge
order remain yours.
