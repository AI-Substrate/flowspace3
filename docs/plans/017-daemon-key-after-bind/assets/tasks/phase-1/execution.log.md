# Phase 1 execution log

## t1 — Reproduce the clobber and pin the writing path

Status: complete (RED witness preserved)

Command:

```text
FS3_CONFIG_DIR=/tmp/fs3-coder-never-default \
FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test \
cargo test -p fs3-cli --test boot_contract another_loopback_address_family -- --nocapture --test-threads=1
```

Result: RED, both normal and `--json` cases. With `daemon.url = http://localhost:<ephemeral>`, daemon A bound IPv6 (`::1`) and published its key. Daemon B's `TcpListener::bind("localhost:<port>")` retried the resolved addresses, bound IPv4 (`127.0.0.1`) after IPv6 was occupied, and reached `StagedAuth::publish`, replacing the shared `daemon.key`. Probe matrix was `v4 old/new=false/true, v6 old/new=true/false` in both modes. The failing assertion names the key path and the exact writer path.

The test uses a scratch `FS3_CONFIG_DIR`, a per-run `FreshDatabase` created on the dedicated `:5434` postmaster, an ephemeral port, and explicit cleanup. This also explained plan 016's isolated health failure: the shared `flowspace3_test` admin anchor was already at migration 0024 while this main-based binary carries 0023; the earlier daemon exited before bind rather than ignoring `FS3_CONFIG_DIR`.

Evidence: test output `artifact://26`; task row t1 and `dw-3111` are checked through global `ddocs`.

## t2 — Publish only with bound-listener proof

Status: complete.

`StagedAuth::publish` now requires `&BoundListener`. `BoundListener` wraps Tokio's already-bound listener behind a private field, so an address or boolean cannot satisfy the API. Normal and sandbox boot wrap their actual listener, publish while borrowing that proof, then consume it into `http::serve_listener`.

The reproduced writer needed an additional address fix: `localhost` is canonicalized once to `127.0.0.1` before Tokio binds, rather than handing a multi-address hostname to `TcpListener::bind`. Both daemons therefore contend for the same socket instead of each claiming one loopback family.

Proof: permanent A/B tests green 2/2 for normal and `--json`; `cargo test -p fs3-daemon --lib -- --test-threads=1` green 172/172; removing the canonicalization guard made both A/B tests red with the original dual-family key rewrite. The A/B test also checks unchanged bytes + mtime, no extra staged file, and daemon A still authenticates.

## t3 — Truthful 401 after a shared-key overwrite

Status: complete.

Production publication records `daemon.key`'s filesystem mtime immediately after the atomic persist and the bound listener's port. Unauthorized middleware compares the current mtime with that recorded baseline using strict `>`: an unchanged file (including the normal publication boundary) reports `key_newer_than_daemon: false`; a later rewrite reports `true`, names the shared-key overwrite, and tells the operator to restart the daemon owning `:<port>`. Key bytes never enter the envelope or logs.

Proof: five auth tests green, including unchanged-key false and rewritten-key true; the real CLI `ping` test rewrites the live daemon's key and sees the overwrite + owner remedy. Reversing the mtime comparison made that live CLI render test red with the old generic advice; the predicate was restored and the test reconfirmed green.

## t4 — Production database owner designation

Status: complete.

`DaemonConfig` now accepts optional `[daemon].owner_root` (and the standard nested override `FS3_DAEMON__OWNER_ROOT`). Before logging, key staging, provider wiring, or database access, normal daemon boot checks the effective database URL. The shipped production URL is refused unless the current cwd is within the canonicalized owner root or `FS3_PROD_OWNER=1` was explicitly set. The refusal is `FS3-E-PROD-NOT-DESIGNATED` and names both remedies plus the exact `:5434` test URL. Non-production databases bypass this ownership rule.

Proof: core configuration test proves file + env layers; daemon tests prove foreign refusal, inside-root acceptance, explicit designation acceptance, and non-production acceptance. Replacing the guard with an unconditional success made the foreign-cwd assertion red; the guard was restored and reconfirmed green. rust-analyzer reports no errors in config, boot, or the CLI integration tests.

## t5 — Regression and gate (local portion)

Status: local gate complete; exact-PR-sha CI pending PR creation.

Focused crate run first exposed one precedence regression: the new production-owner refusal masked the older, more specific unsealed-test refusal. The test-specific guard now runs first; its focused test reconfirmed green.

Full verdict, verbatim fields from `harness checks`: `{"command":"checks","status":"ok","timestamp":"2026-09-02T08:08:18.225Z"}`. Reported gates included docs, lock, testdb, checks-contract, daemon-bounce-contract, fmt, clippy, and the full test gate, all `ok:true`. Separate required daemon library suite: `cargo test -p fs3-daemon --lib -- --test-threads=1` → `175 passed`.

Task t5 remains open only for CI on the exact PR head SHA.

CI run 33607516052 on `43fca08` failed only `the_real_binaries_agree_through_a_discovered_config`: CI's isolated Postgres service intentionally has the same URL spelling as the shipped local default, so the new owner guard refused it. The test now follows the already-ruled per-run rule: it creates a child `FreshDatabase` on the selected test postmaster, writes that unique URL into its scratch config, uses `TestDatabase::FromConfigFile`, and cleans the child database after stopping the daemon. No `FS3_PROD_OWNER` bypass is needed. The formerly failing isolated command is green locally against `:5434`; CI rerun will prove the Linux path.

Why local `harness checks` was green on `43fca08` while CI was red on the same SHA: the environments supplied different database identities. Locally, the mandated `FS3_TEST_DATABASE_URL` was `127.0.0.1:5434/flowspace3_test`, which is not `DatabaseConfig::DEFAULT_URL`, so the production-owner guard correctly did not fire. CI set `FS3_TEST_DATABASE_URL` to `127.0.0.1:5433/flowspace3`, byte-identical to the shipped default URL, so the guard correctly refused the undesignated child. The SHA was the same; the gate inputs were not. The per-run child database removes that URL-identity collision in both environments.

## t6 — Real TEST-setup transcript (pre-PR)

Status: runtime proof complete; PR/CI pending.

Real binary daemon A ran with scratch config, per-run database `fs3_daemon_key_20260902_0810` on the `:5434` test postmaster, and ephemeral port 63359. Daemon B ran from a foreign cwd with the same scratch config and exited 1 on `cannot bind 127.0.0.1:63359`. Daemon A's subsequent real `ping` remained healthy. `daemon.key` stayed at mtime `1788336617` and SHA-256 `3d50f80e19d92941337ab505b205d43c01362f2751c3ee12e2ce679767c8e576`; the config directory contained only `config.toml` and `daemon.key`. Daemon A stopped cleanly and the per-run database was dropped.

Full transcript: `.harness/temp/agent/daemon-key-real-usage.md`.

## Phase complete

All six task rows, seven done-when assertions, five acceptance criteria, and six backpressure rows are checked through global `ddocs`. PR #108 is open against main. After rebasing onto main `689ac27`, focused real-binary health and boot-contract suites were green; CI run `33609408499` was green on implementation head `04ad9a7`. The final commit contains progress receipts only and must receive the same CI gate before the done report.

## Review fix f-17a1 — unset owner fails closed

Reviewer mutation showed the suite covered an explicit foreign `Some(owner_root)` but not production's current `None` state. The existing owner test now calls the guard with `Config::default()`, a foreign cwd, and no explicit designation, and requires an error. Exact mutation `.is_some_and(...)` → `.map_or(true, ...)` now fails at `an unset owner_root must fail closed outside an owner tree`; the predicate was restored and the focused test reconfirmed green.
