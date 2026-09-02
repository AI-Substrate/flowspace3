# review-010 DELTA VERDICT — sha 3606c1397f78564716b0d640f8bfaf59f599b402

**Reviewer**: pij-fiscal-tick (github-copilot/claude-opus-5).
**Delta**: `6377a1f..3606c13`, one commit `fix: cover compatible embedding cap
errors`. Round-1 review of 6377a1fe stands; this judges the delta only.
**Worktree**: `/Users/jordanknight/substrate/flowspace/fs3-review-010`, detached
at 3606c13. `git status --short crates/` **empty** — no code changed by me.

## VERDICT

**APPROVE — `no_material_findings`. All three fold-ins are correct, and all
three are individually mutation-checked by me, not by the author's prose.**

`cap_rejection` went **4 → 7** exactly as claimed. The four alignment numbers are
**unchanged**. impl-guide now validates. The heal-arm mutation, re-performed on
the rewritten arm, is still red in all three tests.

## Every fold-in, individually mutation-checked

I reverted each fix in turn and confirmed its own new test — and only its own
test — goes red. Three fixes landing together is exactly the shape where a test
that never actually pinned anything goes unnoticed.

| fix | revert applied | result |
| --- | --- | --- |
| **f-0001** compat classification | removed the `embedding_input_too_long` call from `OpenAiCompatConfig::try_post` | `openai_compat_cap_rejection_is_typed_with_input_index` **FAILED**; its control and both azure tests stayed green — red correctly isolated |
| **f-0002-A** overlap clamp | restored the pre-delta `assert!(overlap_bytes < window_bytes)` | `chunk_plan_bytes_clamps_overlap_below_tiny_window` **panicked → FAILED** at `enrich.rs:86` |
| **f-0002-B** ratio | restored the integer-division form | `heal_ratio_…_without_integer_truncation` **FAILED**: `left: "1 byte/token"`, `right: "3750 bytes/7500 tokens"` |
| **f-0003** parsed cap | restored the pre-delta hardcoded-8192 guard | `openai_cap_rejection_parses_reported_4096_limit` **FAILED**; the other six stayed green — red correctly isolated |
| **heal arm** (round-1 control, re-performed on the NEW arm) | `… InputTooLong { .. } if false =>` at `enrich.rs:851` | **3 failed**, `retryable: true`, exhaustion still shows `Drained { completed: 0, retried: 1, failed: 0 }` |

Every revert was undone with `git checkout --` and the tree re-verified clean.

**The f-0002-B revert reproduced my round-1 finding verbatim** — the mutant
printed `1 byte/token` for a 3,750-byte window. That is the false ratio I
predicted, observed directly rather than argued.

## f-0001 landed on the right site

Verified by reading the call path, not the diff, as you asked. The patch is at
`OpenAiCompatConfig::try_post` (the single `try_post`, line ~197-206) — the site
`OpenAiCompatEmbedder::embed` reaches via `self.config.try_post(&self.http,
"embeddings", &request)`. **`OpenAiCompatSummarizer::attempt_chat` (line 458) is
correctly untouched.** Limpet did not fall into the trap I wrongly warned about,
and my retraction landed in time.

`GitHubCopilotEmbedder` inherits the fix for free — it is a newtype forwarding to
`OpenAiCompatEmbedder`.

## One correction against myself, and the thing it turned up

**I initially reported the f-0002-A clamp test as a weak red-proof. I was
wrong.** My first mutation removed only the inner clamp, which is *not* how the
fix is composed, and the test passed — so I reported it as unpinned. That was
the wrong revert.

The fix as shipped is the **removal of the `assert!`** plus a clamp preserving
the invariant. Reverting *that* — restoring the assert — makes the test panic.
The red-proof is valid. I record the wrong first attempt because it is the same
error class I flagged against myself over the `try_post` trap: a plausible claim
written before it was checked.

The probe did surface one true, minor fact worth knowing: **the clamp itself is
behaviourally inert here.** With window 468 / overlap 600 the chunk count is
**533 either way** — clamping to `window-1 = 467` still yields a one-byte step,
and the progress guard at `enrich.rs:123-128` already prevented non-advancement.
So the load-bearing half of f-0002-A is removing the release panic; the clamp is
belt-and-braces that keeps the documented invariant true. Correct and harmless —
**not a finding**, and I am not asking for a change.

## Design improvements over what I specified

Two places where limpet's fix is better than my prescription, worth recording so
the choice is deliberate:

1. **The clamp moved *inside* `chunk_plan_bytes`** rather than being duplicated
   at the heal call site. I asked for the caller to clamp; putting it in the
   callee means *neither* caller can violate the invariant, which is what the
   two-callers-disagreeing defect was actually about. Better fix, correctly
   generalised.
2. **`heal_ratio` reports two measured numbers** (`"7500 bytes/7500 tokens"`)
   instead of a quotient, and `window_bytes` is now carried on `PreparedChunk`
   so the message states what was actually used. The failure message also now
   names `chunk.heal_round` rather than `MAX_HEAL_ROUNDS` — same value today,
   but it stays true if the constant ever moves. That was the whole point of
   f-0002.

`embedding_input_too_long` keeps both safety gates (`route == "embeddings"`,
`status == BAD_REQUEST`); only the number became dynamic, and an unparseable
tail returns `None` via `?`. The false-positive direction is unweakened.

## Checks from my criteria file

| check | result |
| --- | --- |
| `cargo test -p fs3-providers cap_rejection` 4 → 7 | **exit 0**, 7 — enumerated per binary: azure_openai_stub 2, openai_compat_stub 2, openai_stub 3 |
| 4096 red-proof present and real | present; **red** under revert |
| `chunk_plan_bytes(_, 468, 600)` no panic | present; **red** (panic) under revert |
| ratio assertion at round ≥ 2 | present (`heal_window_bytes(2) == 3750`); **red** under revert |
| four alignment numbers unchanged | **unchanged**: `7→10, 33→50, 1→2, total 41→62` |
| oversize suite | **exit 0**, 12 passed |
| daemon lib enrich | **exit 0**, 17 passed (was 6 chunk_plan + 11; now +2 new) |
| each fix mutation-checked individually | **done, 4/4**, each red isolated to its own test |
| mutation re-performed on the new arm | **done**, 3 red, `retryable: true` |
| impl-guide validates | **`ok`** — 17 errors → 0 |
| enrichment.md compat rows | **both gaps closed** (below) |

Final green after all five mutation cycles: providers 7 ok · daemon lib 17 ok ·
oversize 12 ok. Tree clean.

## enrichment.md — both gaps I named are closed, and correctly

- New row `openai-compat embeddings | 8192 | current adapter declaration;
  provider rejection still carries its reported cap`, and the old ambiguous
  `openai-compat | 6,000` row is now correctly labelled `openai-compat chat`.
  That was precisely the ambiguity I had to read source to resolve.
- Prose now reads "OpenAI, Azure, and OpenAI-compatible embedding adapters
  classify a 400 whose body reports `maximum input length is N` … carrying the
  reported cap", and the exhaustion sentence names "the actual
  window-bytes/token-cap ratio". Both match the code as shipped.

## Standing items unchanged from round 1

- ac-0005 / ac-0006 still open — post-bounce, yours. A clean drain will **not**
  exercise the heal path (the prod item splits by alignment alone); do not read
  a green drain as proof the heal works. You have confirmed the five keys'
  `last_error` is already captured.
- `put_embeddings` upserts without pruning. Unreachable today because chunk
  counts only ever increase; noted in round 1 as the one way the
  no-duplicate-rows guarantee could later erode silently. Still not a finding.
- `DL-001` remains **captured, not drained**. Yours.
- `fs3_review_010` kept as instructed; not dropped, server not churned.

**One line for the log:** APPROVE at 3606c13 — `no_material_findings`; all three
fold-ins correct and each independently mutation-checked with its red isolated
to its own new test; `cap_rejection` 4→7; alignment numbers unchanged; impl-guide
validates; heal-arm mutation still red on the rewritten arm.

— pij-fiscal-tick, detached at `3606c1397f78564716b0d640f8bfaf59f599b402`.
