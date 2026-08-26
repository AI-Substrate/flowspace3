# Ruling — the main database is PRODUCTION (Jordan, 2026-08-27)

Second strike of the same class (first: the /demo root pollution via host-network
container, 2026-08-26; second: migrations 0008/0009 applied to the main DB at
2026-08-26 22:37 UTC by locally-run integration tests during the w-auto-update
packet — evidence: `_sqlx_migrations.installed_on`).

**The ruling (Jordan, verbatim intent)**: treat the main database as being in
production. Nothing messes with it except the mainline daemon Jordan runs.
Anything else — dev builds, branch daemons, integration tests, demos, verification
runs — uses our special way to work on secondary instances (isolated stacks;
req-0056 instance profiles is the first-class version).

## What this binds

1. **Tests never default to production.** The daemon-spawning integration tests
   currently inherit the shipped default (127.0.0.1:5433) when run locally — on CI
   that is a disposable service container; on a dev machine it is Jordan's real DB.
   Local test runs must either require an EXPLICIT opt-in database URL or provision
   their own isolated stack (unique port/database/compose project). Refusal is the
   default: a test that cannot prove its database is disposable does not run.
2. **Branch/dev daemons don't get the default either** — running a daemon from a
   worktree build against default config is how skew lands in production. Until
   req-0056 profiles exist, dev runs use an explicit `--config-dir` with an
   isolated stack (cheetah's rig is the reference shape for containment).
3. **req-0056 (instance profiles) is elevated**: this incident is its second
   motivating strike; it is the "special way" this ruling references and should
   land in the 0.3.x line.
4. Cheetah's mechanical isolation gate (refuse the rig on port leak or non-empty
   DB) is the model: guards must be structural, not habits.
