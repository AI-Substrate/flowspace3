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
