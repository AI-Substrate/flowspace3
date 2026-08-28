# Dispatch backlog — evidence-backed, triaged (o-prime, 2026-08-28)

Every row born from live fleet dogfooding today. Order = dispatch order.
Rows leave this file by becoming a w-* brief + seat, or a PRD row Jordan cuts.

## Tier 1 — small, sharp, dispatch as seats free

1. **w-epipe**: CLI panics on broken pipe when a downstream reader exits
   early (`status | jq -e`). Fix: EPIPE = quiet exit, every verb. Evidence:
   tapir, w-ask-eval session. Half-day packet, no collisions.
2. **w-daemon-sandbox** (BRIEF EXISTS): promote FreshDatabase to testkit +
   `daemon --sandbox`. Justified by three paid incidents in one day.
3. **w-ingest-lane** (BRIEF EXISTS): ingest must not starve behind its own
   enrichment backlog. Dispatch after 005 merges (touches the same runner).
4. **doc lines**: `--path` character classes land as literals; path filter is
   prefix-match. Two lines in search docs. Fold into any search-adjacent
   packet rather than its own seat (candidate: 006 u-c close-out).

## Tier 2 — real features, need either Jordan's taste or a design pass

5. **newest-first bootstrap registration** — JORDAN TASTE RULING PENDING.
   Design number: 14.8k one-time jobs, new worktree waited 89s (leopon).
6. **flowspace3 explain "<query>"** — retrieval diagnostic verb; every number
   already exists inside search_elements (sloth DL-050, was blocking).
7. **doc-comment spans**: /// text above an item is unindexed, so documented
   intent is unfindable (leopon CONF-001; third docs-vs-code finding).
8. **per-root queue visibility**: global queue with no per-root breakdown
   makes any wait-for-indexing hostage to other seats (DL-001; cost leopon
   15 min; also the boot-wall pair). Pairs with 006's churn note.
9. **status --has-repo**: membership probe so scripts stop piping status
   through jq (tapir).
10. **ask async-job promotion + progress stream**: named follow-up in the
    008 impl-guide, deferred until 007's event wire lands.
11. **ef_search recall benchmark**: sloth's named follow-up — ef 800 gives
    9/12 exact top-10 at +8ms; needs a real recall benchmark, not 12 queries.
12. **re-path search hits to caller cwd** for same repo identity (carp) —
    u-c follow-up nuance, design with 006's provenance field in hand.

## Tier 3 — upstream (not ours to build; evidence packaged and relayed)

13. pij: spawn banner prints the minted id (canary identity class, 3 seats).
14. pij: dispatch verb refuses when packet path absent in target tree
    (leopon DL-003).
15. OMP: Rust diagnostics false-negative (E0425 → OK/empty) — filed via
    ermine/puffin; our standing caveat holds until fixed.
16. pij/harness: seat file-tools resolve relative paths against spawn cwd
    (DL-007/008) — evidence at 005 assets/rescue.
17. harness: boot names which probe it is on + per-probe timeout (4 seats hit
    the 120s silent wall today); harness checks truncates failing assertion
    (zakalwe DL-006).

## Standing evidence conventions

Each dispatched brief cites its DL/CONF ids and the seat that hit it. The
retro drain (o-prime-owned) is the other consumer of these ids — a row
leaving this file should name where it went.

## Added 2026-08-28 (late session)

18. **migration guard defects** (echidna, w-epipe gate): (a) an after-snapshot
    probe failure (config unparsable) is misclassified as production mutation
    ("13 -> blank") with a binding do-not-rerun — probe failure must report
    as probe failure; (b) the guard reads AMBIENT config rather than a
    dedicated production-URL input, so any machine-config transient poisons
    every seat's gate.
19. **config forward-compatibility decision** (flagged into #45): strict
    config parsing makes every new section a flag-day for older binaries on
    the same machine (the [agent] addition broke a sibling's gate live).
    Named trade: typo detection vs rollout. Needs a ruled default.
20. **gate lazy test-db creation races under parallel tests** on any fresh
    database name (echidna; rat's FreshDatabase promotion fixes the testkit
    path by construction — the GATE path needs the same treatment).
21. **config-failure path violates the envelope contract** (roadrunner,
    chainglass): unparsable config → exit 0, bare stderr prose, NO envelope.
    Agents branching on `ok` crash on parse. Must emit an error envelope
    (code + fix) and a nonzero exit. Sharpest agent-contract bug on record.
22. **empty_because needs a scan-pending leg** (roadrunner): agents cannot
    distinguish not-yet-indexed from not-a-match; a coverage hint (pending
    jobs for the scoped root) tells them whether to retry later.
23. **whole-file element spans** ([1,1746]) degrade search→get to
    read-the-whole-file (roadrunner) — ranged get / sub-spans.
24. **prose-above-code ranking — RETRACTED as 5th case by its own reporter**
    (roadrunner re-test): the emphatic miss was COVERAGE (unenriched files),
    proven by the same query hitting #1 after enrichment. Surviving evidence
    is one WEAK case (a doc outranks the named file by 0.006). Lesson kept:
    a retry cannot discriminate ranking from coverage when both probes read
    the same gap. Rows 6/7 evidence stands on its own.
26. **common-word bare identifiers degrade to a confident noise band**
    (roadrunner): 'copyBuffer' (3x in one file) and 'ClipboardAddon' (unique
    to one file) return unrelated files at 0.36-0.38 with the true file
    ABSENT, while distinctive identifiers resolve #1. The band sits below
    the 0.50 calibrated weak-match floor now building in 006 u-c — this is
    live external validation that the hint will fire exactly where needed.
    Candidate deeper fix: identifier-aware matching or a text-match leg.
25. **search lacks --config override** — machine-config fault leaves agents
    with no workaround (roadrunner).
27. **harness team new --packet <slug>** (o-prime, 4 hand-rolled today): worktree + branch + vendored brief for w-* packets — no ordinal mint, no plan scaffold; symmetry with tidy. Candidate absorption into the team extension.
28. **harness checks should report base drift** (silkworm, PR #42 round 3):
    CI builds the MERGE commit; a branch green on all nine local gates broke
    in CI because main moved 8 commits (emit() signature) — the break lived
    in a tree neither gate ever built. "A stale base makes every gate a
    statement about the wrong tree" = the lock-gate argument one level up.
    Encoding: one local line naming commits-behind-base before the red PR.
29. **seal ambient inputs for deterministic validation** (DL-019 + DL-004
    unified): ambient config and ambient providers are mutable
    higher-precedence inputs to gates and daemons; deterministic runs seal
    them (isolated config dir), with user-config compatibility as its own
    separate check. Two incidents, one encoding.
30. **daemon.key rotation vs config-dir divergence** (tapir): a daemon booted under FS3_CONFIG_DIR writes its fresh key there; ambient callers 401 machine-wide until the key is synced. Encoding: daemon prints key path at boot; --sandbox/read-live posture story should own key placement; possibly key also mirrored to ambient when serving the default port.
31. **"how is this tested" query class misses** (two seats, same shape, 006 run): questions about test patterns for a component rank badly — pairs with docs-vs-code ranking rows; strong eval-fixture candidate for the ask suite.
32. **unregistered composed worktree: search silently answers from sibling
    checkouts** (wolf, 007 review, DL-008): semantic search from an
    unregistered flowspace3 checkout warns once then returns results from
    OTHER registered checkouts — a reviewer looking at the code under
    review gets someone else's tree. Should either resolve in place
    (read-only, no index mutation) or refuse honestly with the register
    command named. Pairs with the 006 scope-at-every-choosing-step law and
    row 30's posture story; daemon worktree-lifecycle tracking (feature
    ruled 2026-08-28) is the likely home.
33. **team tidy slug↔worktree naming mismatch** (flea): tidy looks for
    fs3-<slug> by path convention; a worktree named fs3-agentic-query for
    slug w-agentic-query gets E_NOTHING_TO_TIDY though `git worktree list`
    knows the branch. Tidy should resolve by BRANCH, path second. Pairs
    with row 27 (team new --packet would make the naming deterministic).
    CONFIRMED by controlled reproduction (flea, same day): identical tidy
    command, only the worktree name differed — fs3-w-skill-ask tidied
    first try (incl. sha-verified buffer rescue), fs3-agentic-query was
    invisible. Directory-name resolution, not branch.
34. **bundled SKILL.md does not teach ask** (flea, REAL FINDING): the skill
    baked into the binary — the front door shipped to every agent —
    documents search/get/tree and never mentions the ask verb, so the
    question-shape hint points at a verb the skill never introduces. Fix is
    small and high-leverage: teach ask (when to reach for it vs search) in
    the bundled skill. Owner: flea (ask domain) or fold into w-ask-eval
    wave 2 dispatch.
35. **expose --source in ask tool schema WHEN 005 data lands** (flea
    self-correction): conversations/turns are zero rows today, so ask's
    conversation blindness is latent, not live; the day #42/005 lands data,
    source belongs in the ask tool schema. Trigger is the 005 merge, not
    now.
36. **gate cannot notice absence: warn when a merge removes merge-base test
    files** (carp, 007 review HIGH): a merge that deletes a test deletes
    the evidence that it deleted something — 007's merge silently reverted
    #47's EPIPE work (epipe.rs deleted, gate stayed green); second suspect
    same shape (testkit fresh_database.rs from #48 absent, fs3-store
    allowlist row lost). Cheap encoding: harness checks diff-stat rule —
    flag test files present on merge-base and absent on HEAD. Interim
    habit: `git diff origin/main...HEAD --stat -- crates/*/tests/` before
    any PR. Third instance today of the silent-revert family (row 28 is
    the same law one level up).
37. **daemon-restart needs a selector** (o-prime, first real use): refuses
    on multiple candidates, but multiple daemons is the RULED steady state
    (prod 7373 + alt-port test daemons under the prod-daemon rule). Add
    --port (default 7373) / --pane; today it can never restart prod while
    any seat is testing.
38. **sandbox DB drop misses SIGTERM path** (knobbler behavioural
    verification, relayed by leopon): --sandbox drops its minted DB on
    Ctrl-C but SIGTERM (how every supervisor/agent stops daemons) exits 1
    and the database SURVIVES, silently — same class as the misleading
    zero: the observable that would contradict the posture claim is never
    printed. Fix: signal handler covers SIGTERM+SIGINT; process states on
    exit whether it dropped or left the DB; minimum bar: print the DB name
    at exit (a named leak is recoverable, a silent one becomes the next
    6,520-job landmine). Owner: #48 follow-up (w-daemon-sandbox family).
39. **secret detection on conversation ingest** (Jordan, 2026-08-28, ruled
    alongside transcript-consent): fs3 should detect keys/credentials in
    transcript content before it leaves the machine for embedding —
    defense-in-depth behind harness-side scrubbing, since the daemon is the
    last hop before egress. Candidate: ripsecrets (Rust crate + binary,
    low-false-positive secret patterns); alternatively vendor the gitleaks
    regex set. Design note: detection should QUARANTINE/redact the turn and
    say so in the envelope, not silently drop it (position is identity —
    row 33 family; and silent modification is the misleading-zero family).
    Consent ruling recorded: per-repo OPT-IN, off by default,
    HARNESS_NO_TELEMETRY honoured (Jordan ratified meadowlark's posture).
40. **flowspace3 MCP surface unreachable from subagent contexts** (nigel's
    4 scouts, unanimous): AGENTS.md mandates search-first dogfooding but
    subagents cannot reach the MCP tools and fall back to grep; CLI works
    from any shell. Fix: make MCP reachable from subagents or re-point
    AGENTS.md at the CLI. Related: row 34's front-door teaching.
41. **dd derive gap → fs3 second implementation** (nigel/dajeil): dd's
    deriveItems/deriveState/deriveRollup are reachable from neither Node
    API nor CLI, so 008 implements the derived-state rule in Rust — the
    drifting-second-implementation dd itself warns about. Contained behind
    one function (one-line swap); dajeil carrying a `ddocs --json derive`
    ask to Jordan with fs3 as named consumer. If dd declines, this row is
    the standing drift risk.
42. **ask on a fake provider returns ok:true for a non-answer** (DL-059 +
    vicuna's sharpening, both live repros 2026-08-28): 'The offline fake
    has no scripted answer' arrives with ok:true and empty citations — a
    machine consumer branching on ok (as our envelope rules instruct)
    banks it as an answer; grounded:false and the suspicion next_action
    exist but ok is the documented branch point. Route the verdict through
    the ENVELOPE: ok:false + FS3-E-PROVIDER-FAKE (fix naming provider
    setup) or an explicit degraded flag — prose is not a verdict (doctor
    pattern). Prevention half stands from DL-059: prod should refuse to
    boot ask on a fake agent provider silently. Owner: flea (ask domain)
    on next revival; pairs with rows 34/35.
43. **harness checks: detect a conflicted index FIRST** (wolf, 007 closeout,
    CONF-003): during a transient merge, checks reported "production
    database changed version=13 -> blank" while the real state was
    unresolved Rust conflict markers — the prodguard alarm fired on a tree
    that was not a tree yet. Encoding: a cheap unmerged-index/conflict-
    marker probe runs BEFORE tests/prodguard and names the real condition.
    Same guard family as rows 18-19 (the 13->blank misread has now
    appeared twice from two different causes — the guard's error message
    is doing active harm).
44. **flowspace3 explain — the retrieval diagnostic** (DL-050, blocking):
    root-causing one zero-result query took four tools and an afternoon
    (hand curl to the embeddings deployment, SQL extracted from source,
    PREPARE/EXPLAIN in psql). `flowspace3 explain "<query>"` printing
    vector norm, model_key, resolved filters, plan, candidates
    examined-vs-returned, scan-budget verdict — everything already known
    inside one function. A retrieval product with no retrieval diagnostic.
45. **agent-loop ergonomics pair** (DL-052/053, from the ask POC): (a)
    `get` on a struct+impl shared address errors demanding --span — return
    first element or combined outline instead (agents lose a turn); (b)
    search envelope is verbose for LLM consumption — an agent-facing
    --compact output (address/path/score/gist). Both cheap, both measured
    against a live agent loop.
46. **tidy should NAME the process holding a worktree** (carp, 007
    teardown): worktree removal timed out 60s→SIGKILL because two live
    processes held the tree as cwd (a stood-down reviewer whose process
    survived its seat, and the PM standing in the tree it was removing);
    the report said "timed out", the truth was nameable. Encoding: lsof +D
    the worktree before removal and name the holders ("the reviewer seat
    still holds this directory"). Pairs with the squash-orphan rule (tidy
    refuses when the branch carries commits after the merged sha) — both
    are tidy learning to say WHY, not just WHAT.
47. **daemon shutdown drains the QUEUE, not just in-flight jobs** (o-prime,
    live restart with dd backlog): C-c closed the listener then kept
    dequeueing enrichment jobs (5k+ left) — restart stalled until SIGTERM;
    with a large backlog a polite restart could take hours. Shutdown must
    stop dequeueing at the signal, finish in-flight only, and log
    "draining N in-flight" so progress is visibly bounded. Found the same
    hour the SIGTERM sandbox gap (row 38) was — the two signals want one
    coherent shutdown story.
48. **ask follow-up packet (flea's standing list, items 1-3 as ONE
    dispatch)**: (a) output verbosity — answer buried under full working,
    wants --trace (CHECK FIRST whether 007's human renderer already
    solves it — same question asked twice); (b) search hits absent from
    trace/citations — the model can lean on a GIST it never read, ~2/3 of
    calls are searches; provenance covers read-in-full but not
    saw-a-summary-of (contract delta); (c) tokens_used always null on
    Azure — the adapter never reads usage back, so the TOKEN BUDGET
    CANNOT FIRE in production; a configured budget that cannot trigger
    reads as protection and is worse than none (contract delta). Owner:
    flea on revival; the two contract deltas cost tapir one message
    together. Also for tapir wave 2: flea's eval-instrumentation probe —
    a scenario deliberately pointed at a fake-wired daemon must score
    UNKNOWN, proving the suite can tell answered-well from never-asked
    (the eval catching its own blindness).
49. **add's next_action steers agents into polluting the shared index**
    (nigel/u1): the CLI's own next_action told a coder to `flowspace3 add`
    its worktree (333 files queued into the shared index before the no-add
    ruling reached it; cleanup proven). A next_action cannot know it is
    one of five checkouts of an already-indexed repo — it should detect
    same-repo-already-registered and say "this repo is indexed via <root>;
    search works now, add nothing" instead. Pairs with row 32 (unregistered
    worktree honesty) — both are the multi-checkout story reaching the
    guidance layer. Coder no-add rule now in the packet template.
50. **sandbox: premature ready line + incomplete ambient isolation** (#48
    family, knobbler via leopon — URGENT over row 38): (a) the posture
    line prints BEFORE wiring is proven — and the fleet is ruled to trust
    that line as proof (tenet 14 shaped, institutionalised); (b) sandbox
    forces top-level embedder/summarizer to fake but ambient per-surface
    selections (e.g. [agent] active=azure-luna, present since the restore)
    still reach wiring — surfaced here as a hard fail (lucky); the unlucky
    shape resolves to a REAL provider and quietly SPENDS, the precise
    incident #48 exists to prevent, through the door it left open. Fixes:
    sandbox IGNORES ambient config entirely (it already mints its own
    config dir — point the loader at it and nothing else); ready line
    moves AFTER wiring validation. Recorded nuance: the verb superseded
    the four-override recipe but is WEAKER than it on this axis —
    "supersedes" is not "strictly dominates" until closed.
51. **tidy: content-in-main check over ancestry** (silkworm's exit, joins
    DL-049/054/row 33): every squash-merged pij-team plan now ends in
    E_BRANCH_NOT_MERGED + --force, training the reflex the gate exists to
    prevent. The check silkworm ran BY HAND is mechanisable and is the
    right test: diff the PATHS THE BRANCH TOUCHED against main (zero
    differences = content landed; files differing only from LATER merges =
    main ahead, not work absent). Tidy should run that tree comparison and
    say "content verified on main" instead of refusing on ancestry a
    squash destroyed.
52. **schema-moved alarm should say WHO moved it** (knobbler false alarm,
    leopon root-cause): a gate observed 13->15 mid-run and correctly
    stopped — the migrations were the o-prime's announced daemon restart
    carrying #42's 14/15 up, one second before. Encoding: the prodguard
    compares migration installed_on against the daemon's boot time before
    alarming ("schema moved by the daemon's own boot" vs "schema moved by
    an unknown writer" — only the second is an incident). Also real from
    the near-miss: FS3_DATABASE__URL unset + Config default falls back to
    the LIVE database for any spawned binary — seal-ambient-inputs one
    layer down (rows 29/50 family). O-prime process note: daemon restarts
    now announced to ACTIVE PMs, not only telegram — a migration under a
    running gate looks like an incident from below.
53. **daemon.key published BEFORE bind — losing daemon clobbers the
    winner's key** (vicuna's port review, verified on our source:
    auth::generate at boot.rs:82 runs before the listener binds, and the
    doc comment states the order as a feature): a second daemon that will
    LOSE the port race still overwrites the healthy daemon's daemon.key
    first — every client of the survivor 401s until restart. Likely the
    deeper mechanism of the row-30 key-divergence family, and easy to
    trigger (two candidates ran simultaneously today). Fix: stage → bind →
    atomic rename (vicuna's port order). Also verify restart-into-existing-
    key perms: OpenOptions::mode applies only on CREATE (our NamedTempFile
    set_permissions path is probably safe — verify, don't assume).
    Cross-government credit: found by the Rust port reviewing OUR pattern.
54. **long-running harness steps must NAME the active step** (tiger via
    flea, DL-003 their buffer): boot ran 120s "without identifying the
    active step" — slow-but-progressing and hung deserve OPPOSITE operator
    reactions and are currently indistinguishable, so operators kill
    healthy work or wait on dead work. Encoding: boot (and any >10s
    harness step) streams the step it entered + elapsed. Separable from
    and more valuable than the contention finding it arrived with.
55. **ddoc file-ref retention across parser generations — DEFERRED with a
    named trigger** (008/nigel, ruled by o-prime on nigel's recommendation):
    correctness is solved by delivery (rows_referencing takes a required
    parser_version predicate — the caller states what it knows; the store
    cannot infer a "current" generation, and MAX(id) is wrong mid-reindex).
    Retention is pure storage, UNMEASURABLE today (ddoc_file_refs has never
    held a row — file edges arrive with dd PR #12, still open), so ruling
    now would choose policy against imagined data. TRIGGER: the day dd
    PR #12 merges + nigel's composition measurement (rows + bytes per
    generation on this corpus) exists, the retention question comes back
    with a real number. Note: superseded-generation ELEMENT rows accumulate
    today independent of this plan — retention, if it bites, is
    pre-existing and larger than refs.
56. **postgres backend crash under 50k-job load** (go-live probe, exit-2
    environmental half): "terminating connection because of crash of
    another server process" mid-probe; container self-recovered, healthy.
    Wants a look before it fires during real use: container memory limits
    vs shared_buffers/work_mem under the post-#50 job volumes (the
    bootstrap itself minted ~50k jobs). Not urgent; recorded with the
    transcript in leopon's probe report. UPDATE (nigel's DL-006 forensics):
    single backend exit code 2 at 07:09:09Z -> cluster-wide crash recovery,
    every connection in every database died; 16/100 connections, 224MiB of
    31GiB — NOT exhaustion. WAL redo shows a Database/DROP at the crash
    point while four seats churned throwaway sandbox DBs on the container —
    DDL churn is the lead suspect, not memory. Sandbox daemons no longer
    run against the shared container (nigel's own rule, adopted).
57. **newest-first: service order, not just registration order** (probe's
    real finding, FIX-FORWARD dispatched to louse same-day): scan jobs for
    a newly discovered root sat frozen at 553 for 430s behind a 49.9k
    queue — claim query honors priority DESC but detector enqueues at
    default 0. Fix: raised priority on new-root scans + DECLARED priority
    scale (tenet 6). Jordan's AC now reads SERVED immediately, not
    registered immediately. Tenet 16, third surface: we ordered the step
    that registers, not the step that serves.
58. **shared test database makes concurrent gates non-deterministic**
    (flea's fleet alarm, A/B in flight): every seat's harness checks
    points FS3_TEST_DATABASE_URL at one flowspace3_test on one Postgres;
    under concurrent gating a DIFFERENT innocent test reds per run
    (contention on the shared server/admin connection, loser by timing).
    Interim ruling broadcast: per-seat test DBs + rerun-until-green
    forbidden. Durable encoding candidates: harness checks mints an
    entropy-named test DB per RUN (FreshDatabase already exists in testkit
    from #48 — the mechanism exists, the gate does not use it, tenet-16
    shape again) and drops it after; CI unaffected (own runners).
    TWO-LAYER UPDATE (nigel + flea): (a) per-database isolation does NOT
    bound the failure — the CLUSTER is a shared domain (one backend crash
    -> cluster-wide recovery kills every connection in every database;
    219% CPU under ten concurrent suites); the tell is whole-suite-fails-
    together / connection-shaped errors vs one assertion. (b) The
    discriminator is inapplicable IN PRINCIPLE today: checks DISCARDS the
    failure text (details keep only the cargo stdout tail — no panic, no
    tally), so no past red can be classified and no green proven
    connection-clean. FIX ORDER: (1) RETENTION — capture the failing
    suite's stderr tail; (2) classification (connection-shaped = LOUD
    infrastructure verdict); (3) FreshDatabase per-run mint. Never bank a
    PASS over a suite that lost its connection mid-run — "a green that
    means nothing is strictly worse than a red that means nothing" (flea).
59. **guard error must name the LAYER: stale-branch parser vs bad config**
    (fleet blocker, root-caused): harness checks builds fs3-migration-guard
    from THE SEAT'S OWN TREE; a branch predating a config-schema addition
    refuses ambient config newer code wrote, and the error names the FIELD
    and blames the FILE — the fleet hunted a harness second-parser defect
    that does not exist (the guard reuses fs3_core::resolve by design,
    migration_guard.rs:135). Encoding: on parse failure the guard says
    "this branch's parser refused a field newer code accepts — likely
    behind main: rebase, or gate with sealed FS3_CONFIG_DIR". Rows 18/19/
    28/43 family: it is the stale-base story plus the misleading-alarm
    story meeting in one message.
60. **enrichment policy for auto-discovered worktrees** (Jordan's 43k-queue
    concern, 2026-08-28 telegram): the detector registering ~20 checkouts
    queued full bootstrap scans; dedupe zeroes identical content but each
    branch's DIVERGENT WIP files get real summarize spend nobody asked
    for. Candidates: auto-discovered (vs hand-added) roots default to
    scan+embed-only with summarize deferred/off; or summarize only content
    that survives N ticks (WIP churn never enriched); or per-root
    enrichment toggle surfaced in config [repos]. Pairs with rows 30/32
    (worktree posture) and the newest-first lane (#59): the priority story
    solved WHEN work runs; this row is WHETHER it should.
61. **PARSER_VERSION bump does not re-index an existing corpus** (nigel,
    008 composition, MEASURED: bump to @2 + restart + scan = enqueued 0 of
    351 files; only remove+add forced re-parse): roots.rs:197 decides
    enqueue from the stored path->blob map ONLY; parser_version is
    consulted inside scan::run's skip (scan.rs:142) which never runs when
    nothing enqueues. The doc comment at scan.rs:198-201 ("bumping
    re-mints every element row") is FALSE as written — a knob that looks
    like an invalidation mechanism and silently is not (tenet 17 family).
    SHIP-BLOCKING for 008's rollout (ddoc support never reaches indexed
    corpora) and latent for every future parser improvement. Fix: enqueue
    decision treats different-parser_version as changed, OR delete the
    claim and document remove+add as the migration. Own packet, own test;
    outside 008's fence by nigel's correct refusal.
62. **ask answers assert enumeration they cannot know** (roadrunner, graded
    ask run): "two main paths" read as complete by a consumer with no
    ground truth (a third — OSC 52 via ClipboardAddon — existed);
    grounded:true + clean citations REINFORCE false completeness. Fix at
    the contract: the answer distinguishes "what I found" from "all there
    is" — a bounded loop cannot know it enumerated a space and must not
    phrase as if it did. Owner: flea (ask contract) on revival; pairs
    with tapir's fixture doctrine (semantic vs exact).
63. **unsatisfiable path glob reads as absence** (roadrunner, same run):
    loop spent 1 of 7 iterations on --path "src/**" in a repo whose paths
    are repo-root-relative ("apps/web/src/...") — a glob that CANNOT
    match, scored failed:true. envelope should distinguish "glob matched
    no PATHS" (unsatisfiable filter — say so, name the layout) from
    "matched paths, none relevant" — the scoped-zero family
    (empty_because vocabulary gains a member: path_unmatched). Also:
    the ask loop could read repo layout (tree) before path-filtering.
