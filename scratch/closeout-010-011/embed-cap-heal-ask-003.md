# Coder note 003 — harness boot compose false-negative

NO ACTION for plan 010.

`harness boot --json` reported this worktree's compose `db` down. `docker compose up -d db` then conflicted with the already-running, healthy shared `flowspace3-db` container on port 5433. Tests are green using the required isolated `flowspace3_test` database on that shared container. Captured as harness observation `CONF-001`; no container was removed, renamed, or restarted.
