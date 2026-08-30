# Worker brief — plan 003 first light · (seat at canary, pane %43)
**From**: pij-instant-lynx (o-prime) · 2026-08-26 · executes ALL of plan 003 (one phase, 7 tasks)

## The job
`docs/plans/003-first-light/` (dd-native, READY) — read `plan.dd.md` + `assets/tasks/phase-1/tasks.dd.md` COMPLETELY; they are the contract. You are wiring landed, tested parts into the first working system: daemon up → `flowspace3 add <path>` → snapshot+enqueue → job runner scans → enrichment (registry-resolved providers) → semantic search answers. The demo Jordan wants, verbatim: "fire up the daemon, add a repo to scan, have it put all the files into the queue, have the worker process them, update the database, and do a query using embeddings" — with the LIVE run against an ISOLATED FIXTURE SUBSET only (cost ruling; crates/parsers/fixtures corpus), never a whole repo.

## Read order (all exist, all landed today)
1. plan 003 (incl. execution_guardrails + clarifications — retry/re-queue decisions are settled there)
2. `docs/rules-idioms-architecture/fs3-architecture.md` (binding) · workshops 002/003/004 in `docs/plans/prd/workshops/`
3. `docs/services/` — store-schema.md, config.md, scanner.md, discovery.md, git-blob.md, azure-openai.md (each ends with verify commands and code pointers)
4. `docs/plans/prd/daemon-worker-architecture.md` (watcher doctrine is context; watcher itself is OUT of scope)

## Mechanics
- Task/dw state flips via `ddocs set` as you complete them (never hand-edit .dd.md); the-flow is CLI-only and I drive it.
- Fence: `crates/{daemon,cli,core,store,providers}` as the tasks require (core = envelope module + azure snap-in variant; store = only if a flow function is missing — flag first), `docs/services/first-light.md`, README quickstart, plan-003 assets (transcript), e2e test files. Excluded: `.harness/government/**`, `.claude/**`, parsers internals, pocs/, other plans' folders.
- Commit+push per coherent unit; file-scoped adds for shared files; push-first; scoped fmt (ruling `.harness/government/rulings/2026-08-26-commit-push-as-you-go.md`).
- Gates: `harness checks` green at every landing; the e2e CI test is YOUR deliverable and must be green before the live run.
- Azure live run: creds are ambient (Entra via az login; endpoint/deployments per `docs/services/azure-openai.md` — this machine's fs2 config carries the working values). Fixture subset ONLY.
- Report to pij-instant-lynx at: envelope landed (tk-0101), pipeline first green (tk-0103/4), search first hit (tk-0105), and DONE (full report: claim · commits · gates · transcript pointer · observations). Blockers/deviations = stop-and-ask immediately.
