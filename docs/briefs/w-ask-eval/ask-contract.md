# FROZEN CONTRACT — `flowspace3 ask` report shape (w-agentic-query, 2026-08-28)

Vendored verbatim from the w-agentic-query seat's freeze announcement so this
worktree's refs resolve locally. The implementation lives on branch
w-agentic-query (unmerged at vendoring time); this shape is FROZEN — fixtures
assert against it.

ENDPOINT: POST /ask (sync); CLI `--repo` maps to the body request. REQUEST:
{"question": string, "cwd": string|null, "repo": string|null}. `repo` accepts
a repository identity or "all"; `repo="all"` genuinely widens across every
indexed repository.

RESPONSE: standard envelope v=1. `ok` remains the only discriminator.

Successful answers have `ok: true` and `envelope.data`:

- question       string        echoed, so a stored report is self-describing
- answer         string        non-empty synthesized answer text
- citations      string[]      addresses the loop ACTUALLY READ (measured by
                               the tool layer), in order, deduplicated — NOT
                               what the model claimed in prose
- trace          entry[]       every tool call, in order;
                               entry = { iteration u32, tool string,
                               arguments string, failed bool, evidence bool,
                               search_hits string[], result_chars usize }
                               `failed:true/evidence:false` = call broke;
                               `failed:false/evidence:false` = call worked but
                               the index returned no material;
                               `failed:false/evidence:true` = real index material
                               `search_hits` = addresses whose summaries this
                               search surfaced to the model
- coverage       object        measured probe bounds:
                               `{iterations_used u32, iteration_limit u32,
                               retrieval_top_k i64[], exhaustive false}`.
                               One top-k value per valid search call; bounded
                               nearest-neighbour retrieval never proves a
                               complete enumeration.
- iterations     u32
- tokens_used    u64|null      NULL = evidence unavailable: the COST row is
                               UNKNOWN and excluded from denominators. It is
                               never asserted as token-budget compliance.
- grounded       bool          true iff the answer rests on at least one trace
                               entry with `evidence:true`; false means the model
                               insisted on answering from memory after one pushback
- stopped        string        always `answered` on success
- model          string        which chat deployment answered

`stopped: answered` with null or empty `answer` is invalid. The implementation
must turn that shape into a terminal failure.

Bounded and mid-loop provider terminals have `ok: false`, absent `data`, and:

- `error.code`: `FS3-E-QUERY-ASK-ITERATION-LIMIT`,
  `FS3-E-QUERY-ASK-TOKEN-BUDGET`, or `FS3-E-PROVIDER-FAILED`
- `error.details.stopped`: `max_iterations`, `token_budget`, or
  `provider_failure`
- `error.details.grounded`: always false because no answer was synthesized
- `error.details.evidence`: `{label, citations, findings}`, where `label`
  explicitly says partial, `citations` retains addresses read in full, and
  `findings` contains one measured line per completed iteration

The terminal's partial evidence supports a narrower follow-up. It is never an
answer and never makes the failure grounded.

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

Enumerations are reported as what this bounded loop found, never as a closed
inventory. The synthesis prompt requires the answer to state that retrieval
does not prove the surfaced items are the only ones.

When a search tool's `path` glob matches zero indexed paths in its effective
scope, its result says `PATH FILTER UNMATCHED`, includes the indexed top-level
layout, and forbids an absence conclusion. This is a worked call with
`failed:false/evidence:false`, not evidence that the requested code is absent.

## Search-surfaced provenance (added 2026-08-28)

Each trace entry now carries `search_hits: string[]`. For a successful search,
it contains the returned addresses whose summary rows survived tool-result
truncation and were therefore visible to the model, in rank order. It is empty
for failed calls, no-hit searches, and non-search tools. The search limit caps
the list at 15 entries per call, so provenance stays proportional to the
bounded tool result.

`search_hits` and top-level `citations` are deliberately different grades of
evidence. A search hit was OFFERED as a one-line summary; a citation was READ in
full through `get`. An evaluator can now detect an answer that relies only on a
summary, and can measure whether the model ignored a highly-ranked offered hit
that would have answered the question. Neither signal is inferred from model
prose.

## Refusal when the port cannot answer (added 2026-08-28)

`ask` can now fail before any model call. When the agent port is wired to a
provider that cannot answer — the offline `fake` with no script, which is a
legal keyless production value — the envelope is:

    ok: false, error.code = FS3-E-PROVIDER-CANNOT-ANSWER, data absent

This closes a reported defect rather than adding a feature. That configuration
previously returned `ok: true` with `answer` set to the fake's placeholder
prose and `citations: []`. A machine consumer branching on `ok` — which this
contract and the bundled skill both instruct — banked a non-answer as a
finding. `grounded: false` and a suspicious `next_action` were both present
and both insufficient: neither is where a machine looks. **The verdict rides
the envelope, not the prose.**

For fixtures:

- No scenario may assert `ok: true` against a daemon whose agent port is the
  fake — that assertion would encode the defect.
- A run that returns this code is an ENVIRONMENT fault, not a subject fault:
  the daemon under test is misconfigured, and the correct disposition is
  UNKNOWN (excluded from every denominator), never a subject failure.
- The distinction is worth a probe of its own. A suite that never sees this
  code cannot tell "the subject answered well" from "the subject was never
  asked", and the eval should be able to prove it was actually talking to a
  real model.
