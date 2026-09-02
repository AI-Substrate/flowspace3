## Summary

- canonicalize `localhost` to one loopback address before binding, so two daemons cannot each claim one address family and both publish the shared key
- require a private-field `BoundListener` proof to call `StagedAuth::publish`
- diagnose a newer on-disk key in 401 responses without exposing key bytes
- refuse the shipped production database outside `[daemon].owner_root` unless `FS3_PROD_OWNER=1`

## Acceptance receipts

- **ac-0001 — second daemon never touches the key:** permanent real-binary tests start daemon A then daemon B with one scratch config and ephemeral `localhost` port, both normal and `--json`. RED on base: A bound `::1`, B fell through to `127.0.0.1`, then B replaced `daemon.key` (`v4 old/new=false/true`, `v6 old/new=true/false`). GREEN now: B exits non-zero; key bytes + mtime unchanged; no staged file remains; A stays authorized. Removing localhost canonicalization restores both RED failures.
- **ac-0002 — publication strictly requires a successful bind:** `StagedAuth::publish(self, &BoundListener)` requires a token wrapping an actual Tokio listener behind a private field. Normal and sandbox paths pass the proof and consume it into HTTP serving. `cargo test -p fs3-daemon --lib -- --test-threads=1` → 175 passed.
- **ac-0003 — truthful 401 hint:** `Auth` records the published key mtime and bound port. Strict `current_mtime > published_mtime` produces additive `key_newer_than_daemon: true` plus `another flowspace3 daemon overwrote the shared key; restart the daemon that owns :<port>`. Unchanged publication reports false and retains generic guidance. A real CLI `ping` rewrite test is green; reversing the comparison makes it red.
- **ac-0004 — production owner designation:** `[daemon].owner_root` / `FS3_DAEMON__OWNER_ROOT` configure the owner tree. Before key staging or DB access, foreign cwd + `DatabaseConfig::DEFAULT_URL` returns `FS3-E-PROD-NOT-DESIGNATED`, naming `FS3_PROD_OWNER=1` and `postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test`. Inside-root, explicit override, and non-prod URL cases pass. Unconditional guard bypass makes the refusal test red.
- **ac-0005 — real TEST usage:** scratch config `/tmp/fs3-daemon-key-20260902-0810/config`, per-run DB on `:5434`, ephemeral port 63359. A healthy → B from foreign cwd exits 1 on `cannot bind 127.0.0.1:63359` → A ping still healthy. Key mtime stayed `1788336617`; SHA-256 stayed `3d50f80e19d92941337ab505b205d43c01362f2751c3ee12e2ce679767c8e576`; config directory remained exactly `config.toml`, `daemon.key`. Per-run DB dropped.

## Verification

- `harness checks` → `{"command":"checks","status":"ok","timestamp":"2026-09-02T08:08:18.225Z"}`
- daemon lib suite → 175 passed
- rust-analyzer diagnostics → no errors in changed Rust files

The focused existing unsealed-test regression remains in this PR and caught a real ordering mistake during implementation: the new production-owner refusal initially masked the older, more specific `fs3_testkit::sealed` refusal. The test-specific guard now runs first; focused test and full gate are green.

## Ruling record

`daemon-key-prime-reply-002.md`'s t4 paragraph was superseded and declared void by ruling 003. The READY plan's ac-0004 refusal contract is what this PR implements; no ddoc contract was changed.

## Production commands after merge

Before the first bounce, ensure the production config contains the owner root:

```toml
[daemon]
owner_root = "/Users/jordanknight/substrate/flowspace/flowspace3"
```

Then o-prime runs:

```bash
cd /Users/jordanknight/substrate/flowspace/flowspace3
git pull --ff-only origin main
cargo build --release --locked
bin/daemon-restart --binary /Users/jordanknight/substrate/flowspace/flowspace3/target/release/flowspace3
flowspace3 ping --json
flowspace3 status --json
```

Do not start the merged daemon from outside that owner root unless the launch is intentionally prefixed with `FS3_PROD_OWNER=1`.

## CI environment divergence found on the first PR run

Run `33607516052` on `43fca08` failed the real-binary health test because local and CI gate inputs differ. Local `harness checks` used `:5434/flowspace3_test`, which is not `DatabaseConfig::DEFAULT_URL`; CI used `:5433/flowspace3`, byte-identical to the shipped default, so the owner guard fired only in CI. The test now creates a uniquely named `FreshDatabase` on whichever test postmaster the gate supplies. Its non-zero-exit panic also carries the daemon's stdout/stderr instead of discarding the actual refusal. Focused local rerun is green; PR CI is the final Linux receipt.

## Review fix f-17a1

The fail-closed production state is now ratcheted explicitly: `Config::default()` (`owner_root: None`) + foreign cwd + no explicit designation must return an error. Reviewer mutation `.is_some_and(...)` → `.map_or(true, ...)` now fails at that assertion; the predicate was restored. Final head `3d7484a4d1ba09be90d344af9b89e0cb1a7b9e4d` passed CI run `33613725420`.
