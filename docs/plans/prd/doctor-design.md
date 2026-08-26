# `flowspace3 doctor` — living design (no worker yet)
**Started 2026-08-26** (Jordan: "start a doctor design in the PRD … follow along what doctor does") · o-prime maintains; every landed capability adds its row here. PRD reqs 30/34 anchor it (doctor validates config + helps fire up the stack).

## Doctrine

Doctor is the repair-as-it-goes half of the actionable-error doctrine: it walks the dependency chain (engine → stack → database → schema → providers → daemon) and FIXES what it can automatically, reporting each move as an envelope row; it errors only where it genuinely cannot act. It is the diagnosis half (workshop 004): every check emits the standard envelope row — `ok/warn/fail · code · what was checked · found · fix`. It never mutates by default; `doctor --fix` may run the named fix for fixable rows (compose up, create config from template). JSON-only like everything (D5). Checks run in dependency order and keep going past failures (collect-all).

## Check catalog (grows as capabilities land — the "follow along" contract)

| Area | Check | Fail looks like → fix says | Status |
|---|---|---|---|
| config | file parses; unknown keys warned | named key + example line | egret in flight |
| config | **registry sane**: every `active` pointer resolves to a `[providers.*]` instance; every instance validates its `kind` shape; per-repo overrides point at real instances AND real repo identities | "summarizer.active = 'azure-lna' — no such provider; configured: azure-luna, ollama-local" | egret (delta) |
| config | secrets chain: named env vars actually resolve (existence only — never print values) | "AZURE_OPENAI_API_KEY named in providers.azure-luna but unset — export it or edit secrets.env" | egret |
| stack | engine present (`FS3_ENGINE`), compose file valid; **stack running — doctor STARTS it when down** (compose up -d + healthcheck wait; Jordan 2026-08-26: no manual compose step) | "no docker/podman on PATH → install OrbStack or set FS3_ENGINE" / "db container was down → started, healthy in 4.1s ✓" | tk-0108 (plan 003) |
| stack | PG reachable; `vector` present; **migrations current** — and doctor APPLIES missing migrations itself (Jordan 2026-08-26: automatic, no second command); every db-touching command runs the fast check and rejects stale with FS3-E-STORE-SCHEMA-STALE naming doctor | "schema behind → doctor applied 0006-0007 ✓" | tk-0108 (plan 003) — doctor v1 ships EARLY, schema slice first |
| providers | **live probe per ACTIVE instance** (fs2 `doctor llm` heritage): summarizer round-trip (HEALTH_CHECK_OK-style), embedder round-trip + dimension check vs config/store tables | distinguishes the confusable rejections (Azure 401/403/404 mapping is the exemplar); names role/deployment fixes | adapters landed; probe = contract-suite lite |
| providers | per-repo overrides: probe each DISTINCT instance actually referenced | as above, prefixed with which repo selects it | after registry lands |
| daemon | reachable on localhost port; /health; version echo | "nothing is listening on http://127.0.0.1:7373 → not started — run `flowspace3 daemon &`"; overall verdict becomes `degraded` while `ok:true` stays (the COMMAND succeeded; the STACK is degraded) | **LANDED** tk-0108 delta (plan 003) — reported, never started: a diagnostic command must not leave a foreground process the user did not ask for |
| watcher | roots exist, are readable, watch handles alive; per-root event counts moving | "root /x/y no longer exists → flowspace3 remove path" | daemon plan (sailfish learnings) |
| queue | depth, oldest pending age, failed-job count with last_error sample | "14 failed embed jobs, last: provider timeout → doctor providers / re-run flowspace3 retry-failed" | schema landed (jobs table) |
| toolchain (dev) | host rustup complete (rustfmt/clippy present); `which rustc` resolves the rust-toolchain.toml pin (PATH-order trap DL-013: ~/.cargo/bin vs Homebrew makes the same repo pass or fail on PATH alone); cargo/rustc/clippy versions agree (DL-010) | exact `rustup component add` / PATH reorder line | opportunistic |

## Sequencing
Doctor ships with the daemon/integration plan (it needs the daemon to be worth talking to), but egret's config validation and the store's migration check are built NOW as library functions doctor will call — doctor is an assembler of checks that already exist in their owning crates, never a second implementation of them.
