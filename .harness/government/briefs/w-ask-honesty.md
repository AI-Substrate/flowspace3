# w-ask-honesty — ask must not claim more than it measured

**Ruled by Jordan 2026-08-29 ("Yes, do it"). Closes backlog rows 62 + 63.**
One small packet, one coder seat. Domain owner on revival: flea (ask contract);
rows found by roadrunner's graded ask run.

## The two defects (both are honesty-of-the-envelope defects)

### 1. Enumeration overclaim (row 62)

An ask answer said "two main paths" for a mechanism that had three (OSC 52 via
ClipboardAddon was the third). The consumer had no ground truth; `grounded:true`
plus clean citations made the false completeness MORE convincing, not less. A
bounded search loop (≤7 iterations over top-k retrieval) cannot know it
enumerated a space, so it must never phrase as if it did.

**Fix at the contract, not the model's manners:**
- The synthesis prompt (agentic ask loop, `crates/daemon` ask module) instructs
  the model: enumerations are reported as "what the search found", never "all
  there is" — e.g. "I found two paths (…); the search does not prove these are
  the only ones."
- The envelope gains an explicit `coverage` note (or equivalent field/wording in
  `data`) stating the loop's bound: iterations used, retrieval top-k — so a
  consumer can see the answer came from a bounded probe, not an exhaustive scan.
- Eval fixture: a question whose true answer-space is larger than what top-k
  surfaces; grade FAILS an answer that asserts a closed enumeration without
  hedging. (Pairs with tapir's fixture doctrine: exact question, not semantic
  paraphrase — a semantic fixture over an exact question HIDES the capability's
  absence.)

### 2. Unsatisfiable path filter reads as absence (row 63)

The loop spent an iteration on `--path "src/**"` in a repo whose indexed paths
are repo-root-relative (`apps/web/src/...`) — a glob that CANNOT match any path
— and the result read as "nothing there". "Your filter matches no paths" and
"paths matched, none relevant" are different facts and the envelope must say
which one happened.

**Fix:**
- Search/ask envelope: when a path filter matches ZERO indexed paths for the
  scoped repo(s), say so explicitly — `empty_because: path_unmatched` (the
  scoped-zero vocabulary gains a member) — and include a hint naming the layout
  (e.g. the top-level directories of the repo's indexed paths) so the caller
  can correct the glob in one step.
- The ask loop's tool result must carry the same distinction so the LLM does
  not conclude absence from a bad filter; ideally the loop can consult repo
  layout (a cheap tree/prefix listing) before or after a path-filtered miss.
- Eval fixture: a path-filtered question with a deliberately wrong glob; grade
  FAILS an answer that concludes the code does not exist, PASSES one that
  reports the filter was unsatisfiable.

## Scope fence

- IN: synthesis prompt wording, envelope fields (`empty_because` member,
  coverage note), the loop's handling of a path-unmatched tool result, the two
  eval fixtures, tests.
- OUT: the lexical channel (row 64 — separate brief `w-lexical-channel`),
  ranking changes, new retrieval channels, provider config.

## Done-when

- d1: an ask whose answer enumerates alternatives phrases them as findings, not
  a closed set, and the envelope names the loop's bound (fixture proves it).
- d2: a search/ask with a glob matching zero indexed paths returns
  `empty_because: path_unmatched` + a layout hint, never a bare empty result
  (test proves it at the store/daemon layer AND through the ask loop).
- d3: `harness checks` green; both fixtures in the eval suite.
