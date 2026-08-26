# Worker roster — flowspace3
**Maintained by**: pij-instant-lynx (o-prime) · **Updated**: 2026-08-26 (keep current: update on every seat add/release/revive)

Revive a dead seat: `pij revive <id> --print` shows the exact command (Jordan runs it in a pane, or approves me doing it). Native session ids below are the harness-level resume keys captured at canary time.

## Governance seats

| Seat | Role | Harness/model | Pane | Native session id | Status |
|---|---|---|---|---|---|
| pij-instant-lynx | o-prime (me) | claude code / fable-5 | prime window | (this session) | active |
| pij-bitter-swan | PA sensor/relay | copilot gemini-3.7-flash low | %25 · fs3-pa | (pre-compact canary record) | active; watchdog PAUSED (Jordan) — sweeps only on explicit "SWEEP" |
| pij-bitter-gibbon | PM s001 fs3-foundations | omp "Ox Alpha" high | %23 · s001-fs3 | 01a03bb8… (canary `canaries/s001-bitter-gibbon.md`) | active — s001 tail (re-review → exit) |

## Standing specialists

| Seat | Role | Pane/window | Native session id | Status |
|---|---|---|---|---|
| pij-impressive-ox | resident docker manager ("docker dude") | %31 · window `docker` | 01a03bec-d69d-7000-acdd-aa7ff0d60af7 | active — parked at s002 phase-2 gate. Brief `briefs/s002-docker-daemon-brief.md` |

## Task workers

| Seat | Task | Pane/window | Native session id | Status |
|---|---|---|---|---|
| pij-sure-kazimir | Azure OpenAI adapter (w-azure) | %33 | 01a03c3b-469a-7000-9970-95d654d6dea6 | RELEASED — landed 51f16d1, keyed run green. Brief `briefs/w-azure-openai-kazimir.md`. Known phantom alias: pij-shallow-dog |
| pij-recent-cicada | sqlx migrations + db how-to (w-migrations) | %34 | 01a03c46-3fea-7000-815f-eada7d9e02da | RELEASED — landed 0a75c44. Brief `briefs/w-migrations-cicada.md`. Phantom aliases: pij-likely-mosquito, pij-aggregate-mosquito |
| pij-plain-mollusk | scanner v1: element tree + pure scan (w-scanner-v1) | %35 · window `scanner` | 01a03c4c-cd34-7000-8879-385defbdbff2 | ACTIVE — owns 0002_element_kinds.sql too. Brief `briefs/w-scanner-v1.md` |
| pij-technological-egret | global config system (w-config) | %36 · window `config` | 01a03c50-eecb-7000-9378-1992ecdb548c | ACTIVE. Brief `briefs/w-config-egret.md` |

## Historical / other

- pij-likely-sailfish — tree-sitter POC (plan 001 assets), long since done.
- pij-managerial-peacock — engineering-harness standup, done (commit ec66578).
- pij-alright-mink — s001 reviewer (gpt-5.6-sol high), managed by gibbon; phantom aliases ashamed-gecko, back-scallop, dusty-sole, stormy-hippopotamus, unfair-baboon.
- pij-continuing-ermine — pij platform prime (repo ~/pi-hacking/pij) — route pij bugs here (Jordan ruling). Was revived once 2026-08-26.
- Phantom-alias defect: pij#19 (`pij` repo) — one omp process mints extra registry ids; always address the CANARIED id, never close aliases (shared pid).

## Rules

- Address ONLY canaried ids. Canary records live in `.harness/government/canaries/`.
- Released ≠ dead: released seats stay adopted-idle and can be re-tasked with a fresh brief + (a) round-trip re-check; a dead seat needs `pij revive`.
