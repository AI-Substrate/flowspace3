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

113. **CI ran rust-cache TWICE and printed a false error every run**
    (found by vicuna's reviewer 2026-08-31 while adopting our ci.yml for
    pij-rs, returned to us; confirmed by lynx in run 33339025078).
    setup-rust-toolchain@v1 defaults cache:true and runs Swatinem/rust-cache
    internally as its final step, doubling our explicit one. The duplicate's
    POST-step runs `cargo metadata --all-features` after the toolchain tore
    PATH down: every run printed "Error: Unable to locate executable file:
    cargo" while the step reported outcome=SUCCESS. Verdict-cannot-lie
    family AGAIN (fourth variant in two days, after row 111's stranded
    commit, row 112's half-removed worktree + my absence-of-error parse,
    and the PM's tail-exit-status green). Distinct sub-shape worth naming:
    an error printed on a GREEN run is worse than a red — it trains
    everyone to skim, so the real error is the one nobody reads. FIXED:
    PR #90 (cache:false on the setup step, explicit rust-cache kept
    because caching should be readable, not inherited).
    CROSS-GOVERNMENT NOTE: they verified from the action's action.yml
    SOURCE, not its README — the same read-the-producer's-log discipline
    row 108 lists as a tenet candidate. Their reviewer also witnessed our
    lesson #1 (metadata --locked first) catching a real undeclared dep in
    their own review. The CI loan came back with interest.

113-CLOSED (2026-08-31): PR #90 merged. PROVEN by execution, not by the
    merge status: in the fixed run (33354294996) rust-cache instances went
    2 -> 1 and "Unable to locate executable file: cargo" occurrences went
    from every-run -> 0. Gate time unchanged (~5-7min, within normal
    variance). Credit: pij-dominant-vicuna's reviewer.

114. **OUR PRD SPINE IS STALE — nothing forces reconciliation** (surfaced
    2026-08-31 by vicuna's interview about our spine; I went to describe it
    and found it rotted). docs/prd/base-prd.dd.json (governance branch):
    60 rows, 47 checked / 13 unchecked, and the file has ONE commit — the
    governance import. At least three unchecked rows shipped days ago and
    their notes still name retired seats in the present tense: req-0045
    "IN FLIGHT: dingo authoring..." (shipped — .agents/skills/flowspace/
    SKILL.md + crates/cli/docs/agents.md exist), req-0050 "skunk
    prototyping, no report yet" (shipped — crates/cli/src/render/surfaces/),
    req-0053 "QUEUED: dingo unit 2" (shipped in v0.2.0 per roster).
    LINKAGE IS WEAKER STILL: only 2 of 10 plans reference any req id, and
    plan 009 — the biggest of the day — references none. So the spine
    neither knows what shipped nor what work served it.
    FIXES (needs a packet + owner): (a) a plan's done-bar must include
    "names the req rows it advances, and re-states them at close-out";
    (b) a check must cite a RECEIPT (sha/test/run), never a note in prose
    — today's AC-with-receipt discipline applied one level up; (c) a
    reconcile pass that flags any unchecked row whose note names a closed
    seat; (d) decide whether the PRD reconciles at merge or at plan
    close-out, and who owns it (currently: nobody, which is the defect).

114-NOTE (2026-08-31, vicuna): CROSS-GOVERNMENT CONFIRMATION — pij's
    store-native spine cannot answer "which rows did this unit advance"
    either (124k events of mostly daemon chatter, no governance-shaped
    query; three req rows hand-carried that night). So this is a SHARED
    shape, not a flowspace3 defect: two governments with different
    substrates (ddoc file vs event store) both check their top-level
    spine by human judgment while their plan-level ACs carry receipts and
    mutation proofs. Vicuna adopted our (a)/(b)/(d) and the rot detector
    verbatim; strengthens the dispatch case for row 114.

108-NOTE-2 (2026-08-31): third tenet candidate, NAMED by vicuna from our
    exchange — "evidence standards INVERT with altitude": the higher the
    governance altitude, the weaker the evidence we accept (plan ACs get
    receipts + cross-model mutation proofs; PRD rows get a prose note from
    one human). Stated as a defect to detect, not a law to accept. Now
    three candidates on this row: the accept-law (dajeil), the
    reaper/verdict line (owl), and this one.

115. **our own backlog is PROSE, not a ddoc — the most load-bearing
    governance file we have has no state column, no schema, no
    validation** (surfaced 2026-08-31 by vicuna asking for our spine
    TEMPLATE, after Jordan ruled THEIR spine must be a ddoc). This file
    is 1,239 lines of numbered markdown prose. Consequences already felt
    today: row state lives in ad-hoc suffixes I invented as I went
    (-CLOSED, -NOTE, -CORRECTION, -RESHAPE, -CLOSED-NO-DISPATCH), so
    "what is open" is un-queryable and I answer it from memory; there is
    no supersession pointer, so a reshaped row (99) and a corrected row
    (106) look identical to a reader; and nothing validates it.
    Ironic pair with row 114: the PRD is a ddoc that nobody reconciles,
    the backlog is reconciled constantly but is not a ddoc.
    SHAPE TO ADOPT (composed from what we already have): reconstruct/
    spine's beat row {id, ts, kind, title, detail, quote, evidence} —
    chronological, kind already includes `ruling`, quote holds Jordan's
    verbatim words, evidence holds pointers — PLUS prd/requirements'
    state, PLUS a superseded_by pointer (a bare state says a row is
    superseded but not BY WHAT, which is the question a reader has).
    Also missing: no gate validates any ddoc — our `docs` gate only
    checks broken markdown links (.harness/extensions/checks/
    extension.ts), and CI pins ddocs only to RENDER plan docs.

115-NOTE (2026-08-31, vicuna): the sharpest statement of the 114+115 pair,
    theirs, recorded verbatim — "THE DOCUMENT THAT IS RECONCILED
    CONSTANTLY IS THE ONE THAT IS NOT A DDOC, AND THE DDOC IS THE ONE
    NOBODY RECONCILES. Formality and attention went to different files."
    They classify it as the altitude inversion (row 108's tenet) ONE AXIS
    OVER: not evidence-vs-altitude but formality-vs-attention, and both
    governments independently picked the same wrong pair. Corollary for
    the 114/115 packet: do not simply ddoc-ify the backlog and reconcile
    the PRD — ask FIRST which file actually carries the attention, and
    put the formality THERE. Their spine schema (spine beat row + state +
    superseded_by pointer + phases) comes to us when it validates, per
    the symmetry pact, schema file included.

116. **`ask` has no --path/folder scope, though `search` does** (Jordan's
    question 2026-08-31; he first asked this on 2026-08-30 — "were you
    able to filter it to the path?" — and it was never dispatched: row 85
    shipped conversation+source pinning, not path). MEASURED NOW:
    `flowspace3 ask` accepts --repo, --source (code|doc|conversation|all)
    and --conversation <guid>; `--path` is REJECTED ("unexpected argument
    '--path'"), while `flowspace3 search` takes --path <GLOB>. The ask
    AGENT can path-filter internally (its search tool takes globs — that
    is what row 63 observed it doing), but the CALLER cannot pin a whole
    run to a folder, so "answer this using only crates/store/**" is
    unexpressible. Dispatch shape: add --path <GLOB> to ask with the SAME
    hard-binding bovid built for --conversation (immutable filter bound
    to the tools, every model-issued search inherits it, contradictory
    scope refused not silently broadened, coverage facet names the
    narrowed corpus), and reuse row 63's path_unmatched honesty so an
    unsatisfiable glob says so instead of reading as absence.

75-NOTE (2026-08-31): possibly the SAME ROOT as pij's pre-bind/registration
    window. Vicuna logged our false-death-notice report as their req-0003
    and spotted it is adjacent to their req-0002/B-1 — one window, two
    faces: there a pre-bind row is never adopted (a stamped rpc_port
    strands on a phantom row); here the spawn-id is tombstoned before
    registration resolves. Our 75(b)/(c) — E-AMBIG "cannot detect a
    current session" on a REGISTERED omp seat — smells like the third
    face of the same window. If vicuna closes that window, re-test row 75
    before dispatching anything for it; it may close for free.

117. **009's PREDICTED RISK FIRED IN PROD: bytes/3 token estimation
    under-counts, so an unchunked input still exceeds the 8192 cap**
    (lynx, 2026-09-01, discharging ac-0007's observation leg myself after
    krill died). The impl-guide's risk #2 said exactly this — "a
    pathological input could still exceed the true tokenizer cap; keep
    the cap-WARN as the LAST-RESORT guard, its firing post-merge is a
    regression signal" — and it fired.
    MEASURED: 4 failed embed jobs, all 2026-08-31 09:12-11:43, i.e. AFTER
    the 009 deploy (08-30 19:18Z). ONE distinct source, retried 4x, 44
    items. Provider says "Invalid 'input[0]': maximum input length is
    8192 tokens". Max item = 20,872 CHARS -> bytes/3 estimates ~6,957
    tokens, UNDER the ~7,500 window, so chunk_plan never split it — but
    the real tokenizer counts >8,192. Implied true ratio for this content
    is <2.55 chars/token vs the assumed 3.0. Source is pij repo raw code.
    FIX OPTIONS (needs a packet): (a) lower the assumed chars/token to a
    conservative floor (cheap, costs some chunk efficiency everywhere);
    (b) measure real tokens with a tokenizer for inputs near the bound
    (accurate, adds a dependency); (c) treat the provider's cap rejection
    as a SIGNAL and re-split that input automatically instead of failing
    it (self-healing, and the only option that survives a future
    tokenizer change). (c) plus a conservative (a) is my recommendation.
    CONTEXT, so this is not read as 009 failing: in the same window
    470,520 embed jobs completed with ZERO errors and ZERO empty-string
    rejections — the hygiene half of 009 is working exactly as designed.
    This is one residual estimation gap on one source, not a regression.

009-ac-0007 DISCHARGED (2026-09-01, lynx — krill died before reporting, so
    o-prime ran the leg per the original ruling that this leg is mine).
    VERDICT: PASS WITH ONE NAMED RESIDUAL.
    - Hygiene (ac-0004/0005): PASS. 470,520 embed jobs done with ZERO
      errors; ZERO "input cannot be an empty string" rejections in the
      post-deploy window; krill's pre-death baseline additionally
      observed an OLD empty payload dropped at batch assembly with its
      call completing — the second layer meeting real pre-existing poison
      and surviving it.
    - Budget (f-001 fix): PASS. No call over the 200k budget observed;
      max 29,256 tokens in krill's sample.
    - Cap (chunking): RESIDUAL — row 117, the impl-guide's own predicted
      risk, fired 4 times on one source. Named, measured, not hidden.
    The plan's headline promise holds for everything except inputs whose
    real token density beats the bytes/3 estimate; row 117 closes that.

118. **doctor_daemon CLI tests race CREATE DATABASE under concurrent
    suite processes** (ask-path-scope coder DL-006, 2026-08-31). With an
    isolated FS3_TEST_DATABASE_URL, `cargo test -p fs3-cli` runs its
    integration binaries concurrently and five doctor_daemon cases raced
    on pg_database_datname_index; the same binary passed 7/7 with
    --test-threads=1. So the DEFAULT proof command is not race-free —
    an innocent seat gets a red it must reason its way out of, same
    family as row 109's flaky gate. Encoding: make doctor test-DB
    creation idempotent under concurrent processes, or pave the CLI
    suite with the serialization it needs so the default command is
    trustworthy. Row 110 family (test-DB provisioning).

DEGRADED-ATTRIBUTION TALLY (2026-09-01): now FIVE instances — four from
    the 009 PM (all docs-only) and one from the ask-path-scope coder
    (DL-007: connected ingress probe, no refs/notes/ai entry within the
    bound, telemetry-nudge found no buffer to replay). Consistent shape:
    the probe says connected and the note never lands, so the commit is
    made but authorship is unrecorded and git-ai may later attest those
    lines as human. Its encoding is the best stated yet: make the
    collector probe and the note verification share ONE measured
    readiness state, or retain/replay commit events when a connected
    probe is followed by a bounded note miss. Route to the git-ai owners.

119. **an explicit `conv:` address is silently WORKTREE-scoped, and the
    miss claims global absence** (lynx, 2026-09-02, measured while
    answering meadowlark's read-back question). `flowspace3 get
    conv:<guid>#t1` resolves only conversations anchored to the worktree
    you are standing in; anything else returns FS3-E-QUERY-NOT-FOUND
    with the message "no conversation <guid> is indexed" — which is
    FALSE, since `conversation list` shows that same guid one command
    earlier. MEASURED: conv:8c285d65 (flowspace3 repo, anchor worktree
    fs3-embed-split, since tidied) -> NOT-FOUND from the flowspace3 main
    checkout; ok:true with `--repo all`. Also NOT-FOUND for any pij or
    dd conversation until `--repo all` or the matching cwd. Two things
    are wrong: (a) `get`'s own help says "from the INDEX, so it answers
    for every registered repository, not only the one you are standing
    in" — the shipped behaviour contradicts the documented contract; (b)
    the error asserts global absence for what is a scope decision, and
    does not name the flag that widens it. This is the verdict-cannot-lie
    family (107/111/112/113): a probe that cannot distinguish "absent"
    from "out of my scope" is a probe that reports data loss. It nearly
    cost a peer government 250 wasted backfill dispatches — meadowlark's
    `--verify` would have read "never delivered" for conversations
    sitting in the index. ENCODING: an explicit `conv:<guid>` is
    ADDRESS-AUTHORITATIVE (this is row 101, now with a measurement
    behind it — my recommendation to Jordan is that a guid the caller
    typed in full is not a search, and scope should not silently narrow
    it); failing that, the miss must say "not in this scope" and print
    the widening flag. Consider also a first-class `conversation verify
    --harness <h> --session <id>` so the guid derivation lives on our
    side of the wire instead of being reimplemented by every client.

120. **`status` counts failed jobs and no verb will name them** (lynx,
    2026-09-02, hit while sizing meadowlark's backfill). `flowspace3
    status` reports `{kind: embed, state: failed, count: 5,
    with_error: 5}` and `last_error` for ONE of them. There is no verb
    that lists WHICH jobs failed, or their dedupe keys, or their
    attempt counts. To answer "is the conversation corpus affected, or
    only code?" I had to psql the prod database by hand and read
    `select dedupe_key, attempts, terminal from jobs where
    state='failed'`. An operator without database access cannot answer
    that question at all, and an AGENT — our actual audience — certainly
    cannot. A count without an identity is the same defect as a verdict
    without a reason. ENCODING: `flowspace3 doctor jobs` (or `status
    --failed`) listing failed/terminal jobs with dedupe key, kind,
    attempts, and the truncated error. Cheap; unblocks every future
    "what is stuck" question. Row 119's family.

121. **P1 — fs3's OWN `--pij` seat route resolves through a legacy-only
    door, so 250 of 252 live rs seats cannot be ingested** (lynx,
    2026-09-02, measured after meadowlark's packet named the harness
    half of it). pij has split into two daemons — legacy (TS, store
    `~/.pij/<id>.json`) and rs (Rust, 127.0.0.1:7461, `~/.pij-rs/
    pij.sqlite`), with one `pij` binary routing BY VERB. `flowspace3
    conversation ingest --pij <SEAT>` resolves the seat by shelling out
    to `pij sessions --json` (crates/daemon/src/convo_ingest.rs:773) —
    and `sessions` is legacy-routed. MEASURED on this machine:
    `pij sessions --json` = 1198 rows; `pij-rs list` = 252 seats; rs
    seats resolvable through our join = **2 of 252**. Meadowlark
    measured 245 of 248 unresolvable from the harness resolver's end
    (`readPijRegistry`, `~/.pij` only). Same defect, two ends: both
    readers are legacy-only views of a store that split underneath
    them, and the failure is SILENT — the seam discards an unresolvable
    identity by design, so nothing reaches intake and no error appears
    anywhere. Their index result: 45 conversations, zero from any pij
    worktree.
    MY EXPOSURE: 1 rs-resident seat in a flowspace folder today, so my
    corpus is healthy by luck, not design — the next omp boot or `pij
    adopt` moves a worker of mine to rs and it silently stops ingesting.
    ALSO EXPLAINS row 116's coder friction (CONF-001, DL-002, CONF-002,
    DL-003 — two BLOCKING): `pij whoami` said E-AMBIG while `pij adopt`
    refused claiming the seat was already reachable. I read that as a
    registration papercut. It was rs answering for a legacy seat. A
    contradictory recovery loop seen from a coder with no idea two
    daemons exist.
    RULING (o-prime, subject to Jordan): fs3 OWNS intake-side identity —
    we already hold the seat route, so fixing it here gives every
    harness client the fix for free and keeps store knowledge in one
    place. CONDITION: fs3 must not learn a second private store layout
    either. Preferred fix is a generation-agnostic identity contract
    FROM PIJ (`pij sessions --json` unioning both stores), which makes
    our change a version floor and a test; the fallback — unioning
    `pij sessions` + `pij-rs list` inside the daemon — is strictly worse
    because then two codebases encode the split. Meadowlark carries the
    ask to pij o-prime (still-weasel); the answer decides the packet's
    size. Their backfill of ~250 seats waits on this AND on row 117.

ROW 117 ESCALATED (2026-09-02): now FIVE failed embed jobs, and the
    fifth — `embed:conv:recovery:raw:c5a6be2d…`, dated 2026-09-01 — is a
    CONVERSATION job, not code. So the estimation gap is live on the
    exact content class a peer government is about to send us ~250 seats
    of. Prod corpus for scale: 45 conversations / 55,144 turns /
    embeddings table 3.9 GB / database 7.3 GB; meadowlark's backfill is
    plausibly a 5x increase in the turn corpus, and every oversize turn
    in it lands silently in the failed bucket (see row 120 — we cannot
    even enumerate them without psql). Row 117 is therefore no longer a
    named residual, it is BLOCKING A PEER GOVERNMENT'S WORK, and goes
    to the front of the dispatch batch. Fix option (c) — treat the
    provider's cap rejection as a signal and re-split automatically —
    confirmed as the shape I want, because it survives the next
    tokenizer change instead of re-tuning a constant.

ROW 119 GAINS A NAMED CONSUMER AND A DELIVERABLE (2026-09-02, after
    meadowlark carried A1/A2). `conversation verify --harness <h>
    --session <id>` is now a REQUIRED part of row 119's packet, not an
    optional addition. Contract agreed with the consumer, who will hold
    us to it: exit 0 + `ok:true` => delivered; a DISTINCT not-indexed
    error code => not delivered; and it must be **repo-unscoped by
    construction** — row 119's `--repo all` trap must be impossible to
    hit in this verb, not merely documented around. Rationale: the guid
    derivation (convo_ingest.rs:342, with the forced version/variant
    nibbles) belongs on OUR side of the wire; every client that
    reimplements it is a client that can drift from it silently.
    Meadowlark has committed to shipping NO `--verify` at all until this
    exists, rather than ship one that can be held wrong — so this verb
    is on a peer government's critical path, same as row 117.

ROW 121 AMENDED — WE CANNOT FIX THIS ALONE TODAY, AND MUST NOT TRY
    (2026-09-02, weasel's contract answer via meadowlark:
    `pij/.harness/temp/weasel-identity-contract-answer.md`). Two facts
    that change the packet:
    1. **No generation-agnostic identity verb exists.** `pij whoami
       --json` returns E-RS on every seat. The sanctioned surfaces are
       `pij-rs list` (rs) and `pij state <id> --json` / `~/.pij/<id>.json`
       (legacy).
    2. **rs rows carry NO inner session id.** This is the decisive one.
       My stated fallback — union `pij sessions` + `pij-rs list` inside
       the daemon — WOULD NOT WORK EVEN IF I BUILT IT: an rs row cannot
       be resolved to a native session at all right now. The fallback
       was not merely "strictly worse", it was impossible, and I did not
       know that when I offered it. Recording the correction because the
       reasoning, not just the conclusion, is what a future seat needs.
    Filed upstream as pij **req-0033**: rs rows gain `session`, and
    whoami answers under rs. Meadowlark has sent the union ask in our
    preferred shape — `pij sessions --json` returning legacy union rs
    with `{id, harness, session, folder, generation}` — as req-0033's
    concrete scope, and relays the answer when it arrives.
    STANDING INSTRUCTION until then: **row 121 is a version floor in
    waiting, not a packet.** Do not build a second store reader in fs3.
    Any seat that picks this up should verify req-0033 has landed
    before writing a line.
    MEANWHILE the exposure is unchanged and unmitigated: an rs-resident
    seat's conversation is silently not ingested. Meadowlark's resolver
    routes around it for CLAUDE seats by reading the native session env
    (`CLAUDE_CODE_SESSION_ID`, 16 rs seats resolvable today with no pij
    dependency) and handing us `--session`/`--harness`, which our
    existing native route already serves — so that path needs nothing
    from us. pi/omp seats stay dark until req-0033.


ROW 121 — SECOND UPSTREAM ROW (2026-09-02): pij **req-0034** minted by
    weasel for the spawn half — `pij spawn --bin omp` from a legacy prime
    mints an rs child that cannot message its parent, and E-RS tells the
    child its parent "never registered" and to `pij adopt`. Hit live by
    the plan 010 coder (pij-general-limpet) on its canary; it followed the
    packet's file fallback and stopped cleanly. Interim ruling (o-prime):
    file + poll for child→prime, `pij-rs send` (with --msg-id) for
    prime→child; NOT adopting o-prime's own pane into rs (identity split,
    cuts off legacy peers). Encoded in government/pij-two-daemons.md.

ROW 121 — CLAUDE RS SEATS NOW INGEST, PROVEN IN OUR INDEX (meadowlark,
    2026-09-02, harness-engineering #190 merged at 74017086): an rs
    claude seat with no ~/.pij descriptor ran harness commit in a
    consenting worktree; `conversation list --path <worktree>` went
    0 -> 1 (session 06e6dd44…, 3 turns), canary phrase present in all
    three turns via `get --repo all`. Negative control: a commit with no
    resolvable identity printed the ONE stderr line, exit 0, envelope
    untouched. So the dark-fleet class is now: pi/omp rs seats only,
    until pij req-0033. (Their omp negative control never ran — an
    `pij-rs spawn` from a main checkout died before binding, proc null,
    pane gone — reported to weasel by them.)

122. **P1 — search is DB-bound and the database is running on POSTGRES
    DEFAULTS: shared_buffers 128MB against a 2.3GB HNSW index** (lynx,
    2026-09-02, from two coders' frictions: `flowspace3 search` hitting
    the 60s client timeout; my own reproduction 120s from a worktree
    cwd). MEASURED, idle-ish host: one search = 13.4s wall, of which
    >=10s is ONE postgres statement — `WITH candidate_vectors AS
    MATERIALIZED (SELECT source_hash, source_kind, chunk_no, vector …`
    (the 009 nearest CTE). Container: `shared_buffers=128MB`,
    `work_mem=4MB`, `effective_cache_size=4GB`, resident 432MB of a
    31GB limit; `embeddings_1024_vector_idx` = 2,286MB, 283k rows. The
    index cannot live in 128MB of buffers, so every query walks it from
    disk through the OS cache — and under host load (load avg 38-53:
    OrbStack 197%, Defender/DLP ~70%, a `bfs` at 90%) that becomes
    60-120s. The daemon process itself sat at 0.2% CPU throughout: this
    is not a daemon bug and the client timeout misattributes it as one.
    Two fixes, both cheap, in order: (a) INFRA — tune the compose
    postgres (shared_buffers ~4GB, work_mem 64MB, maintenance_work_mem
    for index builds) and prove the delta with the same query; (b)
    QUERY — the MATERIALIZED candidate CTE pulls 1024-float vectors
    into a temp set before the collapse; check whether the collapse can
    run on (source_hash, chunk_no, distance) and fetch vectors never —
    009 u3's own review said search.rs showed no diff, so this is the
    store CTE. (c) HONESTY — the search envelope should carry
    daemon-side timing so a slow answer is attributable to the DB, the
    host, or the client. Baseline needed: was 13s the pre-009 number?
    Nobody measured it (that absence is row 110's family: no latency
    sensor). Blocks nothing that is dispatched but degrades every seat.

123. **jobs table holds DUPLICATE failed rows under one dedupe key, and
    boot's requeue would collide on them** (found by the plan 010 coder
    limpet while planning the ac-0005 drain, 2026-09-02; verified by
    o-prime read-only). `jobs_live_dedupe_idx` is UNIQUE only WHERE
    state IN ('pending','running'), so two rows with the same key can
    both sit in `failed` — jobs 1316706 and 1323215 both carry
    `embed:git:github.com/AI-Substrate/pij:raw:043365681f…` (attempts 3,
    terminal false, 2026-08-31 09:27 and 11:43). `requeue_failed`
    (jobs.rs:506-534) updates every non-terminal failed row to pending
    and "skips live duplicates" — but when BOTH rows of a pair qualify in
    one statement the second insert into the live index collides, and
    the boot sweep fails or half-applies. Two duplicate keys among
    non-done jobs today. How a second failed row was minted under a key
    that already had one is the real question (mint path does not check
    failed rows?) — that is the store fix. ENCODING: (a) requeue picks
    ONE row per key (DISTINCT ON / MIN(id)) and marks the rest terminal
    with a reason; (b) mint refuses/absorbs when a failed row already
    exists for the key; (c) a doctor row that counts duplicate keys. The
    plan 010 drain (ac-0005) has this as a NAMED PRECONDITION; the repair
    at the bounce is o-prime's on Jordan's GO. Row 120's family (no
    verb can even show this without psql).

124. **INCIDENT 2026-09-02 22:48Z — prod postgres backend crash + 20s
    recovery during a coder's gate; the gate's prod tripwire reported
    "absent" for what was UNREACHABLE** (lynx, characterised read-only).
    Sequence from the container log: heavy per-run test-DB churn
    (`checkpoint starting: immediate force wait` bursts = CREATE/DROP
    DATABASE from concurrent suites) under host load ~124; at 22:48:35
    `server process (PID 3147563) exited with exit code 2` → postmaster
    `terminating any other active server processes` → crash recovery
    (`Consistent recovery state has not been yet reached`, WAL redo of
    two test-DB drops) → `ready to accept connections` at 22:48:55. The
    container never restarted (RestartCount 0). Prod data INTACT after:
    22 migrations, 52 conversations, 237,058 elements, 289,459
    embeddings, daemon health ok, search ok. The daemon's queue kept
    working. Zealot's `harness checks` tripwire, probing inside the
    window, printed `a test run changed the PRODUCTION database
    (version=22 -> absent)` and STOPped — the coder did exactly right and
    lost a gate to it. TWO DEFECTS: (a) the tripwire cannot tell
    "dropped" from "unreachable" — a guard that asserts data loss on a
    connection failure is the verdict-cannot-lie family (107/111/112/
    113/119) at its most expensive: it will train seats to distrust a
    guard we NEED them to trust; fix: probe must distinguish connection
    error from a successful read of 0 rows, and say which; (b) per-run
    test DBs on the PROD container — every coder's suite creates and
    drops databases on the same postmaster that serves :7373, so test
    churn under load can (and did) take prod down for 20s. Fix: a
    separate test postmaster (compose service `db-test` on another port)
    is the isolation the guard was pretending we had; until then, one
    DB-heavy job at a time (serialized gates, ruled today). Row 110/118
    family. Backend exit code 2 root cause not determinable from logs.

125. **dot-directories are excluded at INDEX time, so `--path .pi/**`
    (and any hidden tree) is unsearchable — and the honest
    `path_unmatched` hides the real cause** (pij coder via weasel, plan
    124 DL-002, 2026-09-02; characterised read-only by lynx). Measured
    against the pij root: `worktree_files` rows with path LIKE '.pi/%'
    = **0**, `tree .pi` → NOT-FOUND, while `.pi/extensions/` holds real
    source on disk (file-watch-notify, image-see, minih-workbench…). So
    the glob is not the defect — the scanner never indexed the tree.
    `.pi/extensions` is pij's actual product surface for extensions;
    `.harness/` (our governance + extensions), `.agents/skills/`,
    `.github/workflows/` are the same class: dot-prefixed directories
    that ARE the codebase. Blanket hidden-dir exclusion is wrong for
    agent repos. ENCODING: (a) index dot-directories by default, keeping
    an explicit deny-list (`.git`, `target`, `node_modules`, and
    `.gitignore`-derived); (b) when a --path glob matches nothing, the
    `path_unmatched` detail should say WHETHER the prefix exists on disk
    but is excluded by an index rule — "not indexed (hidden dir rule)"
    vs "no such path" — row 119's two-messages principle again; (c) a
    `flowspace3 tree` row / doctor line listing what the index rules
    skipped for this root. Detail file:
    ~/pi-hacking/pij-worktrees/pij-governance/.harness/government/observations/2026-09-02-s124-hawk.md
    ROW 125 POINTER (lynx, same hour): this is an EXISTING knob, default
    off — `[scan] include_hidden = false` (crates/core/src/config.rs:1123,
    :1138) feeding `discovery.rs:827 .hidden(!settings.include_hidden)`.
    So the packet is small: (a) flip the default to true with the
    explicit deny-list above, or expose it per-root on `add`; (b) the
    path_unmatched honesty line "prefix exists on disk, excluded by
    include_hidden=false" — the knob's name in the message is the fix
    for the next person; (c) a config-show line so a root's scan rules
    are readable. Until then, operators can set include_hidden = true
    and rescan; the daemon-side effect on prod (a full rescan of every
    root's dot-trees) needs Jordan's GO because it is load.

ROW 124 ROOT CAUSE (from the plan 010 reviewer, pij-fiscal-tick, who
    disclosed it unasked): the crash at 22:48:35 followed ITS OWN
    `cargo test -p fs3-daemon --test oversize` — 12 `#[tokio::test]`s,
    each `FreshDatabase` issuing `CREATE DATABASE`, so ~12 concurrent
    CREATE DATABASE hit the shared postmaster mid-checkpoint on a host
    at load 124. Same crash shape is in the container history FOUR
    times: 2026-08-27 10:03 (signal 6 during `CREATE DATABASE
    fs3_migrations_…`), 2026-08-28 07:09 and 07:12 (exit code 2), and
    tonight. So it is not the load alone — it is our own test helper.
    The reviewer ran the remaining suite at --test-threads=2 and it was
    clean. Row 126 carries the fix.

126. **FreshDatabase can take down the fleet's database: concurrent
    CREATE/DROP DATABASE from one test binary crashes the shared
    postmaster** (reviewer fiscal-tick DL-001, 2026-09-02; four
    occurrences on record — see row 124). FIX, in `crates/testkit`:
    (a) serialise CREATE/DROP DATABASE behind a process-wide lock (or a
    small semaphore) inside FreshDatabase — a helper must not be able to
    do this by default; (b) `fresh_database.rs:46`'s panic advice
    "Start it with: docker compose up -d" must distinguish "server
    closed the connection / in recovery" from "no server configured" —
    today it sends the agent straight into the container-name collision
    (row 110). (c) longer-term: a separate test postmaster (row 124b).
    Small, self-contained, high leverage: every coder's gate touches it.

ROW 117 / PLAN 010 REVIEW RECORD (2026-09-02): PR #92 APPROVE WITH NOTES
    at 6377a1fe — three findings, all ruled FOLD-IN: f-0001 MAJOR
    (`OpenAiCompatEmbedder` — the `openai_compat` and `github_copilot`
    kinds — shares the 8192 cap but never classifies it, so the heal
    never runs there; one line); f-0002 (latent release-assert panic
    and a false ratio print if MAX_HEAL_ROUNDS is ever tuned); f-0003
    (the cap number baked into the matched string). Also ruled: alignment
    SHIPS (risk #3 gate, explicit); a clean prod drain does NOT exercise
    the heal (the prod item splits by alignment alone) — the fixtures are
    the heal's proof. Pre-bounce evidence captured to
    scratch/plan-010-drain/ (five keys with original last_error, status
    envelope) because `requeue_failed` overwrites last_error at boot.

PROCESS NOTE, 2026-09-02 (o-prime against itself): the plan 010 reviewer
    handed the coder a "trap" — "openai_compat.rs has TWO try_post
    implementations; if in doubt patch both" — and I relayed it to the
    coder APPROVINGLY, calling it the line that justified the file.
    There is exactly one `try_post`; the second rejection site is the
    chat path, and "patch both" would have mis-classified chat errors.
    The reviewer caught it by re-grepping its own handover before
    standing down and retracted by file; the retraction reached the
    coder BEFORE it edited that file (verified: its diff had not touched
    openai_compat.rs). Two lessons, both encodable: (1) verify-then-relay
    applies to REVIEWER text, not only coder claims — a plausible trap
    that reads well is precisely the shape that survives a relay; I
    should have grepped `fn try_post` (one command) before forwarding;
    (2) the receipt discipline the packet imposes on the author (cite
    the command, not the prose) belongs in the reviewer packet's
    handover section too. Adding it to packet-reviewer's i4 in the
    pij-team templates at the next drain.

ROW 119 / PLAN 011 REVIEW RECORD (2026-09-02): PR #93 REQUEST CHANGES at
    3a7124ba (330c0077 docs-only) — three findings, all measured, all
    ruled FIX in the PR. F-0001 MAJOR: `ask --conversation <guid>` now
    resolves index-wide but `with_corpus` drops the resolved anchor and
    `search_filtered` still binds cwd repo/worktree, so a foreign-repo
    pin and an UNANCHORED pin (the default `conversation import` shape,
    repo NULL) return ok:true + an answer + zero hits + a billed loop —
    where pre-PR the same call refused loudly at zero tokens. A new
    member of the verdict-cannot-lie family, introduced by the plan
    meant to close one. F-0002 MAJOR: the new catalog code fell through
    the suffix mapping to HTTP 500 — a correct negative indistinguishable
    from a dead daemon for the delivery-prober consumer; one-word rename
    to …-NOT-FOUND → 404. F-0003: the `payload_in_scope` guard (PR #84's
    whole compensating control) was defended only through a Flag-scoped
    test where it is never reached; reviewer neutered the guard and 36
    tests stayed green. TRUE: ac-0001/2/4/5; mutation redder than
    claimed; census complete; HTTP verify rejects ?repo/?cwd with 400.
    LESSON for the template: "unscopable by construction" must be
    tested on EVERY transport, and a 'resolves' change must be paired
    with a 'retrieves' assertion — resolution without retrieval is the
    silent-empty shape.

PLAN 010 DELTA REVIEW (2026-09-02): APPROVE at 3606c139,
    no_material_findings. Reviewer reverted each of the three fold-ins
    individually and confirmed its own new test — and only it — went red
    (compat classification; overlap clamp → panic; ratio → the false
    "1 byte/token" reproduced verbatim; parsed cap → 4096 test red); the
    heal-arm mutation re-performed on the rewritten arm still red 3/3;
    cap_rejection 4→7; alignment numbers unchanged; impl-guide 17→0
    errors; enrichment.md's two gaps closed. Two of the coder's fixes were
    BETTER than the reviewer's prescription and are recorded as
    deliberate: the overlap clamp lives inside chunk_plan_bytes (so no
    caller can violate the invariant), and the exhaustion message states
    two measured numbers instead of a quotient. Reviewer also recorded a
    wrong first revert against itself. #92 to merge train; bounce HELD on
    Jordan's row-123 GO.

PLAN 011 DELTA REVIEW (2026-09-02): APPROVE at a80e9a5. All three
    fixes defended by their own tests; f-0002 needed TWO mutations
    because the code-string assertion fires before — and masks — the
    wire-status assertion the finding was about (lesson: assert the
    thing the finding names, in the order it can be masked). The seam
    the f-0001 fix CREATES (pinned mode now runs with the tool scope
    wide open, leaving `guard_address` as the sole confinement) was
    hunted unprompted and holds: foreign turn and bare foreign
    conversation refused, pinned transcript readable. Explicit --repo
    mismatch still refuses loudly.

127. **`meta.scope` on a pinned ask reports the PRE-widening scope**
    (reviewer top-sloth f-0014, 2026-09-02, explicitly non-blocking).
    After #93, a pinned `ask --conversation` widens its retrieval scope to
    the resolved transcript, but `http.rs` builds `meta` before widening
    (:167-169, attached :222), so the envelope can say `scope.repo =
    <cwd repo>, source = cwd` for a run that read index-wide inside a
    foreign transcript. Model-facing `scope_line` is correct;
    `coverage.corpus.conversation.guid` names the true corpus. scope.rs's
    own doc says the field exists "so the scope is never something a
    consumer has to infer". Cheap: build `meta` from the widened scope,
    or omit `scope.repo` when a pin widened it. Row 119 family.

128. **the empty-content hash (e3b0c442…) still rides in RECOVERY embed
    batches** (plan 010 coder limpet, 2026-09-02, from prod job 1344012's
    payload: six hashes, five doc sections, the sixth the empty hash).
    Plan 009 filters empties at mint (turn/element enqueue) and drops them
    at batch assembly; `requeue_missing_vectors` (enrich.rs ~485-496, the
    `conv:recovery` placeholder identity) minted a job carrying the empty
    hash anyway — so the mint-side filter has a third entry point it does
    not cover. Assembly-side drop presumably saved the call (the job
    completed), which is the defense-in-depth working; but the mint gap is
    real and cheap: apply the same predicate in the recovery enqueue, plus
    a test that a missing-vector sweep over a corpus containing the empty
    hash mints no job for it. Also: ac-0006's plan premise was wrong
    (`conv:recovery` read as "a conversation job") — recorded in the 010
    receipts as an amendment, not hidden.

ROW 121 — REQ-0033 IN FLIGHT AS PIJ PLAN 128 (weasel, 2026-09-02; relayed
    by Jordan). FROZEN consumer contract, build against nothing else:
    `pij sessions --json` → rows `{id, harness, session, folder,
    generation:"legacy"|"rs", lifecycle?, transcriptPath?}`, union of both
    stores, deduped by id; `pij-rs list` / `/v1/seats` rows gain
    `session` (nullable) + `generation:"rs"`; `pij whoami --json` under rs
    → `{id, harness, pane, folder, session, generation:"rs",
    capabilityGate:"absent"}`; pi/omp seats send their native session id
    at registration. Envelope `v` bumps so we pin a floor. Plan:
    ~/pi-hacking/fs3-seat-session-identity/docs/plans/128-seat-session-identity/plan.dd.md.
    OUR PACKET (row 121, now buildable when the merge sha lands): in
    convo_ingest.rs `pij_sessions()` / fs3_core::SessionRow, (a) parse the
    v-bumped shape, (b) pin the floor and refuse older envelopes with a
    message naming the pij version needed, (c) the `--pij` route works
    for rs seats end-to-end — proof: `conversation ingest --pij <rs omp
    seat>` on one of OUR coders, then `conversation verify` for it.
    FIELD-NAME NOTE sent to weasel: today's rows are `pijId` /
    `harnessSessionId` / `gitCommonDir`; the frozen shape says `id` /
    `session` / `folder`. Either name works, but it must be one shape
    under one `v`, and `folder` must mean what `gitCommonDir` meant (the
    seat's git dir) or say so.
    ROW 121 CONTRACT AMENDED (weasel, same hour): rows KEEP
    `pijId/harness/harnessSessionId/gitCommonDir` (+lifecycle/boundModel/
    spawnedBy/transcriptPath) and ADD `generation` only; gitCommonDir
    keeps its meaning. One shape, one v. So our packet shrinks to: parse
    `generation`, pin the v floor, and prove the rs route end-to-end.

129. **a conversation's anchor cannot be corrected by re-ingest, and
    ingest accepts a --folder that does not exist** (meadowlark's
    backfill pilot, 2026-09-02: 43 of 51 delivered claude transcripts are
    anchored to worktree `/Users/jordanknight/pi/hacking/pij` — weasel
    de-slugged Claude's project-dir name by turning every '-' into '/',
    so `pi-hacking` became `pi/hacking`; the folder does not exist, repo
    resolved to NULL, and `list --repo pij` shows 10 instead of ~50).
    Two fs3 facts, read from source: (a) `upsert_conversation` DOES
    overwrite worktree/repo on conflict (COALESCE(EXCLUDED, old)) — but
    convo_ingest.rs:660-680 only calls it when the poll READ RECORDS; a
    re-run with the correct --folder against an unchanged transcript
    reads zero (cursor), skips the header upsert, and leaves the wrong
    anchor in place silently; (b) ingest never checks that --folder
    exists on disk — Claude's slug is lossy (`pi-hacking` and
    `pi/hacking` both slug to `-pi-hacking`), so the transcript was found
    while the anchor was garbage. ENCODING: (a) on an explicit --folder
    that differs from the stored anchor, upsert the header even on an
    empty poll (or add `conversation reanchor <guid> --folder`); (b)
    refuse a --folder that is not a directory, with the slug it would
    have resolved to in the message; (c) `conversation list` should flag
    anchors whose worktree does not exist on disk. TODAY'S PATH for the
    43: `conversation remove <guid>` then re-ingest with the correct
    folder (remove drops turns + cursor, so re-ingest re-reads and
    re-anchors; costs re-embedding those turns).

130. **`harness daemon bounce` returns ok before the daemon serves; the
    CLI's DAEMON-UNAVAILABLE fix text would double-start prod** (o-prime
    DL-006, 2026-09-02; Jordan hit it with `flowspace3 add .` during the
    #93 bounce). New daemon listened on :7373 within seconds but did not
    answer /health for ~2 min (boot requeue + pending work under load);
    bounce had already reported ok; the CLI told a human to `flowspace3
    daemon &`. ENCODE: bounce waits (bounded) for /health and otherwise
    reports "booting: <pid>, <pending jobs>"; the fix text checks for a
    listening pid and says "booting — retry" instead of "start it".

131. **`harness checks` is opaque for minutes — slow vs stuck cannot be
    told apart** (limpet DL-004, 2026-09-02, at host load 124). ENCODE:
    stream the active stage (fmt/clippy/test-<crate>) with elapsed time;
    a gate that cannot say where it is trains seats to bypass it.

132. **`harness team tidy` refuses after a SQUASH merge with
    E_BRANCH_NOT_MERGED — a false verdict for the only merge shape this
    repo uses** (o-prime, 2026-09-02, tidying plans 010/011). It tests
    ancestry (`git branch --merged`), which a squash never satisfies; the
    content WAS on main (verified by `git diff main <branch> -- crates`
    = empty). ENCODE: tidy checks content by patch identity (`git cherry main
    <branch>` / patch-id — a plain diff against main is ALSO wrong once
    main has moved past the branch, as o-prime found a minute later), and when it refuses it must say
    "diff vs main is non-empty in <paths>" rather than "not merged".
    Row 112's sibling — a tidy that lies in either direction.

133. **conversation `repo_identity` lands as the RAW origin URL when the
    folder is not a registered worktree; and a direct `--session agent-*`
    fails with a misleading "no session file"** (meadowlark's re-anchor
    run, 2026-09-02: 3 sessions whose worktree no longer exists on disk
    anchored to `https://github.com/AI-Substrate/pij.git`, splitting
    `list --repo` 37 vs 3). SOURCE: `ingest()` computes `remote =
    remote_url(&folder)` RAW on purpose for the git-ai metrics scope
    (convo_ingest.rs:942) and passes the same string as the header's
    `repo_identity` (:662); `upsert_conversation` canonicalises only via
    the `canonical_anchor` CTE (registered worktree). Row 100 fixed the
    registered path + backfilled; this is the unregistered path.
    ENCODE: (a) header `repo_identity` = `RepoIdentity::from_remote(raw)`
    (keep raw only for the metrics scope), (b) migration backfill for
    rows already raw, (c) `ingest --session agent-*` should say
    "subagent sessions are ingested through their parent — re-ingest
    <parent>" instead of "no session file" (claude.rs:199-210 knows the
    shape). Restore path documented in scratch/reply-to-meadowlark-subagents.md.

134. **P2 — `conversation import` accepts a file it cannot parse and
    stores HOLLOW turns with ok:true** (meadowlark, 2026-09-02, trying to
    self-serve a subagent restore): fed Claude's NATIVE session JSONL
    (types user/assistant/attachment) with --guid/--repo/--worktree →
    ok:true, accepted=104, verify turns=104 — and every turn empty ("the
    stored turn has no prose or typed items"); positive control on a
    properly ingested conversation shows 955-char bodies under the same
    probe. import's contract is fs3's transcript shape, not a harness's
    native store (that is `ingest`'s job) — but it neither says so nor
    refuses. Verdict-cannot-lie at the INTAKE: 104 empties reported as a
    delivery. ENCODE: import validates the record shape up front and
    refuses with "this looks like a <harness> native session — use
    `conversation ingest --harness <h> --session <id>`" (the first line's
    `type` field is a giveaway); never store a turn with no body and no
    typed items; verify's success shape should carry a non-empty-turn
    count so a hollow conversation cannot verify clean. Sibling of rows
    129/133; the restore path for the 12 is the parent re-ingest in
    scratch/reply-to-meadowlark-subagents.md.

BACKFILL CLOSED (meadowlark, 2026-09-02): pij repo 4 → 49 conversations
    (37 re-anchored + 12 subagents restored via the parent route, turns
    8-82 each, real content) plus 3 under the raw https identity awaiting
    row 133's backfill. Every answer that made this possible was a
    source-read with line numbers, and every one was checked by the
    consumer before use — that is the standard for cross-government
    answers from here on.

135. **`tree <dir>` fans out one row per worktree with no way to tell them
    apart** (forward-worm, external dogfood, 2026-09-02, voxel repo with
    11 roots under one identity): `tree godot/…/Config --json` →
    total=24, entries=169, every file 11x; entry keys are only
    [address, kind, name, path] — no `worktree`. `search` scopes to the
    cwd's checkout and stamps `worktree` on every hit; `tree` does
    neither, and `total` vs `showing` disagreeing is the envelope
    showing its own fan-out. ENCODE: tree scopes like search (cwd
    worktree, `--repo all` to widen) and carries a `worktree` field per
    row. Raw findings vendored: scratch/dogfood-forward-worm-batch-1.md.

136. **C# and GDShader are unparsed, and `tree <file>` with zero children
    gives a bare zero with no reason** (forward-worm, same batch). `.cs`
    files index as file-vectors only (34 hits, all kind=file, zero
    element rows — no grammar); `.gdshader` is not scanned at all
    (`path_unmatched`). For that repo the shader IS the product; a naive
    user read "composition code=0" as "code not indexed" for ten minutes.
    Two halves: (a) HONESTY — `tree <file>` with zero children must say
    "no parser for .cs; file-level only" the way `refs` says "successful
    empty answer" (row 119's two-messages principle, again); (b) SUPPORT
    — tree-sitter has C# and there is a GLSL-adjacent grammar for
    gdshader; the add-language skill exists for exactly this. (a) is
    small and first.

137. **compaction-summary turns read as first-hand findings** (forward-
    worm F5 nit): the `smart` summary of a compaction turn (t4423 in
    their transcript) presents second-hand content as if fresh. ENCODE:
    tag turns whose source record is a compaction/summary so search
    hits and `get` render them as "summary of earlier turns", not
    testimony. Small; touches conversation ingest + smart summariser
    prompt.

ROW 120 SECOND WITNESS: forward-worm F6 — scan_file pending pinned at 463
    for ~7 min with no per-job view, so "queued behind summarize" and
    "stuck" are indistinguishable from `status`. Same encoding as 120
    (`doctor jobs`), now asked for by an external user.

TRANSPORT DIAGNOSIS (weasel, 2026-09-02, Jordan's question "why do
    lynx's messages arrive via tmux while meadowlark's come through the
    Rust daemon"): the legacy TS daemon has NO Claude socket transport at
    ANY commit — every legacy delivery to a Claude seat is tmux
    send-keys, BOTH directions. The asymmetry was the recipient's
    composer: meadowlark's accepted the trailing Enter; lynx's swallowed
    it in the paste-detection debounce (same class Jordan hit by hand
    this morning). Legacy daemon restarted anyway (was c8d99b13, 607
    commits stale; now d5b8be5e). Native turns come only from rs.
    o-prime's decision on `pij adopt` into rs: GATED on one fact — after
    adopting, can an rs pane still deliver to legacy-only seats
    (pij-telegram is the merge-veto channel; meadowlark is legacy)?
    Asked weasel. Yes → adopt today (rs children can then `pij send` me;
    file+poll retires). No → stay legacy until pij plan 128 surfaces
    both ids under one consumer row. Meadowlark's evidence:
    ~/pi-hacking/pij/.harness/temp/meadowlark-delivery-asymmetry-report.md;
    weasel's diagnosis: …/weasel-diagnosis-delivery-asymmetry.md.

138. **P2 — ask's tool view of a file element is cut at 7,000 chars
    SILENTLY, so the agent re-fetches the same truncated file until the
    iteration budget dies** (forward-worm dogfood batch 2, 2026-09-02,
    mechanism measured). CloudPresetLibrary.cs is 8,058 bytes; `get`
    returns all of it; ask's tool result truncates at 7,000 (line 205)
    and the method the question needs (`ParseFileName`, :206) sits in
    the cut tail. Result: scoped and unscoped asks both hit
    FS3-E-QUERY-ASK-ITERATION-LIMIT — 8 iterations, ~44.5k tokens,
    115–154 s, no answer — after finding the RIGHT file in iteration 1
    and re-reading its truncated view seven more times. A narrower ask
    answered from the unit tests and NAMED the truncation itself (the
    honest half). ENCODE: (a) the tool result must say "7,000 of 8,058
    chars shown; tail not available" so the agent stops re-fetching —
    row 119's family, at the model-facing seam; (b) a windowed read
    (`get … --lines A-B` / byte range) available to the ask agent, which
    makes unparsed-language files usable at all; (c) this is the concrete
    cost of row 136(b): for ask, every unparsed-language file over 7k is
    unreadable past the fold. Goods recorded from the same batch: the
    partial-evidence envelope carried the 4 citations + iteration ledger
    + the config knob (row 71 working as designed); `ask --conversation
    <guid>` checked against the user's own written brief — three-dials
    model and four out-of-scope items correct, zero hallucinated rulings
    ("best result of the day"). Raw: scratch/dogfood-forward-worm-batch-2.md.

RS INBOUND PROVEN (2026-09-02): the 012 coder's `pij send
    pij-binding-magpie 'RS-PING…'` arrived at o-prime as a NATIVE Claude
    cross-session-message over the uds socket (/tmp/cc-socks/…) — not
    tmux send-keys, no swallowed Enter. So: rs child → o-prime is now a
    real turn; o-prime → rs child is `pij-rs send`; o-prime → legacy
    peers is `PIJ_DAEMON_GENERATION=legacy pij send`. The packet
    contract keeps the file channel as the durable record and adds the
    rs pointer as the instant one. req-0034's blocking half is routed
    around at o-prime's seat; the spawn defect itself remains weasel's.

ROW 122 CORRECTED BY MEASUREMENT (investigator pij-purring-orangutan,
    2026-09-02, read-only, 742 s time series + 1 s activity sampler +
    EXPLAIN ANALYZE on prod; report vendored at scratch/db-cpu-profile/).
    The stated cause — "128MB shared_buffers vs a 2.3GB HNSW index, every
    query walks it from disk" — is FALSE for search: the HNSW scan is
    12.4 ms / 1,078 buffers and the query hits buffers at 99.7%. The real
    cause: the admission `EXISTS` in crates/store/src/embeddings.rs:557-
    620 (`… OR (source_kind='smart' AND EXISTS (SELECT 1 FROM
    smart_content candidate WHERE candidate.text_hash = e.source_hash
    AND candidate.raw_hash = admitted.raw_hash))`) plans as a nested-loop
    semi-join over a spilling Materialize of a Seq Scan on elements —
    **962,792–1,698,017 smart_content index probes per search**, 3.8–6.8M
    buffer hits, 1.7–2.7 s each, ×up to 9 via the candidate-expansion
    loop (that is the 13 s / 60 s / 120 s). 69% of ALL database CPU in
    the window; smart_content_text_hash_idx measured at 124,551 scans/s.
    JIT adds 281 ms/query on a nonsense cost estimate. NEW TOP PACKET
    (code, no restart): resolve smart_content text_hash→raw_hash once as
    a join and hash-semi-join the admission; carry (source_hash,
    chunk_no, distance) only in the CTE. Acceptance: EXPLAIN loops
    <1,000, search wall <500 ms on prod's corpus, ranking parity on the
    existing fixtures. shared_buffers still worth raising — for row 139,
    not for this.

139. **`queue_depth()` full-scans 1.01M jobs rows on 3 cores every ~6.5 s,
    and nothing ever deletes done jobs** (investigator, same report).
    jobs.rs:569 has no WHERE; measured Parallel Seq Scan, read=114,185
    blocks (892 MB) off disk per call, ~260 ms CPU, called by
    report_progress every 5 s AND by GET /status — 27% of active DB
    samples, the dominant source of ~200 MB/s disk read. 1,009,934 of
    1,016,092 rows are `done` with no retention path anywhere; the table
    doubled TODAY. Also: jobs has never been autovacuumed and is 6,549
    dead tuples from a first vacuum of a 2.15 GB relation with 64 MB
    maintenance_work_mem (latent spike). Stale comment at jobs.rs:558-
    560 names an index that no longer exists. ENCODE: (a) retention —
    purge done jobs older than N days (or move to a history table); (b)
    progress/status use a live-only count (index-only on
    jobs_live_dedupe_idx, 0.77 ms measured); (c) fix the comment. Row 120
    family; no restart.

140. **Postgres config bundle — NEEDS RESTART, Jordan's GO** (investigator):
    shared_buffers 128 MB → 4 GB; shared_preload_libraries =
    pg_stat_statements + CREATE EXTENSION (the investigator hand-rolled
    a sampler because it is absent); work_mem 4 → 64 MB;
    maintenance_work_mem 64 MB → 1 GB; effective_cache_size → 16 GB;
    effective_io_concurrency 200; random_page_cost 1.1; max_wal_size
    1 → 8 GB + wal_compression on (SIGHUP-reloadable); track_io_timing
    on. Container uses 535 MiB of 31 GiB. Buffer pool turns over every
    5.0 s; backends do 66% of their own evictions. Modest win for search
    (CPU-bound), solid win for row 139 and bulk ingest.

141. **Our test suite's DROP DATABASE forces a checkpoint every 23.6 s on
    the PROD postmaster — an FPI death spiral** (investigator): 917
    `immediate force wait` checkpoints in 6 h (bursts of 65/min) vs 54
    timed; 836 requested vs 11 timed; 28% of wall-clock in checkpoint
    writes; 2.03M full-page images / 12.67 GB WAL in 2.2 h; WAL pinned at
    max_wal_size; 25.6% of active DB time stalled on WALSync/WALWrite.
    Each CREATE DATABASE under PG16 WAL_LOG strategy also logs the whole
    template (~8.7 MB × ~900). 56 leaked fs3_* DBs (~490 MB). This is
    rows 126 + 124b + 110 measured as ONE mechanism: tests must have
    their own postmaster; until then serialise (012, in flight), reap,
    and raise max_wal_size. Row 012's ac-0004 gains a check: forced
    checkpoints in the log window must drop to ~0 with the lock in.

RULED OUT with evidence (same report): the 7 idle hass-mcp containers,
    fs3-linuxtest, buildkit = 0.00% CPU each; OrbStack's own overhead
    <1% (its big numbers are postgres CPU + block I/O billed through the
    VM wrapper — 1.51 TB read / 793 GB written in 5 days); the HNSW
    index; jobs_remaining(); the 135–158 s timed checkpoints
    (checkpoint_completion_target=0.9 working as designed). THE MACHINE:
    flowspace3-db = 0.51 cores avg, 2.1 peak, 3.2% of 16 cores, while
    load ran 28→76 at ~14% total CPU — the load is 1,309 processes (210
    node) from the fleet, not the database.

DEGRADED-ATTRIBUTION TALLY: SEVEN (2026-09-02, plan 012 coder mad-crocodile,
    docs receipt commit 05d7d87c — connected ingress, refs/notes/ai note
    missing after 5 s). Six of seven are docs-only commits from rs-resident
    omp seats. Still routed to the git-ai owners; the shape has not changed.

PLAN 012 REVIEW RECORD (2026-09-02): PR #95 CHANGES REQUESTED at 5c7f7bdb
    (05d7d87c docs-only) — 1 HIGH, 3 MEDIUM, every one executed not read.
    f-1a01 HIGH: `cargo test -p fs3-store` still issues unserialised
    CREATE/DROP through 107 sites in a duplicate helper
    (crates/store/tests/support/mod.rs) the lock never reaches —
    measured 25 forced checkpoints in a 38 s store run vs 2 in oversize;
    fix = move the semaphore into fs3_store::{create,drop}_database.
    f-1a03 MEDIUM: the widened sweep now force-drops any aged
    fs3_<label> DB including a LIVE sandbox — the reviewer refused to run
    `harness checks` for that reason (correct); fix = numbackends=0.
    f-1a02: auth-refused told "wait and retry". f-1a04: the concurrency
    test asserts the primitive not the create path. ac-0005 cannot be
    run as specified: list_orphans_from has zero callers → an examples
    binary. SCOPE TRUTH recorded: the semaphore is per PROCESS; N seats
    each gating against one postmaster still produce N concurrent
    creates — row 126 is REDUCED by this plan, not closed; the fix that
    closes it is row 124b (separate test postmaster), promoted to its
    own packet next. All four ruled FOLD IN.

124b. **A separate test postmaster** (promoted from the 124/126/141
    family, 2026-09-02): compose service `db-test` on another port
    (5434) with its own volume; FS3_TEST_DATABASE_URL defaults to it;
    `harness checks` and every coder gate point there; prod's postmaster
    never sees a CREATE/DROP DATABASE again — removes rows 141's 917
    forced checkpoints/6 h and the row-124 crash class entirely, and
    makes the row-110 orphan sweep a test-server concern. Compose + one
    config default + docs; no prod restart.

ROW 140 APPLIED (o-prime, 2026-09-02, under the "restart whenever — get the
    work done" ruling, at a flat queue): ALTER SYSTEM on the live volume —
    shared_buffers 128MB→4GB, work_mem 4→64MB, maintenance_work_mem
    64MB→1GB, effective_cache_size 4→16GB, effective_io_concurrency
    1→200, random_page_cost 4→1.1, max_wal_size 1→8GB, wal_compression
    on (pglz), track_io_timing on, shared_preload_libraries =
    pg_stat_statements + CREATE EXTENSION. Container restart: postgres
    ready in 2 s; daemon reconnected on its own; health 401; 100 roots.
    First bare search after: 7.4 s (modest, as the profile predicted —
    search is CPU-bound on the plan, plan 013 owns it). Compose mirror
    opened as a PR so a fresh volume starts identical. pg_stat_statements
    is now the profiling surface every future DB question uses first.

SKILL-TEXT DEFECT, THIRD SEAT (2026-09-02, plan 013 coder, BLOCKING
    stop-and-ask): the builder implement module says `node_modules/.bin/dd`
    (or ddocs) for task-state mutation; this repo's CLI is the global
    `ddocs` on PATH. Three seats in one day stopped on it (012 coder,
    011 coder, 013 coder) — correctly, because hand-editing ddocs is
    forbidden. ENCODE NOW, not at the drain: (a) the pij-team coder
    packet template i3 names the global `ddocs` explicitly (done in
    o-prime's generator for future packets); (b) Jordan's builder skill
    text (`~/.claude/skills/builder/references/stages/60-implement.md`)
    needs the path fixed — outside this repo, flagged to Jordan.
    ROW 140 CLOSED (2026-09-02): compose mirror merged as #96 (main
    7cea955); live values applied by ALTER SYSTEM earlier; a fresh
    volume now starts identical. No bounce needed (infra only; the
    running postgres already carries the values).

ROW 141 / PLAN 012 — A WRONG METRIC, CAUGHT BY A CODER'S RED (2026-09-02):
    o-prime and the reviewer set "forced checkpoints must fall hard from
    25" as the f-1a01 delta target. The coder ran the full store suite
    serialised: 83 forced checkpoints over 4 m 15 s (137 tests), 0
    recovery lines — RED against the tripwire — and stopped without
    explaining it away. The metric was wrong: DROP DATABASE forces a
    checkpoint by design; serialisation removes OVERLAP between drops,
    so a serialised run legitimately logs MORE lines than a parallel one
    whose drops coalesced (the reviewer's 25 were over 38 s windows).
    The lock's promise is crash-free + bounded concurrency; the
    checkpoint VOLUME is row 141 and only row 124b (separate test
    postmaster) reduces it. Target withdrawn; replaced with: 0 recovery
    lines + the N=8 create-path bound through the store primitive; counts
    reported not gated. LESSON for the reviewer-packet template: a
    "number must fall" target needs a stated mechanism by which the fix
    lowers it, or it is a coalescence artefact waiting to embarrass
    someone.

142. **pij-team is not portable: the team extension hardcodes `fs3-`,
    the templates reference a schema and a TENETS path that only exist
    inside flowspace3** (pij-lonely-antelope, chainglass o-prime, first
    consuming repo, 2026-09-02 — three findings with fixes). (a)
    `.harness/extensions/team/extension.ts` hardcodes the worktree
    prefix `fs3-` at :168, :486, :519, :668 — in another repo it mints
    `fs3-<slug>` beside a clone called `chainglass`, and `tidy` then hunts
    `fs3-<slug>_*` docker volumes that were never created and REPORTS
    SUCCESS while leaving volumes behind (a tidy that lies, row 112/132
    family). Antelope's fix: derive the prefix from `basename(root)` —
    `<clone>-<slug>` — via a `treePrefix()` helper; verified with
    `--propose` (chainglass-<slug> on 094, found a hand-made 093 in the
    ordinal scan). Diff offered; ACCEPT it upstream. (b) consuming repos
    have no `.dd/schemas/pij-team/`, so `ddocs validate` on the
    impl-guide fails "schema pij-team/impl-guide was not found in any
    discovery root" — ship the schemas with the templates or name the
    copy step in the skill. (c) template refs default to
    `.agents/skills/pij-team/TENETS.md`, which does not resolve in a
    consuming repo — violating the skill's own "every ref must resolve
    in the seat worktree" rule; vendor TENETS into assets/inputs/ by
    default. Same shape all three: the templates assume they run inside
    flowspace3. pij-team's stated end state is absorption by
    harness-engineering; these are the first three portability
    findings and they came from real use, which is what the prototype
    was for.

ROW 126 — THE CRASH MECHANISM, MEASURED AT THE SERVER (reviewer cheetah,
    2026-09-02): sampling pg_stat_activity (~5/s) for active
    CREATE/DROP DATABASE during `cargo test -p fs3-store` at default
    parallelism: 634 samples, **MAX CONCURRENT DDL = 16**, 177 samples
    with >1 in flight — against the postmaster that serves prod. The
    oversize suite, already behind the FreshDatabase lock: 437 samples,
    MAX = 1, zero above 1. That is the promise of plan 012 measured
    directly and it is the f-1a01 delta target (16 → 1); the 83-vs-25
    forced-checkpoint comparison in the previous note is STRUCK as
    incomparable (different suite, duration, drop count) — checkpoint
    counts are row 141/124b texture, never a gate. Sixteen concurrent
    CREATE/DROP explains the four crashes better than any checkpoint
    volume.
DEGRADED-ATTRIBUTION TALLY: EIGHT (plan 012 delta commit f3aec311, 2026-09-02).
    ROW 126 CORRECTION (reviewer cheetah, same hour, against itself): the
    16 is CONTAMINATED — the probe counted every CREATE/DROP on the
    postmaster while five worktrees were using it (013's
    search_plan_shape seeding, 014's status_retention, the store suite's
    own fs3_migrations_ helper); a guarded oversize re-run read 2 with a
    neighbour's 1 in it. What 16 honestly means: total DDL concurrency
    the SHARED postmaster saw during one suite on a busy box — which is
    row 141/124b's problem statement, not ac-0001's per-process
    promise. Instrument fix in progress: `application_name` on the test
    URL (sqlx parses it; maintenance_url preserves the query string) and
    filter pg_stat_activity on it. Target 16 → 1 is PROVISIONAL until
    attributed numbers exist. Also observed: 7 leaked `fs3_migrations_`
    databases from concurrent store runs — the store test support leaks
    on its own (row 110). LESSON (third instrument lesson today): a
    server-wide counter cannot prove a per-process property on a shared
    server; attribute or do not gate.

143. **INCIDENT 2026-09-02 ~02:06–02:45Z — second prod postgres crash,
    then DISK FULL took OrbStack down and with it BOTH postgres ports**
    (o-prime, live). Sequence: (1) plan 013's EXPLAIN ANALYZE of the OLD
    pathological search query on a 50k-element seed ran 648 s on the
    shared postmaster while sibling seats issued DDL; backend `exited
    with exit code 2` at 02:06:05Z → crash recovery → ready at
    02:06:43Z; data intact (22 migrations, 101 conversations). (2) o-prime
    stood up the separate test postmaster (row 124b) as compose
    `db-test` on :5434 (PR #97) and moved every seat to it. (3) Within
    minutes `/` hit 96% full (587 MiB free): five flowspace3 worktrees
    each carrying a full cargo target (17+11+8.2+7.8+5 GB ≈ 45 GB) plus
    ~48 GB of pij worktree targets in ~/pi-hacking plus ~170 GB of
    unexplained same-day growth; OrbStack's docker socket vanished, so
    :5433 AND :5434 died; the daemon answers /health on its pool but
    every query returns FS3-E-STORE-QUERY-FAILED — flowspace3 is DOWN
    for users while this stands. (4) Fleet frozen twice; o-prime deleted
    two targets (10 GB), weasel told to reap pij targets; a disk agent
    (w-disk-space) spawned to find the ~170 GB, restore OrbStack, and
    reap in a ruled order; free space 0.6 → 79 GB at the time of this
    note, OrbStack still down. CAUSES named: row 110 (per-seat
    CARGO_TARGET_DIR duplicates a full workspace build per seat — the
    reviewer's line: "the same crates compiled and stored five times";
    fix = shared target + sccache or cargo-sweep on tidy), row 124b
    (done today), and whatever the agent finds. Second-order lesson: a
    reaper that runs on a schedule is the only version of row 110 that
    survives a day like this.
    ROW 143 ROOT CAUSE (disk agent pij-partial-coral + weasel, same
    hour): `~/.orbstack/log/vmgr.log` 12:09:18Z — "block req failed:
    write failed @ 43483119616: StorageFull" → BTRFS transaction abort →
    "VM stopped". The OrbStack VM was killed by HOST disk-full, not by
    anything a seat did. Biggest single consumer: the VM image
    `~/Library/Group Containers/HUAQ24HBR6.dev.orbstack/data/data.img.raw`
    = 142 GB on disk (2 TB sparse) holding the 128.8 GB of docker Local
    Volumes (100 GB "reclaimable" per the profile). Reclaimed so far:
    o-prime 10 GB (two targets), weasel 68 GB (16 pij targets of merged
    branches), disk agent's ~/pi-hacking sweep; free 0.6 → 95 GB.
    o-prime restarted OrbStack (`orb start`); prod postgres recovered
    from the unclean stop with data intact (22/101/323k/327k); test
    postgres :5434 healthy; daemon bounced. Volume prune (non-pgdata)
    is the agent's next step, after prod is confirmed serving.

144. **a scoped zero's reason lives in `next_action`, not
    `meta.empty_because`** (antelope, chainglass dogfood batch 1,
    2026-09-02; confirmed as a well-formed ok:true envelope). A consumer
    branching on the DOCUMENTED field sees a reasonless zero. Contract
    bug: every honest-empty must populate `meta.empty_because` (the
    machine field) and MAY repeat it in `next_action` (the human field).
    Row 119/138 family — the honesty is present but in the wrong slot.

145. **`--source code` is the whole ballgame in a doc-heavy repo, and
    agents-start-here never says the word** (antelope, same batch):
    unscoped, the right file (`pij-records.ts`) is ABSENT from results;
    `--source code`, it is a 1.0 hit. Ranking/composition observation,
    not availability (both legs returned). ENCODE: (a) the agent guide
    and `search --help` teach `--source` in the first screen; (b)
    composition facet on every search says how many code/doc/
    conversation hits were in the candidate pool so a user can SEE the
    doc flood. (Antelope's finding 2 — pool exhaustion on unscoped
    search — was DOWNGRADED by its author: measured inside the outage
    window; re-measure after ALL CLEAR.)
    ROW 143 RECOVERED (02:27Z): OrbStack started by o-prime; prod postgres
    recovered from the unclean VM stop with data intact (22 / 101 /
    323,417 elements / 326,966 vectors); test postgres :5434 healthy;
    daemon bounced, serving after ~52 s (log: 100 roots watched, embeds
    and summaries flowing); one bare search 11.05 s. CLEARED sent to the
    four seats with receipts and ALL CLEAR to the two peer primes, from
    the verification task itself so the clear could not precede the
    proof. Total user-facing outage of queries: ~02:06Z (second crash)
    with intermittent service, then hard down from the VM stop until
    02:27Z.
    ROW 124b NOTE: the test container's initdb was cut short by the
    OrbStack death, so its pg_hba.conf lacked the entrypoint's
    `host all all all scram-sha-256` line — host TCP connections from
    OrbStack's gateway (192.168.97.1) were refused (SQLSTATE 28000) and
    two seats stopped correctly. Appended + reloaded; proven with a
    CREATE/DROP over TCP from a container. If the volume is ever
    recreated, the entrypoint will add the line itself; if it is ever
    missing again, this is the fix.
    ROW 145 / ROW 122 — RE-MEASURED ON A HEALTHY STORE (antelope, 2026-09-02,
    same query, alternated, all exit 0): scoped `--source code` 38 s / 13 s;
    UNSCOPED 185 s / 156 s. Finding 2 corrected for the record: unscoped
    search is NOT pool-hungry (that was the outage) — but scope is a
    4–12× LATENCY lever in a doc-heavy repo, and under store pressure the
    3-minute query is the one that loses the connection race. So row
    122's "5–15 s" envelope holds only for scoped queries; the unscoped
    DEFAULT — what every new agent types first — is 10× outside it. Plan
    013's ac-0005 wall-time proof must therefore include an UNSCOPED run
    on a doc-heavy root (chainglass), not just the flowspace3 bare query.
    ROW 145 / 122 — FINDING 2 WITHDRAWN ENTIRELY (antelope, third attribution
    in one session, all wrong): a later probe measured the SAME unscoped
    query on chainglass at 6 s (vs 156–185 s twenty minutes earlier) and
    flowspace3's bare search at 106 s — the uncontrolled variable was
    DAEMON LOAD (the post-recovery queue was still draining under the
    "all clear"). No version of "scope is a latency lever" or "doc-heavy
    repos are slower" is supported; carry NONE of it. Findings 1 and 3
    stand (ranking observation; empty_because vs next_action). The
    consequential fact for plan 013's ac-0005 remains: wall-time proofs
    must record `status` queue depth before and after each timing, and
    interleave siblings — a timing taken while the queue drains measures
    the queue. LESSON (antelope's words): "an all-clear is a statement
    about availability, not about load". The extra 013 acceptance run
    (unscoped on a doc-heavy root) STAYS, but as a load-controlled
    measurement, not because of a repo effect.
    ROW 126 — ATTRIBUTED BASELINE, TARGET GROUNDED (reviewer cheetah,
    2026-09-02, on :5434 with application_name attribution, pre-fix sha
    5c7f7bdb): UNGUARDED store suite max_concurrent_ddl = 16 (n=2,
    identical: 308 and 700 samples, 52 and 43 samples above one);
    GUARDED oversize suite = 1 (522 samples, 357 active, zero above one).
    The attributed 16 equals the earlier contaminated 16 — foreign DDL
    was present (foreign_ddl_max=1 both runs) but was not what produced
    the number. The correction was still right: a binding number the
    instrument could not attribute was not evidence, and "accidentally
    correct is not the same as being able to show it" (cheetah). The
    16 → 1 delta target is no longer provisional. Artefact: a partial
    failing run during the pg_hba gap still showed 15 concurrent
    unguarded DDL. Post-fix number next, taken in place (no second
    worktree — no second target dir).

146. **P1 — `ask` has no wall-clock deadline: 180 s unscoped / 100 s
    repo-scoped with NO envelope, killed by the caller** (alpaca,
    post-restore dogfood, 2026-09-02). Search on the same words returns
    in 17 s, so retrieval is not the cost; the loop after retrieval
    never returns and the iteration/token budgets (8 / 80k) do not bound
    wall time when each iteration's search takes 15–100 s. A verb with
    no deadline is a verdict that cannot arrive. ENCODE: `ask` gets a
    wall-clock budget (config, default ~120 s) and on expiry returns
    the SAME partial-evidence envelope the iteration limit returns
    (`FS3-E-QUERY-ASK-DEADLINE`, citations so far, iteration ledger,
    the knob). Row 71 family. Raw: scratch/dogfood-alpaca-post-restore.md.

147. **P1 — TypeScript symbol extraction produces NOTHING repo-wide:
    every .ts file is a bare file element with `children: []`** (alpaca,
    same batch; VERIFIED by o-prime read-only on prod: elements joined
    to .ts paths — chainglass 11,286 file elements / 0 non-file;
    harness-engineering 7,237 / 0; pij 5,893 / 0). Consequences, all
    silent: `tree <file.ts>` → 48 s to return an honest-looking
    `entries: [], total: 0`; `refs <symbol>` → 0 for symbols with 5+
    references; and this is the real mechanism behind antelope's
    finding 1 ("doc-heavy repo: unscoped search never surfaces the
    source file") — TS code has no element granularity, so document
    sections dominate the candidate pool. Either the TS grammar is not
    wired in the parser set, or extraction regressed; either way three
    TypeScript repos in the index have no code symbols. ENCODE: (a) fix
    extraction (tree-sitter-typescript exists; the add-language skill is
    the recipe); (b) `tree`/`refs`/`get` on a file with zero children
    must say "no symbols extracted for .ts (no parser / parser error)"
    — row 136(a) generalised; (c) a doctor row: languages present in
    the index by file count vs languages with a parser. Row 136 family;
    supersedes it in priority.

    Also from the batch (goods): verify 0.02 s honest negative; `get
    conv:#t --repo all` reliable throughout the backfill; 101
    conversations survived the outage intact.
    ROW 147 ROOT CAUSE (o-prime, source read): NOT a regression — only
    THREE grammars are wired in crates/parsers (tree_sitter_md,
    tree_sitter_python, tree_sitter_rust). discovery.rs:132 lists ts/
    tsx/swift/sql/svelte/vue/zig/… as DISCOVERABLE extensions, so those
    files are indexed as bare file elements and never parsed. Every
    non-Rust/Python codebase in the index — three TypeScript governments,
    the C# game (row 136) — has no symbol granularity: no tree, no refs,
    no element-level search, and document sections dominate ranking.
    PACKET (next after the perf pair): wire tree-sitter-typescript (+tsx)
    via the add-language skill, prove on harness-engineering's
    `resolveConvoIdentity` (refs ≥ 5, tree non-empty), and ship the
    "no parser for .<ext>" honesty line in the same PR; then C#/GDShader
    (136) on the same rail. A doctor row listing "extensions indexed vs
    parsers wired" is part of it so this cannot be invisible again.

    ROW 144 UPGRADED TO SYSTEMIC (antelope batch 2, 2026-09-02):
    `meta.empty_because` is ABSENT on every zero produced across
    `search --path` and `refs`, while `next_action` carries an excellent,
    precise reason in both — the information exists in the human field
    and is missing from the machine field the contract names. One fix,
    every honest-empty path: populate `meta.empty_because` first, mirror
    into `next_action`. (Praise recorded: `ask --path` answered a real
    plan-093 question with correct constants, the coalescing rule and
    the design intent — grounded, 7 iterations, 2 correct citations —
    "the strongest thing in the product"; `conversation verify` "a model
    of the honesty contract, keep it exactly as is".)

148. **`conversation list` rejects `--limit` which `search` accepts**
    (antelope batch 2). Small: add `--limit` to list for verb symmetry.

149. **agents-start-here must say "parse stdout only; stderr carries a
    human copy of the error"** (antelope batch 2 — the same trap caught
    forward-worm, antelope, and o-prime today): errors print the JSON
    envelope to stdout AND a human line to stderr; an agent that merges
    2>&1 reports a malformed envelope. One paragraph in the guide and in
    `--help` for `--json`. Raw: scratch/dogfood-antelope-batch-2.md.

ROW 143 ADDENDUM — SEAT LOSS: pij-mad-crocodile (plan 012 coder) lost its
    rs event stream in the disk incident (weasel: omps started before pij
    plan 124 do not re-attach; req-0042 filed for "queued must name its
    reason" + a live-subscriber flag in pij-rs list). Its work was safe:
    f3aec31 committed locally, tree clean, not pushed. o-prime killed the
    seat and respawned in place with the full reply set to read;
    successor's canary pending.
    ROW 143 / ROW 110 — THE 170 GB ATTRIBUTED (disk agent pij-partial-coral,
    2026-09-02, report vendored at scratch/db-cpu-profile/disk-space-report.md):
    TODAY's growth is cargo target/ sprawl — ~25 per-worktree target dirs
    totalling ~106 GB, every mtime inside 48 h (flowspace3 main 17.6G,
    s122 8.6G, fs3-search-admission 8.5G, fs3-jobs-retention 8.0G, s121
    5.9G, s110 5.6G, …) across flowspace3 AND pij governments. The docker
    volume pool (128.7 GB, 100 GB reclaimable) is a 10-MONTH standing
    debt, not the spike — newest non-pgdata volume 2026-08-27,
    fs3-cargo-target 21 GB from 08-26, dind/vscode sets from 2025-11;
    only today's four *_flowspace3-pgdata volumes are new and three are
    0 B. OrbStack death proven host-side (vmgr.log 02:09:18Z StorageFull
    → BTRFS abort → VM stopped). Freed by the agent: ~38.8 GB of stale
    targets + 23.3 GB of caches (npm _cacache 16.5 G→2 MB, uv 3.7 G,
    Xcode DerivedData 1.3 G, ShipIt 1.8 G); by weasel 68 GB; by o-prime
    10 GB; Jordan purged personal media separately. Volume prune of
    non-pgdata volumes proceeding, names listed first. THE ENCODING IS
    ROW 110, now with the number that justifies it: one shared
    CARGO_TARGET_DIR per repo (+ sccache) and a cargo-sweep reaper on
    worktree teardown — a full workspace build is ~8–17 GB and today
    there were 25 copies.
    ROW 143 — DISK AGENT FINISHED (pij-partial-coral): 39 dangling
    non-pgdata volumes removed (listed in the report), builder prune
    4.97 GB, image prune 9.9 GB — docker accounting 128.7 → 30.3 GB
    volumes, 13.4 → 3.5 GB images, ~113 GB reclaimed INSIDE the VM; host
    df rose only +14 GiB because OrbStack's sparse data.img.raw (142 →
    128 G) returns freed btrfs space to APFS on trim or the next VM
    restart — the one thing left to watch: if host free does not rise
    ~99 GB after OrbStack's next restart, the image needs an explicit
    reclaim. Protected throughout: every *pgdata* volume (prod 9.96 GB,
    LINKS 1; test). Held back deliberately (stateful, 0.5 GB total):
    jk-claw_caddy_*, minih-otel_lgtm-data, 028-server-mode_uploads.
    Three observations captured (du 512-byte blocks; two reapers on the
    same paths with no claim primitive; btrfs trim lag makes a correct
    prune look failed) — rescued to scratch/db-cpu-profile/buffer-coral.md.
    Free space at close: ~714 GB.
    ROW 126 — POST-FIX MEASUREMENT AT f3aec311 (reviewer cheetah, attributed,
    :5434): store suite max_concurrent_ddl = **2**, not 1 (n=2; 196/137
    samples above one); oversize = 1 (342 active samples, zero above
    one). Isolated per binary: pg_first_light = 2 with clean attribution
    (foreign_ddl_max 0); pg_conversations 1; pg_store_flows 1. THE CAUSE
    is the residual the reviewer had rated minor (f-1a0d): the raw
    `sqlx DROP DATABASE … WITH (FORCE)` at crates/store/tests/
    pg_first_light.rs:628 takes no permit and carries the test's
    application_name — one unpermitted drop is the whole difference
    between the promise kept and not. Reviewer's self-correction, verbatim
    in spirit: "volume was never the criterion — ac-0001 says at most one
    in flight, and one unpermitted drop falsifies it." The two one-line
    swaps already ruled fold-in (pg_first_light.rs:628, daemon/tests/
    support/mod.rs:110) are therefore the fix, not cosmetics; re-measure
    on the successor's sha. Also: cargo does not parallelise test
    binaries (max concurrent binaries of one run = 1, 240 samples).
    REQ-0042 SECOND WITNESS: pij-chosen-arach (plan 014 coder, spawned
    pre-fix) also lost INBOUND rs delivery in the disk incident while its
    OUTBOUND kept working — it sat "blocked on the gate slot" for ~1 h
    after the slot was granted, until a pane-paste reached it. Rule for
    this fleet until pij plan 124's re-attach is universal: after any
    daemon/VM incident, o-prime sends a one-line DELIVERY-CHECK to every
    omp seat spawned before the incident and pane-pastes any that do not
    answer within a minute. Outbound-works/inbound-dead is the signature.
    ROW 126 — BOTH BYPASSES ISOLATED WITH MATCHED CONTROLS (reviewer
    cheetah, f-1a17 MAJOR): pg_first_light alone max_concurrent_ddl 2
    (92/189 samples above one) vs pg_conversations 1 and pg_store_flows
    1; daemon first_light 2 (18/422) vs boot_recovery 1; foreign_ddl_max
    0 on every isolating run. Both sites are `DROP DATABASE … WITH
    (FORCE)` through a pool built from the test URL — no permit, own
    application_name. The successor coder now has a per-site red
    baseline: fix a site, re-run that one binary, watch 2 → 1. Reviewer's
    written correction: rated MINOR on volume grounds when volume was
    never the criterion. Attributed measurements discharged; remaining:
    per-fix red-proofs, ac-0005 via list_orphans, re-measure 16 → 1 on
    the successor's sha.
    ROW 126 — PER-FIX RED-PROOFS on f3aec311 (reviewer): M1 (liveness
    filter) RED :551; M2a (remove recheck) RED :560 via the 10 s
    self-deadlock timeout — the recheck prevents a HANG; M2c RED :584;
    **M2b (re-force the sweep DROP with recheck kept) GREEN — the
    unforced drop was covered by nothing.** Also: the race test leaks
    five epoch-1 databases on failure and poisons the next run's
    window (reviewer's own first red was that poisoning). RULED
    (prime-reply-019): no-FORCE regression guard asserting on the real
    sweep SQL (shared const) + ac-0003 amended to name the unforced drop
    as defence-in-depth covered only by that guard + drop-on-exit
    cleanup for the race test + doc note "one mutation per clean server".
    ROW 131 — second bite (014, arach): `harness checks` RED at
    fs3-test-suite twice; on the second run the failing suite's name was
    in the truncated tail, so the coder had to re-run the runner
    directly to learn which test failed. Encode: checks must print the
    failing runner's last N lines untruncated, or the path of a full log.
    ROW 126 — REVIEW CLOSED EXCEPT RE-MEASURE (reviewer, 25 findings):
    M3 (drop credentials branch) RED :183; M4 (drop the store permit in
    create_database) RED at admin.rs:631 (N=8 bound). ac-0005 PROVED,
    not merely executed: the example and an independent catalog query
    both printed nothing — two empty sets agreeing proves nothing — so
    the reviewer minted four databases on :5434 (aged+idle+conforming,
    aged+conforming+live connection, young, malformed epoch) and
    list_orphans printed exactly the first. METHOD for future reviews:
    an empty listing is proved only against a seeded positive AND
    negatives. Only the 16 → 1 re-measure on the successor's sha remains.
150. **LSP litter: `.serena/` appears untracked in seat worktrees** (junglefowl,
    plus three siblings). Small repair applied: `.serena/` added to the common-dir
    `info/exclude` (local, not committed). Encode: add `.serena/` to `.gitignore`
    in the next packet that touches the root, so fresh clones inherit it.
    ROW 126 — 012 DONE at 09509b7 (PR #95, CI gate pass 4m53s; report
    fresh-db-serialise-report.md). Attributed probes 2→1, 2→1, 16→1 with
    0 samples over one. NOTE: foreign_ddl_max=15/16 during those runs —
    OTHER seats (pre-012 trees, other governments) still hit the :5434
    test postmaster with 16 concurrent DDL. The permit only protects
    trees that carry 012; until every worktree rebases past the merge,
    the test postmaster still sees the row-141 storm pattern. Encode
    (after merge): a fleet notice "rebase on main for the DB permit".
    ROW 126 — REVIEW 012 DELTA VERDICT: APPROVE on 09509b7 (cheetah, 27
    findings). Reviewer's own measurement: store suite max_concurrent_ddl
    1 across 1003 samples (774 active, 0 above one, foreign 0); trajectory
    across three shas 16 → 2 → 1. Both guards mutation-proved (no-FORCE
    RED :571; permit removal RED admin.rs:705). Merging #95 via the veto
    train. NON-BLOCKING follow-ups for a small packet AFTER merge:
    f-1a1a — widened parser derives age from unique_seed's low 64 bits
    (nanos) with no range check; if the bit layout changes, ~10% of
    names decode as immediately sweepable. Clamp the decode to a
    plausible window + one round-trip-within-seconds test. f-1a10 — two
    cfg(test) lines remain in create_database behind create_test_hook.
    ROW 126 — **#95 MERGED** f73dee0 (guarded squash after the veto
    window; head + gate re-read; Jordan "approved" on Telegram). Review
    012 CLOSED; full record + ack + verdict + probe + buffer vendored to
    scratch/review-012/ (md5-verified). Follow-up packet 012b dispatched
    to junglefowl (prime-reply-022): f-1a1a clamp+test, f-1a10, ship the
    probe to bin/, commit the review record into the plan folder.
    Fleet notice sent (weasel, arach, alpaca, antelope): rebase for the
    permit. Reviewer worktree removed.
151. **Flaky test in CI: `fs3_providers::github_copilot_file_is_used_before_the_omp_store`
    fails with a "No such file" race** (arach, PR #98 docs-only CI run; the
    implementation head's run was green). A test that races the filesystem is a
    defect: find the shared temp path / ordering assumption and pin it. Small packet.
    ROW 139 / PLAN 014 — REVIEW CRITICAL (takin, reproduced on :5434 both
    pre- and post-0023): a failed non-terminal scan_file job now keeps
    its dedupe key forever; the documented recovery (re-add / edit →
    watcher re-fire) is absorbed by ON CONFLICT DO UPDATE and never
    leaves state='failed'; claim_job takes only 'pending' and the boot
    sweep requeue_failed covers only summarize+embed (boot.rs:186). Net:
    the file is permanently unindexable and silent. PR #98 head d04ae3e
    is docs-only over cc8da52 (review valid). Coder told: no push; fix
    shape = a re-fire into a failed row re-arms it to pending + boot
    sweep covers scan_file + a test that re-adds a failed file and proves
    claim_job returns it. Ruling after the full verdict.
    PLAN 014 — VERDICT (takin): REQUEST CHANGES — f-001 CRITICAL (above),
    f-002 MEDIUM (plan says 7 days, code ships the ruled 1 day). ACs
    0001/0002/0004 TRUE re-derived; ac-0003 true-as-written but only ever
    required one row to exist, never that absorbed work runs — that gap IS
    f-001. Seven reviewer experiments refuted the rest (0023 converges 13
    in/13 out across 6 duplicate shapes; atomic; no purge/recovery race
    because nothing leaves done; purge 42.5 ms with 5 rows held FOR
    UPDATE, touched none; boot pass 38.8 ms/10k batch). RULED: reviewer's
    exact CASE re-arm in DO UPDATE (failed→pending, attempts 0), flip the
    pinning test at pg_jobs_retention.rs:230, terminal-vs-nonterminal
    control, ac-0003 amended; boot requeue NOT widened to scan_file.
    BOUNCE NOTES: n-001 purged_last_run is overwritten by every hourly
    sweep — capture status --json within the hour or use the log line;
    n-002 0023 builds a UNIQUE index non-concurrently over 1.01M rows in
    the migration txn — first boot pauses there, not a hang.
152. **`ddocs build` truncates table-cell values at 768 chars in the rendered
    `.dd.md` — silently ate the three owed lists in the 014 reviewer packet and
    the reviewer's own ac-0003 row** (takin n-004 / DL-001). The reviewer only
    caught it by reading the `.dd.json`. Tooling defect for the dd prime
    (dajeil, rs pij-joyous-rooster — routed by meadowlark 2026-09-02); until fixed, pij-team packets must keep long
    items OUT of table cells (use instructions[] entries) — encode in the
    pij-team template notes.
    PLAN 014 — fix AMENDED after the reviewer mutation-checked its own CASE
    on :5434 (four controls: revivable-failed → pending/claimable; pending
    unchanged; running NOT demoted; terminal-failed mints a second row;
    zero keys with >1 live owner): the DO UPDATE also resets `parks` so
    mint-time revival matches requeue_failed (jobs.rs:515-517) — a revived
    row at parks=20 could never park again (runner.rs:1035) and would burn
    attempts against a provider asking us to slow down. Relayed to the coder
    with the two extra pins (running preserved; second row minted).
    ROW 152 — CORRECTED (dajeil not-reproduced; o-prime measured on disk):
    `ddocs build` does NOT truncate — the reviewer packet's .dd.md carries
    1024/533/643-char values whole (line lengths 1060/557/672, tails
    intact) and the review record renders a 2748-char cell in full
    (ddocs 0.1.0). The 768-char cut is on the READER side; the reviewer is
    asked which tool showed it. Withdrawn against dd; re-owned once the
    reader is named (candidates: omp file-view cap, fs3 read path).
    Lesson (again): a symptom seen through a viewer is not a defect in the
    writer until the file on disk is measured.
    PLAN 014 — f-003 MEDIUM (takin): /status still Seq Scans jobs via
    `last_failure` (no serving index; 200k-done seed → Seq Scan cost 5167;
    at a 1-day window prod settles at ~515k done rows and every /status
    call scans them forever). RULED into the same fix commit: partial index
    `jobs_failed_recent_idx ON jobs (updated_at DESC) WHERE state='failed'
    AND last_error IS NOT NULL` in migration 0023 (reviewer verified
    Index Scan cost 4.14) + daemon.md: terminal failures only under
    --history. The 5 s progress loop is clean (never calls last_failure).
153. **pij wire-version mismatch kills new seats' inboxes**: the 014 reviewer
    (spawned 13:31) reports `pij inbox` → "pij daemon wire v1 is unsupported by
    this extension (v2); upgrade pij" (rs 127.0.0.1:7461, CLI 0.1.0). Its inbound
    is dead from birth; the coder pij-chosen-arach has been inbound-dead since
    02:30Z. O-prime relays by file + pane-paste. Also: legacy `pij report now`
    fails with E-RS "answered with something this CLI cannot read" and
    `pij-rs report` refuses without a seat it cannot be given. For the pij
    government; fs3 cannot fix it.
    ROW 143 — disk agent's 24 h large-file sweep (82 min, finished 13:38
    local, after reaping and Jordan's media purge): on the 1.9 TB volume
    only 13 files >200 MB were written in 24 h; largest apart from
    data.img.raw is 4.38 GB. Negative result confirming by elimination
    that the 170 GB was many small files — the cargo target-tree shape.
154. **`~/.git-ai/internal/metrics-db` is 4.38 GB and growing with no visible
    retention policy** (disk agent, flagged not reaped). For the git-ai owner,
    not fs3; noted so it is not rediscovered at the next disk incident.
    ROW 153 — CAUSE + FIX (pij o-prime, file answer 13:58): pij plan 128
    (ac87f4a3, ~13:45) bumped the envelope wire 1→2 on both sides with
    strict equality; the rs daemon was restarted on v2 at 13:52 (pid
    71772). Any omp started BEFORE the merge holds v1 in memory and now
    fails the other way. FIX: no hot-reload — exit omp (Ctrl-C) and `omp -c`
    in the same pane (session continued, same seat id). Claude seats just
    retry. `pij send` returning "queued" to an unreadable recipient is now
    a req-0042 item (named outcome: wire-skew / no-live-subscriber). fs3:
    all four omp seats restarted in place at ~14:00 while idle; card posts
    again. Lesson for the pij government (recorded by them): a wire bump
    needs a fleet notice BEFORE the merge.
    ROW 152 — CLOSED, not a ddocs defect (reviewer retraction in writing,
    review-014-to-prime-003.md): the 768-char cut was the reviewer's own
    harness READ tool footer ("[Some lines truncated to 768 chars]") and a
    bash "...[+N]" marker — viewer-side; its grep "confirmation" was a
    broken regex (`\[+` = one-or-more '[') matching an ordinary "[0]".
    On disk: ac-0003 row 1007 chars, longest packet line 1313, longest
    record line 4441, zero truncation markers. ENCODE (pij-team reviewer
    template + how-we-work): before filing a tooling defect, reproduce it
    with a DIFFERENT tool than the one that showed it — one `awk length`
    on disk ends it. Told dajeil; closed.
    ROW 153 — fs3 cutover receipt: PONG probe to all four seats; only the
    reviewer (takin) answered and only it shows a delivered_at after the
    13:52 daemon restart in ~/.pij-rs/pij.sqlite `delivered_messages`
    (arach 02:30, weasel 03:40, junglefowl 03:48 UTC — the last thing each
    ever received). Ctrl-C does NOT exit omp (the first "omp -c" attempt
    landed as a user message the seats read as "continue"); `/exit` then
    `omp -c` is the in-place restart. Restarted the three at ~14:08 while
    idle. METHOD worth encoding in pij: `max(delivered_at)` per recipient
    is the inbound-liveness signal a prime can read without asking the seat.
    ROW 153 — restart lesson: pij-spawned omp panes run omp AS the pane
    process, so `/exit` closes the PANE, not just omp (three coder panes
    lost at 14:08; sessions intact on disk in ~/.omp/agent/sessions/
    <cwd-slug>/, 5–6 MB each). Recovery: new tmux window per worktree +
    `omp -c` (resumes the latest session for that cwd). `pij-rs revive`
    refused ("seat is live") because the daemon still held the dead pid.
    ENCODE for pij: an in-place restart verb (`pij restart <seat>`) that
    keeps the pane, and liveness that notices a dead pane.
    TIDY 2026-09-02 14:10 (Jordan): worktrees already tidy (only active:
    fs3-review-014 detached, three coder trees); closed the idle panes of
    pij-partial-coral and pij-purring-orangutan (both CLOSED in roster);
    the other closed seats' panes were already gone. rs `list` still shows
    tombstone-less rows for gone seats — pij's to reap.
155. **pij → Claude direct delivery is only PARTIALLY logged in telemetry**
    (audit 2026-09-02, scratch/pij-claude-delivery-telemetry-report.md;
    Jordan's ask). Facts: every Claude-side delivery gets a
    `delivered_messages` row + `message.pushed`/`delivery.outcome` spine
    events; transcript cross-check 123 sent = 123 consumed, 0 dropped, 0
    duplicated. GAPS for the pij government: (A) Claude rows are
    write-only `injected-to-transport` (uds.rs:538) — no consumption
    receipt, unlike omp/pi `reader-read`; (B) 23/123 today sit at
    `delivery.outcome=queued` forever on the spine although the drain
    worker delivered them (pointer/worker.rs:299-303 acks the ledger,
    publishes nothing); (C) the uds reply address is stored nowhere,
    daemon.log has zero uds lines, and `pij sessions` joins the Claude
    session to the LEGACY id (pij-instant-lynx) not the rs id where the
    ledger rows live. The eng-harness collector (trace2 → git-ai →
    refs/notes/ai) is out of scope — no link exists. Only durable
    consumption evidence today: flowspace3 `conversation ingest` of the
    Claude transcript. Encode (pij): a consumption ack from the Claude
    hook, a `delivered` spine event from the drain worker, and the rs id
    in the sessions join.
    ROW 126 — **#99 MERGED** b528860 (012b follow-ups: seed-age clamp +
    round-trip test, test hook out of the shipped body, probe shipped at
    bin/ac-0001-ddl-probe.sh, review-012 record committed under the plan).
    PLAN 012 FULLY CLOSED. Seat pij-sufficient-mite ended; worktree
    fs3-fresh-db-serialise removed; deliverables in scratch/closeout-012/.
    PLAN 014 — FIX PUSHED c5242ea (barnacle): mutation receipts — remove
    state CASE → red failed!=pending; remove attempts reset → red 3!=0;
    remove parks reset → red 20!=0; drop jobs_failed_recent_idx →
    last_failure Seq Scan cost 5358; store+migration 13/13 green;
    running-row-preserved and terminal-row-mints-fresh pinned; plan AC +
    default amended. Handed to takin for the delta (fresh 200k seed, both
    f-001 directions, EXPLAIN + drop-index mutation, migrating_twice).
156. **`harness commit` reported the attribution note MISSED despite a connected
    ingress** (barnacle DL-009, fs3-jobs-retention, commit c5242ea). The
    "confirmed" path promised a landed `refs/notes/ai` note or a named miss;
    it named the miss — good — but the cause (collector accepted, note never
    written) is the harness prime's to chase. For meadowlark.
    ROW 124b — **#97 MERGED** c53a911 (docker-compose: db-test on :5434,
    already live). Row CLOSED. Open PRs now: #98 (014, fix sha c5242ea
    under delta review, CI in progress); 013's PR pending amistad's gate.
    PLAN 014 — DELTA VERDICT: APPROVE at c5242ea (takin, d-001..d-007).
    f-001 cured against the worst stuck state (attempts=MAX, parks=MAX →
    pending 0/0, claimable); f-003 cured (Index Scan 4.14; drop-index
    mutation reverts to Seq Scan 5167); f-002 cured and ac-0003 now
    requires the absorbed re-fire to be claimable. Reviewer added a
    FOURTH mutation: an unguarded CASE demotes running rows and wipes
    budgets — the guard is load-bearing. ac-0001 not regressed. CI green
    5m13s. n-005 (0023 checksum changed in place): prod _sqlx_migrations
    max = 22, safe; only six throwaway :5434 test/orphan DBs carry 0023.
    Merging #98 via the veto train; bounce next (n-001 capture within
    the hour; n-002 first boot pauses on the non-concurrent unique index).
    PLAN 014 — **#98 MERGED** b86593c (guarded squash after the veto
    window). Release build of main in progress; bounce via
    bin/daemon-restart next; then the AFTER receipt (purge count from the
    log line + status --json within the hour per n-001; status/search
    timings vs scratch/plan-014-prod-before.md). Review record to land on
    main as a docs PR.
    PLAN 014 — BOUNCED 04:17:07Z via bin/daemon-restart (pane %50, old
    pid 7703 → new 901, binary target/release/flowspace3 built from
    b86593c in 1m14s). "store schema is current" at +17 s (0023 applied,
    _sqlx_migrations max 23); authenticated ping healthy at +130 s
    (n-002's pause observed as the gap before serving). AFTER receipt
    → scratch/plan-014-prod-after.md.
157. **Reviewer records: `ddocs build` ≠ `ddocs validate`** (takin, review-014).
    The record built clean but failed the global validator with 14 errors
    in four classes: ids not `<prefix>-<4 hex>`; severity outside
    MAJOR/MINOR/NIT/NA (CRITICAL/MEDIUM used); kind outside
    defect/dim0/question; resolution outside confirmed/refuted/fixed/
    deferred; and extra sections (acceptance/delta/refuted/notes) that
    builder/review does not declare. Re-minted in place (f-3a01..1c,
    vd-3b01..1c, legacy ids retained inline; meta.id_note records the
    mapping). ENCODE in the pij-team reviewer template + done_bar: "run
    `ddocs validate` from the worktree root with the GLOBAL ddocs and
    paste its status line" — and list the enums in the packet so the
    reviewer does not invent CRITICAL. Also: reviewer worktrees are
    detached and gitignored-for-temp — the record must be COLLECTED by
    o-prime before tidy (twice now: 012 and 014). Encode: `harness team
    collect <seat>`.
158. **O-prime bounced prod during a held gate slot → the gate's production
    migration guard fired a false CRITICAL STOP** (013, amistad, ask-010).
    Prod 0023 installed_on 04:17:23Z = the 014 daemon bounce; the gate ran
    04:12–04:21Z; before=22/after=23 straddled it. No test touched :5433.
    Two encodes: (1) a held gate slot and a prod bounce are MUTUALLY
    EXCLUSIVE — o-prime checks the slot before `bin/daemon-restart`
    (add the check to the script: refuse if `harness checks` is running
    anywhere on the box); (2) the guard must print the migrating
    application_name + installed_on from _sqlx_migrations / pg_stat so a
    reader can attribute the change instead of assuming the tests.
    Observed against o-prime.
    PLAN 014 — PROD RECEIPT (ac-0005, scratch/plan-014-prod-after.md):
    boot purge pass completed 04:20:27Z (+43 s after healthy):
    purged_last_run = 898,802; done rows 1,155,022 → 258,421 (the 1-day
    window); `status --json` wall 0.58 s → 0.27–0.34 s under fleet load
    avg 30–63; jobs relation still 2.8 GB (dead tuples until autovacuum —
    watch it shrink; if it does not, a VACUUM (FULL) window is a separate
    o-prime call). ac-0005 verdict: purge + live-only census MET; the
    "three status timings < 200 ms" bar NOT met as measured (CLI wall
    under a load average of 63) — re-measure at a quiet moment before
    calling it closed. Search timings unchanged (013 not landed):
    6.7 / 3.8 / 13.9 s vs 8.3 / 4.0 / 14.5 s before.
    DRAIN 2026-09-02 04:30Z (o-prime): retro 002 recorded
    (records/retro/2026-09-02/002-plans-012-014-drain.md, harness-shaped,
    46 entries: shared buffer 11 + closeout-012 16 + review-012 6 +
    closeout-014 11 + review-014 2); shared buffer CLEARED after the
    record landed. Encode-next ranked in the retro (top: LSP reference
    sanity probe; `harness checks --no-sweep`; test-slot URL preflight).
    **#100 MERGED** 57b25df — the 014 review record is on main; docs
    worktree removed.
    PLAN 013 — SLOT RELEASED 04:30:38Z: harness checks green on beee1491
    (fixture stabilised 10k→20k), PR #101 open (head 065acfd = docs-only
    on top). Claude reviewer pij-select-carp spawned 04:36 on
    fs3-review-013 detached at beee1491 with the three owed lists
    (bounded candidate page can admit zero eligible rows → sentinel +
    expansion; one-representative-per-hash dedupe losing results; scoping
    byte-identical; JIT off scoped to the statement; the 20k fixture is a
    fix or a bigger coin). NOTE: the packet was the unfilled template for
    its first ~60 s (a heredoc with backticks executed `git diff`);
    rewritten and the reviewer told to re-read. Encode: never put
    backticks in an unquoted heredoc — use <<'"'"'EOF'"'"'.
    PLAN 015 — ts-grammar (row 147): plan/impl-guide/tasks/backpressure/
    coder packet written and committed on 015-ts-grammar (validate: 0
    errors, 0 warns, 24 completable items); evidence.md records which TS
    node kinds classify today and the arrow-binding gap. Coder (sol)
    spawning 04:37.
    PLAN 013 — reviewer carp ack RULED GO 04:46 (review-013-prime-reply-001):
    prod NOT bounced for 013 → ac-0004/0005 pre-registered thresholds,
    o-prime's post-merge receipt; read-only prod AUTHORISED bounded (client
    search/status any time; ONE EXPLAIN ANALYZE per statement in BEGIN READ
    ONLY + statement_timeout 30 s + no parallel workers, load < 15 only — the
    row-143 lesson); timings only with load stated. Reviewer's #1 hunt: a
    selective scope over a >10,240-vector corpus expanding to the bound and
    raising candidate_limit_exhausted where the old query returned a short
    valid page. Reviewer reported the 768-char cut again — its viewer (row 152).
    PLAN 015 — fox ack GO; the pushed GO turn did not land while the seat
    was mid-turn (delivered_at stayed 04:42:26) — pane-pasted 04:47.
    PLAN 015 — t1 PROVEN (fox, 04:50): tree-sitter-typescript 0.23.2
    resolves and compiles against workspace tree-sitter 0.26; Language::
    {TypeScript,Tsx} + extensions + discovery family wired; `cargo test -p
    fs3-parsers --lib` 29 passed. Only stop condition hit was the newly
    exhaustive LanguageFamily match (mapped to Source). Continuing to
    fixtures/goldens (t2) and the value-shape + namespace rules (t3).
    PLAN 015 — ask-001 RULED: fence amended for one line in
    crates/testkit/arch-allowlist.toml (tree-sitter-typescript under
    fs3-parsers external). O-PRIME MISS: the repo has an `add-language`
    skill and docs/services/scanner.md with a five-step language-addition
    contract (grammar dep → Language → allow-list → fixtures → docs) and the
    plan did not cite it — `flowspace3 search "how do I add a language
    grammar"` would have found it. Coder told to follow all five and map
    them to tasks. Encode: pij-team plan template line "cite the repo's own
    how-to (search first) for the change class".
    PLAN 014 — ac-0005 quiet-time status re-measure DEFERRED: load average
    held 20–63 from 04:20 to 04:56 (reviewer + coder builds and tests);
    two load-gated attempts never fired. Owed at the 013 bounce, which
    needs a quiet moment anyway; row stays "partial" until then.
    (receipt) `flowspace3 search "how do I add a new language grammar to the
    parser" --limit 3` → #1 docs/services/scanner.md "Adding a language",
    #3 crates/parsers/src/lib.rs grammar. The product would have caught
    o-prime's miss in one call. Defender note sent to Jordan (wdavdaemon
    ~174 % CPU for 3 h 46 m + dlpdaemon ~48 % — two cores on cargo churn).
159. **Search JSON envelope contains an unescaped control character** (o-prime,
    04:58: `flowspace3 search "what owns the watcher debounce" --limit 3
    --json` → json.decoder "Invalid control character at line 22 col 37" —
    a snippet carrying a raw control byte). Agents parsing the envelope
    crash. Fix: escape/strip control chars in snippet rendering; a test with
    a fixture containing \x1b / \x00 in content. P1 for agent consumers.
160. **Search exposes no timing anywhere** — no `took_ms`/phase timings in the
    envelope, no daemon log line per search (grep of daemon.rs/search.rs/http.rs
    finds none; doctor is the only surface with elapsed_ms). Jordan asked
    "what is going on" and the only answer came from pg_stat_statements.
    Encode: per-search `timing{embed_ms, sql_ms, rank_ms, total_ms}` in
    meta + one INFO log line; the ask-tool view likewise.
    ROW 122 / PLAN 013 — LIVE RECEIPT (pg_stat_statements, prod, 04:58):
    the `WITH candidate_vectors AS MATERIALIZED …` search statement
    mean_exec_time 10,696 ms over 132 calls; one measured search 12.5 s
    wall. ~85 % of search latency is that statement. Same counter is the
    after-metric for 013 (reset or note calls/mean at the bounce).
    ROW 159 — WITHDRAWN (o-prime's own artefact): re-run with stdout only
    → 4,467 bytes, zero control bytes, zero raw tabs, `json ok`. The first
    measurement piped `2>&1`, so a stderr line (the pij stale-card warning
    / an ANSI sequence) landed inside the JSON. Row 149 already says it:
    parse stdout only. Not a product defect. Lesson kept: reproduce with
    a different tool before filing (row 152, twice today).
161. **rs seats SPAWNED after the wire v2 cutover receive nothing** (05:03):
    `delivered_messages` has 0 rows for pij-select-carp (spawned 04:36)
    and pij-resonant-fox (04:37) while the RESUMED seats (barnacle 3,
    amistad 3, mite 2) deliver. Every `pij-rs send` to the two new seats
    returned ok/queued and sat undelivered (row 155 gap B: "queued
    forever"); both seats went idle waiting on rulings that never landed
    (carp 16 min, fox 10 min) until o-prime pane-pasted. Outbound from
    them works. For the pij government: spawn-time subscription after
    the v2 daemon restart. fs3 workaround: every ruling to a post-cutover
    seat is pasted, not sent; delivered_at is the liveness signal.
    PLAN 015 — t2 DONE (fox, 05:06): fixtures sample.ts/sample.tsx with
    nested declarations, a namespace, six function-valued bindings, the
    `const x = 1` / `const cfg = {}` negatives and grep traps; goldens
    check kind/subkind/address/parent/sibling/span + no-empty-name
    invariant; 13 run, 11 pass, exactly the 2 t3-mechanism goldens RED
    (internal_module + value-shape bindings). Allow-list line in. On to t3.
    PLAN 015 — t3 DONE (fox, 05:12): generic function-valued binding rule
    in source.rs (no Language branch); internal_module → Container in
    classify.rs with an exhaustive TypeScript decision test; fixture
    forests green (13 passed; classify 10 passed); mutations: rule removed
    → exactly six binding Functions vanish → golden red; internal_module
    removed → members flatten to file scope → red; both restored. Five-step
    add-language mapping recorded. Gate slot GRANTED in advance for t4.
    PLAN 015 — t4: parsers/core regression 311 green; the local gate
    refused ("FS3_TEST_DATABASE_URL is not set") because the coder
    honoured "no database" literally (ask-002). RULED: run the gate with
    the :5434 test URL — the gate IS the DB-backed suite; "no database"
    means no database code. Slot re-granted 05:20. Encode: the coder
    packet template carries the test URL line for EVERY packet (not only
    DB ones), and `fs3-test-db-check`'s refusal names the URL to set.
162. **`fs3-test-suite` exit 124 (timeout) after 735 s with no assertion
    failure, and the gate names neither the timed-out command nor a
    targeted rerun** (fox, plan 015, load avg 30–40; ask-003). Same family as
    row 131. Encode: the runner prints the suite it was in when the timeout
    fired, the timeout value, and the exact `cargo test -p … --test …` to
    rerun; `harness checks` exposes the timeout (env or flag). RULED for 015:
    release the slot, PR, CI on the exact sha is the gate; no local retry on
    a loaded box.
    ROW 156 — second bite (fox, plan 015, commit 3649c0fd, 05:33): collector
    connected, `refs/notes/ai` note missing, nothing buffered. Two seats in
    one hour; the harness prime has it.
    PLAN 015 — PR #102 open at 3649c0f (feat(parsers): add TypeScript and
    TSX grammars); CI running (the local gate timed out at load, row 162).
    Claude reviewer spawned 05:36 on fs3-review-015 detached at 3649c0f
    with the three owed lists (double-emission / non-function values /
    destructuring; hint-coupled classify; nested namespace addresses; TSX
    error-recovery blank names; scan-table reproducibility; discovery).
    PLAN 014 — third status measurement 05:26Z (load 27–37, the box never
    quietened): `status --json` wall 0.25 / 0.37 / 0.26 s; done rows
    274,266; jobs relation still 2,812 MB an hour after the purge —
    autovacuum has not reclaimed it. ac-0005 stays PARTIAL: purge and
    census MET; the <200 ms CLI wall not demonstrated under fleet load
    (0.25 s is process start + auth + one round trip). Two follow-ups:
    measure server-side (row 160 timing) rather than CLI wall; and if
    pg_stat_user_tables.n_dead_tup stays ~900k, schedule a VACUUM window.
    (correction) autovacuum DID run on jobs at 04:21:04 (n_live 271,430,
    n_dead 37,596); the 2.8 GB is free space inside the relation, which
    only VACUUM FULL returns to the OS — not a defect, no window needed.
    PLAN 013 — VERDICT (carp): REQUEST CHANGES — f-9c41 CRITICAL: moving
    admission from inside candidate_vectors to a post-filter after LIMIT
    makes a repo-scoped search whose scope is a small share of the index
    (12,000 nearer foreign vectors, 5 in-scope, limit 10) ERROR after 9
    passes / 200 ms / 246k shared blocks, where the old query returned 5
    hits in one 29 ms pass — the exact geometry search_scope_starvation.rs
    was written to defend, surfacing as an outage via search.rs:341-345.
    Three MINOR (f-2e07 non-discriminating test; f-7b13 JIT assertion
    cannot fail; f-4d88 parity table overclaims) + 1 NIT. Every measurable
    AC TRUE; ac-0004/0005 pre-registered with thresholds for o-prime's
    post-merge receipt (incl. the unscoped admitted_elements 484 MB heap
    caveat). RULED (prime-reply-016): admitted-growth sentinel; never Err
    at the bound (short page + empty_because); bound admitted_elements to
    the candidate page; discriminating paired-geometry test; JIT assertion
    that can fail; parity honesty; one commit on 065acfd; delta re-review.
    PLAN 013 — reviewer additions adopted for o-prime's post-merge receipt:
    ac-0004 also asserts the HNSW node remains the childless `<=>`-ordered
    driver on prod stats; ac-0005 fails if ANY run errors, however fast.
    Coder addendum: fix the stale iterative_scan comment (embeddings.rs:
    669-685); the paired-geometry test must gate on returning the 5
    scoped results. Round-1 record (schema-valid, 7bfe9856/4acd658b)
    vendored to scratch/review-013/. Reviewer retracted its 768-char
    claim (viewer; third time today).
    PLAN 013 — ask-011 (amistad): prime-reply-016's two requirements
    conflicted — with admission strictly above the HNSW page, scoped rows
    behind 12,000 nearer foreign vectors are unreachable within the bound,
    so the only "truthful" result was 0, not the 5 the geometry demands.
    RULED option B (prime-reply-017): 013's promise is "no correlated
    smart_content probe per candidate", not "all admission above HNSW" —
    cheap scope/anchor predicates go back INSIDE candidate_vectors as
    pre-resolved keys (iterative_scan keeps pulling until the scoped page
    fills); expensive smart_content resolution stays above the page,
    bounded to its hashes; store returns a page result with
    candidate_limit_exhausted (never Err); daemon search.rs/http.rs edits
    authorised, envelope additive; shape assertion relaxed to the real
    promise. O-prime's 016 was over-specified; the coder's stop-and-ask
    was correct. Reviewer forewarned of the shape.
    PLAN 015 — reviewer boar ack RULED GO 05:34 (by paste; its ack send
    never reached o-prime — the seat sat idle 6 min; the watcher caught it).
    O-PRIME MISS: packet-reviewer-015 was cloned from 014's and leaked two
    plan-specific instruction rows (i6/i7 queue-status material) and the
    014 deliverable filenames in w1; the reviewer caught both and executed
    neither. Encode (pij-team): generate reviewer packets from the template
    + plan, never by cloning a sibling; a packet lint that greps for the
    previous plan's slug. Row 161 note: outbound from post-cutover seats
    is also lossy (boar's ack send lost) — file mirror `review-015-status.md`.
    PLAN 015 — reviewer's early flag resolved: the pre-prod scan table IS
    in PR #102's body (327 .ts files: 327 → 3,452 elements, 43 file-only,
    0 scan errors); the vendored copy in the review tree was stale
    (copied before the coder edited the PR) — refreshed; t5 unchecked in
    tasks.dd.md is a docs gap for the coder's delta. Encode: vendor the
    PR body from `gh pr view --json body` at spawn, never from the seat's
    draft file.
    PLAN 013 — reviewer de-risks forwarded as BINDING to the coder: no-Err
    only in search_elements; reuse search.rs:462 scan_incomplete as the
    landing zone; page-bounded admitted_elements must union raw hashes
    with raw hashes reached via smart_content.text_hash for smart
    candidates (else every smart hit drops / the correlated probe returns).
    PLAN 013 — reviewer precision on option B forwarded as BINDING: the fix
    is a relocation (existing admitted_sources semi-join, BOTH legs raw +
    smart text_hash, moves into candidate_vectors WHERE before ORDER BY/
    LIMIT; payload + chooser stay above) so candidate_count counts admitted
    rows again; GAP found: the page signal has no daemon carrier
    (empty_because is empty-only, truncated means the opposite, fusion can
    mask a starved semantic leg) → distinct `scan_incomplete: bool` on the
    outcome/meta + test (authorised daemon edit). THE residual risk: the
    planner keeping the HNSW driver with a hash semi-join inside ORDER BY/
    LIMIT — booked in the ac-0004 post-merge receipt.
    PLAN 015 — VERDICT (boar): APPROVE, no blocking findings. All five ACs
    true on the reviewer's own evidence at 3649c0f; 6/6 mutations (the
    author's two + four of its own: widening the value rule reddens the
    golden AND invents_nothing; 'type' hint 1+1 red; 'mod' hint 3+3 red;
    .tsx arm removal collapses TSX); no double emission; t5 table
    reproduced to the digit (327/327/3452/284/43, 0 errors); CI green on
    the reviewed sha. Findings: f-2b01 MINOR object-literal members
    (get(){} at file scope, `put: () => {}` emits nothing) → JS/JSX
    packet; f-2b02 MINOR `declare module "pkg/sub"` keeps quotes in the
    name → one-condition fix, same packet; f-2b03/04 NIT (anonymous
    default exports; "declares anything" vs callable/container — all 43
    file-only files are value-bindings-only, e.g. name-corpus.ts 1,720
    lines → 1 element); f-2b05 MINOR, O-PRIME'S: impl-guide.dd.json has
    2 schema errors (arrays where strings required) and packet-coder has
    duplicate id i10 — `harness plan validate` passed while `ddocs
    validate` fails: two validators, different rules (encode: the team
    scaffold runs BOTH). Docs-only commit by the coder, then merge.
163. **JS/JSX grammar packet** (follow-up to 015): tree-sitter-javascript;
    object-literal members (f-2b01) and quoted module names (f-2b02) solved
    there; anonymous default exports (f-2b03) decided there.
