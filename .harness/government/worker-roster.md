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
| pij-impressive-ox | resident docker manager ("docker dude") | %31 · window `docker` | 01a03bec-d69d-7000-acdd-aa7ff0d60af7 | active — cross-platform ACCEPTED (66bdefb+4a1099d); NOW executing re-scoped s002 phase 2 (docker/ + harness extension, gate opened 2026-08-26). Brief `briefs/s002-docker-daemon-brief.md` |

## Task workers

| Seat | Task | Pane/window | Native session id | Status |
|---|---|---|---|---|
| pij-sure-kazimir | Azure OpenAI adapter (w-azure) → RE-OPENED: structured summaries (w-structured-summaries) | %33 | 01a03c3b-469a-7000-9970-95d654d6dea6 | ACTIVE — response_format + PROMPT_VERSION + port key() + Summary.extras. Phantom alias: pij-shallow-dog |
| pij-recent-cicada | sqlx migrations + db how-to (w-migrations) | %34 | 01a03c46-3fea-7000-815f-eada7d9e02da | RELEASED — landed 0a75c44. Brief `briefs/w-migrations-cicada.md`. Phantom aliases: pij-likely-mosquito, pij-aggregate-mosquito |
| pij-plain-mollusk | scanner v1: element tree + pure scan (w-scanner-v1) | %35 · window `scanner` | 01a03c4c-cd34-7000-8879-385defbdbff2 | RELEASED — landed 0962ba8+17878b6+f2f3cc4 (scanner + docs); add-language skill authored from its recipe |
| pij-technological-egret | global config system (w-config) | %36 · window `config` | 01a03c50-eecb-7000-9378-1992ecdb548c | RELEASED — landed 0d0fb1c+3e33161+5e73135+a99ceed (registry, config show, boot contract) |
| pij-surprising-sailfish | daemon shell prototype: watcher+web (w-daemon-shell) | %38 · window `daemon-shell` | 01a03c58-e85e-7000-8a21-4e81b739b51b | RELEASED — landed 885f745+6ea471e+427353c; LEARNINGS.md = daemon watcher doctrine |
| pij-devoted-cattle | ignore-aware file discovery (w-discovery) | %39 · window `discovery` | 01a03c5c-d775-7000-8c41-a25729de92af | RELEASED — landed ca27e63+f0912f9 (204x walker) |
| pij-xenophobic-wren | git/blob layer: identity + snapshot diffing (w-git-blob) | %40 · window `git-blob` | 01a03c5c-d938-7000-bd92-9136895e64c5 | RELEASED — landed c7670cd+b9f5b47 (fs3-git crate, 8-crate amendment) |
| pij-musical-sylac | schema v1: migrations 0003+ + typed store API (w-schema) | %41 · window `schema` | 01a03c60-310c-7000-b5fb-d6069937b73a | RELEASED — landed 2dd5a25+b58173a; first fully-green harness checks |
| pij-inevitable-hummingbird | local embeddings validate+adapter (w-local-embed) | %42 · window `local-embed` | 01a03c75-6993-7000-a09f-05afc8b1e2c4 | ACTIVE — stage 1 POC. Brief `briefs/w-local-embed.md` · phantom aliases: scrawny-ape, thorough-zakalwe |
| pij-broad-sawfish | plan 003 first light: wire scan->enrich->query (w-first-light) | %43 · window `first-light` | (canary 2026-08-26) | ACTIVE — executes all of plan 003. Brief `briefs/w-first-light.md` |

## Historical / other

- pij-likely-sailfish — tree-sitter POC (plan 001 assets), long since done.
- pij-managerial-peacock — engineering-harness standup, done (commit ec66578).
- pij-alright-mink — s001 reviewer (gpt-5.6-sol high), managed by gibbon; phantom aliases ashamed-gecko, back-scallop, dusty-sole, stormy-hippopotamus, unfair-baboon.
- pij-continuing-ermine — pij platform prime (repo ~/pi-hacking/pij) — route pij bugs here (Jordan ruling). Was revived once 2026-08-26.
- Phantom-alias defect: pij#19 (`pij` repo) — one omp process mints extra registry ids; always address the CANARIED id, never close aliases (shared pid).

## Rules

- Address ONLY canaried ids. Canary records live in `.harness/government/canaries/`.
- Released ≠ dead: released seats stay adopted-idle and can be re-tasked with a fresh brief + (a) round-trip re-check; a dead seat needs `pij revive`.
