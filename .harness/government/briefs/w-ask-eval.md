# w-ask-eval — evaluation suite for `flowspace3 ask`

Jordan ask 2026-08-28 (routed via flea): an eval suite we can run, review and
score for the ask verb — correctness of an LLM verb is not observable in a
diff; only known questions with scored answers measure it. Design derived
from meadowlark's frozen flow-eval engine (docs/how/flow-conformance-eval.md
in harness-engineering); we copy the ANATOMY, not the host.

## Ruled design (prime, 2026-08-28)

1. Front door: `flowspace3 eval` — a product verb. The harness may later
   DRIVE it via a thin extension, but the engine and truth live in fs3
   (dependency direction: fs3's eval must not require the harness).
2. Scenarios are PURE DATA, committed: question + assertions + a BLIND
   subject prompt (eval framing and report contract only, never how to do
   the work). Adding an evaluation costs an assertions file, not an engine
   change. Anatomy copied from flow-eval: scenario tree, run tree,
   append-only ledger.jsonl grouped by --compare, timestamped run-ids.
3. Scoring is hybrid, mostly deterministic:
   - GROUNDEDNESS deterministic: every cited address run through
     `flowspace3 get`; N cited / M resolve / K support the claim;
     hallucinated-address rate falls out free.
   - HONESTY deterministic via NEGATIVE CONTROLS (non-optional): questions
     whose answers are PROVABLY ABSENT (deleted symbol, never-added repo),
     asserting refusal. "An eval made only of answerable questions
     structurally cannot detect a model that never refuses."
   - RESPONSIVENESS is the only judged axis: committed rubric, judge emits a
     CLASS, never a number.
4. CONTAINMENT RULE (binding, verbatim from flow-eval): the LLM never
   produces a number that enters the deterministic score; all arithmetic
   upstream of every score/CI/test is deterministic code.
5. THREE-VALUED ROWS: pass / fail / UNKNOWN; UNKNOWN = evidence channel
   unavailable, EXCLUDED from every denominator. 3p/0f/7u reads "3 of 10
   measurable, all passed", never 30%.
6. JUDGE HYGIENE: pinned model+version, DIFFERENT FAMILY from subject,
   temperature 0, identity-stripped, anti-verbosity, fed ONLY VERIFIED
   ARTIFACTS — never the subject's own prose.

## Waves

- Wave 1 (spawns when flea freezes the ask envelope/report contract):
  fixture authoring — answerable set across the indexed repos, negative
  controls, blind subject prompt, rubric. No code dependency on the verb.
- Wave 2 (after w-agentic-query merges): the eval runner verb + ledger +
  --compare + CI wiring decision.

## Notes

- Scope-trap evidence to encode in fixtures: cwd inside a repo silently
  narrows search scope; a scoped zero must never be read as global absence
  (flea's finding, 2026-08-28) — at least one fixture must catch a subject
  that concludes "does not exist" from a scoped-zero.
