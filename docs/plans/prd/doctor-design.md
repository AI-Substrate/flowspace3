# `flowspace3 doctor` — living design (no worker yet)
**Started 2026-08-26** (Jordan: "start a doctor design in the PRD … follow along what doctor does") · o-prime maintains; every landed capability adds its row here. PRD reqs 30/34 anchor it (doctor validates config + helps fire up the stack).

## Doctrine

Doctor is the diagnosis half of the actionable-error doctrine (workshop 004): every check emits the standard envelope row — `ok/warn/fail · code · what was checked · found · fix`. It never mutates by default; `doctor --fix` may run the named fix for fixable rows (compose up, create config from template). JSON-only like everything (D5). Checks run in dependency order and keep going past failures (collect-all).

## Check catalog (grows as capabilities land — the "follow along" contract)

| Area | Check | Fail looks like → fix says | Status |
|---|---|---|---|
| config | file parses; unknown keys warned | named key + example line | egret in flight |
| config | **registry sane**: every `active` pointer resolves to a `[providers.*]` instance; every instance validates its `kind` shape; per-repo overrides point at real instances AND real repo identities | "summarizer.active = 'azure-lna' — no such provider; configured: azure-luna, ollama-local" | egret (delta) |
| config | secrets chain: named env vars actually resolve (existence only — never print values) | "AZURE_OPENAI_API_KEY named in providers.azure-luna but unset — export it or edit secrets.env" | egret |
| stack | engine present (`FS3_ENGINE`), compose file valid | "no docker/podman on PATH → install OrbStack or set FS3_ENGINE" | ox territory |
| stack | PG reachable on configured url; `vector` extension present; **migrations current** (embedded vs `_sqlx_migrations`) | "store schema behind by 2 migrations → restart the daemon (it migrates at boot)" | store landed; wire when doctor exists |
| providers | **live probe per ACTIVE instance** (fs2 `doctor llm` heritage): summarizer round-trip (HEALTH_CHECK_OK-style), embedder round-trip + dimension check vs config/store tables | distinguishes the confusable rejections (Azure 401/403/404 mapping is the exemplar); names role/deployment fixes | adapters landed; probe = contract-suite lite |
| providers | per-repo overrides: probe each DISTINCT instance actually referenced | as above, prefixed with which repo selects it | after registry lands |
| daemon | reachable on localhost port; /health; version match CLI↔daemon | "daemon not running → flowspace3 daemon start (or doctor --fix)" (PRD 34's suggest-then-help flow) | daemon plan |
| watcher | roots exist, are readable, watch handles alive; per-root event counts moving | "root /x/y no longer exists → flowspace3 remove path" | daemon plan (sailfish learnings) |
| queue | depth, oldest pending age, failed-job count with last_error sample | "14 failed embed jobs, last: provider timeout → doctor providers / re-run flowspace3 retry-failed" | schema landed (jobs table) |
| toolchain (dev) | host rustup complete (rustfmt/clippy present) — the 2026-08-26 incident, encoded | exact `rustup component add` line | opportunistic |

## Sequencing
Doctor ships with the daemon/integration plan (it needs the daemon to be worth talking to), but egret's config validation and the store's migration check are built NOW as library functions doctor will call — doctor is an assembler of checks that already exist in their owning crates, never a second implementation of them.
