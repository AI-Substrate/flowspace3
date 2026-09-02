---
record_kind: "retro"
harness_version: "0.14.0"
branch: "main"
repo: "https://github.com/AI-Substrate/flowspace3.git"
created_at: "2026-09-02T07:15:00Z"
agent: "o-prime (pij-binding-magpie)"
plan_id: "013-search-admission"
schema_version: "1.2"
retro_id: "2026-09-02T07:15:00Z-o-prime-magpie-013"
started_at: "2026-09-02T01:38:00Z"
ended_at: "2026-09-02T07:12:00Z"
summary: "Plan 013 (search-admission, PR #101, merged c2f4709) rewrote search_elements so the correlated smart_content probe (962,792 loops, 1,667 ms per search in the profile; 10,696 ms pg_stat mean on prod) is resolved once per HNSW page. The coder (amistad) fought the migration cost of the new :5434 test postmaster (unreachable, pg_hba, a 648 s ANALYZE that crashed the shared :5433 postmaster, a false CRITICAL STOP when o-prime bounced prod inside the held gate slot) and shipped green. The reviewer (carp) then found a CRITICAL regression the acceptance criteria did not cover: admission moved above the vector LIMIT turned a repo-scoped search behind 12,000 nearer foreign vectors from 5 hits in one 29 ms pass into a 9-pass, 246k-block hard ERROR - the exact geometry search_scope_starvation.rs defends. O-prime's fix ruling then demanded two mechanically incompatible properties; the coder's ask-011 caught it and the ruling was corrected to option B (cheap scope keys back inside candidate_vectors, expensive resolution page-bounded above, a scan_incomplete carrier on the envelope). The post-merge receipts were blocked twice by prod incidents that were not 013's: a pij-government seat's daemon squatted :7373 (row 165) and a second foreign daemon clobbered the shared daemon.key without ever binding (row 169), plus two bin/daemon-restart failures (rows 164/167). Once prod ran the 013 binary: ac-0004 EXPLAIN 35 ms / 8,063 buffers / 1 smart loop, and ac-0005 sixteen runs at 0.45-0.77 s (8.28 -> 0.56 s, 14.45 -> 0.70 s) with load coupling gone; all five ACs TRUE."
entries:
- {id: HB-DL-001, kind: difficulty, severity: annoying, fp: 27da8759b537, first_seen_at: "2026-09-02T04:47:51.467Z", description: "o-prime wrote plan 015 without citing the repo's add-language skill / docs/services/scanner.md five-step contract; the coder found it and had to stop-and-ask for the allow-list line — a flowspace3 search for 'how do I add a language grammar' would have surfaced it", workaround: "fence amended by ruling; coder follows the contract", suggested_encoding: "pij-team plan template: a mandatory 'repo how-to cited' line per change class, found via flowspace3 search"}
- {id: HB-CONF-001, kind: confusion, severity: degrading, fp: 31d0e3b23877, first_seen_at: "2026-09-02T05:31:50.721Z", description: "o-prime's fix ruling for the 013 CRITICAL required both 'admission above the HNSW page' and 'return scoped rows buried behind 12,000 nearer vectors' — mechanically impossible; the coder's stop-and-ask caught it and the ruling was corrected to keep cheap scope predicates inside the scan", workaround: "ruled option B after the ask", suggested_encoding: "reviewer 'smallest fix' text is a hypothesis: o-prime restates the PROMISE and lets the coder pick the mechanism; never prescribe architecture in a ruling"}
- {id: HB-DL-002, kind: difficulty, severity: degrading, fp: 3dcfb3624941, first_seen_at: "2026-09-02T06:04:40.843Z", description: "bin/daemon-restart refused with two 'flowspace3 daemon' candidates (a pij test process without a port + prod on :7373); o-prime's receipt script ignored the non-zero exit and measured the OLD daemon", workaround: "manual bounce in the daemon pane; re-run the receipt", suggested_encoding: "daemon-restart selects by :7373 listener/pane; receipt scripts assert new pid != old pid"}
- {id: HB-DL-003, kind: difficulty, severity: blocking, fp: 9ff9c3828638, first_seen_at: "2026-09-02T06:58:11.692Z", description: "a foreign 'flowspace3 daemon' that lost the :7373 bind still overwrote ~/.config/flowspace3/daemon.key, locking every client out of prod (FS3-E-DAEMON-UNAUTHORIZED) until o-prime bounced prod", workaround: "manual bounce of prod to republish the key", suggested_encoding: "publish the key only after the listener binds; key file keyed by port; refuse the prod database without owner designation"}
- {id: DL-013C-001, kind: difficulty, severity: degrading, fp: 3abe25b65583, first_seen_at: "2026-09-02T01:39:48.967Z", description: "Serena initial-instructions MCP timed out twice before coding; the direct Rust LSP is configured, so continue via xd://lsp and report the MCP path as degrading friction.", workaround: "Use the configured xd://lsp rust-analyzer surface.", suggested_encoding: "Make Serena initial-instructions return a bounded actionable timeout envelope or remove it as a mandatory duplicate when native LSP is healthy."}
- {id: CONF-013C-001, kind: confusion, severity: degrading, fp: 3fbed9e54915, first_seen_at: "2026-09-02T01:41:28.355Z", description: "RS identity mismatch during canary: plain pij whoami returned pij-imperial-weasel, but pij whoami --json refuses E-RS and pij node show says E-NOID; spawn metadata had to come from PIJ_SPAWN_ID.", workaround: "Use plain pij whoami plus PIJ_SPAWN_ID and the runtime-provided model; do not pij adopt.", suggested_encoding: "Give RS whoami a schema-safe JSON envelope and make its returned identity queryable by node/session inspection."}
- {id: DL-013C-002, kind: difficulty, severity: degrading, fp: 72ad398d1d18, first_seen_at: "2026-09-02T01:43:11.748Z", description: "Worktree harness docker up cannot start the isolated stack: compose hard-codes /flowspace3-db, which conflicts with the already-running main/prod container; boot therefore reports db stopped although a same-named container exists.", workaround: "Use FreshDatabase against the required :5433 endpoint with a seat-specific database label; never target the prod daemon on :7373.", suggested_encoding: "Make worktree compose container names project-scoped and make boot distinguish name collision from a stopped service."}
- {id: DL-013C-003, kind: difficulty, severity: blocking, fp: 75785867247a, first_seen_at: "2026-09-02T01:43:21.921Z", description: "Approved implementation is blocked before task 0101: node_modules/.bin/ddocs is absent, matching harness doctor, but builder discipline and the prime ruling require ddocs for task progress and the ac-0005 amendment. Manual edits are forbidden.", workaround: "Stop and ask o-prime for the canonical installed command or permission to restore the package; do not run npx or hand-edit deterministic documents.", suggested_encoding: "Boot should fail fast when an active plan requires deterministic-document mutation but the repo-local ddocs CLI is absent."}
- {id: DL-013C-004, kind: difficulty, severity: degrading, fp: 1a2b983e9239, first_seen_at: "2026-09-02T01:45:11.808Z", description: "rust-analyzer LSP references for exported search_elements returned no references despite verified imports/calls in crates/store/tests and daemon code. Symbol-level coverage cannot be trusted for this packet.", workaround: "Use exact-identifier repository search for callers and verify every edited span by exact read; do not use LSP rename.", suggested_encoding: "Add a harness LSP canary that queries a known exported symbol and fails when rust-analyzer returns zero despite textual callsites."}
- {id: DL-013C-005, kind: difficulty, severity: blocking, fp: 5128c3f475c1, first_seen_at: "2026-09-02T02:06:18.325Z", description: "Focused search_plan_shape test ran 648 seconds then lost the Postgres connection with UnexpectedEof during EXPLAIN. The prod-shaped old-query mutation is too expensive/unsafe for a routine test and leaked its scratch database on panic.", workaround: "Do not rerun. Inspect the failing EXPLAIN boundary, clean only the named scratch database, and redesign mutation proof to avoid ANALYZE on the pathological old plan while retaining new-query ANALYZE.", suggested_encoding: "Add a bounded query timeout to plan-shape tests and make scratch database cleanup panic-safe with the shared fs3_testkit helper."}
- {id: DL-013C-006, kind: difficulty, severity: degrading, fp: b36089175cbd, first_seen_at: "2026-09-02T02:09:33.814Z", description: "harness commit created WIP commit 27d214fe45f3b1e89b0078cef68672d23f336ef7 in direct-verified mode but reported verify=missing: the refs/notes/ai attribution note did not land.", workaround: "Keep the named commit SHA and report the missing note to o-prime; do not rewrite or rollback the commit.", suggested_encoding: "Make the collector health gate prevent direct-verified from sounding healthy when the post-commit note is missing, and provide an in-command repair receipt."}
- {id: DL-013C-007, kind: difficulty, severity: blocking, fp: cb0abcfcbe64, first_seen_at: "2026-09-02T02:11:26.177Z", description: "First focused test on the newly ruled :5434 test postmaster failed before DDL: maintenance connection to postgres timed out after 5 seconds. No container operation or retry was attempted.", workaround: "Stop and ask o-prime to confirm readiness/credentials for flowspace3-db-test; never fall back to :5433.", suggested_encoding: "Add the separate test-postmaster readiness probe to harness boot and make the test command wait boundedly with a clear service-not-ready verdict."}
- {id: DL-013C-008, kind: difficulty, severity: blocking, fp: 753cefaa3994, first_seen_at: "2026-09-02T02:28:31.092Z", description: "Separate :5434 test postmaster answers flowspace3_test but rejects the standard maintenance connection to database postgres with SQLSTATE 28000 (no pg_hba.conf entry for host 192.168.97.1/user flowspace3/database postgres). FreshDatabase-style isolation cannot create per-run databases.", workaround: "Stop and ask o-prime to allow the maintenance database or provide an approved admin URL; never use :5433 or weaken isolation locally.", suggested_encoding: "Make test-postmaster readiness prove the exact maintenance URL and CREATE/DROP scratch-database contract used by FreshDatabase, not only select 1 on the base database."}
- {id: DL-013C-009, kind: difficulty, severity: blocking, fp: c76d6a7b86d0, first_seen_at: "2026-09-02T02:31:40.183Z", description: "On the separate :5434 test postmaster, the rewritten shipped search query exceeded the binding 30-second statement_timeout on the 50k-element/10k-smart corpus. The red contract verdict occurred before the old non-ANALYZE mutation check.", workaround: "Stop immediately; do not rerun or raise the timeout. Ask o-prime before using non-ANALYZE plan inspection to redesign the join while preserving HNSW ordering.", suggested_encoding: "Keep the 30-second statement_timeout and separate test postmaster as permanent backpressure; add a cheap non-ANALYZE preflight that names loss of the vector index before ANALYZE runs."}
- {id: CONF-013C-002, kind: confusion, severity: blocking, fp: 4d2d395807b5, first_seen_at: "2026-09-02T02:33:33.534Z", description: "RS inbox truncated o-prime's inline ask-006 ruling after 'Assert in the…', then pij tail could not recover it because pij-binding-magpie is not in the registry (E-NOID). The binding test assertion is missing.", workaround: "Ask o-prime to persist and resend the full ruling as a file pointer; do not infer the truncated contract.", suggested_encoding: "RS inbox should spill long messages to a durable file automatically or emit a recoverable message address; RS peers should be tail-addressable."}
- {id: DL-013C-010, kind: difficulty, severity: blocking, fp: e423391b6c9a, first_seen_at: "2026-09-02T02:47:57.221Z", description: "Focused daemon search regressions on separate :5434 found three scoped-starvation failures: expected 10/10, 10/10, and 1/1 semantic hits but the rewritten search returned zero. Existing expansion contract is red; no further tests ran.", workaround: "Stop and report to o-prime. Do not raise expansion bounds or weaken tests; preserve the existing contract.", suggested_encoding: "Keep search_scope_starvation in the focused admission gate so post-filter rewrites cannot silently return empty before candidate expansion."}
- {id: DL-013C-011, kind: difficulty, severity: blocking, fp: 96e6d51ac6f2, first_seen_at: "2026-09-02T03:50:42.178Z", description: "After merging main, pij inbox cannot receive o-prime's queued message: rs wire v1 is unsupported by the current v2 extension; CLI refuses fallback because delivery state is unknown.", workaround: "Do not retry/fallback/adopt. Ask o-prime via send to persist the message as the next durable reply file.", suggested_encoding: "Version-negotiate rs inbox responses or keep the client compatible across daemon rolling upgrades; auto-spill queued messages to the agent record when versions differ."}
- {id: DL-013C-012, kind: difficulty, severity: blocking, fp: a43d08504c21, first_seen_at: "2026-09-02T04:21:35.434Z", description: "Final harness checks on head beee1491 changed the production database schema version from 22 to 23 despite FS3_TEST_DATABASE_URL pointing at the separate :5434 test postmaster. Gate emitted the binding STOP verdict; no rerun attempted.", workaround: "Stop immediately, retain the exclusive slot, report exact before/after and head to o-prime, and perform read-only source diagnosis only if ruled.", suggested_encoding: "Make the production migration guard prevent the write rather than detecting it after cargo test --all, and seal every spawned daemon/database URL before process start."}
- {id: DL-R13-001, kind: difficulty, severity: degrading, fp: a9b6e96fd71a, first_seen_at: "2026-09-02T05:26:47.004Z", description: "No command creates a migrated scratch database. To build a hand-made fixture for a review probe I had to apply crates/store/migrations/*.sql by hand; plain 'psql -f' fails at 0020_one_file_root_per_blob.sql because its CREATE TEMP TABLE ... ON COMMIT DROP needs the whole file in ONE transaction (sqlx does this, psql autocommit does not). Discovering the '-1' flag was guesswork.", workaround: "docker exec ... psql -1 -f - < each migration file, in sort order", suggested_encoding: "harness db scratch <name> — create + migrate a throwaway database on FS3_TEST_DATABASE_URL and print its URL; harness db drop <name> to reap it"}
- {id: CONF-R13-001, kind: confusion, severity: annoying, fp: 23a7d63ed55a, first_seen_at: "2026-09-02T05:26:47.156Z", description: "ddocs validate's builder/review vocabulary is only discoverable by failing. 'resolution: open' was rejected with 'value \"open\" is not in confirmed, refuted, fixed, deferred' — correct, but there is no way to read that enum up front, and none of the four words means 'not yet measurable', which is the honest state of an acceptance criterion whose environment has not been bounced yet. I used 'deferred'.", workaround: "ran ddocs validate --json and read the enum out of the error message", suggested_encoding: "ddocs schema builder/review should print field enums; consider a 'not-yet-measurable' resolution distinct from 'deferred'"}
- {id: DL-R13-002, kind: difficulty, severity: degrading, fp: 039a04941d4a, first_seen_at: "2026-09-02T05:30:19.289Z", description: "Third agent today mis-reported the read tool's '[Some lines truncated to 768 chars]' footer as the FILE being truncated, and spent review budget on it before retracting (o-prime, 2026-09-02: 014 reviewer, then me on 013, then a third). Backlog row 152 already records the phenomenon, but recording it has not stopped recurrence: each agent independently reads a long ddoc cell, sees it cut mid-sentence, and reasonably concludes the document or 'ddocs build' is lossy. The cost is real — I nearly refused the packet under i1b because the owed lists looked absent, and I raised it as an intake defect in my ack.", workaround: "Read the .dd.json source instead of the rendered .dd.md, and confirm with awk '{print length}' <file> | sort -n | tail -1", suggested_encoding: "Make the footer name itself as a VIEWER limit, not a file property — e.g. 'display truncated to 768 chars/line by this tool; the file is intact, re-read with :raw or check with awk'. Recurrence three times in one day means the wording, not the docs, is the defect."}
- {id: DL-R13-003, kind: difficulty, severity: degrading, fp: 0e88980dd5f0, first_seen_at: "2026-09-02T06:50:07.722Z", description: "The search plan-shape fixture is a SHAPE fixture being read as a COST fixture, and it misled this review three times. seed_search_plan_corpus gives all 20,000 embeddings the SAME vector (shape_vector()) and 14-byte raw_text ('shape body N'), against prod's 333,182 real vectors and a 484 MB elements heap. Consequences observed: (1) round-1 I feared 60-75k prod buffers from an unbounded admitted_elements scan; prod measured 8,063. (2) round-1 fixture latency was 105-121 ms; prod is 35 ms. (3) round-2 I reported smart_content loops 1->160 and wrote that the plan's 'resolved ONCE' prose was no longer true; on prod the loops are 1 — the 160 is an artefact of identical vectors plus small tables making a nested loop cheapest. Each time the fixture pointed the opposite way from production.", workaround: "Took the o-prime-authorised read-only prod EXPLAIN (BEGIN READ ONLY, statement_timeout 30s, no parallel, load<15) and scored the criteria against real statistics instead of the fixture", suggested_encoding: "Say so at the fixture: a doc comment on seed_search_plan_corpus stating it pins PLAN SHAPE only and that no cost, buffer or loop number may be read off it. Better, give the seeded vectors spread directions and realistic raw_text so loop counts and buffers mean something; or add a harness command that runs the shipped statement read-only against prod for cost questions, so nobody infers cost from a shape fixture again."}
- {id: DL-R13-004, kind: difficulty, severity: blocking, fp: a17ab836d71d, first_seen_at: "2026-09-02T06:57:24.253Z", description: "TWO flowspace3 daemons share one global ~/.config/flowspace3/daemon.key, and the second one to start silently locks every client out of the first. Measured 2026-09-02: prod daemon pid 1548 (main checkout, 013 binary c2f4709) took 127.0.0.1:7373 at 16:51:27 and holds its key in memory. A DIFFERENT flowspace3 daemon, pid 89658 'flowspace3 daemon --json' with cwd /Users/jordanknight/pi-hacking/fs3-spawn-reports-bind, started at 16:54:32 and rewrote daemon.key — the file mtime is exactly 16:54:32. It never took the port, so nothing looks wrong: lsof shows the correct prod daemon listening, prod is genuinely healthy, but every client including 'flowspace3 status' and 'flowspace3 search' now gets {ok:false, command:authenticate}. The next_action tells you to read the key from the file and send it as a bearer, which is precisely the key that does NOT work, so the advice actively misleads. This blocked the ac-0005 measurement pair on a healthy prod. Note this is NOT the documented pij-two-daemons hazard (that is pij legacy vs rs); it is the same failure SHAPE in flowspace3 itself.", workaround: "None available inside a read-only reviewer fence: recovering requires stopping another seat's daemon or relaunching prod, neither of which is mine to do. Reported to o-prime instead.", suggested_encoding: "Two parts. (1) Make the key per-instance rather than a global singleton, or have the daemon refuse to rewrite a key file owned by a live daemon on the configured port. (2) Fix the 401 next_action: when the on-disk key mtime is NEWER than the listening daemon's start time, say 'another flowspace3 daemon has overwritten the shared key; the daemon on :7373 (pid N, started T) predates the key file (written T2)' instead of telling the operator to send a key that cannot work."}
---

# Retro 2026-09-02 — plan 013 (search-admission): the drain

Drafted for o-prime (READ-ONLY drain; nothing cleared). Sources, 23 observations: shared harness buffer at
`harness observe --list` on the main clone (4, all o-prime: HB-DL-001..003, HB-CONF-001 — buffer cleared 04:30Z after
retro 002); vendored seat buffers: closeout-013/session-buffer.md (14, coder amistad: DL-013C-001..012,
CONF-013C-001..002) and review-013/session-buffer.md (5, reviewer carp: DL-R13-001..004, CONF-R13-001). Asks
ask-001..011 and the coder/review-fix reports were read for context; backlog rows 158–169 hold what is already encoded.

## The run in one paragraph
See `summary` above. The shape of the day: the code was right by every pre-registered criterion and wrong on the
one geometry the criteria did not cover; the ruling that followed prescribed a mechanism instead of a promise; and
the receipts that would have closed the plan were held hostage for ~50 minutes by two foreign daemons and a restart
script — none of it 013's code, all of it on 013's clock.

## Observations, grouped

### A. The criteria were TRUE and the search was broken — 4 obs
- The coder hit scoped starvation mid-run (DL-013C-010: 10/10, 10/10, 1/1 expected, 0 returned) and repaired the
  *count* with a sentinel row (ask-008); the repair fixed the empty-page expansion but not reachability, so the
  reviewer's 12,000-foreign / 5-scoped geometry still ERRORed after 9 passes (verdict f-9c41). The suite that was
  "written to defend" the geometry passed because its fixture is small enough for the bounded loop to reach the rows.
- The 20k plan-shape fixture (identical vectors, 14-byte text) pointed the opposite way from prod three times
  (DL-R13-003 → row 168); only the authorised read-only prod EXPLAIN settled cost.
- The reviewer's round-2 shim proved the two halves independently load-bearing (Rust sentinel: outage → diagnosed
  empty page; SQL relocation: recovers the hits). That splice-the-old-SQL-back probe is the reusable idea.

### B. Ruling as architecture — 2 obs
- Prime-reply-016 required both "every admission join above the HNSW page" and "return the 5 scoped rows behind
  12,000 nearer vectors within 2 passes" — mechanically impossible; ask-011 caught it, option B ruled (HB-CONF-001).
- Same failure shape on plan 015: o-prime authored a plan without citing the repo's add-language contract, which one
  `flowspace3 search` would have surfaced (HB-DL-001). Both are o-prime writing mechanism where it should write promise.

### C. Prod incidents on 013's clock — 4 obs
- `bin/daemon-restart` refused on two `flowspace3 daemon` candidates and the receipt script measured the OLD daemon
  (HB-DL-002 → row 164); it then crashed "Bus error: 10" after Ctrl-C and before relaunch (row 167).
- A foreign daemon squatted :7373 (row 165); a second foreign daemon that LOST the bind still rewrote
  `~/.config/flowspace3/daemon.key`, locking every client out of a healthy prod — and the 401 `next_action` told the
  operator to send exactly the key that cannot work (HB-DL-003, DL-R13-004 → row 169). Reviewer fence: read-only,
  so the ac-0005 pair waited on o-prime's bounce.

### D. The :5434 test postmaster landed mid-run — 8 obs
- Worktree compose collides with the prod container name (DL-013C-002); the old-query ANALYZE ran 648 s and put the
  SHARED :5433 postmaster into recovery, leaking a scratch DB (DL-013C-005, ask-003); the ruled :5434 then timed out
  (DL-013C-007), rejected the maintenance URL via pg_hba (DL-013C-008), and the first rewrite blew the new 30 s
  statement_timeout (DL-013C-009). Each stop-and-ask was correct and cost ~30–40 min of seat time.
- The final gate's prod-migration guard fired 22→23 while o-prime bounced prod for 014 inside the held slot
  (DL-013C-012, ask-010 → row 158, ruled false positive). No test touched prod.

### E. Wire, records, and reader tooling — 5 obs
- Inline ruling truncated after "Assert in the…" and not tailable (CONF-013C-002); rs wire v1/v2 cutover killed
  inbound delivery, seat resumed as sharp-amistad (DL-013C-011 → rows 153/161). Pointer delivery restored both.
- No command creates a migrated scratch DB; `psql -1 -f` per migration was guesswork (DL-R13-001). `ddocs validate`'s
  resolution enum is only discoverable by failing, and none of its four words means "not yet measurable"
  (CONF-R13-001). Third agent in one day read the 768-char READ-tool footer as file truncation (DL-R13-002; row 152
  is closed but recurrence says the wording is the defect). `harness commit` note miss (DL-013C-006 → row 156).

### F. Tools that answer wrong instead of "unknown" — 4 obs
- Serena init timed out twice (DL-013C-001); `pij whoami --json` E-RS / node show E-NOID (CONF-013C-001); LSP
  references empty for `search_elements` with verified callers (DL-013C-004 — fourth run in a row, retro 002 §A);
  builder discipline named a repo-local `node_modules/.bin/ddocs` that does not exist (DL-013C-003, ask-001).

## Encode next (ranked; smallest deterministic change first — rows 158–169 are cited, not re-proposed)
1. **Ruling template: PROMISE, not mechanism.** pij-team prime-reply/fix-ruling template gets a mandatory pair of
   lines — "the invariant this fix must preserve" and "the geometry that proves it" — and forbids naming plan
   nodes/join positions; o-prime restates the reviewer's "smallest fix" as a hypothesis. Plan template adds
   "repo how-to cited (via flowspace3 search)" per change class. Where: pij-team skill. Retires HB-CONF-001, HB-DL-001.
2. **The 401 `next_action` diagnoses a clobbered key.** When `daemon.key` mtime is newer than the listening daemon's
   start time, say "another flowspace3 daemon overwrote the shared key; daemon on :7373 (pid N, started T) predates
   the key file (T2)" instead of "send the key". Small, ships with row 169's write-after-bind. Where: fs3
   `crates/cli` auth error path. Retires the misleading half of DL-R13-004 / HB-DL-003.
3. **`harness db scratch <name>` / `harness db drop <name>`** — create + migrate a throwaway DB on
   FS3_TEST_DATABASE_URL (one transaction per migration file) and print its URL; drop is panic-safe and also the
   reaper for leaked `fs3_*`. Where: harness fs3 extension over `fs3_testkit::FreshDatabase`. Retires DL-R13-001, the
   leak half of DL-013C-005; makes retro 002 #3's slot preflight prove CREATE/DROP, not `select 1` (DL-013C-007/008).
4. **Query-rewrite packets name their invariant suites and a splice-back mutation.** pij-team coder template i-line:
   "list the existing behavioural suites this rewrite must keep green (here: `search_scope_starvation`) and run them
   BEFORE the plan-shape test; the mutation check splices the pre-change SQL back and must go RED on a geometry the
   old code passed." Where: pij-team coder/reviewer templates. Retires DL-013C-010; makes the f-9c41 class a packet
   tripwire rather than a reviewer catch.
5. **Bounded EXPLAIN helper in testkit.** `fs3_testkit::explain_bounded(sql, timeout=30s, analyze: bool)` sets
   transaction-local `statement_timeout` + no parallel, runs non-ANALYZE shape first and names loss of the vector
   index BEFORE any ANALYZE; the pathological old shape is non-ANALYZE forever. Where: `crates/testkit`. Retires
   DL-013C-005 (crash half), DL-013C-009; the 013 test already does this inline — lift it so the next packet inherits it.
6. **`ddocs schema <kind>` prints field enums; add a `not-yet-measurable` resolution.** Where: dd. Retires CONF-R13-001.
7. **Reader-footer wording + template line.** The READ tool footer names itself as a VIEWER limit ("display
   truncated…; file intact; check with awk"); until then the coder/reviewer template carries one line saying so.
   Where: harness prime (footer), pij-team templates (line). Retires DL-R13-002's third recurrence.
8. **Builder names the canonical `ddocs` and boot proves it.** builder skill says `ddocs` on PATH (not
   `node_modules/.bin`); `harness boot` fails fast when an active plan requires dd mutation and `ddocs --version`
   is absent. Where: builder skill + harness boot layer. Retires DL-013C-003.

Routed elsewhere: rs identity/delivery (CONF-013C-001, CONF-013C-002, DL-013C-011) → pij government req-0042/0046,
rows 153/161; commit note miss (DL-013C-006) → row 156; LSP references (DL-013C-004) and Serena (DL-013C-001) →
retro 002 encode #1; compose name collision (DL-013C-002) → retro 002 #3 / row 124b.

## Already encoded during the run — do not re-encode
- Row 158: gate slot and prod bounce mutually exclusive; guard prints migrating application_name (DL-013C-012).
- Rows 164/167: daemon-restart selects the :7373 listener, receipt asserts new pid ≠ old, launch-before-kill (HB-DL-002).
- Rows 165/169: refuse the prod database without owner designation; publish the key only after bind (HB-DL-003,
  DL-R13-004 part 1). Row 168: the shape fixture is not a cost fixture (DL-R13-003).
- In the merged code (#101, c2f4709): admission keys inside `candidate_vectors`; `scan_incomplete` + `passes` on the
  envelope; paired 12k/5 geometry and no-growth geometry as discriminating tests; non-ANALYZE old-shape mutation with
  a 30 s statement_timeout on the new shape; JIT off transaction-locally. Rows 122/145 latency RESOLVED; row 160
  (timing in meta) stays open for the remaining ~0.4 s.
- Row 124b: dedicated `flowspace3-db-test` on :5434 with maintenance access proved (the D-group pain was its landing
  cost). Row 152 closed as viewer-side; row 153/161 wire skew with pij; row 156 commit note with the harness prime.

## NOT drained — buffer left intact; o-prime clears after review
The shared buffer (4 obs) and the two vendored seat buffers were LISTED only. No `harness observe --clear`, no
`harness record`, no pij message was run by this drafter. O-prime reviews this record, runs `harness record retro`
if it wants the harness-side pointer, then clears its own buffer.
