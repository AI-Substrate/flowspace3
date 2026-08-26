# Worker roster — flowspace3
**Maintained by**: pij-instant-lynx (o-prime) · **Updated**: 2026-08-26 (keep current: update on every seat add/release/revive)

Revive a dead seat: `pij revive <id> --print` shows the exact command (Jordan runs it in a pane, or approves me doing it). Native session ids below are the harness-level resume keys captured at canary time.

## Governance seats

| Seat | Role | Harness/model | Pane | Native session id | Status |
|---|---|---|---|---|---|
| pij-instant-lynx | o-prime (me) | claude code / fable-5 | prime window | (this session) | active |
| pij-bitter-swan | PA sensor/relay | copilot gemini-3.7-flash low | %25 · fs3-pa | (pre-compact canary record) | active; watchdog PAUSED (Jordan) — sweeps only on explicit "SWEEP" |
| pij-bitter-gibbon | PM s001 fs3-foundations | omp "Ox Alpha" high | %23 · s001-fs3 | 01a03bb8… (canary `canaries/s001-bitter-gibbon.md`) | RELEASED — s001 phase exit ACCEPTED at b7e4230; fleet teardown cleared |

## Standing specialists

| Seat | Role | Pane/window | Native session id | Status |
|---|---|---|---|---|
| pij-impressive-ox | resident docker manager ("docker dude") | %31 · window `docker` | 01a03bec-d69d-7000-acdd-aa7ff0d60af7 | active — s002 COMPLETE (both phases); NOW executing plan 004 ship-it (CI gate, release-please, cross-platform release, installer). Brief `briefs/s002-docker-daemon-brief.md` |

## Task workers

| Seat | Task | Pane/window | Native session id | Status |
|---|---|---|---|---|
| pij-sure-kazimir | Azure OpenAI adapter (w-azure) → RE-OPENED: structured summaries (w-structured-summaries) | %33 | 01a03c3b-469a-7000-9970-95d654d6dea6 | RELEASED — landed c971902+e101e9b+cc0c83a (structured outputs, contract tolerance two-side-pinned, openai-compat LAN adapter). Phantom alias: pij-shallow-dog |
| pij-recent-cicada | sqlx migrations + db how-to (w-migrations) | %34 | 01a03c46-3fea-7000-815f-eada7d9e02da | RELEASED — landed 0a75c44. Brief `briefs/w-migrations-cicada.md`. Phantom aliases: pij-likely-mosquito, pij-aggregate-mosquito |
| pij-plain-mollusk | scanner v1: element tree + pure scan (w-scanner-v1) | %35 · window `scanner` | 01a03c4c-cd34-7000-8879-385defbdbff2 | RELEASED — landed 0962ba8+17878b6+f2f3cc4 (scanner + docs); add-language skill authored from its recipe |
| pij-technological-egret | global config system (w-config) | %36 · window `config` | 01a03c50-eecb-7000-9378-1992ecdb548c | RELEASED — landed 0d0fb1c+3e33161+5e73135+a99ceed (registry, config show, boot contract) |
| pij-surprising-sailfish | daemon shell prototype: watcher+web (w-daemon-shell) | %38 · window `daemon-shell` | 01a03c58-e85e-7000-8a21-4e81b739b51b | RE-OPENED 2026-08-26 — live watcher in daemon (w-watcher-live, brief 1ee33f4): Reconcile trait+runner, WatcherSupervisor, debounce+max-age, events→scan_file jobs |
| pij-devoted-cattle | ignore-aware file discovery (w-discovery) | %39 · window `discovery` | 01a03c5c-d775-7000-8c41-a25729de92af | RELEASED — landed ca27e63+f0912f9 (204x walker) |
| pij-xenophobic-wren | git/blob layer: identity + snapshot diffing (w-git-blob) | %40 · window `git-blob` | 01a03c5c-d938-7000-bd92-9136895e64c5 | RELEASED — landed c7670cd+b9f5b47 (fs3-git crate, 8-crate amendment) |
| pij-musical-sylac | schema v1: migrations 0003+ + typed store API (w-schema) | %41 · window `schema` | 01a03c60-310c-7000-b5fb-d6069937b73a | RELEASED — landed 2dd5a25+b58173a; first fully-green harness checks |
| pij-excellent-dingo | flowspace skill for agents (w-flowspace-skill, req-0052) | %53 · window `flowspace-skill` | 01a03d08-7c56-7000-ac9b-95c4b3ef34d7 | ACTIVE 2026-08-26 — canary PASS (pane %53 pid 94659); authoring skill; resident, Jordan follow-on tasks queued |
| pij-inevitable-hummingbird | local embeddings validate+adapter (w-local-embed) | %42 · window `local-embed` | 01a03c75-6993-7000-a09f-05afc8b1e2c4 | RELEASED — landed e265429+efedd0b+181c4c4 (live un-ignored contract, slow tier, GPU recipe, alternatives check). Phantom aliases: scrawny-ape, thorough-zakalwe |
| pij-broad-sawfish | plan 003 first light: wire scan->enrich->query (w-first-light) | %43 · window `first-light` | (canary 2026-08-26) | RELEASED — PLAN 003 COMPLETE: envelope/doctor/runner/enrichment/search + live first-light run + fault-path fixes (597a99d..9ce9fae) |
| pij-varied-skunk | human-render prototype over frozen envelopes (w-human-render) | %46 · window `human-render` | 01a03c9b-9f4c-7000-a313-e60b097d3436 | ACTIVE — pocs/human-render, rich-style renderer + TTY strategy. Brief `briefs/w-human-render.md` |

## Historical / other

- pij-likely-sailfish — tree-sitter POC (plan 001 assets), long since done.
- pij-managerial-peacock — engineering-harness standup, done (commit ec66578).
- pij-alright-mink — s001 reviewer (gpt-5.6-sol high), managed by gibbon; phantom aliases ashamed-gecko, back-scallop, dusty-sole, stormy-hippopotamus, unfair-baboon.
- pij-continuing-ermine — pij platform prime (repo ~/pi-hacking/pij) — route pij bugs here (Jordan ruling). Was revived once 2026-08-26.
- Phantom-alias defect: pij#19 (`pij` repo) — one omp process mints extra registry ids; always address the CANARIED id, never close aliases (shared pid).

## Rules

- Address ONLY canaried ids. Canary records live in `.harness/government/canaries/`.
- Released ≠ dead: released seats stay adopted-idle and can be re-tasked with a fresh brief + (a) round-trip re-check; a dead seat needs `pij revive`.
