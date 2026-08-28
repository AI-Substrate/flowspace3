# pij-team — prototype experiences log

Living log while we prototype the prime→pm→coders→reviewer pipeline. Every
friction, surprise, or improvement idea lands here AS IT HAPPENS (newest at the
top of each section), so the templates/packets/extension can be improved before
we hand the pattern to pij and the harness. Also `harness observe` each one.

## Decisions taken while prototyping

- 2026-08-28 — dd schemas for the new doc types live in `.dd/schemas/pij-team/`
  (settings, packet, impl-guide); templates live in the skill folder and are
  `cp`'d into plan folders. One shared `packet` schema for all three roles;
  the role field + template selects the flavour.
- 2026-08-28 — settings live at `.harness/government/settings.dd.json`
  (o-prime single-writer). First settings: model defaults per role
  (pm/coder = omp github-copilot/claude-opus-5 high; reviewer = gpt-5.6-sol,
  id verified against `pij models`).
- 2026-08-28 — impl-guide default isolation = worktree-per-coder branching off
  the PM's plan branch, PM merges units back (retro 008: shared-tree was the
  #1 hurt cluster; era-2 cutover ruling). Fences partition write intent, not
  the build — waves must respect build deps.

- 2026-08-28 — TENETS.md added (Jordan: give me the core tenets incl. the
  importance of the arch split; source = scratch/reconstruct manifesto 04/07 +
  retro evidence). It is a LIVING doc: packets cite it by path, every run must
  improve it, and the graduation target is harness/pij first-class substrate.

- 2026-08-28 — Jordan ruling: the skill is TECH-AGNOSTIC (works in any repo);
  Rust/this-repo mechanics are worked examples only, instantiated per-plan in
  the impl-guide. And each run gets a telemetry analysis pass (xoxarle) over
  the seats' actual transcripts to drive template iteration.

- 2026-08-28 — Graduation path made concrete (Jordan): pij-massive-meadowlark,
  the harness-engineering prime, will absorb pij-team into the harness as
  FIRST CLASS after our initial trials + fixes complete. It is trialling the
  technique on its current bun work now; compare-notes session follows both
  runs. Everything in this folder is therefore written to be handed over:
  tenets, templates, schemas, and this log are the absorption spec.

## Frictions / open issues

- 2026-08-28 (duck, team-new POC): untracked skill/schema folders are INVISIBLE
  in fresh worktrees — ddocs build dies E401 there while passing on main
  (DL-032; fix = commit the substrate, done via PR). ddocs resolves schemas
  from CWD not the document's ancestors — scaffold tooling must keep cwd =
  worktree root (CONF-009). `git worktree remove` needs --force on a fresh
  plan worktree (untracked plan folder, DL-033). `harness plan new` writes
  meta.ordinal as number 5 while the folder says 005 — never assume string
  equality. `pij whoami` exists but was not discoverable (DL-034 amended).
- 2026-08-28 (meadowlark, bun run): ordinal-minting before human approval is a
  bad fit for investigate-mode work (F2) — scaffold extension gains a
  --propose/dry-run mode (compute + print, mint nothing until GO).

- 2026-08-28 (narwhal, PM run 1, at ACK): BLOCKING packet defect — prime's
  packet referenced gitignored scratch/ inputs (recipe, payload spec, oracle
  script) that exist only in the main clone; invisible to every dispatched
  worktree. Ruled: vendor inputs into the plan's assets/inputs/, sha-pinned.
  Encoded as the "refs must resolve where the seat lives" rule in SKILL.md.
  Also confirmed twice-over: the ack-as-control-point works (this + horza's
  fence extension + duck's status-vocabulary deviation all caught pre-code).

- 2026-08-28 (piranha, PM run 1 after seat death): a successor cannot tell
  "written and passing" from "written and never run" off disk alone — it had
  to re-run everything. Encoding idea (strong): phase work writes its own
  GREEN RECEIPT (gate output + timestamp + tree sha) as an artifact at each
  edge, so inheritance starts from a proven point. Candidate for the
  harness-native absorption (a `harness gate receipt` shape).
- 2026-08-28 (piranha): tk-c105's oracle turned out to be a SUBSET oracle
  (reconvo.py reads 3 of 4 stores, omp messages only) — the honest fix was
  grading proof per store (oracle-derived / mirror-derived / PM-derived) and
  a UNIVERSAL structural claim (emitted ordinals = in-order repeat-free
  subsequence of the store). Lesson for impl-guide templates: state the
  oracle's COVERAGE, not just its identity.

- 2026-08-28 (silkworm PM3, the near-miss of the run): a DONE unit — green
  gate, mutation-checked — carried a silent-data-loss defect (turn numbering
  scoped per session while the PK is per conversation; colliding turns
  idempotently DROPPED while the ledger said stored). It surfaced ONLY because
  the hold-for-composition step let the PM ask the live seat what it had
  ASSUMED about the composition it does not own. Encoded: coder done-reports
  now require an explicit ASSUMPTIONS section (packet-coder d5); PMs ask
  before wiring (packet-pm i5). Had the seat died with its siblings, this
  shipped. Also: silkworm chose the authoritative fix (MAX(turn_no) high-water)
  over the remember-a-rule fix — the right instinct, constraints nobody
  enforces mechanically are future defects.

- 2026-08-28 (silkworm PM3, the platform find of the run — DL-007/008, evidence
  at 005 assets/rescue/): worktree-resident seats' FILE TOOLS resolve relative
  paths against the SPAWN cwd (main clone) while their shells resolve against
  the worktree — three obedient seats had their edits land in the PM's shared
  tree, tools echoing success byte-identical to the correct case, producing
  FALSE GREEN builds (exit 0 on a workspace that never heard of the new dep;
  proven by a controlled mtime comparison, path form the only variable).
  Encoded: absolute-paths-always in the coder scope note; prove-in-tree
  evidence (lockfile/mtime/artifact) in the done-bar — an exit code is honest
  about what it built and silent about what it did not. REAL FIX IS PLATFORM'S
  (pij/harness): seat file tools must resolve against the dispatched worktree
  or refuse relative paths when spawn-cwd and shell-cwd disagree.

- 2026-08-28 (u2 via silkworm, three seats converging — a done-bar DESIGN
  PRINCIPLE, not a plan fact; CORRECTED same day by u2 reviewing its own
  claim): a subsequence assertion constrains order, repeat-freeness, and
  membership in the store id set — and is blind to CARDINALITY IN BOTH
  DIRECTIONS (over-emission: a split group is still a valid subsequence;
  under-emission: a subset in order IS a subsequence). Two seats' mutation
  checks passed both frozen assertions while emitting 20-for-13 and
  22-for-16. When writing a contract suite's done-bar: assert SET EQUALITY
  wherever the expected set is genuinely known; where only counts are known,
  pin the count from a source INDEPENDENT of the implementation (oracle or
  hand-count, provenance-graded) — a count harvested from the implementation's
  own output is circular wherever it lives. Ruled as a frozen-contract
  amendment on 005 with the two escaped mutations as the regression pair.

- 2026-08-28 (silkworm PM3, amendment close-out — two review-craft lessons):
  (1) MUTATION-CHECK, don't assert-check: run the faithful mutation, and when it
  PASSES, chase why before moving on. Silkworm's weaker claude mutation
  (adjacent-run fold within the assistant projection only) legitimately passes —
  the fixture's assistant-projection ids are contiguous, so it is not a
  behaviour change on those bytes. Recording that honest negative in the
  amendment stops a later reader misreading its passing as a coverage hole; a
  harsher first-draft mutation would have claimed success without ever finding
  it. (2) A reviewer can be RIGHT about the weakness and WRONG about the
  exploit: u2's "empty ordinal" defect was unreachable (the filter already
  required uuid.is_some()) — the real defect was an invariant held several
  functions away from the code depending on it, with nothing connecting them.
  The author proving which is what made the fix the right shape: move the
  invariant into the TYPE, not add a test for the phantom exploit. Also:
  cross-store claims must read the COMMITTED corpus bytes, not a grown scratch
  fixture — the SourceFixture torn-record hold-back makes a grown copy
  legitimately one record short, and an honest failure for the wrong reason is
  exactly what gets "fixed" by weakening the expectation.

- 2026-08-28 (planarian, DL-049 fix): FIXTURE FIDELITY — when a refusal chain
  has an order, an unfaithful fixture makes an EARLIER guard answer for the one
  under test. Its first sandbox had no real origin, so the squashed commit read
  as unpushed and tidy refused E_UNPUSHED_COMMITS — the OLD code path — and a
  careless reading of that run would have concluded either "fix doesn't work"
  or "harness broken". Rebuilt with an isolated bare remote and a genuine
  push → squash-merge → push cycle so the merge is shaped exactly like
  GitHub's. Pair each verdict with its raw evidence (git cherry output beside
  every envelope) so the verdict is auditable, not asserted.

- 2026-08-28 (fleet dogfood survey, silkworm's answer the headline): A MANDATE
  WITHOUT A SENSOR LAPSES SILENTLY. The seat most motivated to dogfood the
  product, on the plan that builds it, ran ZERO product searches for hours and
  never noticed — "there was never a moment where I weighed search against
  grep; the question never arose." Its own verdict: "the mandate needs a
  sensor, not more emphasis." Generalises to every packet instruction that is
  a standing behaviour rather than a deliverable: if compliance is not a
  visible fact (a count, a boot line, a receipt), non-compliance is invisible
  even to the non-complier. Suggested encoding: surface per-seat usage counts
  at boot/status so zero reads as a fact. Same survey, independent retrieval
  finding twice over: verb/function-shaped queries rank prose ABOUT an action
  above the code PERFORMING it (silkworm: enqueue question hit a docs section;
  dogfood-research run: claude-adapter.ts at rank 11/12) — candidate boost for
  code-with-verb-intent, and sloth's `flowspace3 explain` diagnostic verb
  (DL-050) is the companion product idea.

- 2026-08-28 (w-agentic-query promotion, Jordan caught it): a POC PROMOTED TO
  PRODUCT SLIPS PAST THE PACKET SUBSTRATE by default — the seat had a scratch
  brief, a VERDICT.md, and rulings scattered across messages, but no
  impl-guide and no packet doc, because promotion happened conversationally
  ("make it real") rather than through the pipeline's front door. Messages are
  not a packet: they don't survive compaction, can't be validated, and the
  fence/done-bar were nowhere durable. Rule: PROMOTION RE-ENTERS THE DOCTRINE
  — even a single-coder packet instantiates impl-guide + packet-coder from the
  templates before code, with the POC's VERDICT as the impl-guide's primary
  input. The POC is evidence FOR the packet, never a substitute for it.

- 2026-08-28 (leopon PM 006, two ack-surfaced defects worth the doctrine):
  (1) TEMPLATE d4 AMENDED (done same day): the blanket "you never wire it in
  yourself" contradicted a done bar that is a live probe-predicate flip —
  unreachable if the unit is never wired. New rule: wire yourself only when
  the wiring file is uncontended AND the done bar is behavioural; recipe
  ships regardless so composition verifies rather than re-derives. The
  template was wrong, not the coder — acks catch template defects too.
  (2) CALIBRATE, DON'T PICK, ANY THRESHOLD THAT TEACHES: u-c justified a 0.78
  weak-match floor off a hit that was an excellent semantic match whose
  wrongness was PROVENANCE, not similarity — while 0.63 answered the best
  real query correctly. A mis-set floor flags successes as weak and teaches
  agents to ignore the hint. Ruled: a should-match vs should-not table with
  best scores, floor at the separation point, named constant documenting it
  was calibrated against one embedder on one corpus. Generalises: any
  constant whose job is to CHANGE AGENT BEHAVIOUR gets calibrated against
  measured cases, never picked from one anecdote.

- 2026-08-28 (leopon CONF-002, its own defect, generalises): SAFETY RULES MUST
  NAME THE FORBIDDEN OPERATION, NEVER THE RESOURCE. "Never point at the live
  database" (a resource) was read by a conservative coder as a total ban — it
  withdrew a perfectly good READ-ONLY calibration table it had already
  produced and nearly rebuilt a synthetic corpus to re-derive an approved
  number. The rule meant "never WRITE to it" (an operation class). The
  conservative agent over-complies by discarding valid work, which is a
  quieter failure than under-compliance because nothing errors — work just
  silently gets worse. Packet safety lines should be phrased as operations
  (write/mutate/enqueue), with reads explicitly allowed when they are.

- 2026-08-28 (DL-004 spend incident, leopon/louse/knobbler): three lessons.
  (1) COST HAS A SIGNAL: louse stopped a paid-provider drain from its own log
  alone — "a fast summarize call is cached; a slow one is a provider" — a
  latency instinct that beat the PM's check by a minute. Teach it: unexpected
  slowness on a should-be-cheap path means someone is paying.
  (2) A SHARED TEST DATABASE IS A LOADED GUN when it accumulates real roots +
  a job backlog and ambient config selects real providers: any daemon boot
  starts paying. Neutralise residue deliberately; never let "test" imply
  "harmless to attach a daemon to".
  (3) WHEN TWO SEATS INDEPENDENTLY REINVENT AN ISOLATION RECIPE WRONG WITHIN
  MINUTES, THE FIX IS A VERB, NOT A BETTER RECIPE — and check whether the
  primitive already exists first: FreshDatabase already solved this for the
  test tier; the gap was reach (test-support-only) and tier (nothing for
  hand-run daemons). Promote existing primitives before building new ones.
  (4) THIRD INSTANCE (silkworm, same day, ~560 summaries + 570 embeddings):
  "THE DATABASE WAS SEALED; THE WALLET WAS NOT." Each burned seat sealed a
  DIFFERENT subset of the overrides — db but not config, port but not db —
  because a checklist lets you pass the items you know about. A verb seals
  everything or nothing; that asymmetry is the whole argument for --sandbox.
  (5) A CHECK IS ONLY A GATE WHEN THE READER KNOWS THE FAILURE VALUE:
  silkworm READ embedder=azure_openai in its boot line and took it as
  reassurance that wiring was correct. State the expected value in the rule
  ("verify it prints embedder=fake"), never just "check the line".

- 2026-08-28 (leopon retraction, knobbler refutation — the subtlest verify
  failure of the day): SOURCE-BACKED AT THE WRONG LAYER. A defect claim
  ("--path treats _ as a SQL wildcard") was verified against the STORE (doc
  comment + LIKE binding) — but a conversion boundary (glob_to_like, with
  unit tests escaping _ and %) sat between the user and that store, so the
  user-facing claim was false despite every cited line being real. Verify-
  then-relay must mean THE LAYER THE USER TOUCHES; a claim about semantics is
  only proven at the surface that exposes them. Counterpart credit: the coder
  refuted its own PM with source and tests inside twenty minutes and did not
  implement around the claim in either direction.

- 2026-08-28 (sloth, w-search-degenerate — two test-craft lessons for the
  log): (1) ITS REGRESSION TESTS PASSED AGAINST THE BROKEN CODE on first run
  — the defect lived on exactly one query plan, and Postgres never chooses
  that plan on a table small enough to seed. Only the MUTATION run exposed
  the vacuous pass; the fixture was then rebuilt to REMOVE the alternative
  plans rather than out-cost them, with the trap documented in the test file.
  Plan-dependent defects need plan-pinned fixtures. (2) IT DELETED A FLAKY
  TEST RATHER THAN SHIP A COIN FLIP: pgvector randomises HNSW construction,
  so a scan-budget squeeze passed 1-of-3 then 2-of-3 — replaced by a
  deterministic decision table over the reason enum plus one end-to-end test
  on a deterministic path. A test that fails on a coin flip teaches people to
  re-run, which is worse than no test.

- 2026-08-28 (005 cross-model review, all four findings CONFIRMED — three
  lessons): (1) A SAFETY CHECK THAT CANNOT FAIL IS NOT A CHECK: the PM-s
  accounting backstop was an arithmetic identity (accepted + (len-accepted)
  == len), true of every input — it was cited in receipts as a safety
  property. When a reviewer or coder hands you the correct check in plain
  words, implement THAT check, and mutation-test backstops like any other
  assertion. (2) A STRUCTURAL CLAIM IS VERIFIED AGAINST THE MECHANISM, NOT
  THE NAME: SERIAL_KINDS meant claimed-one-at-a-time, not run-one-at-a-time
  — the documented serialisation did not exist and first light had two live
  queue keys for one conversation. Named like a guarantee ≠ is a guarantee.
  (3) NEVER NARROW AN AC AFTER CHECKING IT to match what was built — ruled on
  F-002: the link the AC promised gets persisted (migration), because
  retro-narrowing is the move we refuse from coders and therefore from
  ourselves. Cross-model review earned its seat: 4-for-4 confirmed findings
  on a gate-green, mutation-checked composition.

- 2026-08-28 (carp, 007 — the byte-witness collision): A BYTE-FREEZE BELONGS
  ONLY WHERE A DIFF CAN MEAN NOTHING BUT A CONTRACT BREAK. Two goldens went
  red because their verbs SERVE bundled editorial docs as payload, and the
  same PR is required to edit those docs — a tripwire that fires on every
  deliberate improvement gets muted, and a muted tripwire protects nothing.
  Ruled: computed payloads stay byte-frozen; editorial payloads get SHAPE
  assertions including a POSITIVE content check (the page must contain the
  rules the plan shipped). Two doctrine points: (1) A RED GOLDEN MUST NEVER
  BECOME A CAPTURE CHORE — the coder stopped instead of re-capturing, which
  is the whole mechanism working; re-capture-as-routine kills the witness.
  (2) Check what already witnesses the thing: editorial bytes are committed
  content, so git diff + PR review already guards them — the golden was
  duplicating a protection on exactly the payload class where it misfires.

- 2026-08-28 (carp, 007 — the witness meets the real world): the byte-goldens
  caught their first FOREIGN change — daemon auth (#43) merged under the
  plan, and all fifteen cases went red in one command with UNAUTHORIZED: the
  bytes had not moved, the CLI-s PRECONDITIONS had. A frozen witness guards
  against drift the plan cannot foresee, not just drift the plan makes.
  Corollary ruled same hour: NEVER ADD A KNOWINGLY-RED ASSERTION — a content
  check whose subject lives in a sibling-s undelivered tree waits for
  composition; red-you-plan-to-ignore teaches ignoring red (same family as
  the capture-chore rule). Also: when a base-move poisons live smoke tests
  for multiple in-flight coders, merge main IMMEDIATELY, not at composition.

- 2026-08-28 (silkworm, the reviewer-brief lesson — encoded into the
  reviewer template as i2b): A CROSS-MODEL REVIEWER IS ONLY AS GOOD AS ITS
  BRIEF. Tell it (1) where the author is LEAST confident, (2) to DISBELIEVE
  the author's own receipts, (3) what is already KNOWN-OPEN. Result: its
  entire budget lands on what the author could not find — 4 findings, 4
  confirmed, zero style noise, zero findings wasted re-reporting known
  items, including a defect the author had hunted for in writing and could
  not see from inside. Twenty minutes of brief-writing is the price of a
  review worth its model. Companion schema lesson (F-002): when a link
  arrives on only ONE of many upserts, COALESCE beats EXCLUDED (the polls
  that do not carry it must not erase it), and arrival order of independent
  files must never be a correctness condition — a dangling reference that
  means "not ingested yet" is a state to report, not a constraint violation
  to refuse.

- 2026-08-28 (leopon, 006 first light): A MISLEADING ZERO IS WORSE THAN A
  REFUSAL. Fake providers (correct for spend safety) produce vectors with no
  semantics — so a wrong-version-leak predicate read 0 from BOTH checkouts
  and nearly got reported as "leak closed" when it was measuring garbage
  ranking equally against garbage. Fixed in the harness: the probe reads the
  embedder from the daemon-s own boot line and emits
  unmeasurable-fake-embedder instead of a number, plus a control (the
  probe-s own marker must be findable from its OWN worktree, or a leak of 0
  is measuring absence, not exclusion). Same principle as the eval-s
  three-valued rows: missing instrumentation must never masquerade as a
  result, in either direction. Corollary ruling: when "the test" of a
  feature IS the feature going live for everyone (auto-registration into the
  production index), it is a DECISION with the human named, never a side
  effect of proving something.

- 2026-08-28 (roadrunner, external dogfood — a probe-design lesson): TWO
  PROBES READING THE SAME GAP ARE ONE PROBE. A miss was attributed to
  ranking because "a code-vocabulary retry missed identically" — but both
  queries were blind to the same unenriched files, so the retry could not
  discriminate ranking from coverage; enrichment finished and the same query
  hit #1. To rule out coverage you need a probe with a DIFFERENT blind spot
  (e.g. a known-enriched control), not a rephrasing. The reporter retracted
  its own emphatic claim before we built on it — propagate corrections at
  the same volume as the original claim.

- 2026-08-28 (leopon, 006 composition — the emergent-defect lesson): SOME
  DEFECTS ARE CREATED BY COMPOSITION ITSELF: u-a auto-registering 20
  checkouts of one repo produced the exact data shape (caller holds NEITHER
  blob of a divergent path) that u-c-s exclusion mishandled
  (resolve-and-shrug → null-field rows with stripped addresses). Neither
  unit-s tests could see it — each unit was correct against its own
  assumptions — and fake-vector first light could not either. It took
  RUNNING THE COMPOSED ARTIFACT ON A REALISTIC CORPUS. Second instance in
  one day of run-the-artifact catching what read-the-artifact (and green
  tests) could not. Encode: composition includes an adversarial run on a
  realistic population, and the corpus a unit's success CREATES is part of
  a sibling's test surface.
  MECHANISM (corrected same hour by the coder — sharper than the PM's
  guess, kept in its precise form): the candidate gate correctly ADMITS the
  hit (an anchored element with that raw_hash exists in the caller's
  worktree), but the REPRESENTATIVE resolver then independently picks the
  globally-lowest-id element with that raw_hash without repeating the
  anchor scope — content-addressing means one function body lives in many
  blobs, so a scoped query resolves to an unscoped representative; LEFT
  JOIN provenance laterals then null every field rather than kill the row.
  The generalisation: A SCOPE FILTER OVER CONTENT-ADDRESSED STORAGE MUST BE
  APPLIED AT EVERY STEP THAT CHOOSES A ROW, NOT ONLY AT THE STEP THAT
  ADMITS ONE — the dedupe that makes storage cheap is exactly what lets a
  chooser escape scope. And guards that drop invalid rows must LOG, never
  drop silently — a silent guard is the same bug found again in six months.

- 2026-08-28 (silkworm round 2 — review-craft): A FIX ACCEPTED ON THE
  FIXER-S REPORT IS A FIX THAT GETS TO DEFINE ITS OWN SCOPE. Twice in one
  round, a fix was NARROWER than the finding it answered (a backstop
  re-suppressed by its own qualifier; a duplicate traded for a different
  duplicate the test-s last-two-items assertion could not see) — caught only
  because the reviewer re-judged the DIFF, never the author-s summary of it.
  Reviewer rounds must review fixes as diffs, with the original finding
  beside them. Companion Rust/sqlx lesson (F-006, permanent): an advisory
  lock taken on a POOLED connection leaks its session lock on unwind (drop
  returns the connection, session intact) — detach the connection so drop
  closes the session, and prefer TRY-lock when a held lock means the work is
  already being done. An alarm with a qualifier is an alarm with an off
  switch (F-005: delete the qualifier, don't refine it).

- 2026-08-28 (flea, 008 review rounds — the fix-review lesson, twin to
  silkworm's): A FIX IS NOT A FINDING CLOSED — IT IS FRESH UNREVIEWED CODE
  IN THE MOST SENSITIVE PART OF THE SYSTEM, WRITTEN UNDER THE MOMENTUM OF
  HAVING JUST BEEN WRONG THERE. Flea explicitly asked its reviewer to attack
  its own grounding fix on those grounds and was right twice over: the fix's
  grounded flag was a FALSE POSITIVE (defined as "some tool call did not
  fail" — but a search that works and matches nothing is a successful call
  carrying no evidence; a wrong mechanism is worse than the prompt it
  replaced, because now it is TRUSTED), and its pushback could spend past
  the budget. Dispatch rule that falls out: reviewer rounds explicitly
  target the diffs written in response to earlier findings. Semantics rule:
  "worked and found nothing" and "broke" are different facts only the
  layer that ran the call can distinguish — never collapse them into one
  boolean at a higher layer.

- 2026-08-28 (silkworm round 4, F-009/F-010 — two closing lessons):
  (1) DON'T FIGHT A MECHANISM WITH ITSELF: a contention re-queue enqueued
  from INSIDE the running job collided with its own row via the live-jobs
  dedupe index, moved its own deadline, and got settled done — "there is no
  starvation loop because the intended loop never exists." When a re-run is
  needed, hand it to the machinery that already knows how to re-run (the
  runner's retryable-failure backoff), never a sibling mechanism.
  (2) FIXES FROM SEPARATE ROUNDS CAN COMPOSE INTO A NEW DEFECT: one fix
  (None for undatable rows) created the panic precondition for another
  (index-before-push registration) — invisible to both rounds because each
  reviewed one finding. Per-finding verification is necessary but not
  sufficient; a final WHOLE-DIFF review round catches fix×fix interaction.

- 2026-08-28 (carp/007 review round): A RULING RECORDED WHERE IT WAS MADE IS
  NOT RECORDED WHERE IT IS READ. PROVENANCE.md held the full reasoning for
  deleting two goldens under a PM ruling, yet the first-light transcript
  still carried the flat sentence "no golden file was modified" — false at
  the surface a reader actually checks. Reviewer's top finding was against
  the PM, and the PM accepted it and amended BOTH the criterion (scope
  stated on ac-0002 itself) and the transcript (re-captured vs
  removed-with-ruling distinguished). Same family as DL-007: an answer that
  exists somewhere is not an answer that reached where it is needed —
  claims must be true at every surface that asserts them, not just where
  the justification lives.

- 2026-08-28 (dajeil/dd o-prime, cross-repo): SHELL-COMPOSED MESSAGE BODIES
  EXECUTE THEIR BACKTICKS. A `pij send "..."` body built in double quotes ran
  `ddocs build` via command substitution and spliced the error envelope into
  the delivered message where a citation should have been — and relayed text
  (peer messages, file excerpts, user pastes) runs on the RELAYER's machine
  the same way. Canonical safe form
  (ALREADY SHIPPED, pij Plan 093): `pij send <peer> --body-file <path>` (or
  `--body-file -` with a quoted heredoc) — never the double-quoted form, and
  not the "$(cat file)" trick either. The sharper half (credit
  pij-continuing-ermine): a MATCH gets recorded while a DISAGREEMENT gets
  investigated — dajeil's own: "the uneventful result is the one nobody
  interrogates". And the whole warning was already in `pij send --help`,
  verbatim, at the exact place a caller would look — so the cheap habit that
  beats first-principles diagnosis: RUN --help ON THE VERB YOU ARE ABOUT TO
  USE. A recurrence of a documented hazard is a distribution problem, not a
  missing feature.

- 2026-08-28 (flea, DL-014 recurrence — same hazard twice in one session):
  ENCODE THE CLASS, NOT THE INSTANCE. Flea logged "use --body-file for gh"
  after a backtick mangled a PR body, then an hour later lost a phrase from
  a COMMIT MESSAGE to the identical mechanism — because the encoding named
  one tool instead of the family. Fleet rule now: EVERY tool that takes
  prose (commit messages, PR bodies/comments, pij bodies) takes it from a
  FILE written via quoted heredoc, never a double-quoted inline argument.
  Both failures were silent — the command succeeded and the text was wrong —
  so verification of delivered prose beats trust in exit codes.

- 2026-08-28 (silkworm, #42 conflict resolution): TWO PLANS GRANTED "THE
  THIRD PORT" ON THE SAME DAY, not knowing about each other (#45 ChatProvider,
  005 ConversationSource) — both doc comments claimed third, both guards said
  a fourth is stop-and-ask, and BOTH grants were legitimate for the same
  reason (real implementation, a choice not otherwise expressible). The
  resolver did not pick a side: four ports now stand, guard reads
  a-FIFTH-is-stop-and-ask, both grants named and dated, plus a sentence
  noting the coincidence is worth noticing. Two lessons: the port-count
  guard WORKED twice independently (the discipline is the count's reason,
  not the number); and a merge resolver choosing LEGIBLE over TIDY turns a
  conflict into a governance record. Also ruled there: merge-over-rebase
  when the branch's merge structure IS the composition record — a rebase
  destroys who-contributed-what to buy nothing a merge does not give.

- 2026-08-28 (carp/007 close-out harvest — the measured run): THE NUMBER:
  three coders simultaneously in ONE crate produced TWO conflicts across the
  entire convergence, both additive (Cargo.lock + allowlist); main.rs
  auto-merged every time because every packet carried the identical
  line-level collision map. Tenets 1+3 paying out in their promised
  currency. Template changes landed from this run: coder packet now carries
  a TRIPWIRE field (the check proving the plan invariant + what red MEANS —
  every 007 coder stopped correctly on a red golden) and a READS
  DECLARATION (what a unit consumes that it does not own — the
  zero-shared-files claim was false because only ownership was mapped;
  consumption is where fences collide). Also confirmed the hard way:
  DL-011's reviewer-own-checkout rule exists for the PM too — carp
  invalidated two reviewer gate runs by merging main under a shared tree;
  the PM freezes the tree during a review round or the reviewer gets its
  own checkout. And the review-value line worth quoting whole: "a reviewer
  that reads the diff against the PROMISES finds what a gate cannot:
  things that are absent, and things that are claimed."

- 2026-08-28 (carp/007 teardown): A SQUASH MERGE ORPHANS EVERYTHING
  COMMITTED AFTER IT. The PR merged, then the PM kept committing close-out
  artifacts to the branch (rescued coder buffers, the reviewer's APPROVE
  receipt, the final task receipt) — all of it existed ONLY on the branch,
  and the ruled tidy-behind-merge would have deleted the very record the
  close-out exists to preserve. Caught because the PM checked what was on
  main BEFORE removing, not after; recovered with a docs-only follow-up PR.
  Rule: after a squash merge, anything you commit goes through a new PR
  before any teardown — and the encoding candidate is a tidy warning when
  the branch carries commits dated after the merged sha.

- 2026-08-28 (nigel/008 wave 0 → silkworm/005): THE KNOWN FLAKE WAS A
  DEFECT WEARING A FLAKE LABEL. the_same_transcript_mints_the_same_guid sat
  on an ignore-if-red-alone advisory (DL-013) all day; a PM running the full
  suite in an unrelated plan root-caused it instead of waving it through:
  the guid is seeded from a now()-filled second-resolution timestamp, so
  re-import idempotence — the property the plan's docstring itself calls
  load-bearing — held only within one second. "An alarm with a qualifier is
  an alarm with an off switch," proven in the field: the qualifier was the
  off switch, and the alarm was ringing truthfully the whole time. Rules:
  a flake advisory must carry an expiry or a root-cause owner, never stand
  as a permanent waiver; and the fix converts the flaky test into the
  regression proof (force the boundary the flake straddled).

- 2026-08-28 (flea, #55): AMEND BEFORE HARNESS COMMIT, NEVER AFTER — a
  message amend after `harness commit` orphans the refs/notes/ai note on
  the pre-amend sha, silently losing attribution for the commit that
  ships. (Ownership routes to o-prime not git per DL-011, so nothing is
  truly lost, but the note is.) Flea flagged it in its own PR body rather
  than leaving it to be found. Same family as verify-delivered-prose:
  the mutation after the verified step is where silent loss lives.

- 2026-08-28 (silkworm's triptych, self-reported): AN ALARM WITH A
  QUALIFIER IS AN ALARM WITH AN OFF SWITCH — violated three ways in one
  session by the people who wrote it: (1) a check shipped that could not
  fail; (2) a qualifier added that silenced a check on the exact case it
  existed for; (3) a red test collectively waved through as a known flake
  for hours while it rang truthfully (the guid defect: SILENT and
  EXPENSIVE — every re-import stored the whole conversation again under a
  new address and paid the provider twice, on the plan whose subject was
  incremental ingest). The doctrine is only as good as the day you
  re-apply it to yourself; the fix's rule is the keeper: A VALUE THE
  PROGRAM INVENTED NEVER ENTERS AN IDENTITY SEED.

- 2026-08-28 (pij#19 arc, three governments, ~2h from recurrence to causal
  closure): a phantom pij seat can mint MID-SESSION whenever a seat spawns a
  LOGICAL internal subagent (no OS fork — a read-only critic task suffices),
  and the alias is MESSAGE-CAPABLE (it speaks the subagent's output as if a
  seat). Fleet doctrine: canary identity is POINT-IN-TIME proof — address
  only the canaried id forever; never close an alias (it shares the live
  seat's PID/pane); traffic from an uncanaried id sharing a known spawnId is
  the subagent speaking, discard but understand. The method is the bigger
  lesson: recurrence report → containment held under uncertainty → control
  pair (only differing variable isolated) → finder self-corrected its own
  attribution TWICE on the record (wrong processes, wrong delta) → corrected
  aim handed to a third government → two-arm controlled falsifier closed
  causality. Correlation directed the search; only the controlled test
  closed the ticket; and the chain was trustworthy because every error in
  it was self-reported, not discovered.

- 2026-08-28 (nigel/008 wave 1): THE ACK REALLY IS THE CONTROL POINT —
  three defects surfaced at ack, none in a diff: a coder caught the
  impl-guide contradicting its own packet (docker compose vs the no-compose
  rule); a coder kept `turn` in a widened CHECK the PM's frozen DDL sketch
  had omitted (copying the sketch verbatim would have broken conversation
  indexing); and a coder's SELF-SPAWNED read-only critic found a real bug
  neither PM nor coder had seen (schema-declared string sections falling
  through to the unschema'd embed basis). Corollary proven the same wave:
  cd-first spawning made isolation structural — both wave-2 seats
  ready-pinged from their OWN worktrees with their own branches in the
  footer; "nobody has to be careful" is the goal state of every encoding.

- 2026-08-28 (leopon, gate-contention compliance): ONE LAW PAYS IN
  CURRENCIES YOU DID NOT APPLY IT FOR. Leopon isolated its test databases
  for SPEND reasons (the shared test DB held 15 roots and a 6,520-job
  backlog burning real provider calls); when the shared-DB CONTENTION
  defect surfaced fleet-wide hours later, every one of its gate verdicts
  was already clean — the seal-ambient-inputs law had bought
  determinism it was never asked for. Its phrasing is the keeper:
  per-seat isolation is not a nicety, it is what makes a green verdict
  MEAN anything.

- 2026-08-28 (nigel/008 wave 2, six contract corrections in one day —
  four overturning PM rulings): A PM RULING FROM MEMORY OR A SINGLE PROBE
  IS EXACTLY AS UNRELIABLE AS A STALE BRIEF — and it stays cheap only
  because contradicting it costs a coder nothing. The sharpest three: a
  flat probe corpus could not distinguish "these are the same thing" from
  "these coincide here" (nested-document counterexample overturned an
  address ruling); a singular Arc<RwLock<...>> was not stale but
  permanently WRONG for every root but one (the daemon serves many
  worktrees); and "probe once at boot" was unimplementable, not
  suboptimal (sync from_config vs async probe) — the PM had ruled a shape
  it never tried to write. Every reversal recorded on the fleet channel
  rather than quietly amended. Corollary to "the ack is the control
  point": the control point only works while dissent is free.

- 2026-08-28 (leopon, guard misdirection): TAG YOUR INFERENCES — THE TAG
  PAYS EXACTLY WHEN THE INFERENCE IS WRONG. Leopon marked "the harness
  keeps a second parser" as [INFERENCE]; that one tag converted a
  plausible fleet-wide misdirection into "here is exactly what to verify",
  and the verification overturned it (the guard reuses fs3's own resolve —
  the strictness was WHICH TREE builds the guard binary). The class it
  named alongside: TRUE STATEMENTS THAT POINT AWAY FROM THE CAUSE — the
  guard blaming the config file, the schema alarm saying what moved but
  not who — both accurate, both misdirecting; error messages must name the
  LAYER, not just the fact. And the tell for reviewers of any inference
  about a binary's behaviour: ask WHERE THE BINARY COMES FROM before
  believing anything about what it does.

- 2026-08-28 (leopon, run-three hardening — two lessons, both its own rule
  applied to its own code): (1) DENY-LISTS FAIL OPEN ON THE CASE NOBODY
  THOUGHT OF. The embedder gate refused fake and unknown — then production
  reported `offline`, a third value meaning exactly "no real vectors",
  and it sailed through. When the thing gated is "do I have grounds to
  answer at all", allow-list the states that PROVE grounds and refuse
  everything else BY NAME. The coda kept honestly: the author wrote tenet
  14 in the morning and implemented its structurally-incapable shape the
  same day — a right rule does not protect you from a wrong shape.
  (2) TRUE WHEN READ, STALE WHEN ACTED ON — a new category, distinct from
  a wrong reading: every check that samples a mutable system has it, and
  the fix is not more careful parsing but ASKING THE THING THAT IS
  RUNNING rather than the record of what ran (gate now pings the live
  daemon; the receipt records WHICH source answered).

- 2026-08-28 (leopon + lynx, same error at different altitudes): READING
  THE INTENT OF A MECHANISM INSTEAD OF WHAT IT ACTUALLY DOES. Three
  instances in one evening: tenet 14 implemented as a deny-list (cannot
  keep the promise); `trap ERR` without `set -E` (not inherited by
  functions/subshells — the abort reporter was structurally incapable of
  firing in the case it existed for, and two runs died silently behind
  it); and the o-prime reading "claimed one at a time" as the worker
  count when it described the claim call (dispatching a build for
  parallelism that already existed). All three failures INVISIBLE: a
  deny-list letting a case through looks like a pass, a trap that never
  fires looks like a clean death, a wrong attribution looks like a
  diagnosis. The countermeasure is the same audit in both directions —
  walk what the system actually does, not what its words say: for every
  mechanism ask WHAT INVOKES IT (tenet 17), and for every plan to build
  ask DOES IT ALREADY EXIST (louse's check, the sixth sighting's
  prevention).

- 2026-08-28 (nigel/008 review round 1 — the frozen contract as the failure
  point): EVERYONE DID THEIR JOB AGAINST A WRONG SPECIFICATION. The gate
  filter answered from the stored claim — the exact field the plan exists
  to distrust — because the PM's frozen contract defined it that way; the
  coder implemented faithfully, mutation-checked twelve ways, the PM
  reviewed and approved, and it reached a green composed branch. No
  diligence INSIDE a plan can catch the plan itself being wrong — that is
  what cross-model review is FOR. Paired ruling worth keeping: additive
  reads over persisted data may land under review pressure; identity-model
  changes may not — same reviewer, same session, opposite rulings, the
  difference is structural risk not effort.

- 2026-08-28 (leopon, section F of 006's process feedback): THE CLAUSE
  DEFINES THE BOUNDARY, AND TASTE DECIDES WHERE INSIDE IT YOU STAND. A
  probe wait is not a predicate, so extending it to get a pass was LEGAL
  under the binding clause — and still wrong, because "I extended the
  timeout and then it passed" is a sentence no closing receipt may
  provoke. Recording that (a) was legal and (b) was right, as doctrine:
  rules bound the space; they do not pick the point in it.

## Template improvement ideas

- 2026-08-28 (carp's 007 fan-out — two dispatch patterns worth template rank):
  (1) SAME LINE-LEVEL COLLISION MAP IN EVERY PACKET: when sibling units share
  a crate, name the shared files down to the match-arm/line and put the
  IDENTICAL map in each coder's packet — where the coder reads it — rather
  than only in the impl-guide. Any need to cross the map is a stop-and-ask.
  (2) THE DECLINED LIST: an allowlist grant should also name the crates that
  were CONSIDERED AND REJECTED (with the source that rejected them), so no
  coder re-litigates a settled choice mid-unit. Both from packet set e06894b.
- (add as they arise)
