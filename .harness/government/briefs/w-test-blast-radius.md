# Brief: w-test-blast-radius — no test may ever reach the production database (EMERGENCY, Jordan's production DB was migrated by a test 2026-08-27)

**Seat**: (fill at canary — fresh seat). PR-era done-bar: own worktree + branch off
main, `fix:` commits, harness checks green, PR, report number, never self-merge.
AGENTS.md binds. THIS PACKET IS THE TOP OF THE MERGE QUEUE — one other coder
(w-conversations) is HELD from running its test gate until you land.

## The incident (today, ~09:16Z, second breach of the production-DB ruling)

During a coder's `cargo test --all` (FS3_TEST_DATABASE_URL correctly set), the
PRODUCTION database (5433/flowspace3) received migration 0012 sixteen seconds after
the test DB did. Jordan's installed CLI hard-refused on skew; production was
effectively down until an emergency merge+rebuild. The PR #18 testkit gate held for
every path that CALLS testkit — the breach came from a path that never does.

## The culprit (investigated, high confidence — verify then fix, MW-002)

`crates/daemon/tests/health.rs:121-152` `the_real_binaries_agree_through_a_discovered_config`:
spawns the REAL `flowspace3 daemon` subprocess with FS3_CONFIG_DIR pointing at a temp
config that has NO `[database]` section → config layering falls back to
`DatabaseConfig::DEFAULT_URL` (`crates/core/src/config.rs:449-451`) which IS the
production URL → daemon boot runs `fs3_store::migrate` (`daemon/src/boot.rs:127`)
against production, then starts a runner that can claim production jobs.
`boot_contract.rs:47-58` shows the correct pattern: scrub inherited `FS3_*`, set
`FS3_DATABASE__URL` explicitly.

Secondary hygiene (same class, no DB reach TODAY): `crates/cli/tests/ping.rs:54,80,107,136,152`
and `crates/cli/tests/docs_bundle.rs:51,137` spawn subprocesses with no
FS3_CONFIG_DIR and no scrub — they read the user's real config/secrets and are one
startup-path change from being incident #3.

## Deliverables

1. **Fix the culprit**: health.rs adopts the boot_contract pattern — scrub `FS3_*`,
   set `FS3_CONFIG_DIR` AND `FS3_DATABASE__URL` (derive from
   `fs3_testkit::test_database_url()` — a subprocess-spawning test is still a test).
2. **Fix the secondaries**: ping.rs and docs_bundle.rs get the same isolation even
   though they touch no DB today — reading the user's real secrets chain in a test
   is itself the defect.
3. **Close the structural hole**: any test that spawns a `flowspace3` subprocess
   must be UNABLE to inherit or default its way to production. Your judgment on
   mechanism, candidates in preference order: (a) a testkit helper
   (`fs3_testkit::spawn_env()` or similar) that returns the scrubbed+pinned env map,
   used by every spawning test, with a test that greps/asserts no spawning test
   builds its env by hand; (b) a debug-assert or env-marker refusal in the daemon
   itself (e.g. refuse to boot against DEFAULT_URL when a to-be-defined test marker
   env is present). Do NOT change DEFAULT_URL semantics for real users.
4. **The backstop gate** (from the incident seat's DL-001 suggestion, approved in
   shape): the harness test gate snapshots `max(_sqlx_migrations.version)` on the
   CONFIGURED production database.url before and after `cargo test --all` and FAILS
   the run on any diff — the breach class becomes un-shippable even when a new leak
   path appears. Wire it into the checks pipeline the way the existing seven gates
   are wired (read `.harness/` extension config to see how; keep it POSIX).
5. **Tests mutation-checked**: revert the health.rs fix → the backstop/assert
   catches it (prove the gate would have caught THIS incident).

## Out of scope

Whether the daemon should auto-migrate at boot at all (design debate, not this
packet). The doctor-applies-migrations behaviour (ruled, stands).
