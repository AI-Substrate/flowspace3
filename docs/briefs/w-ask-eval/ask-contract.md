# FROZEN CONTRACT — `flowspace3 ask` report shape (w-agentic-query, 2026-08-28)

Vendored verbatim from the w-agentic-query seat's freeze announcement so this
worktree's refs resolve locally. The implementation lives on branch
w-agentic-query (unmerged at vendoring time); this shape is FROZEN — fixtures
assert against it.

ENDPOINT: POST /ask (sync); CLI `--repo` maps to the body request. REQUEST:
{"question": string, "cwd": string|null, "repo": string|null}. `repo` accepts
a repository identity or "all"; `repo="all"` genuinely widens across every
indexed repository.

RESPONSE: standard envelope v=1, NEW payload (no existing bytes move).
envelope.data:

- question       string        echoed, so a stored report is self-describing
- answer         string|null   NULL = loop hit a bound before answering (a
                               caller must say so, never present another field
                               as the answer)
- citations      string[]      addresses the loop ACTUALLY READ (measured by
                               the tool layer), in order, deduplicated — NOT
                               what the model claimed in prose
- trace          entry[]       every tool call, in order;
                               entry = { iteration u32, tool string,
                               arguments string, failed bool, evidence bool,
                               result_chars usize }
                               `failed:true/evidence:false` = call broke;
                               `failed:false/evidence:false` = call worked but
                               the index returned no material;
                               `failed:false/evidence:true` = real index material
- iterations     u32
- tokens_used    u64|null      NULL = evidence unavailable: the COST row is
                               UNKNOWN and excluded from denominators. It is
                               never asserted as token-budget compliance.
- grounded       bool          true iff at least one trace entry has
                               `evidence:true`; false = the model insisted on
                               answering from memory after one in-conversation
                               refusal
- stopped        string        "answered" | "max_iterations" | "token_budget"
                               (a refusal IS "answered" — it is an answer,
                               not a bound hit)
- model          string        which chat deployment answered

envelope.meta carries {"scope": <resolved Scope>} on BOTH outcomes, as search
does.

## Deterministic lanes this shape enables (no judge)

- GROUNDEDNESS: inspect `grounded` first as the cheap filter, then resolve every
  citations entry with `flowspace3 get`; N cited / M resolve / K support the claim.
- HONESTY (negative controls): the trace makes searched refusal directly
  distinguishable from guessing. `grounded:false` plus at least one worked
  `failed:false/evidence:false` entry, a non-finding answer, and no supporting
  citation is the correct searched refusal. `grounded:false` with an EMPTY trace
  is a subject failure: refusal without looking. If a worked call returns
  `evidence:true` and the answer is affirmative, the fixture's absence premise
  is stale: mark a fixture fault, exclude it from the subject denominator, and
  refresh the control rather than blaming the subject.
- BOUNDEDNESS: stopped + iterations directly assertable.
- RECOVERY: `trace[].failed == true` identifies a broken call independently of
  evidence; a later completed run proves self-correction. A recovered error is
  HEALTHY, never scored as a fault.
- COST: `tokens_used == null` is UNKNOWN and excluded from denominators; the
  field is observational only and never proves budget compliance.
- HALLUCINATION SIGNAL: model-claimed sources (prose inside `answer`) vs
  actually-read (`citations`) — both halves available, directly comparable.

## Scope behaviour (implemented)

Every search tool result NAMES its scope; an empty scoped result says
explicitly that it does NOT mean global absence and how to widen.
`repo="all"` is documented in the tool schema at argument-choosing time and
genuinely widens across indexed repositories. A fixture whose answer lives in
another repo is a legitimate scope-trap negative-control variant: it must test
that the loop WIDENS rather than concluding absence. Because an ungrounded
answer receives one in-conversation refusal, that path may consume one extra
iteration; fixtures assert an iteration ceiling, never an exact count.
