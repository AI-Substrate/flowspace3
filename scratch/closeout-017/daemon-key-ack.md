# daemon-key-after-bind coder acknowledgment

Worktree: `/Users/jordanknight/substrate/flowspace/fs3-daemon-key-after-bind`
Branch/base: `017-daemon-key-after-bind` from `c2f4709`

## Evidence and thesis

- Incident row 165 records a foreign-government daemon taking prod `:7373` against prod `:5433`, preventing o-prime's intended daemon from binding.
- The evidence timeline records the sharper failure at 16:54:32: prod pid 1548 was already listening on `127.0.0.1:7373`; foreign-cwd pid 89658 ran `flowspace3 daemon --json`; `~/.config/flowspace3/daemon.key` changed; every client then received `FS3-E-DAEMON-UNAUTHORIZED`; pid 89658 never held `:7373` (`assets/inputs/evidence.md`, Code facts).
- Main already documents and implements a two-phase sequence: `auth::stage` creates a `NamedTempFile`, `boot::serve` binds, then `StagedAuth::publish` renames it. Therefore the observed writer is not established by inspection. Task t1 must first produce a red test and identify the actual boot/config path; no speculative fix comes first (`impl-guide.dd.md`, Architecture; `assets/backpressure.dd.md`, bp-0001/bp-0002).
- New prime evidence narrows t1: `crates/daemon/tests/health.rs::the_real_binaries_agree_through_a_discovered_config` passes inside full `harness checks` but fails alone against `:5434`, reporting that the real daemon never served the ephemeral `/health` URL and appeared not to honour `FS3_CONFIG_DIR`. I will compare the isolated invocation with the gate's binary/env/cwd setup while pinning the clobber path; source log: `/Users/jordanknight/substrate/flowspace/fs3-hidden-dirs/.harness/temp/agent/health-isolated.log`.

## Numbered implementation plan

1. **Reproduce before changing source.** Mark t1 in progress through global `ddocs`; inspect the exact daemon launch/config and `auth::stage`/`StagedAuth::publish` call graph with rust-analyzer references. Add an isolated two-daemon regression using only scratch `FS3_CONFIG_DIR`, `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test`, and an ephemeral port. Exercise daemon B both normally and with `--json`; assert B fails, A remains authorized, key bytes and mtime are unchanged, and no staging file survives. Run it red on the unmodified `c2f4709` behavior and record the concrete writer/config path plus the full-gate-versus-isolated difference.
2. **Make publication require bind evidence.** Apply the smallest structural change justified by t1 so `StagedAuth::publish` is callable only with a successfully bound listener or bind-proof value on every normal/`--json`/sandbox path. Preserve RAII cleanup for failed binding. Prove the two-daemon case green and prove a publish-before-bind guard mutation red (bp-0001/bp-0002; ac-0001/ac-0002).
3. **Make unauthorized guidance truthful.** Carry daemon boot time and additive `key_newer_than_daemon: bool` evidence into the 401 envelope/rendering. When key mtime is newer, name a shared-key overwrite and instruct restart of the daemon owning the configured port; retain normal stale-key guidance otherwise and cover the non-false-positive restart boundary (bp-0003; ac-0003).
4. **Add prod-owner designation.** Implement `FS3_PROD_OWNER=1` and `[daemon].owner_root` handling within `crates/core/src/config.rs` plus daemon boot enforcement. A foreign cwd targeting the prod database must fail with `FS3-E-PROD-NOT-DESIGNATED`, naming both `FS3_PROD_OWNER` and the `:5434` test URL; env designation or cwd inside `owner_root` proceeds (bp-0004; ac-0004).
5. **Run deterministic regression proof.** Update every task/done-when/backpressure row via global `ddocs`, keep the execution log current, run focused `fs3-daemon`/`fs3-cli`/`fs3-core` tests and clippy, then request o-prime's gate slot and run `harness checks`. A red plan tripwire or out-of-fence requirement is a stop-and-report, not a workaround (bp-0005).
6. **Exercise the real TEST setup and ship the packet.** Start only scratch-config daemons against `:5434` on a free port; capture first-daemon health, foreign-cwd second-daemon refusal, authorized first-daemon ping, unchanged key bytes/mtime, and no temp residue. Put the reproducible transcript and exact post-merge prod commands for o-prime in the PR body; commit with `harness commit`, open the PR, and send the durable done report with assumptions and receipts (bp-0006; ac-0005).

## Operating constraints and current state

- Prod `:7373`, prod DB `:5433`, and `~/.config/flowspace3` are read-only/untouched. No daemon process will run without explicit scratch `FS3_CONFIG_DIR`, test DB `:5434`, and ephemeral port.
- Fence remains the packet's responsibility-scoped paths. Any migration, dependency, public-envelope incompatibility, or unrelated module is a stop-and-ask.
- rust-analyzer is configured and available (`configured, not started` before first request).
- `harness boot --json` built all targets but returned `degraded` because compose service `db` was not running; no code has been changed.
- Waiting for o-prime's ruling before t1 or any source/test mutation.
