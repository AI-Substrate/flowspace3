# Worker roster — flowspace3
**Maintained by**: pij-instant-lynx (o-prime) · **Updated**: 2026-08-27 (keep current: update on every seat add/release/revive)

**2026-08-27 fleet close-out (Jordan's order)**: all 13 task/specialist workers CLOSED after the v0.2.0 ship and retro-prep reports landed. Every seat filed its retro observations into the shared buffer first (drain still pending, o-prime-owned). Revive any seat with `pij revive <id> --print`; native session ids below are the harness-level resume keys captured at canary time.

## Governance seats

| Seat | Role | Harness/model | Pane | Native session id | Status |
|---|---|---|---|---|---|
| pij-instant-lynx | o-prime (me) | claude code / fable-5 | prime window | (this session) | active |
| pij-bitter-swan | PA sensor/relay | copilot gemini-3.7-flash low | %25 · fs3-pa | (pre-compact canary record) | active; watchdog PAUSED (Jordan) — sweeps only on explicit "SWEEP" |
| pij-bitter-gibbon | PM s001 fs3-foundations | omp "Ox Alpha" high | %23 · s001-fs3 | 01a03bb8… (canary `canaries/s001-bitter-gibbon.md`) | DEAD (pid gone 2026-08-26) — s001 phase exit ACCEPTED at b7e4230; s001 observations drained at plan close |

## Active workers (PR era)

| Seat | Task | Pane | Harness/model | Status |
|---|---|---|---|---|
| pij-strange-edeard | w-auto-update (req-0054/0058/0059): daemon self-update, user messages queue, config reference | %60 · window `auto-update` | omp / github-copilot claude-opus-5 high (spawnId s1787782128553-97893) | COMPLETE 2026-08-27 — PR #13 merged (9905ffe): auto-update engine, user messages queue, SHA256SUMS, doctor upgrade, config reference; 23 tests; caught SHA256SUMS-absence retry-forever bug via live smoke; standing by for review fixes / next packet |
| pij-squealing-xoxarle | telemetry survey + F1-F6 field report for plan 090 | (Jordan-run claude session) | claude | ACTIVE — ack'd handover packet `handovers/2026-08-27-telemetry-retro-xoxarle.md`; Jordan directing narrative-script work |
| pij-religious-cheetah | STANDING Linux tester (w-linux-test, reusable — re-ping for every Linux verification; headline: auto-update proof at v0.3.0→next) | %62 · window `linux-test` | omp / github-copilot claude-opus-5 high (spawnId s1787785262734-68991) | ACTIVE 2026-08-27 — canary PASS; baseline v0.2.0 rig-proof dispatched |

## Closed workers — who did what (2026-08-27)

All revivable. "CLOSED" = pane killed, descriptor dissolved, work fully landed on main.

| Seat | Domain / what it landed | Native session id | Status |
|---|---|---|---|
| pij-impressive-ox | docker/build/release engineering: plan-002 build container, CI gate, release-please, cross-platform release v0.2.0 (9 tag cycles), installer fixes (Linux musl 404, ps1 refusal), release-preflight harness f0578f0, mac `--lib` tier (committed for it at e3e6757), README agent funnel + install proof | 01a03bec-d69d-7000-acdd-aa7ff0d60af7 | CLOSED 2026-08-27 |
| pij-sure-kazimir | Azure OpenAI adapter (w-azure), structured summaries c971902+e101e9b+cc0c83a, openai-compat LAN adapter, in-call provider retry (429/Retry-After) 5e532a3. Phantom alias: pij-shallow-dog | 01a03c3b-469a-7000-9970-95d654d6dea6 | CLOSED 2026-08-27 |
| pij-recent-cicada | sqlx migrations + db how-to (w-migrations) 0a75c44; boot contract hardening. Phantom aliases: pij-likely-mosquito, pij-aggregate-mosquito | 01a03c46-3fea-7000-815f-eada7d9e02da | CLOSED 2026-08-27 |
| pij-plain-mollusk | scanner v1: element tree + pure scan 0962ba8+17878b6+f2f3cc4; add-language skill authored from its recipe | 01a03c4c-cd34-7000-8879-385defbdbff2 | CLOSED 2026-08-27 |
| pij-technological-egret | global config system (w-config) 0d0fb1c+3e33161+5e73135+a99ceed (registry, config show, boot contract) | 01a03c50-eecb-7000-9378-1992ecdb548c | CLOSED 2026-08-27 |
| pij-surprising-sailfish | daemon shell + reconcile substrate + live watcher (w-daemon-shell, w-watcher-live): 10 commits incl. forever-rescan fix de7ae1b, CI-flake fix 894eca5. Queued next: watcher-filter unification, req-0054 update awareness. Phantom alias: pij-damp-clownfish | 01a03c58-e85e-7000-8a21-4e81b739b51b | CLOSED 2026-08-27 |
| pij-devoted-cattle | ignore-aware discovery (w-discovery), 2 tours: ca27e63/74cecf7/515155e/2f78b7f/84b7db8/c2dba76 (deny list, cross-filter contract, prune ledger) | 01a03c5c-d775-7000-8c41-a25729de92af | CLOSED 2026-08-27 |
| pij-xenophobic-wren | git/blob layer: fs3-git crate c7670cd+b9f5b47, 8-crate arch amendment | 01a03c5c-d938-7000-bd92-9136895e64c5 | CLOSED 2026-08-27 |
| pij-musical-sylac | schema v1 + typed store API 2dd5a25+b58173a; extras-persistence fix across all read paths; first fully-green harness checks | 01a03c60-310c-7000-b5fb-d6069937b73a | CLOSED 2026-08-27 |
| pij-excellent-dingo | flowspace agent skill (req-0052) 55a9016; skill distribution + doctor row 546f475/22485cc; agents-start-here e4533fc (req-0055, shipped in v0.2.0) | 01a03d08-7c56-7000-ac9b-95c4b3ef34d7 | CLOSED 2026-08-27 |
| pij-inevitable-hummingbird | local embeddings validate+adapter (w-local-embed) e265429+efedd0b+181c4c4; memory-per-model-copy measurement. Phantom aliases: scrawny-ape, thorough-zakalwe | 01a03c75-6993-7000-a09f-05afc8b1e2c4 | CLOSED 2026-08-27 |
| pij-broad-sawfish | plan 003 first light (envelope/doctor/runner/enrichment/search, 597a99d..9ce9fae); daemon streaming logs 74bf8a7+a293b2e; doctor provider/info rows; throughput packet: multi-claim embed batching, lanes, RateLimited parking, embed pre-check (788481d, e610bb9, f4b7961, eb8a0e5, b61473b) | (canary 2026-08-26) | CLOSED 2026-08-27 |
| pij-varied-skunk | human-render prototype over frozen envelopes (pocs/human-render): rich-style renderer + TTY strategy, piped-vs-tty assertion | 01a03c9b-9f4c-7000-a313-e60b097d3436 | CLOSED 2026-08-27 |

## Historical / other

- pij-likely-sailfish — tree-sitter POC (plan 001 assets), long since done.
- pij-managerial-peacock — engineering-harness standup, done (commit ec66578).
- pij-alright-mink — s001 reviewer (gpt-5.6-sol high), managed by gibbon; phantom aliases ashamed-gecko, back-scallop, dusty-sole, stormy-hippopotamus, unfair-baboon.
- pij-continuing-ermine — pij platform prime (repo ~/pi-hacking/pij) — route pij bugs here (Jordan ruling). Was revived once 2026-08-26. NOT ours to close.
- Phantom-alias defect: pij#19 (`pij` repo) — one omp process mints extra registry ids; always address the CANARIED id, never close aliases (shared pid). Alias exit tombstones (damp-clownfish, diverse-jerusalem) after the 2026-08-27 close-out are expected noise.

## Rules

- Address ONLY canaried ids. Canary records live in `.harness/government/canaries/`.
- Released ≠ dead ≠ closed: a released seat is adopted-idle; a CLOSED seat's pane is gone and needs `pij revive <id>`; native session ids above are the resume keys.

## 2026-08-30 seats (post-governance-cutover; canaries verified by owl for its coders)

| seat | packet / role | status |
|---|---|---|
| pij-associated-owl | PM plan 009-embed-split (opus-5 med, fs3-embed-split). Two doctrine-grade stop-and-asks (S4 phantom collapse; impossible shared-worktree + u2->u1 type dep) — both ratified, plan amended aa165aaf/50aeeb71 | ACTIVE |
| pij-above-ferbin | 009 u1-store (sol-fast-1m high, fs3-embed-split-u1, branch u1-store): chunk_no key + terminal-heal | ACTIVE |
| pij-striped-guan | 009 u2-enrich (fs3-embed-split-u2, u2-enrich): chunk_plan + two-layer hygiene | ACTIVE |
| pij-surprised-hare | 009 u3-read (fs3-embed-split-u3, u3-read): element collapse in nearest CTE + ac-0002/3/6 fixtures | ACTIVE |
| pij-unhappy-mollusk | w-ask-budget-honesty row 71 (fs3-ask-budget-honesty): honest ask terminals + partial-evidence salvage; lands ask.rs BEFORE bovid (ruling) | ACTIVE |
| pij-light-bovid | w-ask-conv-scope row 85 (fs3-ask-conv-scope): enforced --conversation/--source pinning; rebases after mollusk | ACTIVE |
| pij-cloudy-krill | standing read-only queue/ingest-efficiency monitor (Jordan-ordered); audit + live alerts in scratch/queue-waste-audit.md | STANDING |
| pij-double-halibut | w-daemon-bounce #77 (bounce verb) — merged; observations rescued to main .harness/temp/agent/daemon-bounce-observations.md (row 93) | CLOSED 2026-08-30 |

## 2026-09-02 seats (sol codes, Claude reviews — ruling 2026-09-02; generation recorded per how-we-work 2b)
| seat | packet | generation | status |
|---|---|---|---|
| ~~pij-general-limpet~~ | plan 010 embed-cap-heal (fs3-embed-cap-heal, branch 010-embed-cap-heal); omp / gpt-5.6-sol-fast-1m / high; spawnId s1788300029005-71257; pane %2428 | **rs** — omp boot self-registered into rs; cannot `pij send` a legacy prime (E-RS, no fallback); channel = `.harness/temp/agent/embed-cap-heal-*.md` + `pij-rs send` from o-prime; transcript will NOT ingest (omp, no session env, req-0033) — accepted knowingly | ACTIVE — canary verified by pane + file 2026-09-02; released to ack (prime-reply-001) |
| ~~pij-zealot~~ | plan 011 conv-verify (fs3-conv-verify, branch 011-conv-verify); omp / gpt-5.6-sol-fast-1m / high; spawnId s1788300509654-6142; pane %2432 | **rs** — spawned with the file channel pre-declared; transcript will NOT ingest (omp, req-0033) — accepted knowingly | ACTIVE — canary verified against pij-rs list 2026-09-02; released to full ack (prime-reply-001) |
| ~~pij-fiscal-tick~~ | plan 010 REVIEWER (fs3-review-010, detached at 6377a1fe = PR #92 head); omp / claude-opus-5 / high; pane %2506 | **rs** — file channel (review-010-ack/verdict.md); transcript will NOT ingest | ACTIVE — spawned 2026-09-02 with the three owed lists |
| ~~pij-top-sloth~~ | plan 011 REVIEWER (fs3-review-011, detached at 3a7124ba = PR #93 review sha; head now 330c0077 docs-only); omp / claude-opus-5 / high; pane %2600 | **rs** — file channel (review-011-ack/verdict.md) | ACTIVE — spawned 2026-09-02 with the three owed lists |


CLOSED 2026-09-02 (plans 010/011 shipped as #92/#93, closeout #94): limpet, zealot, fiscal-tick, top-sloth — buffers rescued to scratch/closeout-010-011/ and drained into records/retro/2026-09-02/001; worktrees tidied; scratch DBs dropped.
| ~~pij-mad-crocodile~~ | plan 012 fresh-db-serialise (fs3-fresh-db-serialise, branch 012-fresh-db-serialise); omp / gpt-5.6-sol-fast-1m / high; spawnId s1788309142136-5995; pane %2644 | **rs** — file channel | ACTIVE — canary verified 2026-09-02; GO issued (prime-reply-001) |
| ~~pij-purring-orangutan~~ | w-db-cpu-profile READ-ONLY investigator (main clone, no code); omp / claude-opus-5 / high; spawnId s1788309908670-61353; pane %2721 | **rs** — file channel | ACTIVE — canary + 8-step plan received 2026-09-02; sampling |


O-PRIME IDENTITIES (2026-09-02): legacy pij-instant-lynx + rs pij-binding-magpie (pane %21, claude session a5a5588f). rs children send to the magpie name; legacy peers keep lynx.

CLOSED 2026-09-02: pij-purring-orangutan — w-db-cpu-profile delivered (scratch/db-cpu-profile/); buffer rescued.
| pij-common-cheetah | plan 012 REVIEWER (fs3-review-012, detached at 5c7f7bdb = PR #95); omp / claude-opus-5 / high; pane %2742 | **rs** — file + pij send pij-binding-magpie | ACTIVE — spawned 2026-09-02 with the three owed lists + the store-crate bypass hunt |
| ~~pij-quixotic-takin~~ | plan 014 REVIEWER (fs3-review-014, detached at cc8da52c = PR #98); omp / claude-opus-5 / high; pane %3064 | **rs** — file + pij send pij-binding-magpie | CLOSED 2026-09-02 — REQUEST CHANGES → delta APPROVE at c5242ea; record on PR #100; worktree removed |
| ~~pij-select-carp~~ | plan 013 REVIEWER (fs3-review-013, detached at beee1491 = the gate-green sha; PR #101 head 065acfd docs-only); omp / claude-opus-5 / high; pane %3148 | **rs** — file + pij send pij-binding-magpie | CLOSED 2026-09-02 — REQUEST CHANGES → APPROVE → final confirmed at 8d04a77; prod receipts ac-0004/0005 (16 runs); records on main via #105; worktree removed |
| ~~pij-resonant-fox~~ | plan 015 ts-grammar coder (fs3-ts-grammar, branch 015-ts-grammar from 57b25df); omp / gpt-5.6-sol-fast-1m / high; pane %3150 | **rs** — file + pij send pij-binding-magpie | CLOSED 2026-09-02 — #102 merged c11ab19, receipt proven on prod; worktree removed |
| ~~pij-respective-boar~~ | plan 015 REVIEWER (fs3-review-015, detached at 3649c0f = PR #102); omp / claude-opus-5 / high; pane %3170 | **rs** — file + pij send pij-binding-magpie; rulings TO it by pane paste (row 161) | CLOSED 2026-09-02 — APPROVE + delta PASS at a45fdc9; record on main via #103; worktree removed |
| pij-upper-dormouse | plan 016 hidden-dirs coder (fs3-hidden-dirs, branch 016-hidden-dirs from 82f60ec); omp / gpt-5.6-sol-fast-1m / high; pane %3283 | **rs** — file + pij send pij-binding-magpie; rulings TO it by pane paste (row 161) | ACTIVE — spawned 2026-09-02 06:35; canary PASS; ack RULED GO 06:43 |
| pij-comparative-cod | plan 017 daemon-key-after-bind coder (fs3-daemon-key-after-bind, branch 017-daemon-key-after-bind from c2f4709); omp / gpt-5.6-sol-fast-1m / thinking high; HAND-STARTED in tmux window 017-coder (pane %3286) per the spawn-bind rule | **rs** — file + pij send pij-binding-magpie; rulings by pane paste until delivery is proven | ACTIVE — started 2026-09-02 07:09; ack RULED GO 07:14 (reproduce first) |
| ~~pij-just-barnacle~~ | plan 014 coder, RESUMED session of pij-chosen-arach (omp -c after the pij wire v2 cutover, 14:14); fs3-jobs-retention; pane %3111 | **rs** | CLOSED 2026-09-02 — #98 merged b86593c, prod receipt taken; worktree removed |
| ~~pij-sharp-amistad~~ | plan 013 coder, RESUMED session of pij-imperial-weasel; fs3-search-admission; pane %3112; holds the gate slot | **rs** | CLOSED 2026-09-02 — #101 merged c2f4709, on prod; worktree removed |
| ~~pij-sufficient-mite~~ | plan 012b coder, RESUMED session of pij-little-junglefowl; fs3-fresh-db-serialise; pane %3113 | **rs** | CLOSED 2026-09-02 — #99 merged b528860; worktree removed |
| ~~pij-imperial-weasel~~ (pane lost 14:08, session resumed under a new id) | plan 013 search-admission (fs3-search-admission, branch 013-search-admission); omp / gpt-5.6-sol-fast-1m / high; pane %2810 | **rs** — pij send pij-binding-magpie + files | ACTIVE — spawned 2026-09-02, reading |
| ~~pij-chosen-arach~~ (pane lost 14:08, session resumed under a new id) | plan 014 jobs-retention (fs3-jobs-retention, branch 014-jobs-retention); omp / gpt-5.6-sol-fast-1m / high; pane %2811 | **rs** — pij send pij-binding-magpie + files | ACTIVE — spawned 2026-09-02, reading |

| ~~pij-little-junglefowl~~ (pane lost 14:08, session resumed under a new id) | plan 012 coder RESTARTED (fs3-fresh-db-serialise, branch 012-fresh-db-serialise @ f3aec31); omp / gpt-5.6-sol-fast-1m / high; pane %2907 | **rs** | ACTIVE — spawned 2026-09-02 after crocodile lost its event stream (req-0042) |
CLOSED 2026-09-02: pij-mad-crocodile — event stream dead after the disk incident; work safe at f3aec31; buffer rescued to governance.

| ~~pij-partial-coral~~ | w-disk-space (main clone, no code); omp / claude-opus-5 / high; pane %2845 | rs | CLOSED 2026-09-02 — 113 GB reclaimed in the VM, report vendored |
