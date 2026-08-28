---
record_kind: retro
harness_version: 0.13.0
branch: w-retro-drain
repo: https://github.com/AI-Substrate/flowspace3.git
created_at: '2026-08-28T01:32:01.938Z'
agent: pij-urgent-bobolink (successor reviewer seat; predecessor lost to post-snapshot disk event)
plan_id: cross-plan (release + auto-update + worktree/PR cutover + pij-team prototype + watcher incident)
schema_version: '1.2'
retro_id: 2026-08-28T01:32:01Z-pij-urgent-bobolink-2dc3d3a60529
started_at: '2026-08-26T21:37:17.579Z'
ended_at: '2026-08-28T00:54:24.958Z'
summary: 'Fleet snapshot drain of 61 observations from six immutable buffers: 49 difficulties and 12 confusions; 6 blocking,
  35 degrading, and 20 annoying. Every source-qualified observation is retained, grouped into ten root-cause themes, adjudicated
  against fixes landed through PR #38, and ranked into five implementation priorities. A post-snapshot disk-full event that
  killed multiple seats, including this seat’s predecessor about 60 seconds after spawn, is recorded separately and excluded
  from snapshot counts.'
entries:
- id: DL-001
  kind: difficulty
  description: 'docker compose up -d in a per-coder worktree collides with the shared flowspace3-db container (container_name
    is hardcoded, compose project name is derived from the worktree dir) — it creates a stray network+volume for the worktree
    project and then fails with a name conflict, while harness boot reports ''compose: service db is not running'' even though
    a healthy shared db is up on :5433.'
  target: infra
  severity: degrading
  workaround: Do not run docker compose up in a worktree; use the already-running flowspace3-db on 127.0.0.1:5433 and delete
    the stray fs3-<slug>_default network / fs3-<slug>_flowspace3-pgdata volume.
  suggested_encoding: harness boot should detect an externally-running flowspace3-db (probe :5433 / docker ps) and report
    ok-shared instead of degraded; or compose should set a fixed project name so every worktree resolves to the same stack.
  fp: d5cf2c98cfd5
  disposition: kept
  system:
    source_buffer: fs3-convo-ingest-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-28T00:13:51.980Z'
- id: DL-002
  kind: difficulty
  description: 'Inheriting a dead PM seat mid-phase, I could not tell from disk which of the uncommitted work was written-and-passing
    versus written-and-never-run, so I re-ran the whole gate to find out. Also: tk-c105 assumed a reference oracle that turned
    out to cover 3 of 4 stores and to be a subset-not-equality oracle — a plan-time assumption about a vendored tool that
    nobody had executed against the fixtures.'
  target: tooling
  severity: degrading
  workaround: Re-ran cargo test -p fs3-testkit and the full harness checks before trusting any inventory claim; read the pinned
    oracle's source before building expectations on it.
  suggested_encoding: 'A phase gate that writes its own green receipt into the plan ddoc (command + exit code + timestamp
    + tree sha), so a successor reads proof rather than re-deriving it. And: any vendored reference tool named as an oracle
    should be executed against the fixtures at vendoring time, not at consumption time.'
  fp: 123262694ad0
  disposition: kept
  system:
    source_buffer: fs3-convo-ingest-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-28T00:51:39.099Z'
- id: DL-001
  kind: difficulty
  description: 'docker compose up -d fails in a second worktree: the compose file pins container_name flowspace3-db, so a
    worktree whose compose PROJECT is fs3-team-ext hits ''container name /flowspace3-db is already in use'' against the main
    clone''s container, and it also leaves an orphan volume (fs3-team-ext_flowspace3-pgdata) behind. Worse, harness boot probes
    postgres with ''docker compose exec -T db pg_isready'', which in a worktree resolves to a project with no db service,
    so boot reports degraded even though postgres is up and healthy on :5433 and every test would pass against it'
  target: infra
  severity: degrading
  workaround: do not up the stack in a worktree; reuse the main clone's running flowspace3-db container (the port is host-wide
    anyway)
  suggested_encoding: either drop container_name from compose so each worktree gets its own project-scoped container, or make
    the boot probe address the DB by URL/port (pg_isready -h 127.0.0.1 -p 5433) instead of via docker compose exec, so the
    probe answers the question 'is postgres reachable' rather than 'does THIS directory own the container'
  fp: ef34a874b186
  disposition: kept
  system:
    source_buffer: fs3-team-ext-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-28T00:12:25.087Z'
- id: DL-002
  kind: difficulty
  description: 'the pij-team templates were committed in PR #35 without their .dd.md siblings, so harness doctor reports ''4
    of 23 deterministic documents have no rendered sibling'' on a clean checkout of main; the row is not gated by harness
    checks, so it will persist until someone builds and commits them'
  target: skill
  severity: annoying
  workaround: 'none needed for the team extension: it builds the .dd.md siblings for the COPIES it seeds into a plan folder,
    so the missing siblings are only on the template originals'
  suggested_encoding: run ddocs build on the four templates and commit the siblings, or add a docs/ddocs sibling check to
    harness checks so the doctor row has teeth
  fp: 3d61bc0c1c0d
  disposition: kept
  system:
    source_buffer: fs3-team-ext-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-28T00:27:09.232Z'
- id: DL-001
  kind: difficulty
  description: Shared scratch database flowspace3_test on 5433 was at migration 13 while main's tree carries only 0012 — an
    unmerged branch migrated the shared test DB. Symptom is doctor reporting schema skew, which turns cli/tests/doctor_daemon.rs
    RED and reads exactly like 'main is broken'. Cost a full detour to disprove.
  target: infra
  severity: degrading
  workaround: Created a private database fs3_blastradius via flowspace3 doctor with FS3_DATABASE__URL pinned, and ran the
    gate against that
  suggested_encoding: The test-database rule should be 'a scratch database nobody else shares', not one shared name. Either
    fs3_testkit derives a per-worktree database name, or the testdb gate warns when the configured test DB's max migration
    is AHEAD of the tree's migrations directory — the same comparison prodguard already makes.
  fp: 3918f68b213f
  disposition: kept
  system:
    source_buffer: fs3-test-blast-radius-observations.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T10:20:23.523Z'
- id: DL-002
  kind: difficulty
  description: 'gh pr create with a heredoc body containing backticks: the backticks were evaluated as command substitution
    despite the quoted ''EOF'' delimiter, executing cargo test repeatedly and burning a 300s timeout. No PR was created and
    the failure looked like a hang, not a quoting bug.'
  target: tooling
  severity: annoying
  workaround: Wrote the body to a file with the file-write tool (no shell involved) and used gh pr create --body-file
  suggested_encoding: Agent guidance and any harness PR helper should mandate --body-file for PR bodies. Markdown PR descriptions
    in this repo are full of backticked identifiers, so the dangerous case is the normal case.
  fp: 046da0252b28
  disposition: kept
  system:
    source_buffer: fs3-test-blast-radius-observations.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T10:20:30.628Z'
- id: DL-001
  kind: difficulty
  description: 'A mutation check done with ''sed -i.bak <file>'' then ''mv <file>.bak <file>'' silently kept testing the MUTANT:
    mv restores the original file''s OLD mtime, cargo''s staleness check is mtime-based, so the crate was not rebuilt and
    the guard-disabled rlib stayed linked. Three test binaries then reported green/red for the wrong build and I chased a
    phantom logic bug in a store query for ~15 minutes.'
  target: tooling
  severity: degrading
  workaround: touch the file after restoring, then re-run
  suggested_encoding: harness needs a 'harness mutate <file> -- <cmd>' (or a documented recipe) that applies a mutation, runs
    a named test, restores, and TOUCHES the file both ways so cargo cannot serve a stale artifact. Mutation-check transcripts
    are a standing deliverable in this repo's briefs and the naive shell recipe for them is unsound.
  fp: dda9a729ceeb
  disposition: kept
  system:
    source_buffer: fs3-watcher-ignore-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-28T00:54:24.958Z'
- id: DL-001
  kind: difficulty
  description: 'The DL-063 backtick trap bit o-prime live: a pij send composed in a double-quoted shell string containing
    a backticked example command (harness telemetry survey --since <date>) died on shell parse (backtick substitution + <d
    read as redirect). Same session in which the retro record ranking that exact defect was committed — knowing about the
    trap did not prevent reaching for the character. Confirms the retro''s encoding-over-teaching conclusion.'
  target: tooling
  severity: annoying
  workaround: single-quoted the message, dropped backticks and angle brackets
  suggested_encoding: message-bearing CLI verbs (pij send, harness commit) need a -F file/stdin path so prose never transits
    shell argv
  fp: 558e566c5d37
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T21:37:17.579Z'
- id: DL-002
  kind: difficulty
  description: 'O-prime made THREE wrong-mechanism diagnoses in one day, each citing a plausible mechanism without citing
    the command+output that proved it: (1) claimed the runner retries ANY error when it discriminates via catalog retryable;
    (2) blamed release-please version-bumping for a Cargo.lock desync it cannot cause; (3) claimed gh run list --commit does
    not match tag-triggered runs when the truth was registration lag. Each was stated confidently to Jordan or the fleet and
    later retracted.'
  target: tooling
  severity: degrading
  workaround: 'MW-002 rule adopted: cite the command and its output, not the conclusion; empty result = re-query, never eyeball'
  suggested_encoding: governance rule template requires an evidence line (command + observed output) before any mechanism
    claim ships in a ruling or relay
  fp: 88ae8850d5c2
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T21:38:09.562Z'
- id: DL-003
  kind: difficulty
  description: 'Tag-cycling a published GitHub release silently re-drafts it: after the 9th green release run, v0.2.0 was
    invisible to users because each re-tag flipped the release back to draft, and nothing in the workflow or gh output says
    so. Separately the latest-pointer takes ~30s to propagate, so immediate verification 404s and reads as a broken installer.'
  target: project
  severity: blocking
  workaround: 'gh release edit v0.2.0 --draft=false --latest; runbook rule: after any tag cycle, verify isDraft=false before
    telling anyone the release exists'
  suggested_encoding: release workflow final step asserts isDraft=false and latest points at the tag - the machine checks
    its own visibility
  fp: d5ec78cf2d3d
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-26T21:38:09.682Z'
- id: DL-004
  kind: difficulty
  description: GITHUB_TOKEN-created tags do not trigger workflows (recursion guard), so release-please tagging produced NO
    release run and the absence looked like a workflow bug. Cost a diagnosis cycle; the fix (cycle the tag as the real user)
    is tribal.
  target: project
  severity: degrading
  workaround: delete and re-push the tag from the human credential
  suggested_encoding: 'preflight runbook states it; better: release workflow triggered on release-published event rather than
    tag push, which does not have the recursion hole'
  fp: 9584d37e3e05
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T21:38:09.801Z'
- id: DL-005
  kind: difficulty
  description: 'Identity management consumed real o-prime attention all session: phantom aliases (5 ids sharing one spawnId),
    canary-verify needed before every address, alias exit tombstones arriving as messages, and one send to a wrong-but-plausible
    id (pij-frank-cicada) failing E-NOID mid-dispatch. The roster is the only defense and it is hand-maintained.'
  target: tooling
  severity: degrading
  workaround: canary-verify + hand-kept roster with canonical ids and alias lists; address only canaried ids
  suggested_encoding: 'pij: one process = one id, or registry marks aliases as aliases and rejects direct address; send should
    fuzzy-suggest the canonical id on E-NOID'
  fp: c348584db39b
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T21:38:09.925Z'
- id: DL-006
  kind: difficulty
  description: 'Fleet visibility costs scale with seats: keeping 6+ concurrent workers honest required me to invent per-incident
    rules (single CI watcher, worktree naming, rate-limit budgets) reactively after each shared-resource incident, because
    nothing surfaces fleet-level resource pressure proactively - the first symptom is always an outage (API 403, fmt red on
    main, swept commit).'
  target: tooling
  severity: degrading
  workaround: incident-driven rule encoding in .harness/government/ after each event
  suggested_encoding: 'fleet dashboard primitive: shared-resource budget lines (API remaining, dirty shared paths, active
    watchers) queryable by any seat before acting'
  fp: 0d4723b01fc9
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T21:38:10.059Z'
- id: DL-007
  kind: difficulty
  description: 'Dogfooding per AGENTS.md: `flowspace3 agents-start-here` fails with ''unrecognized subcommand'' because /usr/local/bin/flowspace3
    is a symlink to a STALE local target/release build reporting 0.1.0, while the repo has shipped v0.2.0 (which contains
    agents-start-here). An agent following AGENTS.md verbatim hits a hard error on its very first dogfood command.'
  target: project
  severity: degrading
  workaround: Read the repo tree instead; checked git tags to confirm the installed binary is behind.
  suggested_encoding: 'This is precisely req-0054 auto-update (w-auto-update packet). Two encodings: (1) the update checker
    must make staleness visible in the envelope rather than leaving agents to discover it via an unrecognized-subcommand error;
    (2) note that the real-world install path here is a SYMLINK into target/release owned by root - the atomic-swap design
    must decide symlink-vs-target and will hit the not-writable notify-only fallback on this very machine.'
  fp: a06f47617a9d
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T22:10:21.092Z'
- id: DL-008
  kind: difficulty
  description: 'Fresh-agent dogfood: flowspace3 search fails FS3-E-QUERY-NO-INDEX (stored fake@1024 vs active text-embedding-3-small-no-rate@1024).
    Error envelope is excellent (names both models + exact fix) but out-of-box experience on a machine with stale index is
    a dead end unless agent re-indexes; doctor doesn''t proactively flag the mismatch at add/search time'
  target: project
  severity: degrading
  workaround: read code directly instead of searching
  suggested_encoding: doctor row (or search pre-check) that flags active-vs-stored embedder mismatch before query time
  fp: 4337ff0ec4c4
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T22:13:33.302Z'
- id: DL-009
  kind: difficulty
  description: 'harness observe writes to a CWD-RELATIVE buffer (.harness/temp/agent/session-buffer.md). In the worktree-per-coder
    era that silently splits the shared buffer: my CONF-001 and DL-001 were written into ../fs3-w-auto-update/.harness/temp/,
    NOT the main clone''s buffer where every other agent''s observations live. Removing the worktree at packet close would
    have destroyed them without a word - and the tidy instruction at close-out is exactly when that happens.'
  target: infra
  severity: degrading
  workaround: Copied the worktree buffer to /tmp/edeard-observations.md before removal and re-filed the entries from the main
    clone.
  suggested_encoding: Either resolve the buffer path against the git COMMON dir (git rev-parse --git-common-dir) so every
    worktree of one repo shares one buffer, or have the worktree-tidy step refuse when the worktree holds an undrained buffer.
    The failure is silent and lands precisely at close-out, which is when nobody is looking.
  fp: fc1865831792
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T23:02:37.598Z'
- id: DL-010
  kind: difficulty
  description: 'A dependency added to a crate manifest can land on main with Cargo.lock un-updated and every gate green: plain
    ''cargo test'' updates the lock IN PLACE and passes, so fmt/clippy/test/arch all prove things about a dependency set that
    was never written back to disk. release.yml builds with --locked, so the failure surfaces at a TAG, on a matrix of runners,
    for whoever is shipping - the furthest possible point from the change that caused it. Hit on w-auto-update (#13): sha2
    + tempfile added to fs3-daemon, main left failing ''cargo metadata --locked'' (exit 101).'
  target: project
  severity: blocking
  workaround: 'PR #15: committed the lock AND added a ''lock'' gate (cargo metadata --locked) to both harness checks and ci.yml,
    running first. Mutation-checked - red without the fix, green with it.'
  suggested_encoding: 'Already encoded in PR #15, but the general lesson outlives this bug: when one workflow builds with
    a stricter flag than the gate does (--locked, --frozen, --offline, a pinned toolchain), the gate is not proving what ships.
    Any such asymmetry between the PR gate and the release build should be treated as a missing check, not as a difference
    in intent.'
  fp: 0178bd53c59c
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-26T23:02:47.571Z'
- id: CONF-001
  kind: confusion
  description: 'Live smoke of ''doctor upgrade'' against the REAL GitHub releases surfaced a design bug the fake-server tests
    structurally could not: releases published BEFORE this feature (v0.1.0, v0.2.0) carry no SHA256SUMS asset, and the first
    implementation reported that as FS3-E-UPDATE-UNREACHABLE (retryable) — i.e. every existing installation would have been
    told ''the release could not be read, retry'' once a day forever, instead of ''there is a newer version I cannot verify,
    install it yourself''. [RE-FILED from the w-auto-update worktree buffer, originally CONF-001 — see DL-009 for why it was
    stranded.]'
  target: project
  severity: annoying
  workaround: 'Changed fetch() to distinguish 404 from a transport failure; missing SHA256SUMS and missing-asset-for-this-triple
    both return Outcome::Blocked (user-facing news) rather than Err. Two regression tests. Landed in dcd6ac3 / PR #13.'
  suggested_encoding: 'A fake-server test suite models the release shape you are ABOUT to publish, never the ones already
    out there. Any feature that reads PUBLISHED artifacts needs at least one smoke run against the real, historical surface
    before it is called done: the fake proves the happy path, the real world proves the migration path.'
  fp: 2dcebdcddefe
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-26T23:02:55.538Z'
- id: DL-011
  kind: difficulty
  description: 'First PR-era packet: the pull_request CI run never fired for PR #13 despite ci.yml triggering on pull_request
    into main and the branch being a clean fast-forward. ''gh api .../commits/<head>/check-suites'' returned an EMPTY list
    — the decisive datum, because zero check-suites means the event was never delivered, not that a run is queued or awaiting
    approval. Neither o-prime''s close/reopen (which did fix mergeable:UNKNOWN -> CLEAN) nor an empty synchronize commit produced
    a run; only a manual ''gh workflow run ci.yml --ref <branch>'' gave a green signal. [RE-FILED from the w-auto-update worktree
    buffer, originally DL-001 — see DL-009 for why it was stranded.]'
  target: project
  severity: degrading
  workaround: Triggered ci.yml via workflow_dispatch and reported both the interim green and the anomaly to o-prime; did not
    touch branch protection or workflow triggers.
  suggested_encoding: The PR era makes 'CI green on the PR' a merge requirement, so a silently-absent run is a silently-absent
    GATE — and it presents as 'checks pending' rather than 'no gate ran'. Worth a harness command that, given a PR number,
    asserts a pull_request-event run EXISTS for the head sha, so absence can never be mistaken for pending.
  fp: f0e50f86c9ce
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T23:03:04.051Z'
- id: DL-012
  kind: difficulty
  description: 'v0.2.0 release binary reports ''flowspace3 0.1.0'': release-please config uses release-type ''simple'' so
    it bumps .release-please-manifest.json only, never workspace Cargo.toml (version=0.1.0 at tag v0.2.0). Found on a from-scratch
    Linux install via the real curl|sh installer.'
  target: project
  severity: degrading
  workaround: read the tag/manifest instead of --version
  suggested_encoding: switch release-please to a rust release-type or add Cargo.toml to extra-files so the crate version tracks
    the tag; add a release-preflight check asserting binary --version == tag
  fp: f5978624cfe8
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-26T23:04:31.873Z'
- id: DL-013
  kind: difficulty
  description: 'README first-run is impossible for a real curl|sh user: ''flowspace3 doctor'' shells ''docker compose up -d''
    in CWD, which needs the REPO''s docker-compose.yml. From a clean ubuntu:24.04 with only the installed binary, doctor hard-fails
    FS3-E-STORE-UNAVAILABLE (''no configuration file provided: not found'') and its fix string says ''run it by hand from
    the repository root'' - a repository the README never told the user to clone.'
  target: project
  severity: blocking
  workaround: curl the raw docker-compose.yml from main into a working dir, then doctor succeeds
  suggested_encoding: embed the compose file in the binary and write it to the config dir (or run the container directly without
    compose) so doctor is self-sufficient for a binary-only install
  fp: b593602e0c47
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T23:13:18.216Z'
- id: DL-014
  kind: difficulty
  description: '''poll flowspace3 status until the queue is empty'' never terminates. After an add, status.data.queue keeps
    state:''done'' rows permanently; it is only [] before the first add. Both the README and the in-binary agents-start-here
    guide instruct agents to poll until empty. I implemented that literally and burned 300s in a loop that could never exit.'
  target: project
  severity: degrading
  workaround: poll for absence of rows whose state != done, or read the daemon's progress phase=idle line
  suggested_encoding: 'either drop done rows from status.queue, or change the documented stop condition to a field an agent
    can actually test (e.g. data.indexing_complete: true)'
  fp: 726e6db3066a
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T23:13:18.360Z'
- id: DL-015
  kind: difficulty
  description: fs3 daemon writes ANSI colour escapes into a redirected (non-TTY) log. README says 'flowspace3 daemon &'; capturing
    that to a file yields lines like ESC[2m2026-08-26T...ESC[0m, so grep/parse of the log silently matches nothing until you
    strip escapes.
  target: project
  severity: annoying
  workaround: sed 's/\x1b\[[0-9;]*m//g'
  suggested_encoding: detect non-TTY stdout and disable ANSI in the tracing subscriber
  fp: 5075e3b2c73e
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T23:13:18.499Z'
- id: CONF-002
  kind: confusion
  description: 'Fresh install steering is wrong twice: (1) with roots:[] and nothing indexed, status.next_action says ''the
    queue is empty - search will answer from the index'' instead of steering to ''add''; (2) doctor on a fully healthy default
    install reports verdict ''degraded'' when the only non-ok rows are the SHIPPED default offline provider (warn) and an
    optional skill install (info) - a first user reads ''degraded'' as broken.'
  target: project
  severity: degrading
  workaround: read healthy:true and ignore verdict
  suggested_encoding: verdict should be 'healthy' when the only findings are info/opt-in; status next_action should branch
    on roots being empty
  fp: 92941f673a03
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T23:13:18.639Z'
- id: DL-016
  kind: difficulty
  description: 'onnxruntime prints ''cpuid_info warning: Unknown CPU vendor. cpuinfo_vendor value: 0'' on aarch64 Linux (ubuntu:24.04
    container) before EVERY command, including fully offline ones like agents-start-here and docs list. Envelope stdout stays
    clean (warning is on stderr) but a first user sees a scary warning as the very first output of the product.'
  target: project
  severity: annoying
  workaround: 2>/dev/null
  suggested_encoding: silence ORT's cpuinfo logging, or don't initialise the ONNX runtime for commands that never embed
  fp: f608b2fa2c3e
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-26T23:13:18.778Z'
- id: DL-017
  kind: difficulty
  description: FS3_* environment overrides leak from a developer shell into integration tests and silently invert them. I
    exported FS3_DATABASE__URL while smoke-testing, and crates/cli/tests/boot_contract.rs - which writes a fixture config
    naming an UNREACHABLE store and asserts the daemon fails fast - spawned a child that inherited the override, reached a
    store that WAS running, served happily, and failed after 60s as 'the daemon did not fail fast'. The failure names a product
    contract (PRD req 37) and points nowhere near the actual cause; I bisected against main before spotting my own shell.
    [Filed from the main clone; a duplicate sits in the w-migration-skew worktree buffer - ignore that copy, see DL-009.]
  target: infra
  severity: degrading
  workaround: Test now scrubs every FS3_* key from the child environment before setting the ones it means; mutation-proven
    by re-running with the pollution deliberately exported.
  suggested_encoding: Config layering makes the environment the HIGHEST precedence layer, so any test whose fixture is a config
    file is testing the developer's shell unless it scrubs first. Worth auditing every test that spawns the binary or builds
    a Config, and a testkit helper returning a pre-scrubbed Command would make that decision once rather than per-test - same
    shape as the FS3_TEST_DATABASE_URL gate in this packet.
  fp: 830ddf02f9ec
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-26T23:52:00.275Z'
- id: DL-018
  kind: difficulty
  description: 'Jordan''s daemon failed twice on the same migration-skew error AFTER the fix was built, because TWO flowspace3
    binaries coexisted: the installer had dropped a stale release binary in ~/.local/bin (which shadows /usr/local/bin in
    his PATH) while the dev symlink in /usr/local/bin pointed at the fresh target/release build. which -a showed both; the
    shell ran the stale one. Compounded by the version lie (the stale v0.2.0 asset reports 0.1.0), making it look identical
    to the already-fixed problem.'
  target: project
  severity: degrading
  workaround: removed ~/.local/bin/flowspace3; single binary remains
  suggested_encoding: 'doctor row: detect multiple flowspace3 binaries on PATH with differing versions and name which one
    the shell resolves; auto-updater canonicalization already handles the symlink half'
  fp: b2cc5c572d02
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T03:06:47.312Z'
- id: DL-019
  kind: difficulty
  description: 'The daemon has no log file: tracing goes to stdout only, so when the summarize lane died mid-run the only
    copy of the panic/error evidence was Jordan''s terminal scrollback. Diagnosing a dead lane required asking the human to
    eyeball their terminal. Compounds cheetah''s finding 5 (redirected output is ANSI-polluted).'
  target: project
  severity: degrading
  workaround: asked Jordan to check terminal scrollback; DB state used for everything else
  suggested_encoding: daemon writes a rolling log file (e.g. ~/.local/state/flowspace3/daemon.log, size-capped) in ADDITION
    to stdout, ANSI-free when non-TTY; doctor names the log path; prerequisite for phase-2 self-restart where there is no
    terminal at all
  fp: fb524ce1901d
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T03:37:29.837Z'
- id: DL-020
  kind: difficulty
  description: 'Throwaway test databases accumulate on the shared Postgres with nothing ever reaping them: 30 leftover fs3_daemon_*/fs3_store_*
    databases totalling 244 MB. FreshDatabase::destroy is explicit because Drop cannot await, and its own comment calls a
    leftover database ''visible, harmless, and a truthful record that the run failed'' - which is a deliberate and defensible
    design. What the design did not address is that nothing ever collects them afterwards, so every panicking test run leaks
    one forever. The irony is sharp: this very packet gave fs3''s own data a garbage collector while its test fixtures still
    have none.'
  target: infra
  severity: annoying
  workaround: 'Counted and reported rather than mass-dropping: the names encode a throwaway seed so they are unambiguously
    test residue, but destroying dozens of databases on Jordan''s server unasked is his call, and dropping them would also
    destroy the failed-run record the design deliberately keeps.'
  suggested_encoding: 'A reaper with an age threshold - drop fs3_daemon_*/fs3_store_* databases older than N days - keeps
    the truthful-record property for a recent failure while stopping the unbounded leak. Natural homes: a harness command
    beside the testdb gate, or a step in the same test-support module that creates them, so the thing that leaks is the thing
    that cleans. Worth pairing with the FS3_TEST_DATABASE_URL gate, since both are about test runs leaving marks on a shared
    server.'
  fp: f9a1481157c5
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T03:45:03.459Z'
- id: DL-021
  kind: difficulty
  description: 'PUBLISH-WINDOW OUTAGE: the documented install command is broken for every user for the whole duration of a
    release build. release-please publishes the GitHub Release at tag time; .github/workflows/release.yml only THEN builds
    and ''gh release upload''s the assets + SHA256SUMS. So ''latest'' points at an assetless release for the entire build,
    and install.sh (which resolves latest) dies with a raw ''curl: (22) 404'', exit 22, no binary, and no fs3 error envelope.
    Reproduced live during the v0.3.0 window (published 07:50:46Z, zero assets).'
  target: project
  severity: blocking
  workaround: wait for the build, or pin a known-good tag
  suggested_encoding: have release-please create the release as a DRAFT, upload assets+SHA256SUMS in release.yml, then 'gh
    release edit $TAG --draft=false' as the last step, so 'latest' never resolves to an assetless release; secondarily give
    install.sh a real error message instead of raw curl output
  fp: 426c3318aa05
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-27T07:52:10.664Z'
- id: DL-022
  kind: difficulty
  description: '''Leave the daemon running and it will catch the next release'' is not true out of the box: update.check_interval_hours
    defaults to 24, so a daemon that has just checked sleeps ~24h. Setting up a release-hop verification, my rig would have
    slept straight through the target release. Config is u64 HOURS with a validated minimum of 1, so the fastest automatic
    cadence available is 1 hour - the shortest possible verification loop for this feature is an hour.'
  target: project
  severity: degrading
  workaround: check_interval_hours = 1, restart the daemon, and use 'doctor upgrade' (which ignores the interval) to force
  suggested_encoding: minutes-granularity interval or a 'doctor upgrade --watch' so the feature is testable at human speed
  fp: f87023fdf6cd
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T08:08:43.306Z'
- id: CONF-003
  kind: confusion
  description: README promises 'Every command answers one JSON envelope' and 'ok is the only discriminator', but 'flowspace3
    config show' answers raw TOML with no envelope and no --json flag. An agent trusting the documented invariant and piping
    it to a parser breaks.
  target: project
  severity: annoying
  workaround: treat config show as a human surface
  suggested_encoding: give config show a JSON mode, or qualify the README's absolute claim
  fp: 7e9438133571
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T08:08:43.449Z'
- id: DL-023
  kind: difficulty
  description: No documented way to install a specific version or roll back. install.sh always resolves 'latest'; the only
    pin is the UNDOCUMENTED env var FS3_INSTALL_ASSET_BASE (install.sh:44), which happens to work when pointed at .../releases/download/vX.Y.Z.
    Now that auto-update ships and can move a user's binary unattended, 'how do I go back to the version that worked' is a
    question the product cannot currently answer - and the README's install section offers no pin, no downgrade, and no list
    of installable versions.
  target: project
  severity: degrading
  workaround: FS3_INSTALL_ASSET_BASE=https://github.com/AI-Substrate/flowspace3/releases/download/vX.Y.Z curl ... | sh
  suggested_encoding: document a pinned install (install.sh accepting a version argument) and name the downgrade path in the
    auto-update docs, since auto-update makes rollback a first-class need
  fp: 65661eeb0b99
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T08:12:18.173Z'
- id: DL-024
  kind: difficulty
  description: 'Auto-update state is keyed to the STORE but an install is keyed to a PATH. With two installs on one database
    (root at /usr/local/bin, a user at ~/.local/bin - which is exactly what the installer itself produces depending on permissions)
    the update_state row and message queue thrash last-writer-wins and bleed across installs: a healthy current install showed
    ''cannot update /home/alice/.local/bin/flowspace3 ... not writable'' on its own doctor and envelopes, and later carried
    another install''s ''restart the daemon to pick it up''. Worse, the row is never reconciled against disk - after a pinned
    reinstall at an older tag the row claimed 0.3.1 was installed at a path holding 0.3.0. next_action is NOT NULL to guarantee
    actionability, and these messages are unactionable for the install receiving them.'
  target: project
  severity: degrading
  workaround: run doctor upgrade from the affected install to overwrite the row
  suggested_encoding: key update_state by install path, and verify the on-disk version before declaring 'installed at X'
  fp: 8a100048d4f6
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-27T08:38:28.988Z'
- id: DL-025
  kind: difficulty
  description: 'The egress surface for auto-update is undocumented: it needs github.com AND release-assets.githubusercontent.com.
    I got it wrong myself - blackholed objects.githubusercontent.com (the host GitHub has moved away from) and the download
    succeeded anyway, firing the upgrade early and invalidating that test run. Anyone running fs3 behind an egress allowlist
    discovers this by failure.'
  target: project
  severity: annoying
  workaround: allow release-assets.githubusercontent.com
  suggested_encoding: name the required egress hosts in docs/services/auto-update.md
  fp: de798b328474
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T08:38:29.147Z'
- id: CONF-004
  kind: confusion
  description: 'Stale update:blocked message from a YESTERDAY worktree-debug daemon (path fs3-w-auto-update/target/debug,
    v0.2.0-era text) survived daemon restart and surfaced on Jordan''s add today: the update supervisor starts at boot (every_hours=24)
    but does NOT check immediately, so a fossil standing message sits up to 24h before the producer re-evaluates and self-retracts.
    Confirms cheetah finding 12 (update state keyed to store not install path).'
  target: project
  severity: degrading
  workaround: none needed - message is honest-but-stale and clears on first supervisor cycle
  suggested_encoding: run_once at supervisor start so boot always refreshes/retracts update messages; key update state by
    install path (finding 12 packet)
  fp: 8af654c12ed3
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-27T08:49:59.386Z'
- id: DL-026
  kind: difficulty
  description: Preflight leg C1 (cargo test workspace fast tier) failed transiently while two coder seats were building in
    parallel worktrees, passed clean on immediate rerun — likely cargo registry/target contention; cost one full preflight
    rerun (~10min) on the v0.3.2 cycle
  target: infra
  severity: annoying
  workaround: rerun the full preflight
  suggested_encoding: preflight could retry a failed leg once automatically, or serialize with a build-lock shared with coder
    gates
  fp: cc31d454f7f7
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T09:01:44.299Z'
- id: CONF-005
  kind: confusion
  description: Auto-update's hourly cadence creates a real false-negative window for anyone verifying it. v0.3.2 published
    at 09:09:33Z but my daemon's check fell due at 09:04:59Z, 4.5 min earlier; it correctly saw 0.3.1 as newest and reset
    its clock. For the next 56 minutes the box read 'new release published, daemon still on the old version' - indistinguishable
    from a broken updater. Only a liveness-first read protocol (pid continuity, then last_checked_at advanced past published_at,
    THEN interpret) stopped that becoming a false defect report.
  target: project
  severity: annoying
  workaround: read update_state.last_checked_at against the release published_at before concluding anything
  suggested_encoding: surface 'next check due at T' in the doctor update row so the wait is visible rather than inferred -
    it turns an apparent failure into an obvious pending state
  fp: 641b222e9410
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T10:06:59.777Z'
- id: CONF-006
  kind: confusion
  description: In one doctor output the update row said 'running 0.3.2' while the daemon row said 'version 0.3.1'. Both are
    correct - update.running means this CLI/the binary on disk, daemon.version means the live process - but two rows in the
    same output disagreeing about what is 'running' reads as a contradiction to anyone who has not read the source.
  target: project
  severity: annoying
  workaround: know that the two 'running's mean different things
  suggested_encoding: 'word the update row as ''installed on disk: X'' so it cannot be misread as the live process'
  fp: 4e377dd3d8b2
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T10:06:59.932Z'
- id: DL-027
  kind: difficulty
  description: 'Observation buffers are PER-WORKTREE (.harness/temp is gitignored so each worktree has its own): a seat capturing
    observations in its worktree loses them SILENTLY at git worktree remove. Lamprey caught its own 36-line buffer about to
    die and hand-copied it; I rescued marlin''s 24-line buffer from fs3-test-blast-radius the same way. Unknown whether earlier
    removed worktrees (goat/orlandine) lost captures. Shared buffer also shows per-seat id collisions (3x DL-001, 3x CONF-001).'
  target: infra
  severity: degrading
  workaround: hand-copy worktree buffers to main .harness/temp/agent/ before removal
  suggested_encoding: harness observe should write to one repo-level buffer regardless of worktree (resolve git common dir),
    or worktree-remove guidance must include a buffer-drain step; also namespace observation ids by seat
  fp: 145eb2260bbb
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T11:11:02.466Z'
- id: DL-028
  kind: difficulty
  description: 'Phantom one-off red on cargo test --all, now seen by TWO seats (marlin during w-test-blast-radius: exit 101
    then 5 consecutive greens; tick during w-read-surface-sweep: one red harness checks then 3 greens + CI green), neither
    reproducible, truncated output named no failing test. Tick suspects a shared-5433 interaction rather than seat code.'
  target: infra
  severity: annoying
  workaround: rerun; treat one-off --all reds as environmental until a test name surfaces
  suggested_encoding: harness checks should preserve the full cargo test output on failure (truncation hid the failing test
    name both times); if it recurs, bisect shared-DB interactions
  fp: 4a63aae52297
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T11:21:20.902Z'
- id: DL-029
  kind: difficulty
  description: 'Auto-update would silently CLOBBER a dev symlink install: Jordans /usr/local/bin/flowspace3 -> target/release
    symlink counts as an install path; had his binary still reported 0.3.2 when the supervisor checked against published 0.4.0,
    the atomic-rename swap would have replaced the symlink with a downloaded release binary, silently breaking the rebuild-picks-up-immediately
    dev flow. Averted today by rebuilding before the check fired.'
  target: project
  severity: degrading
  workaround: rebuild target/release before the supervisor tick so running==latest
  suggested_encoding: updater detects install_path is a symlink and goes notify-only (a symlink is a decision someone made;
    overwriting it un-makes it silently)
  fp: 21543f48e350
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T11:43:16.832Z'
- id: DL-030
  kind: difficulty
  description: 'Migration 0012 (re-key update_state by install_path) is DESTRUCTIVE: select * from update_state returned 0
    rows immediately after it ran. The old singleton table ALREADY stored install_path (populated with /usr/local/bin/flowspace3),
    so the row was preservable by construction and was dropped instead. Observable harm: the pending ''flowspace3 0.4.0 is
    installed, restart the daemon'' message was destroyed, so doctor read ''update: ok ... never checked'' and ''messages:
    no standing messages'' while the daemon was still running 0.3.2. The user was covered only because the same release carried
    migrations and the SCHEMA producer raised its own error - a coincidence of this release''s shape, not a guarantee. Also
    discards last_checked_at/latest_seen/installed_at, and the resulting ''never checked'' state spends an extra probe immediately
    after restart.'
  target: project
  severity: degrading
  workaround: restart the daemon; the schema-skew producer happens to name the same remedy
  suggested_encoding: 0012 should carry the old row forward into the new key using the install_path it already stored, rather
    than starting empty
  fp: 3f31dd634cff
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T12:08:35.251Z'
- id: CONF-007
  kind: confusion
  description: 'Correction to my own DL entry on migration 0012: I called the schema-producer cover a coincidence of v0.4.0''s
    shape. It is STRUCTURAL. At the instant 0012 runs, any install holding a pending update message must be a pre-0.4.0 binary,
    because a 0.4.0 daemon can only be running if boot::serve already migrated - which means 0012 already ran and there is
    no row to drop. A pre-0.4.0 binary carries migrations <=0011 against a database now at 0013, so the schema producer raises
    the same remedy by construction. Holds for CLI-only installs too. What is guaranteed is that the user is told to act,
    NOT that the update producer''s own notification survives. Ruled a craft defect with NO fix packet (o-prime): 0012 has
    shipped and run everywhere, and residual harm self-heals in seconds under the 0.4.0+ supervisor boot-check.'
  target: project
  severity: annoying
  workaround: n/a - analysis correction
  suggested_encoding: when a defect's blast radius depends on migration ordering, work the case split before calling coverage
    accidental; ordering constraints often make coverage structural
  fp: e979d339c260
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T12:10:33.657Z'
- id: DL-031
  kind: difficulty
  description: 'prototyping pij-team pipeline: dd schema authoring for custom doc types (settings/packet/impl-guide) worked
    first try via .dd/schemas/<ns>/ — pluggable schema roots are a strong substrate for templated team docs'
  target: skill
  severity: annoying
  workaround: No workaround recorded in the snapshot.
  suggested_encoding: No encoding recorded in the snapshot; retained for traceability.
  fp: 8240ace62b83
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T23:01:51.960Z'
- id: CONF-008
  kind: confusion
  description: 'canary ceremony gap: seats sometimes answer the canary in their own terminal instead of via pij send - the
    canary text must explicitly instruct reply-via-pij-tooling (Jordan ruling 2026-08-28)'
  target: tooling
  severity: annoying
  workaround: No workaround recorded in the snapshot.
  suggested_encoding: No encoding recorded in the snapshot; retained for traceability.
  fp: f08377107908
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T23:24:32.102Z'
- id: DL-032
  kind: difficulty
  description: ddocs build fails with E401 schema-not-found in a fresh worktree because .dd/schemas/pij-team/ is untracked
    in main, so git worktree add never carries it across; the same is true of .agents/skills/pij-team/ (templates)
  target: skill
  severity: degrading
  workaround: team-new POC copies .dd/schemas/pij-team/ from the main clone into the new worktree and records it as a named
    fudge in its envelope
  suggested_encoding: commit .dd/schemas/pij-team/ and .agents/skills/pij-team/ so every worktree has them; then the extension
    needs no copy step
  fp: ab3a4d28acc4
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-27T23:33:58.372Z'
- id: CONF-009
  kind: confusion
  description: 'ddocs build resolves schemas from CWD, not from the document''s path ancestors: ''cd docs/plans/X && ddocs
    build impl-guide.dd.json'' fails E401 while ''ddocs build docs/plans/X/impl-guide.dd.json'' from the repo root succeeds
    on the same file'
  target: skill
  severity: annoying
  workaround: always invoke ddocs build with cwd set to the repo/worktree root and pass a path
  suggested_encoding: discover schema roots by walking up from the document path as well as from CWD, or say the resolution
    rule in the E401 message
  fp: bdd64cd735c8
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T23:33:58.519Z'
- id: DL-033
  kind: difficulty
  description: git worktree remove refuses a freshly scaffolded plan worktree because the new docs/plans/NNN-slug/ folder
    is untracked; --force is always required to tidy an unstarted plan worktree
  target: skill
  severity: annoying
  workaround: git worktree remove --force
  suggested_encoding: the team extension's teardown path should use --force and say why, or commit the scaffold immediately
    after creating it
  fp: 32be1b932aac
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T23:33:58.672Z'
- id: DL-034
  kind: difficulty
  description: 'no pij whoami: a spawned seat cannot cheaply learn its own pij id; PIJ_SPAWN_ID/PIJ_PARENT_ID are exported
    but not the seat''s own id, and grep -rl of ~/.pij to find it ran 300s before timing out'
  target: tooling
  severity: degrading
  workaround: grep the spawn id against ~/.pij/<id>/events.ndjson paths
  suggested_encoding: export PIJ_SELF_ID in the spawned environment, or add pij whoami
  fp: 299fd8f8da40
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T23:33:58.823Z'
- id: CONF-010
  kind: confusion
  description: 'CORRECTION superseding DL-034: pij whoami DOES exist and prints session id, folder, data dir, state, role
    (verified by running it). My DL-034 claim that it did not exist was wrong. The real gap is discoverability: whoami sits
    at line 67 of pij''s 90-line top-level help under a late section, while identity-adjacent commands (identity release,
    compact-self, canary) appear in the early Control-plane block, so an agent reading the top of the help and stopping concludes
    there is no whoami. My own error compounded it: I read only the first 30-40 help lines, then resorted to grep -rl of ~/.pij
    which hung for 300s'
  target: tooling
  severity: annoying
  workaround: run pij whoami; it was there all along
  suggested_encoding: 'surface whoami in the Control-plane block at the top of pij help (it is the first thing a fresh seat
    needs), and/or export PIJ_SESSION_ID into spawned seats so no lookup is needed at all; also: read the WHOLE help before
    concluding a verb is missing'
  fp: 6f41e4631eeb
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T23:40:05.168Z'
- id: DL-035
  kind: difficulty
  description: 'flowspace3 watcher indexes gitignored directories: a watcher event inside an ignored dir (scratch/, .claude/,
    .harness/temp/) makes discovery walk with that dir as the WALK ROOT, so a trailing-slash gitignore pattern (e.g. ''scratch/'')
    never matches — 886 gitignored harness-engineering scratch files were scanned, 4436 raw vectors and 222 summaries were
    bought for content the index cannot return'
  target: project
  severity: degrading
  workaround: none applied; read-only investigation
  suggested_encoding: discovery must re-anchor the ignore matcher at the WORKTREE ROOT when walking a subdirectory, or the
    watcher must refuse to walk a directory the root walk would have excluded
  fp: 1657414a7342
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-27T23:46:56.446Z'
- id: DL-036
  kind: difficulty
  description: 'asymmetric spend guard in crates/daemon/src/enrich.rs: summarize() gates on fs3_store::raw_hash_is_referenced
    (line 389) so unreferenced content costs nothing, but embed()/embed_items() has NO equivalent reference guard — it only
    dedupes on existing_embedding_hashes. Result: 4436 raw vectors were paid for content no worktree maps, while ~26k summarize
    jobs for the same content were correctly skipped free'
  target: project
  severity: degrading
  workaround: none; read-only
  suggested_encoding: apply the same held_by_a_live_root predicate in embed_items before the provider call, filtering unreferenced
    source_hashes out of the batch
  fp: c86497250c9e
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-27T23:47:05.947Z'
- id: CONF-011
  kind: confusion
  description: 'crates/daemon/src/scan.rs module doc contradicts its own code: the header claims ''a scan that does run over
    already-known bytes writes nothing and enqueues no enrichment'', but run() line 103-108 deliberately RE-EMITS enrichment
    via enrich::enqueue_for_tree on the content-addressed skip (and first_light.rs::a_scan_whose_parse_already_landed_still_enqueues_its_enrichment
    asserts exactly that). The stale doc is what makes a queue surge look like a bug'
  target: project
  severity: annoying
  workaround: No workaround recorded in the snapshot.
  suggested_encoding: fix the module header to state that the skip re-emits downstream work, matching the inline comment at
    line 88-102
  fp: 3314cdf0ebc4
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T23:47:06.095Z'
- id: DL-037
  kind: difficulty
  description: 'pij-team packet template defect found by PM at ack: packet refs pointed at gitignored scratch/ paths invisible
    to dispatched worktrees; ruled vendor-into-assets/inputs sha-pinned; encoded in SKILL.md refs rule'
  target: skill
  severity: degrading
  workaround: No workaround recorded in the snapshot.
  suggested_encoding: No encoding recorded in the snapshot; retained for traceability.
  fp: e8089b6008d1
  disposition: fixed-now
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-28T00:13:47.419Z'
- id: DL-038
  kind: difficulty
  description: 'pij#19 phantom-alias defect LIVE and multiplying: narwhal (pid 74138) minted at least 2 alias registry ids
    (shrill-haddock, previous-riss) with identical spawnId during parallel harvester work; ready-pings from aliases route
    to o-prime as if new seats; protocol held (aliases never addressed)'
  target: tooling
  severity: degrading
  workaround: No workaround recorded in the snapshot.
  suggested_encoding: No encoding recorded in the snapshot; retained for traceability.
  fp: 4e851dbf860b
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-28T00:18:15.723Z'
- id: DL-039
  kind: difficulty
  description: 'pij send fails from inside a git worktree: E-AMBIG ''cannot resolve self: no local session and PIJ_SESSION_ID
    unset''. pij resolves the seat by FOLDER, and a worker on the worktree-per-coder model works in ../fs3-<packet>, not the
    seat folder — so the report-back path breaks exactly when a worker has something to report'
  target: tooling
  severity: degrading
  workaround: run pij send from the seat folder, or prefix PIJ_SESSION_ID=<seat> (I used PIJ_SESSION_ID=pij-corresponding-duck)
  suggested_encoding: export PIJ_SESSION_ID into spawned seats (same fix CONF-010 asks for), or have pij walk up from cwd
    to the enclosing git common-dir and resolve the seat from the main clone's folder
  fp: 451907d42067
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-28T00:27:40.600Z'
- id: DL-040
  kind: difficulty
  description: 'pij revive fails E-NOREG (pi native session artifact is missing) for two dead seats whose omp session jsonl
    DEMONSTRABLY EXISTS on disk (narwhal: 1MB at sessions/-substrate-flowspace-fs3-convo-ingest/...) - registry session-link
    likely scrambled by the pij#19 alias minting; revive unusable exactly when needed after a mass seat death at 00:30:13Z'
  target: tooling
  severity: blocking
  workaround: No workaround recorded in the snapshot.
  suggested_encoding: No encoding recorded in the snapshot; retained for traceability.
  fp: ed396e2a5e6c
  disposition: kept
  system:
    source_buffer: main-session-buffer.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-28T00:31:57.902Z'
- id: DL-001
  kind: difficulty
  description: 'cargo test --all migrated the PRODUCTION flowspace3 database (5433/flowspace3), not just flowspace3_test.
    Evidence: _sqlx_migrations version 12 installed_on 09:16:40Z in db ''flowspace3'' vs 09:16:24Z in ''flowspace3_test'',
    both inside my single ''harness checks'' run 09:14:45-09:17:32; my worktree is the only source of migration 0012 on this
    machine. Consequence: the installed flowspace3 0.3.1 now hard-refuses every command with the schema-ahead error until
    a binary carrying 0012 is installed - Jordan''s CLI is down. The production-database ruling and crates/cli/tests/doctor_daemon.rs:19-24
    exist for exactly this and did not hold; I could not identify the culprit test in-context.'
  target: infra
  severity: blocking
  workaround: 'None applied. Recovery is to install a binary carrying 0012 (merge PR #27, or cargo install --path crates/cli
    from the branch). Do NOT delete the _sqlx_migrations row: the schema change is applied, so the row is the only thing telling
    a binary the truth.'
  suggested_encoding: A gate that fails the run if any database other than FS3_TEST_DATABASE_URL gained a migration during
    cargo test --all - snapshot _sqlx_migrations max(version) for the configured database.url before and after the test gate
    and diff it. Cheap, deterministic, and it names the offending run instead of the next person's broken CLI.
  fp: a00ed01c8aab
  disposition: fixed-now
  system:
    source_buffer: w-update-truth-observations.md
    compound:
      status: encoded
      source: agent-self
      first_seen_at: '2026-08-27T09:21:15.663Z'
- id: CONF-001
  kind: confusion
  description: flowspace3 search missed the implementation for a meaning-shaped question. Asked 'where does the daemon decide
    it is time to check for a new release'; the top 10 were UpdateConfig (a config struct), four auto_update integration TESTS,
    doctor's check_daemon, and three planning docs. The actual answers - fs3_store::claim_check in crates/store/src/updates.rs
    and UpdateSupervisor::reconcile in crates/daemon/src/update.rs - did not appear at all. This repo writes long doc comments
    on tests, and they appear to outrank the implementations they are about.
  target: project
  severity: degrading
  workaround: Read the code directly; I already knew where to look.
  suggested_encoding: Either down-weight test files for meaning-shaped queries, or expose a scope flag (search --exclude-tests)
    so an agent can ask for the implementation rather than its proof.
  fp: f42246a45020
  disposition: kept
  system:
    source_buffer: w-update-truth-observations.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T09:21:24.008Z'
- id: DL-002
  kind: difficulty
  description: 'docker compose up -d fails in a per-coder worktree: the compose project name is derived from the directory
    (fs3-update-truth_default), but the container name flowspace3-db is fixed, so it collides with the one the main clone
    already owns - and it silently creates an orphan network and volume (fs3-update-truth_flowspace3-pgdata) before failing.
    harness boot also reports ''compose: service db is not running'' in a worktree even though the shared container is up
    and healthy, so boot says degraded when the stack is fine. This bites every seat now that worktree-per-coder is the cutover
    workflow (2026-08-27).'
  target: infra
  severity: degrading
  workaround: docker start flowspace3-db, and read harness boot's compose row as advisory when working in a worktree.
  suggested_encoding: Pin COMPOSE_PROJECT_NAME=flowspace3 in docker-compose.yml or an .env at the repo root so every worktree
    addresses the ONE shared stack, and have harness boot check the shared container by name rather than by compose project.
  fp: 9a879094db32
  disposition: kept
  system:
    source_buffer: w-update-truth-observations.md
    compound:
      status: open
      source: agent-self
      first_seen_at: '2026-08-27T09:21:32.572Z'
---

# Retro — fleet observation snapshot, 2026-08-28

## Scope and accounting

- **Input:** six immutable files from `.harness/temp/retro-snapshot-2026-08-28/`; live buffers were neither read into this drain nor cleared.
- **Count:** 61 observations: 49 `difficulty`, 12 `confusion`; 6 blocking, 35 degrading, 20 annoying.
- **Traceability:** duplicate ids are real because buffers are per-worktree. Body references are source-qualified (`main:`, `convo:`, `team:`, `blast:`, `watcher:`, `truth:`). Frontmatter preserves each original `id` and records its exact filename in `system.source_buffer`.
- **Disposition:** all 61 were retained. None were dropped. `main:DL-031` is positive evidence filed as a difficulty; it remains verbatim rather than being silently reclassified.

## Post-snapshot addendum — disk event and successor mechanics

After the snapshot was taken, the host disk event killed multiple seats. The predecessor assigned this drain died about 60 seconds after spawn and produced no work; this successor had to repeat canary, identity, brief-read, worktree creation, and harness boot. Exact fleet death count is not present in the snapshot, so it is not invented here. This addendum is not included in the 61-entry totals.

The event raises the cost of three already-observed gaps: fleet resources become visible only after outage (`main:DL-006`), `pij revive` failed for two dead seats despite native session artifacts existing (`main:DL-040`), and successors cannot tell whether inherited phase work ever had a green gate (`convo:DL-002`). The recovery path worked only because o-prime retained the brief and snapshot outside the dead seat.

## Theme synthesis

| Theme | Count | Status at 2026-08-28 | Source-qualified observations |
|---|---:|---|---|
| Release, install, and update truth | 20 | 7 encoded, 13 open | main:CONF-001, main:CONF-004, main:CONF-005, main:CONF-006, main:CONF-007, main:DL-003, main:DL-004, main:DL-007, main:DL-010, main:DL-011, main:DL-012, main:DL-013, main:DL-018, main:DL-021, main:DL-022, main:DL-023, main:DL-024, main:DL-025, main:DL-029, main:DL-030 |
| Agent read and observability surfaces | 8 | 8 open | main:CONF-002, main:CONF-003, main:DL-008, main:DL-014, main:DL-015, main:DL-016, main:DL-019, truth:CONF-001 |
| Test and database isolation | 6 | 2 encoded, 4 open | blast:DL-001, main:DL-017, main:DL-020, main:DL-026, main:DL-028, truth:DL-001 |
| Worktree lifecycle and shared infrastructure | 5 | 5 open | convo:DL-001, main:DL-009, main:DL-027, team:DL-001, truth:DL-002 |
| pij identity, messaging, and recovery | 7 | 7 open | main:CONF-008, main:CONF-010, main:DL-005, main:DL-034, main:DL-038, main:DL-039, main:DL-040 |
| pij-team schemas, templates, and packet inputs | 6 | 2 encoded, 4 open | main:CONF-009, main:DL-031, main:DL-032, main:DL-033, main:DL-037, team:DL-002 |
| Watcher ignore and enrichment correctness | 3 | 2 encoded, 1 open | main:CONF-011, main:DL-035, main:DL-036 |
| Evidence discipline and phase proof | 3 | 3 open | convo:DL-002, main:DL-002, main:DL-006 |
| Shell-safe prose transport | 2 | 2 open | blast:DL-002, main:DL-001 |
| Mutation-test correctness | 1 | 1 open | watcher:DL-001 |

## Landed or encoded before this drain

Snapshot `system.compound.status: open` values were stale. The following were adjudicated against current `main` and merged PRs; frontmatter marks them `fixed-now` / `encoded`:

- **main:CONF-001 — fixed/encoded:** PR #13 distinguished historical release assets and added regressions.
- **main:CONF-004 — fixed/encoded:** PR #27 runs update reconciliation at boot.
- **main:DL-003 — fixed/encoded:** PR #22 made tag cycles draft-first and undrafts only after assets exist.
- **main:DL-010 — fixed/encoded:** PR #15 added the lock gate and committed Cargo.lock.
- **main:DL-012 — fixed/encoded:** PR #16 stitched release-please into Cargo.toml; current workspace version is 0.4.0.
- **main:DL-017 — fixed/encoded:** the spawning test now scrubs FS3_* and was mutation-proven.
- **main:DL-021 — fixed/encoded:** PR #22 keeps releases draft until binaries and SHA256SUMS are attached.
- **main:DL-024 — fixed/encoded:** PR #27 keyed update state per install path and reconciled disk state.
- **main:DL-032 — fixed/encoded:** PR #35 committed the pij-team schemas and skill assets.
- **main:DL-035 — fixed/encoded:** PR #38 re-anchored watcher ignore handling.
- **main:DL-036 — fixed/encoded:** PR #38 added the live-reference guard before embedding.
- **main:DL-037 — fixed/encoded:** PR #35 encoded sha-pinned vendoring into packet assets/inputs.
- **truth:DL-001 — fixed/encoded:** PR #32 added sealed subprocesses, provenance refusal, and prodguard before/after the test gate.

No item is marked in-flight at drain time. The brief’s in-flight watcher items (`main:DL-035`, `main:DL-036`) landed in PR #38 before this record was authored. `main:DL-034` is superseded factually by `main:CONF-010`, but the discoverability/env-export gap remains open.

## Top 5 — implementation order

### 1. Add a disk-capacity gate for worktree-per-coder builds

- **Evidence:** the post-snapshot disk-full event killed multiple seats, including this drain’s predecessor; `main:DL-006` says fleet resource pressure appears first as outage; `main:DL-020` found 30 leaked test databases (244 MB); `convo:DL-001`, `team:DL-001`, and `truth:DL-002` show worktrees also leave stray Docker networks and volumes.
- **Cost:** mass seat loss, abandoned context, failed builds with misleading downstream errors, and unsafe cleanup because ownership is unknown.
- **Encoding:** add `harness capacity` (called by boot and before full builds/worktree spawn): enforce a configurable free-space reserve, report largest repo/worktree targets and Docker assets with owner labels, recommend only ownership-safe reclamation, and support a shared per-repo Cargo target directory where isolation permits it.
- **Where:** harness/pij platform, with a flowspace3 harness extension for Cargo/Docker inventory.

### 2. Make successor recovery a first-class, proven transition

- **Evidence:** `main:DL-040` says revive failed for two dead seats even though native session artifacts existed; `main:DL-005` and `main:DL-038` show alias corruption around the same identity layer; `convo:DL-002` forced a successor to rerun the whole gate because no phase proof existed; the post-snapshot predecessor died before work and this successor repeated setup manually.
- **Cost:** dead work cannot be distinguished from unproved work, identity repair consumes o-prime attention, and every successor pays a rediscovery/gate tax.
- **Encoding:** add `pij successor <dead-id>` that resolves the native artifact when registry links are stale, creates/rebinds one canonical successor id, and injects the durable assignment pointer. At every phase edge, pij-team writes a green receipt containing command, exit code, timestamp, commit/tree SHA, and artifact path; successor reports the newest valid receipt instead of inferring state from files.
- **Where:** pij platform plus the pij-team phase template.

### 3. Make observation storage worktree-safe and crash-safe

- **Evidence:** `main:DL-009` stranded observations in a worktree that was about to be removed; `main:DL-027` records two rescued buffers, unknown losses from removed worktrees, and colliding ids. At least three seats were affected before this drain.
- **Cost:** the harness silently deletes the evidence needed to improve itself at the exact close-out step that tells workers to remove worktrees; fleet counts and ids are not trustworthy without manual rescue.
- **Encoding:** `harness observe` resolves storage from the Git common directory, uses append-safe repo-global ids with the seat id recorded separately, and `harness worktree close` refuses while an uncommitted local buffer remains. Keep drain/clear single-writer semantics.
- **Where:** harness observe/record subsystem.

### 4. Ship a mutation-test verb that cannot reuse stale Cargo artifacts

- **Evidence:** `watcher:DL-001`; one seat restored with `mv`, preserved the old mtime, linked the mutant rlib, got false test verdicts from three binaries, and lost about 15 minutes chasing a phantom store bug.
- **Cost:** mutation evidence can assert the opposite of what ran. This is proof corruption, not test inconvenience.
- **Encoding:** `harness mutate <file> -- <test-command>` records original bytes/hash, applies the mutation, forces a monotonic mtime before the red run, restores bytes, forces mtime again before the green run, and refuses success unless the final hash matches and both expected verdicts occurred.
- **Where:** harness core.

### 5. Make boot test service reachability, not worktree ownership

- **Evidence:** `convo:DL-001`, `team:DL-001`, and `truth:DL-002` independently hit the same bug: boot used worktree-local `docker compose exec`, reported degraded despite healthy Postgres on `127.0.0.1:5433`, and attempts to follow the implied fix created orphan resources. Reproduced again while authoring this record; that live observation stayed outside this snapshot drain.
- **Cost:** every coder starts from a false degraded verdict, wastes time proving the stack is healthy, and can create disk-consuming Docker debris by trying to repair a non-failure.
- **Encoding:** boot probes the configured database URL with `pg_isready`/TCP first and reports `ok-shared`; compose ownership is a separate diagnostic. Never prescribe `docker compose up` from a secondary worktree when the configured external service is reachable.
- **Where:** flowspace3 harness boot extension and Docker configuration.

## Ruthless exclusions from the top five

- **Production-database safety:** weighed and not ranked because the blocking incident is closed. PR #32 added sealed subprocesses, provenance refusal, and a prodguard before/after the test gate (`truth:DL-001`, `main:DL-017`). Shared scratch-DB skew and reaping (`blast:DL-001`, `main:DL-020`) remain open but are lower leverage than the five above.
- **Gitignored-reference/blind-spot family:** weighed, not deferred. `main:DL-032` and `main:DL-037` were encoded by PR #35; `main:DL-035` and `main:DL-036` landed in PR #38. Remaining schema-CWD and rendered-sibling items are real but lower-cost.
- **Release cluster (20 entries):** largest by count, but not one root cause. PR #16 fixed binary/version drift; PR #22 fixed re-draft and assetless-latest outages; PRs #13/#27 fixed several update-state defects. The remaining installer, rollback, cadence, and observability items need separate product packets; merging them would create a fake mega-item.
- **Shell-safe prose transport:** `main:DL-001` and `blast:DL-002` remain important, but `pij send --body-file` and `gh pr create --body-file` now exist. The residual `harness commit -F` gap is smaller than the top five.
- **No padding:** `main:DL-031` is a gift filed under `difficulty`; it is evidence that pluggable ddoc schemas worked, not an implementation priority.
