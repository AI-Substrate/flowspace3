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
    DISPATCHED 2026-08-29: brief w-test-db-isolation.md, coder seated.
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
    RULED (Jordan, 2026-08-29): current design is the intent — dedupe by
    content hash means unchanged files are shared rows (tagged into each
    checkout, zero provider spend, 006-measured) and only a worktree's
    genuinely changed files enrich; that changed-file spend is the
    feature, not waste. No cheaper tier; auto-discovery stays on. CLOSED.
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
    RULED (Jordan, 2026-08-29): WONTFIX the invalidator — re-index only
    when the document itself changes; remaining scope is to delete the
    false doc claim at scan.rs:198-201 and document remove+add as the
    migration path (folds into a governance/docs pass, no packet).
62. **ask answers assert enumeration they cannot know** (roadrunner, graded
    ask run): "two main paths" read as complete by a consumer with no
    ground truth (a third — OSC 52 via ClipboardAddon — existed);
    grounded:true + clean citations REINFORCE false completeness. Fix at
    the contract: the answer distinguishes "what I found" from "all there
    is" — a bounded loop cannot know it enumerated a space and must not
    phrase as if it did. Owner: flea (ask contract) on revival; pairs
    with tapir's fixture doctrine (semantic vs exact).
    DISPATCHED 2026-08-29: brief w-ask-honesty.md (with row 63).
63. **unsatisfiable path glob reads as absence** (roadrunner, same run):
    loop spent 1 of 7 iterations on --path "src/**" in a repo whose paths
    are repo-root-relative ("apps/web/src/...") — a glob that CANNOT
    match, scored failed:true. envelope should distinguish "glob matched
    no PATHS" (unsatisfiable filter — say so, name the layout) from
    "matched paths, none relevant" — the scoped-zero family
    (empty_because vocabulary gains a member: path_unmatched). Also:
    the ask loop could read repo layout (tree) before path-filtering.
    DISPATCHED 2026-08-29: brief w-ask-honesty.md (with row 62).
64. **no lexical channel: provably-present exact strings do not retrieve**
    (leopon, run eight — the strongest instance of the lexical family):
    a phrase existing verbatim in three indexed elements (SQL-confirmed
    against raw_text) returns NONE of them; top hits unrelated at 0.31.
    Pure-vector retrieval has no fallback for exact-string lookup, so an
    agent searching for a symbol/error-code/identifier it just wrote gets
    nothing. Joins rows 21-26 + CONF-003 as the anchor case (content
    provably present). Candidate: a text-match leg (trigram/tsvector) 
    fused with vector rank, or an exact-substring escape hatch the
    envelope names. Paired lesson for probe/fixture authors, encoded:
    DISTINCTIVE-TO-A-HUMAN IS NOT DISTINCTIVE-TO-AN-EMBEDDER.
65. **sandbox daemons auto-discover sibling worktrees — measurement
    contamination** (crayfish, cost probe): adding the main clone to a
    SANDBOX daemon auto-registered a sibling fleet worktree mid-
    measurement (740 → 1,481 files), poisoning the corpus. Auto-discovery
    (006's feature) is correct for prod and wrong for a measurement
    posture: --sandbox should default to index-exactly-what-you-add
    (detector off, or opt-in), because a probe corpus that grows itself
    invalidates the numbers taken over it. Rows 50/60 family; hand to the
    same owner as the sandbox-hermetic work (feran's packet touches the
    posture seam — evaluate whether it is one line there or its own
    follow-up).
66. **the reviewer packet needs a THIRD terminal verdict:
    correct-but-not-complete** (nigel, CONF-002, from the 008 ship
    decision): a pipeline whose only terminal verdicts are APPROVED and
    BLOCKED will quietly convert one into the other under pressure. Today
    the third state was invented in prose by a reviewer and carried up
    unlaundered by a PM — two people's judgement where structure should
    stand. Encoding: packet-reviewer gains the third verdict; a PM cannot
    report done without naming which of three it earned; a reviewer never
    chooses between blocking a working feature and rubber-stamping open
    promises. Template change, one field; HELD under drain order.
    RULED (Jordan, 2026-08-29): WONTFIX — REQUEST_CHANGES was the
    technically correct verdict (it did request a fix); keep the two
    verdicts, no third state.
67. **the sealed gate proves REPRODUCIBILITY, not PORTABILITY** (nigel,
    #65's CI red): nine green gates on six developer machines all sealed
    the SAME hidden dependency (`ddocs` resolving to the local sibling
    repo via symlink, not a registry install) — CI was the first
    environment honest enough to lack the binary. Sealed-input discipline
    makes local runs reproducible and still cannot see a dependency every
    seat shares. Encoding candidates: harness checks reports external
    binaries the test suite RESOLVES outside the repo/toolchain pin (a
    which-audit over test-invoked commands); and/or a periodic clean-
    runner canary gate. Beside row 66; HELD under drain.
    RULED (Jordan, 2026-08-29): CLOSED, no further work — CI-on-PR is the
    clean-host proof and it caught this one; no scheduled from-scratch job.
68. **embed job sends an empty string to the provider** (o-prime, live
    prod queue 2026-08-29): job
    embed:conv:recovery:raw:5704b480… failed with Azure 400 "Invalid
    'input[1]': input cannot be an empty string" — a conversation-recovery
    row produced an empty chunk and we shipped it to the provider instead
    of filtering it pre-batch. One bad row can poison a 64-batch. Fix:
    drop/skip empty (or whitespace-only) inputs before batching, and
    decide the row's terminal state (done-with-note, not failed-forever).

69. **team tidy deletes the active seat's cwd out from under it** (DL-002,
    retro drain 2026-08-29): immediately after tidy removed a seat's
    worktree, a parallel shell init failed ENOENT despite a surviving
    explicit cwd; retry from the main clone succeeded. Encoding: tidy
    should relocate/rebind (or at least warn about) an ACTIVE seat's cwd
    before deleting its worktree. Joins the tidy family (rows 33/46/51);
    fold into w-team-tidy.md.

70. **lean-ctx wrapper mangles harness command output for OMP seats** (two
    strikes 2026-08-29: wee-viper — 'harness docker up' failure collapsed to
    an unactionable form, needed a second jq-selected call; wolverine —
    'lean-ctx -c "harness docs the-harness-distilled"' returned an
    unrelated 'mocha: 0 passed' instead of the document). The context-
    compression layer is eating exactly the output an agent needs when
    something goes wrong. Encoding candidate: seats bypass lean-ctx for
    harness verbs (or lean-ctx passes harness envelopes through verbatim);
    route to whoever owns the lean-ctx config for OMP spawns.
    THIRD strike same day: mawhrin — lean-ctx erased a shell loop variable
    in a stat probe, yielding empty paths (silent data corruption, not just
    output collapse). Escalates the row: this is now correctness, not
    ergonomics. FOURTH strike: mawhrin — backticks in a flowspace3 element
    address were shell-interpreted by lean-ctx, silently CHANGING the
    address queried. FIFTH strike: grasshopper — 'lean-ctx ls
    crates/store/migrations' returned a project-root overview instead of
    the requested directory. SIXTH strike: camel — the injected rule
    instructs 'lean-ctx -c "ctx_read ..."' but the wrapper reports
    'ctx_read: command not found' — the mandate names tools the wrapper
    does not provide. SEVENTH/EIGHTH (camel): compression erased search
    hit identities into '{...13 keys}' placeholders; absolute-path ls
    again listed repo root. NINTH (camel): a failed cargo test compressed
    to 'FAILED: 0 pass, 1 fail' with the actionable panic hidden — the
    evidence-retention defect (row 58's lesson) reintroduced at the
    wrapper layer.

71. **ask token-budget exhaustion returns ok:true + grounded:true +
    answer:null** (sylac, dogfood 2026-08-29): an 8-iteration run that
    ran out of budget reports SUCCESS shape with a null answer —
    grounded:true describing an answer that does not exist is incoherent,
    and ok:true makes a consumer parse null. Fix: budget exhaustion is its
    own honest terminal (ok:false or an explicit stopped envelope where
    grounded is absent/false), next_action already good. Joins the ask
    honesty family (rows 42/62/63; brief w-ask-honesty just merged — this
    is a NEW surface of the same law: the envelope must not claim more
    than it measured). Second sighting (camel, 2026-08-30): focused ask
    stopped answer=null WITH 6 citations attached. THIRD sighting
    (bedbug): 4 citations, answer:null. Three seats in one day — this is
    the next ask packet.

72. **no per-path freshness introspection: "is my edit indexed yet?" is
    unanswerable** (mawhrin, dogfood 2026-08-29): post-edit search missed
    a new roots.rs predicate in a registered+watched worktree — probably
    backlog lag, but nothing in the envelope or any verb can SAY when a
    path was last indexed, so an agent cannot distinguish "not indexed
    yet" from "indexed and not retrieved" (the row-64 lexical gap and this
    row compound each other). Candidates: `flowspace3 status --path <p>`
    returning last-indexed blob + timestamp + pending-job presence, and/or
    search hits carrying indexed_at. Kin: rows 21-26 family.

73. **scan failures: a ddoc blob produces 2 file roots where exactly 1 is
    expected** (jackal, prod status read 2026-08-29): 14 failed scan_file
    jobs, last_error names the invariant violation. New defect surface in
    the ddoc scan path (008 family) — needs a repro + fix packet. Also
    note: prod still runs pre-#69 code, so worktree scan churn continues
    (12,371 pending) until the ruled rebuild+bounce. ESCALATED 2026-08-30
    (bedbug): the same blob-with-2-file-roots invariant violation now
    makes 'flowspace3 status' itself fail (FS3-E-STORE-QUERY-FAILED, blob
    402efae…) — a read verb broken by dirty data, on the daemon post-#69.
    Two defects: the writer that minted the duplicate root, and the
    reader that hard-fails instead of reporting the inconsistency.

74. **get returns ok:true + content:null while next_action claims the whole
    element was returned** (camel, dogfood 2026-08-30): both
    jobs.rs::claim_jobs and migration 0016 resolved as hits but get
    delivered null content with a next_action asserting delivery. Same
    law as rows 71/42: the envelope claims more than it measured — a
    null-content get must be an honest error/empty_because, never
    ok:true+delivery-claim. Repro is exact (two named addresses).

75. **PLATFORM (pij) evidence awaiting an ermine revival — omp delivery +
    session-detection regression** (2026-08-30): (a) pij sends to
    pij-hard-camel stay state=queued indefinitely (three ACKs never
    delivered; tmux pane-paste used as the authoritative fallback);
    (b) camel's 'pij inbox' fails E-AMBIG 'cannot detect a current
    Claude, Copilot, or Codex session' on a REGISTERED omp pi seat;
    (c) jackal hit the same on 'pij inbox --wait' (workaround: explicit
    PIJ_SESSION_ID). All ermine seats dead; deliver on revival — never
    fix locally (standing doctrine).

76. **compose hard-codes container name flowspace3-db — every worktree's
    'harness docker up' collides with the main checkout's container**
    (third seat today: jackal, bedbug, camel; all self-resolved to the
    shared :5433 cluster + disposable DBs). Encoding candidates: drop the
    fixed container_name / derive it from the checkout path so per-seat
    stacks are possible, OR make 'harness docker up' in a linked worktree
    detect the running shared cluster and say 'using shared :5433' instead
    of failing into a name collision. Post-#70 the gate no longer needs a
    per-seat stack, so the second (cheaper) shape may be all we want.

77. **checks red envelope truncates the decisive failure detail — second
    sighting** (heron 2026-08-30; jackal saw the same during its d3 proof:
    'background delivery truncated even the bounded details before the
    failing test identity'). #70 fixed retention INSIDE checks (labelled
    stdout+stderr tails); the truncation now lives a layer up — whatever
    relays the envelope to the seat clips it. Find the clipping layer and
    bound-and-label there too; the failing test IDENTITY must always
    survive. THIRD sighting (camel): exit 124 timeout at 766s and the
    tail omitted WHICH suite was unfinished — the timeout path needs the
    same identity guarantee as the failure path.

78. **embed settlement deadlocks under a large ingest burst** (Jordan's
    pane paste, 2026-08-30): dozens of 'deadlock detected' WARNs across
    concurrent embed jobs (attempt=1 retrying=true) right after a 7,436-
    turn conversation ingest; retries cleared them and the queue kept
    draining, so no work lost — but sustained deadlock+retry is wasted
    provider/DB work and a lock-order smell in embeddings insert vs job
    settlement. Find the two lock orders and make them consistent (or
    batch the settlement); a burst should drain deadlock-free.

79. **checkout-scoped search excludes conversation rows — conversation
    content is invisible to the DEFAULT search** (o-prime proof
    2026-08-30): 7,788 conv elements stored, raw_text present, vectors
    joined by raw_hash — yet cwd-scoped search returns 0 for them and
    '--repo all --source conversation' returns them at 0.75. The
    conversation row NAMES this repo (conversations.repo) but the scope
    join (006, worktree_files) never consults it, so 005's content
    silently fell out of 006's default view — the tenet-15/16 composition
    shape: the scoping rule was never walked to the conversation surface.
    Fix: scope filter admits conversation elements via conversations.repo
    (and worktree when recorded). Also re-verify `get conv:...#t<n>`
    content:null (row 74 kin) on the same walk.

80. **a LOSING daemon mutates shared state before its bind refusal**
    (bobcat, 2026-08-30): a second daemon started during a bounce window
    spent ~108s on schema/ddocs/requeue work — including the
    requeue_failed sweep, a WRITE — then lost 7373 to the incumbent and
    exited. Boot's mutation phase runs before port ownership is
    established, so every losing racer re-queues failed jobs and touches
    ddoc state it does not own. Fix: bind/reserve the port (or take an
    ownership lease) BEFORE any mutating boot work; probes/reads may
    precede, writes may not. Kin: row 53 (key-before-bind) — same
    boot-ordering family, opposite direction.

81. **adopt from the pij rust port (arch-compare verdicts, 2026-08-30,
    scratch/arch-compare-pij.md)**: (a) raw unknown-event-byte
    preservation — fs3 deserialises unknown kinds to a unit
    EventKind::Unknown and LOSES the fields (events.rs:201-206); pij
    keeps kind + raw line so old readers forward new facts losslessly.
    (b) arch-gate hardenings: required 'why' rationale per allowlist row,
    runtime workspace resolution (not compile-time CARGO_MANIFEST_DIR),
    and cargo metadata --locked (never silently repair a stale lock).
    (c) housekeeping: stale test comment health.rs:155-156 still says
    'publishes daemon.key before binding' — implementation moved on.

82. **encode the bounce ritual as 'harness daemon bounce'** (self-identified
    while answering vicuna's adoption ask, 2026-08-30): the merge->restart
    rule is a ruling + o-prime muscle memory today. The verb: refuse if
    HEAD != origin/main (the stale-binary bounce I shipped), cargo build
    --release, drain-restart the pane daemon, poll health until the 401
    tell, print version. Encode-don't-document — this is the exact
    twice-inferred shape the harness doctrine names.

83. **ask silently substitutes the nearest ANSWERABLE question**
    (meadowlark, cross-repo dogfood 2026-08-30): asked about 'harness team
    new' in a repo where that verb does not exist; ask answered about
    'harness new' (a different verb) — grounded:true, real citations,
    WRONG QUESTION. A near-miss substitution with citations reads as
    authoritative and is MORE dangerous than a refusal. Fix at the
    contract (rows 42/62/71 family): when evidence matches a DIFFERENT
    subject than the question names, the answer must say 'X does not
    appear; the nearest thing is Y' — never silently answer Y as if it
    were X. Feed flea's negative-fixture lane. ALSO: state expected
    latency somewhere honest (~53s on glm-flash is fine for considered
    questions, unusable in agent inner loops — envelope or docs should
    say which it is for).

84. **dot-dirs (.harness/extensions/**) are excluded from indexing —
    invisible-by-construction for exactly the code agents ask about**
    (meadowlark cross-repo + halibut same day; FS-1 fourth confirmation):
    fs3's own team-new extension source never surfaces for any query, and
    #67's path_unmatched correctly names a layout that excludes .harness.
    Decide the policy deliberately: index .harness/extensions (agent-
    facing CODE) while keeping noisy dot-dirs (temp, buffers) out; walker
    skip-list becomes explicit config, not an accident of dot-prefixing.

85. **ask cannot be scoped to a single conversation** (Jordan's ask,
    2026-08-30): ask takes only --repo; there is no --conversation <guid>
    / --source filter, so "ask this question of THAT conversation" (e.g.
    'what did we decide about X in yesterday's session?') is not
    expressible — after #80 the mixed default may surface conv turns, but
    nothing pins retrieval to one transcript. Fix: ask gains the same
    source/scope filters search has, plus --conversation <guid> pinning
    retrieval (and citations) to that transcript's turns. Natural
    follow-on to #80.

86. **conv-guid trap + whale-scale searchability lag (docs/envelope)**
    (meadowlark's 14.5k-turn ingest, 2026-08-30): (a) every integrator
    will query conv:<session-uuid> once and get NOT-FOUND, because the
    conversation guid is DERIVED — the ingest envelope's next_action
    names only claude/<session>; it should ALSO name the derived
    conv:<guid> so the trap dies at the moment it fires. (b) at whale
    size the gap between "landed" (get serves 14.5k turns instantly) and
    "findable" (10k summarize pending, hours) is real — one honest docs
    sentence: rows are immediate, semantic findability follows the
    enrichment queue. RECEIPT of the day: 14,575 turns, 29ms submit,
    ZERO deadlocks — the same burst shape that stormed at 7.4k pre-#72/73;
    the settlement+microbatch fixes measurably cured row 78.

87. **oversize embed caps are BACK in volume — conversation turns hit the
    truncate-now path constantly** (Jordan's log paste, 2026-08-30): the
    WARNs are the 2026-08-27 ruling working as designed (truncate to the
    8192-token cap rather than fail forever; 'split later' explicitly
    deferred) — but whale conversation turns (20KB+ tool dumps) hit it
    at a rate code elements never did, and each one's TAIL is
    semantically unsearchable. The deferred future is now due: chunk
    oversize turns into multiple vectors (the design-note slot was left
    in the embed path by w-embed-oversize). Conversation-shaped fix, not
    a revert of the ruling.

88. **archived docs outrank live source for mechanism queries — now a
    DEFECT, not an anecdote** (meadowlark x4 + independent cold-context
    coder seat pij-changing-vulture; two observers, six instances,
    2026-08-30): semantic search for 'how does X work' returns archived
    plan/README prose top and unrelated source after — agents decline
    the results as evidence (correctly). Candidates: rank-time source
    weighting (source code > docs > archived docs for mechanism-shaped
    queries), an archived: flag/downweight on docs/plans/archive/**, or
    kind-aware boosts in the fused ranking (#74 opened the fusion seam).
    Distinct from row 84 (dot-dir exclusion).

89. **no query/ask ledger — searches and asks leave no durable trace**
    (Jordan, 2026-08-30): the ask envelope's trace (iterations, tool
    calls, queries issued, citations, tokens, stop reason) is returned to
    the caller and GONE; search queries aren't persisted at all
    (user_messages is an operator-notice table, empty). So 'what is ask
    being used for and is it behaving' is unanswerable after the fact —
    especially bad for ask, which is non-deterministic and tool-using.
    Fix: a query ledger — persist every ask run (question, full trace,
    citations, tokens, model, outcome) and search (query, filters,
    hit-count, top score, channel mix) with timestamps + caller identity;
    surfaces: 'flowspace3 ask history / show <id>', retention policy,
    and it becomes eval-fixture MINING (real failed asks -> fixtures).
    Jordan's emphasis: tool use matters — the trace is the artifact.

90. **daemon bounce verify times out during heavy boots** (first prod run
    of the new verb, 2026-08-30): freshness/build/locate/drain all ok,
    but boot ran migration 0020 + a big requeue sweep and exceeded the
    120s verify bound — the verb reported E_DAEMON_BOUNCE_VERIFY_TIMEOUT
    while the daemon was in fact healthily booting (it came up ~2min
    later, fully healed). Fix: verify should distinguish 'no process /
    no listener' from 'listener pending, boot log advancing' — poll the
    pane/log for boot progress and extend the bound while progress is
    visible, or take a --verify-timeout. The honest failure was correct
    behaviour; it just needs a third verdict: BOOTING.

91. **ask TIMEOUT class — scoped ask died at 120s** (meadowlark, 091 fleet,
    2026-08-30 — seat pij-changing-vulture via PM coral, harness-engineering
    repo). A scoped ask for transcript-path identity hit a hard 120s timeout
    and returned nothing. This is a DISTINCT defect class from bad ranking
    (row 88) and question-substitution (row 83): the ask never answered at
    all. Unknown split: client timeout vs daemon-side LLM stall vs tool-loop
    runaway inside the ask agent. Row 89's query/ask ledger is the missing
    diagnostic — without it we cannot see what the ask agent was doing at
    death. Meadowlark notes today's dogfood misses now span FOUR seats and
    TWO repos. Dispatch shape: instrument ask with a per-stage duration
    trace in the envelope (retrieval / tool loop / synthesis), surface a
    partial answer + citations on timeout instead of a bare death, and make
    the timeout configurable. Pairs naturally with row 89.

92. **pre-existing semantic duplicate-hit defect — nearest CTE constrains
    model_key but NOT source_kind** (owl PM recon, 2026-08-30, line-anchored:
    embeddings.rs:469-563). A raw row and a smart row for the same content
    can both survive one nearest-k call and both resolve to the same element,
    emitting duplicate semantic hits that `fuse` cannot merge (it compares
    lexical-vs-semantic only, search.rs:685-715). ac-0003 "one row per
    element" was never actually true. STATUS: being closed INCIDENTALLY by
    plan 009 u3's element collapse (o-prime-ratified amended S4 — collapse
    inside the nearest CTE, LIMIT over elements). Row exists so the ledger
    is honest about the defect predating 009; verify closed at 009 review.

93. **halibut's rescued observation buffer (4 items, w-daemon-bounce)** —
    tidy's stash-rescue recovered these; full text at main-clone
    .harness/temp/agent/daemon-bounce-observations.md. (a) semantic search
    cannot see .harness extension sources — index excludes .harness paths,
    agents fall back to raw reads (pairs with row 84's exclusion-policy
    question: make the exclusion explicit or index repo-trusted harness
    code). (b) docker-compose hardcodes container_name flowspace3-db, so
    worktree-scoped `harness docker up` collides with the main checkout's
    container — should be compose-project-scoped. (c)+(d) two bounce-rig
    test-harness lessons (persistent-pane fixture; envelope-discriminator
    assertion) — fold into any row-90 BOOTING-verdict dispatch.

91-ADDENDUM (2026-08-30, meadowlark verbatim capture): "[Command timed out
    after 120 seconds] / Wall time: 121.61 seconds / NO flowspace3 envelope
    at all." Meadowlark classifies CLIENT-BOUND (no envelope = client
    killed it). Lynx refinement: that message text is the signature of the
    SEAT'S OWN Bash-tool default timeout (120s in most harnesses), not a
    flowspace3 limit — the ask was probably still legitimately working.
    So the defect splits: (a) agent-guidance defect — ask is a minutes-
    scale command and our docs/next_actions never say "raise your tool
    timeout or background it" (cheap fix, agents-start-here + ask docs);
    (b) coral's product defect, endorsed: ask emits NOTHING until done, so
    a killed-slow ask and a hung ask are indistinguishable — a heartbeat/
    partial-progress line before the deadline separates the classes for
    free and delivers row 89's client-side half. The capture is the
    evidence; no rerun.

94. **paginated, time-bucketed conversation timeline view** (raven via
    vicuna, 2026-08-30, from the ermine-dossier exercise — dossier at
    /Users/jordanknight/pi-hacking/pij/scratch/answers/raven-ermine-dossier.md).
    Reconstructing a 23-day tenure meant hand-stitched get #t<n> windows
    and tree bounds; the ask is a first-class timeline verb: bucket a
    conversation by time/turn-range with decision/incident/delivery
    clustering, paginated. Their what-worked list (quoted-literal search,
    get windows 0.04-3.5s, tree for bounds, semantic-for-vocabulary-then-
    literal) is the manual protocol this verb would encode.

95. **supersession-aware ranking within conversations** (raven via vicuna,
    2026-08-30). A claim that is later corrected/withdrawn/superseded in
    the same conversation should have its TERMINAL version outrank the
    retracted one; today both rank equally and a reader can retrieve the
    withdrawn claim with no signal it was retracted. Sibling of row 88
    (archive-over-source) — both are "staleness should demote" ranking
    defects, likely one dispatch. Hard part: detecting the correction edge
    (explicit retraction language vs silent revision).

88-NOTE (2026-08-30): sixth independent confirmation of the archive-ranking
    trap (meadowlark's fifth-observer dogfood seat: archived 047 plan docs
    outranked live docs/how/telemetry/*.md on every query; explicitly
    code-flavoured query still returned ~90 doc / ~5 code). Their magic
    wand matches the dispatch shape: --prefer-live / archive-downweight
    ON BY DEFAULT for docs/plans/archive/**.

96. **per-source indexing status — the landed-vs-findable gap made
    inspectable** (meadowlark fifth-observer seat, 2026-08-30; endorsed
    hardest by meadowlark, seconded by lynx). An empty --source
    conversation result is indistinguishable between "nothing relevant"
    and "nothing enriched yet" — the seat hit zero twice during whale
    enrichment lag and COULD NOT TELL, and the empty-envelope next_action
    only offers the boilerplate embedder-mismatch hint. Dispatch shape:
    `flowspace3 status --source conversation` (per-source landed/
    summarized/embedded counts + queue depth), AND the empty-result
    next_action should point at it when the searched source has a
    non-empty enrichment backlog. Closes the diagnosis half of row 86's
    whale-lag docs ask.

97. **ask plain-text rendering buries the answer under the tool trace**
    (same seat, 2026-08-30). Human-mode ask output leads with a long
    tool-trace tail; the seat's first read caught only the trace and
    re-ran with --json — one wasted probe. Fix: answer first, trace
    below a fold/flag (--trace to show), citations adjacent to answer.

38-NOTE (2026-08-30, coral via meadowlark): sandbox DB leak confirmed
    KILL-only — graceful exit drops the minted DB (verified against
    docker pg_database). Row narrows to the SIGTERM/SIGKILL path.

98. **conversation ingest envelope echoes CALLER INTENT, not the resolved
    store** (coral R1 proof via meadowlark, 2026-08-30; fixture-only,
    sandboxed, read-only). dedupe_key/envelope carry the REQUESTED folder
    while the daemon resolves the session store from ITS OWN env — so a
    client-side HOME override silently ingests the REAL store while every
    visible signal says fixture. Misleading-success family (same genus as
    the git-rm no-op and stale-binary bounce). Fix: envelope must name the
    STORE PATH actually read (and the dedupe_key should derive from the
    resolved path, not the request). Cheap, high-trust fix — this is an
    honesty defect in a surface we are telling other fleets to adopt.

98-NOTE (2026-08-30, coral): the DAEMON-KEY error from the same session is
    the in-repo counterexample — it printed the exact path it could not
    read and was fixed in seconds. Dispatch framing: "make the ingest
    envelope look like the key error" (name what you looked at, never
    assert a cause you didn't resolve).

99. **incrementality CI canary — coral's three-fire rig, offered as a
    regression test** (via meadowlark, 2026-08-30). Daemon-isolated
    (HOME=<fixture> on the DAEMON, not the client — row 98 in action),
    fixture transcript copied under <fixture-home>/.claude/projects/...,
    sandbox daemon.key copied into fixture config. Three fires: cold /
    grown / UNCHANGED — fire 3 is the discriminator (full-re-read-with-
    dedupe matches fire 2's count; only true position-tracking gives
    no-growth), and as CI it is ONE number: fire-3 wall drifting 39ms ->
    ~1900ms alarms incrementality regression with zero semantic
    assertions. CAVEAT RECORDED WITH THE RECEIPT (coral's words: "a
    caveat that arrives with the claim is just the claim stated
    properly"): the 2026-08-30 incremental proof ran under daemon
    --sandbox with FAKE providers — PROVEN: ingest/store position
    tracking; UNTESTED: real embedder/summarizer timing. The banked
    contract receipt is hereby regraded to that honest scope.

99-RESHAPE (2026-08-30, coral retraction, relayed before action): absolute
    wall-time alarm WITHDRAWN — coral ran its own rig twice (1909/373/39ms
    vs 516/571/107ms): ~4x run-to-run drift, and the grown<cold ordering
    reversed. Row 99 gates on STRUCTURAL invariants only, reproduced twice
    on independent DBs: growth ingested (91->168), no-growth re-fire adds
    NOTHING (168->168), guid stable. Timing prints as an advisory NOTE of
    the within-run ratio (fire3 << fire1; 49x and 4.8x measured). Script
    to lift wholesale: scratch/smoke091/run-smoke.sh in the s091 worktree
    (daemon-side isolation baked in, client-HOME trap documented). Also:
    third confirmation of row 38's graceful-stop-drops-DB narrowing.

100. **P1 REGRESSION — conversations are WRITE-ONLY: listed but neither
    gettable nor searchable** (dajeil dd-repro + lynx flowspace3-repro,
    2026-08-30, post-#80/bounce). conversation list shows guid + turn
    counts; get "conv:<guid>#t<n>" returns FS3-E-QUERY-NOT-FOUND "no
    conversation ... is indexed"; --source conversation search returns
    composition conversation:0 for ANY term (e.g. "watchdog") in ANY repo
    scope; unscoped search returns code+doc healthy, conversation 0.
    PRIME SUSPECT: #80's conversation scope filter compares mismatched
    repo-string formats — list renders repo "github.com/AI-Substrate/dd"
    while query scope carries "git:github.com/AI-Substrate/...". ask with
    explicit --repo still retrieves turns (ermine dossier + lynx 179s ask
    both worked TODAY), so storage is intact and at least one read path
    joins correctly. Every fleet's compaction-recovery mechanism is dark
    until this lands. Dispatched w-conv-readback (o-prime initiative,
    severity; Jordan may veto).

101. **PRODUCT QUESTION for Jordan — should explicit conv:<guid> get be
    address-authoritative (cross-repo)?** (surfaced by zakalwe's
    stop-and-ask on w-conv-readback, 2026-08-30). #80's shipped law:
    scoped get REJECTS foreign-repo conversations with an explanatory
    envelope; the row-100 hotfix preserves that byte-for-byte. But the
    fleet use case is real: dajeil validating a dd ingest, vicuna reading
    ermine's pij transcripts, primes reading each other's handovers — an
    explicit full-guid address arguably IS consent to cross the repo
    boundary (it is not discoverable by accident). Options: (a) keep
    rejection, callers pass --repo; (b) explicit conv: address admits any
    repo, search stays scoped. One-question ruling when convenient.

102. **rust-analyzer returns silent-zero cross-crate references — FOUR
    seats in one hour** (owl's three 009 coders: embed_items vs
    runner.rs:642, search_elements vs search.rs:296/:303; zakalwe:
    fs3_store::upsert_conversation; 2026-08-30). An empty reference
    result is indistinguishable from "no callers" — the failure mode
    where an agent confidently concludes nothing calls a thing. All four
    fell back to exact identifier search (correct per repo guidance).
    Encoding (owl's DL-003): probe the LSP at boot with a known-multi-
    caller symbol and REFUSE/mark-degraded rather than serve zeros.

103. **ddocs render staleness is SILENT — .dd.json newer than .dd.md
    sibling has no gate** (owl's DL-002, 2026-08-30). My ratified S4
    amendment sat in the JSON while coders read stale markdown carrying
    the disproven premise; u3 refused to code off it (correctly) and owl
    rebuilt by hand. Two-file design means a reader cannot tell which
    file lies. Encoding: harness gate fails when any .dd.json mtime/hash
    is newer than its rendered sibling; bonus: `harness ddocs render`
    discoverable from the CLI surface (prime could not find it in one
    probe).

104. **harness boot reports "compose db is not running" from ANY worktree**
    (owl's CONF-003, 2026-08-30; cousin of row 93b's container_name
    collision — three seats bitten today). docker compose scopes projects
    by directory, so worktree seats see a false negative while the
    container is Up and the testdb gate passes; some seats will try to
    "fix" it and hit 93b. One dispatch should take 93b + 104 together:
    compose-project-scoped naming + boot probing the CONTAINER, not the
    project.

100-CLOSED (2026-08-30): PR #83 merged + bounced; store-write
    normalization + migration 0021 backfill. Verified live by lynx (get
    ok:true, conversation search composition 132 vs 0) — dajeil's
    enriched-proof re-run is the formal acceptance. Report-to-prod ~1h.

105. **empty body is not an empty TURN — item-bearing turns must not be
    skipped by embed hygiene** (dajeil, 2026-08-30, from row-100
    acceptance read-back). A turn can carry "body": "" with
    body_empty_reason "typed items but no prose" and its full content in
    `items` (tool calls — which in agent transcripts carry most of the
    measurements). If plan 009's u2 mint-side empty predicate keys on
    BODY emptiness it silently drops tool-call turns from search; if the
    current pipeline embeds body-only, item-bearing turns are ALREADY
    unsearchable and the empty-string poison jobs may be exactly these
    turns. Judgment: 009's predicate must key on the PREPARED text;
    whether items feed prepared text is a VERIFY for u2 (relayed to owl);
    embedding FROM items when body is empty is this row, dispatched
    separately if 009 confirms the gap.

104-NOTE (2026-08-30, bovid post-coding): sharper failure order for the
    93b/104 dispatch — `harness docker status` MISSES the global named DB
    entirely, then `up` creates a stray worktree volume BEFORE hitting the
    container-name collision, leaving cleanup debris. Fix status/adoption
    to see the named container first; four seat-hits today across the
    family. Also recurring: cargo multi-filter misuse x3 -> encoding is a
    harness focused-test verb accepting multiple names.

106. **harness CLI E100 — duplicate 'convo' command kills ALL harness
    verbs, including daemon bounce** (lynx, 2026-08-30, during #84 train).
    "cannot add command 'convo' as already have command 'convo'" — the
    global CLI now ships a convo verb (meadowlark's harness convo sync?)
    colliding with a same-named registration in this repo's extensions;
    the collision aborts the whole CLI, so the #84 bounce silently never
    ran and prod sat on a stale binary until manually bounced via pane %50
    (fresh pid 22889 verified by lstart). Misleading-success adjacent: the
    train printed 'bounce: error E100' but health 401 from the OLD process
    looked healthy. Fix: namespace or dedupe the registration; report to
    meadowlark; ALSO the train script should fail loudly when bounce
    errors rather than trusting a 401 that may be the old process.

99-NOTE-2 (2026-08-30, rig delivered): coral's smoke rig lives at
    /Users/jordanknight/substrate/s091-smoke-rig/ (run-smoke.sh +
    example-settings.json + README). Properties: daemon-side isolation
    baked in; fire-3 discriminator; structural gates never wall-time;
    NO fixture data travels (regenerates from SMOKE_SOURCE/SMOKE_SESSION
    you point at — a rig with live conversation content stapled on would
    be a quiet leak). TRAP 4, learned from their s095 false-green: the
    rig CANNOT catch registry-dependent dispatch bugs — synthetic HOME
    removes the pij registry, i.e. a fixture that isolates by removing
    things removes TRIGGERS too; a correct control still passed a broken
    build. Any adopter of this rig must pair it with one non-synthetic
    check that exercises the registry path.

98-NOTE-2 (2026-08-30): cross-product confirmation — harness-engineering
    commit-service:496 is the SAME intent-echo defect found the same day;
    the daemon-key error is the agreed acceptance pattern for both repos.
    The one-field fix both sides want: ingest's job/envelope reports the
    STORE PATH actually resolved. That field alone kills the isolation
    trap for every rig user.

105-CLOSED-NO-DISPATCH (2026-08-30): u2 traced the chain with line
    evidence — Turn::canonical renders trimmed body PLUS every item block
    (core/conversation.rs:233-244,:294-303) and that canonical text is
    what elements/embeds consume; u2's predicate keys on
    Element.raw_text after canonical rendering, so item-bearing
    body-empty turns are RETAINED. The measured poison jobs were
    genuinely contentless. 009's contract sufficient; dajeil's concern
    answered with evidence, not assertion. (Had the predicate keyed on
    body, it would have been silent content-loss wearing a hygiene fix's
    clothes — the verify was worth mandating.)

106-NOTE (2026-08-30): upstream halves landed same-day — harness-eng s093
    (c2050aef): loader now SKIPS a core-verb-colliding extension with a
    degraded row naming both sides, never aborts (repo can never be
    bricked this way again); s095 (ef895a3c): convo-sync dispatch
    false-green fixed (argv + DOA detection + read-back delta). Our side
    (#85 rename) + their side both closed; row 106 fully resolved.

106-CORRECTION (2026-08-30, meadowlark): s095's DOA check proves the
    dispatch child SURVIVED, not that turns landed — every +delta receipt
    today was measured out-of-band, so the envelope's 'delivery is not
    verified' is exactly honest, not underselling. The upgrade (optional
    --verify doing the read-back, raising the claim to DELIVERED; seams
    stay fire-and-forget) is theirs as harness-eng backlog 22.

107. **ingest accepts a NONEXISTENT session ok:true — failure surfaces
    only to a status reader** (dajeil, 2026-08-30, live repro on prod:
    invented guid accepted with dedupe_key + 'queued for ingest'
    next_action; drain leaves ingest_session state=failed x3; index
    stays clean — no bogus conversation created). Third instance THIS
    WEEK of one shape across three systems (conv store accepted-listed-
    unreadable row 100; convo-sync ok while dead child row 106/s095;
    now this): THE WRITER GETS A SUCCESS SIGNAL ONLY A READER CAN
    FALSIFY. Dajeil's law, doctrine-grade: "an accept is a statement
    about the request, never about the thing requested — and every one
    of these surfaces phrases it as though it were about the thing."
    Cheap fix (same argument as #83): validate the session store entry
    EXISTS at accept time and refuse while the caller is still
    listening. EVIDENCE PRESERVED: the 3 failed jobs stay in the prod
    queue as the live repro until this dispatches (dajeil instructed
    not to clear; queue hygiene is o-prime-owned).

108. **TENETS adoption candidates from the harness-eng compare-notes
    close-out** (meadowlark, 2026-08-30; Jordan concurred close-by-
    summary; their retro: harness-engineering prime-governance
    records/retro/2026-08-30-plan-091-and-the-first-fleet-day.md).
    Graduate into pij-team TENETS at next skill revision: (a) the
    packet's law (one rule, not five rows — escalating people -> tool ->
    structure -> adjacent product); (b) the fixture rule (one level above
    probe-sees-opposite; pairs with our Trap 4 note on row 99); (c)
    dajeil's read-the-producer's-log; plus dajeil's accept-law from row
    107. Their honest still-hurts list to steal fixes from: canary/ack
    ritual text under-specifies (two corrections in one day);
    effort requested-never-observed (pij#306); fast/full test-scope
    split letting three truths coexist — scope-aware done-bar is the
    real fix. Standing cross-government agreements: row 98 <->
    commit-service:496 symmetry pact; daemon-key acceptance pattern;
    their row 22 (--verify). UNRULED — quiet-window item, pairs with
    row 81. Day totals for the product ledger: six observers, two
    repos, ~12 rows, "the cost structure was the complaint, never the
    capability."

102/106-FAMILY-NOTE (2026-08-30, u2's DL-003 via owl): second same-class
    instance in one day — a GLOBAL npm CLI update broke every worktree
    mid-run with an error naming ANOTHER repo (transient harness-eng npm
    break during u2's gate). Family: fleet-wide shared-tool blast radius
    with misleading attribution. The s093 loader fix covers verb
    collisions; the npm-transient case wants a pinned-version or
    vendored-CLI story for gates — fold into any row 106-family dispatch.

109. **flaky shared-gate test — summaries_completing_together_produce_one_
    smart_embed_call asserts a BATCHING OUTCOME, racing the debounce
    window against machine load** (owl escalation, 2026-08-30; reviewer-
    era discovery during 009 f-001 fix). Proven flaky by control: same
    tree, same sha, same command — one run FAILED at embed_batch.rs:207
    (2 calls vs 1), a later run passed 5/5; u2's instrumentation showed
    the two calls carried [14,2] items / [238,34] tokens — orders below
    budget, i.e. two smart jobs missed the microbatch window under load,
    not a splitter defect. It nearly cost a correct fix (u2 rightly
    refused to commit past an unexplained red; owl paid the control-run
    cost). Encoding (owl's DL-009): drive the window DETERMINISTICALLY —
    inject the clock or the debounce boundary — never trust wall time;
    same family as w-test-db-isolation's shared-cluster flakiness. Needs
    its own packet and an owner who is not the 009 PM. Doctrine line for
    the retro: "a flaky test makes every green that contains it weaker
    than it looks" — owl's three earlier composed greens contained it and
    were luck, not proof, on that axis.

110. **orphan test-DB accumulation + the sweep spec defect + shared cargo
    package cache** (owl escalation, 2026-08-30; 328 DBs on the shared
    test postgres, container at 75% CPU idle; owl lost TWO composed gates
    to infrastructure and ZERO to code). Three defects, one packet:
    (a) SPEC DEFECT, tenet-17-inside-a-brief: w-test-db-isolation item 3
    said sweep "fs3_test_*" but FreshDatabase mints fs3_<label>_<entropy>
    — a sweep built to the brief WOULD MATCH NOTHING and look like it
    worked; the specification itself carried the defect (prime-authored;
    recorded honestly). (b) cleanup depends on the happy path: destroy()
    is never reached by panicking/timing-out/SIGKILLed tests — durable
    shape is Drop-on-Drop + age-based sweep keyed on the REAL prefix.
    (c) DL-010: per-worktree CARGO_TARGET_DIR isolates artifacts but NOT
    cargo's global package cache — "per-seat target dirs give independent
    gates" is FALSE; four concurrent builds serialize into the harness's
    own timeout, and exit 124 with empty diagnostics reads as RED to
    every agent. Gate must report NO VERDICT (timed out on shared lock)
    distinctly from red. RULINGS 2026-08-30: one-off manual clear of
    unconnected fs3_%_% orphans executed BY O-PRIME (313 dropped; owl
    correctly refused cross-tree drops); sweep mechanism gets its own
    packet + owner (not the 009 PM); timeout-as-no-verdict-retry
    ratified as classification, not rerun-until-green.

110-NOTE (2026-08-31, centipede): the shared cargo package-cache lock now
    has a defect PRODUCER — pij bg cancel reports cancelled while leaving
    the cargo child alive holding the machine-wide lock (pid 73490
    incident; PM hand-killed). Reported to vicuna (pij platform): kill
    full process group + await before claiming cancelled. Same accept-law
    family as row 107.

111. **squash-merge race silently strands late pushes** (owl, 2026-08-31,
    verified by grepping the merged artifact: #87 merged at head 84118ef
    while docs commit 14c6738d was landing — no error, no conflict, the
    branch just keeps a commit main never sees). Silent-loss family,
    beside the DEGRADED-attribution pattern (4x docs-only this session).
    The tell: grep the merged artifact for a phrase you know you wrote.
    Rescue precedent: cherry-pick onto a fresh branch from main (#88).
    Encoding candidates: train re-reads PR head immediately before merge
    and refuses if it moved since evidence was taken; or freeze-window
    rule (PM declares branch frozen when handing to prime).

92-CLOSED (2026-08-31): the 009 element collapse closes the raw+smart
    duplicate-hit defect, CONFIRMED BY THE REVIEWER by execution
    (removing both DISTINCT ON clauses reds the best-score leg 3-rows-vs-1)
    — closed on reviewer confirmation per the side-benefit framing, not
    author claim. Shipped in #87/888eeab.

108-NOTE (2026-08-31): second tenet candidate added from owl's completion
    report: "an isolation mechanism must ship with its reaper, and a
    verdict command must never be able to lie."

112. **harness team tidy leaves a HALF-REMOVED worktree: files deleted,
    registration kept, then refuses as "dirty"** (lynx, 2026-08-31, three
    instances: ask-conv-scope/conv-scope/ddoc-dup-root). First run returns
    DEGRADED having already deleted the working-tree files; the worktree
    stays registered, so a second run reports E_WORKTREE_DIRTY with ~390
    "D" (deleted) paths and demands --force. To whoever finds it next,
    that status is indistinguishable from catastrophic data loss in a
    live tree — I had to verify via merged-PR lookup that nothing was
    lost. Fixes: (a) tidy must be atomic or ordered registration-first;
    (b) a degraded tidy must NAME what it already did; (c) the dirty
    report should distinguish "all files deleted by a prior tidy" from
    "user has uncommitted work". Aggravated by, but not caused by, the
    row-110 disk thrash (each attempt is slow enough to look hung).
    SELF-CATCH, same day I adopted "a verdict command must never be able
    to lie": my own status parse printed "removed" for these because it
    only checked for an error field — I reported three removals that
    never happened. Encoding: never render a success word from the
    ABSENCE of an error; render it from the presence of the effect.

112-NOTE (2026-08-31): the sweep-before-force rule paid for itself within
    the hour. The 009 PM worktree held the reviewer's FINAL DELTA APPROVE
    verdict (f-001 marked fixed, with the exact-200k-token edge probe and
    comment-only batch.rs diff receipts) UNCOMMITTED — main's review record
    stopped at round 1 REQUEST_CHANGES, so the government's record of the
    plan was incomplete and a --force teardown would have destroyed the
    only copy. Rescued as PR #89. Encoding candidate: PM close-out should
    assert a CLEAN worktree (git status empty) as a done-bar item, since
    "the review is recorded" and "the review record is committed" are
    different facts — same family as row 111's stranded commit.
