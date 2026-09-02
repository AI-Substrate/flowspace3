# PR 108 CI retry

Initial CI run `33607516052` on `43fca08` failed only `crates/daemon/tests/health.rs::the_real_binaries_agree_through_a_discovered_config`.

Cause: CI's isolated test service uses the same URL spelling as `DatabaseConfig::DEFAULT_URL`. The test pointed its daemon directly at that shared anchor, so the new owner guard correctly refused it. This was the same non-isolated test setup already identified in DL-002, now exposed on Linux.

Fix `23e6bf2098c07255f73cacb40a16523e6f0fb294`: the health test creates a per-run `FreshDatabase`, writes the unique child URL into scratch `config.toml`, uses `TestDatabase::FromConfigFile`, stops the daemon, and drops the child DB. It does not bypass the owner guard.

Focused local command against `:5434`: green (1/1). Pushed; PR CI rerun pending.

## Why local green differed from CI red

The same SHA received different test inputs. Local `harness checks` used the mandated `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test`, which is not the shipped production default; the owner guard did not fire. CI configured `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3`, byte-identical to `DatabaseConfig::DEFAULT_URL`; the guard refused the undesignated child. The local gate was green because it exercised a genuinely non-production URL, while CI exercised a test service with production's URL spelling. The fixed test derives a uniquely named child database in either environment.
