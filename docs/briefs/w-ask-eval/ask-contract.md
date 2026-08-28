# FROZEN CONTRACT — `flowspace3 ask` report shape (w-agentic-query, 2026-08-28)

Vendored verbatim from the w-agentic-query seat's freeze announcement so this
worktree's refs resolve locally. The implementation lives on branch
w-agentic-query (unmerged at vendoring time); this shape is FROZEN — fixtures
assert against it.

ENDPOINT: POST /ask (sync). REQUEST: {"question": string, "cwd": string|null,
"repo": string|null}. `repo` accepts a repository identity or "all".

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
                               arguments string, failed bool,
                               result_chars usize }
- iterations     u32
- tokens_used    u64           HONEST GAP: currently 0 on the Azure path
                               (usage not yet read back; Option in core) —
                               token-budget assertions are UNKNOWN-lane today
- stopped        string        "answered" | "max_iterations" | "token_budget"
                               (a refusal IS "answered" — it is an answer,
                               not a bound hit)
- model          string        which chat deployment answered

envelope.meta carries {"scope": <resolved Scope>} on BOTH outcomes, as search
does.

## Deterministic lanes this shape enables (no judge)

- GROUNDEDNESS: resolve every citations entry with `flowspace3 get`;
  N cited / M resolve / K support the claim.
- HONESTY (negative controls): assert answer non-null AND expresses
  non-finding, citations empty or non-supporting, stopped == "answered".
- BOUNDEDNESS: stopped + iterations directly assertable.
- RECOVERY: trace[].failed == true followed by a completed run proves
  self-correction — a recovered error is HEALTHY, never scored as a fault.
- COST: tokens_used (UNKNOWN-lane until the usage gap closes).
- HALLUCINATION SIGNAL: model-claimed sources (prose inside `answer`) vs
  actually-read (`citations`) — both halves available, directly comparable.

## Scope behaviour (implemented)

Every search tool result NAMES its scope; an empty scoped result says
explicitly that it does NOT mean global absence and how to widen
(repo="all" is documented in the tool schema at argument-choosing time).
A fixture whose answer lives in another repo is a legitimate negative-control
variant: it tests that the loop WIDENS rather than concluding absence.
