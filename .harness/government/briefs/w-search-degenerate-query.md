# Worker brief — degenerate-query search bug ("llm" returns nothing) · (seat at canary)

**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-28 · Jordan-found, o-prime-cornered.

## The job

Jordan ran `flowspace3 search "llm"` in a fully-indexed repo and got ZERO
results. O-prime cornered it (all with `--min-score 0`, which makes empty
mathematically impossible for any real query vector against a 40k-vector index):

| query | results |
|---|---|
| "llm", "LLM", "llms", "lllm", "mll", "llm ", " llm" | 0 (stable, repeated) |
| "LLM provider" | 0 (the token poisons compound queries too) |
| "x", "gc", "watcher", "qqqzzzword provider" | 10 |

"llm" appears literally 76 times in the indexed code, so lexical absence is
not it; unmatched nonsense tokens do NOT nuke queries (qqqzzzword+provider
worked). Working hypothesis, unproven: the query embedding for this token
family comes back degenerate (zero/NaN vector) from the active embedder and
the similarity stage silently yields the empty set. Transient interference
was considered and excluded by interleaved retests (gc→10 while llm→0 in the
same seconds).

Deliverables (numbered):

1. ROOT CAUSE with a mechanism: trace one failing query end-to-end (CLI →
   daemon query path → query embedding → similarity SQL) and name exactly
   where the results become empty. Instrument locally, do not guess: dump the
   actual query vector for "llm" vs "gc" (its norm is the likely tell).
2. FIX at the right layer: a degenerate query embedding must never silently
   return an empty set — either fix the embedding path (if the model/tokenizer
   is mishandling the input) or detect the degenerate vector and return an
   honest error/envelope note naming it. A user cannot distinguish "no
   matches" from "your query vector was garbage" today.
3. ENVELOPE HONESTY regardless of root cause: when zero results occur with a
   floor of 0 against a non-empty index, the envelope should say something
   true about why (ties into the low-confidence-hint friction family:
   CONF-003, narwhal's long-NL miss, the subagent's weak-match ask).
4. Regression tests: the repro table above as tests (fixture-backed where the
   embedder allows; the local/fake embedder path may need a crafted input) —
   mutation-checked.
5. Check the flip side: does the same degenerate path affect INDEXING (are
   there stored vectors with zero norms from content that hit the same
   tokenizer edge)? Measure, report counts, do not fix in this packet if
   found — name it.

## Rules & fence

- Worktree `../fs3-search-degenerate`, branch `w-search-degenerate` off main.
- Fence: the query path (crates/store query code, crates/daemon search
  serving, crates/cli search) + the embedder query-side call
  (crates/providers/src/local.rs if the local ONNX path is implicated) +
  their tests. Nothing else without stop-and-ask.
- ABSOLUTE PATHS for every file read/edit (DL-007/008). PIJ_SESSION_ID export
  for pij sends from the worktree. CARGO_INCREMENTAL=0. No docker compose up
  (shared db on 5433). rustc-LLVM IO failure = disk, report not debug.
- READ-ONLY against the live store for diagnosis; your tests run on a scratch
  db (FS3_TEST_DATABASE_URL, sealed spawns per testkit).
- Gate green in your worktree; PR into main, DO NOT MERGE (Telegram precedes).
- DOGFOOD: flowspace3 search for code questions first; misses = friction.
- `harness observe` frictions; list, never clear.

## Report back

claim · the mechanism with the dumped vectors · files · gate output ·
mutation transcripts · PR number · observations. Ack via pij send to
pij-instant-lynx with your read + numbered plan before coding.
