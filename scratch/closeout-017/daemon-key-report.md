# daemon-key-after-bind done report

## Delivery

PR: https://github.com/AI-Substrate/flowspace3/pull/108  
Final head: `3d7484a4d1ba09be90d344af9b89e0cb1a7b9e4d`  
State: OPEN; not merged.  
Base: rebased onto main `689ac27` before final push.

- Reproduced the incident on base: `localhost` let daemon A bind `::1` and daemon B fall through to `127.0.0.1`; both reached `StagedAuth::publish`, and B replaced the shared key in normal and `--json` modes.
- Canonicalized `localhost` once to `127.0.0.1`, so both daemons contend for one socket.
- Added private-field `BoundListener`; `StagedAuth::publish` requires a reference to an actually bound listener on normal and sandbox paths.
- Added strict mtime diagnosis: additive `key_newer_than_daemon`, shared-overwrite explanation, and restart-the-listener-owner remedy. Unchanged publication remains generic.
- Added optional `[daemon].owner_root` / `FS3_DAEMON__OWNER_ROOT`; normal boot refuses `DatabaseConfig::DEFAULT_URL` before key staging unless cwd is inside that root or `FS3_PROD_OWNER=1`.
- Kept the older unsealed-test refusal ahead of the production-owner refusal, preserving its specific `fs3_testkit::sealed` remediation.
- Made the real-binary health test allocate a per-run `FreshDatabase` and preserve child stdout/stderr on early exit.
- All task, done-when, acceptance, phase, and backpressure state was updated through global `ddocs`; generated siblings report no drift.

## Evidence

- RED witness on base, normal + `--json`: dual-family matrix `v4 old/new=false/true`, `v6 old/new=true/false`; failing assertion named `StagedAuth::publish` as the writer path.
- GREEN A/B contract: B exits non-zero; key bytes + mtime unchanged; no staging residue; A remains authorized.
- Mutation receipts:
  - remove localhost canonicalization → both A/B tests RED;
  - reverse mtime comparison → live CLI rendering test RED;
  - bypass owner guard → foreign-cwd refusal test RED.
- Local `harness checks`: `{"command":"checks","status":"ok","timestamp":"2026-09-02T08:08:18.225Z"}`.
- Required daemon lib suite: 175/175 green.
- Rebased focused proof: real-binary health 1/1 green after workspace build; `boot_contract` 5/5 green.
- Final exact-head CI: `gate pass`, run `33613725420`, head `3d7484a4d1ba09be90d344af9b89e0cb1a7b9e4d`.
- Real TEST transcript: scratch config, per-run DB on `:5434`, port 63359; A healthy, B foreign cwd exit 1, A still healthy; key mtime and digest unchanged; no temp residue; database dropped. Full transcript: `.harness/temp/agent/daemon-key-real-usage.md`.
- Final tracked worktree status: clean.

## Deviations and rulings

- Reply 002 accidentally changed ac-0004 while paraphrasing. Stop-and-ask 001 compared it to the READY ddocs; ruling 003 voided that paragraph and retained fail-closed ac-0004. No contract ddoc was changed.
- CI and local gate differed on the same SHA because their DB inputs differed: local harness minted a unique child URL; CI used a test anchor byte-identical to the shipped prod URL. The health test now derives its own child in either environment. This does not weaken the guard.
- Initial CI failure discarded the daemon refusal. The test panic now includes child stdout/stderr.
- Review f-17a1 added the previously missing production-state ratchet: `Config::default()` (`owner_root: None`) plus foreign cwd and no explicit designation must error. Exact `.is_some_and(...)` → `.map_or(true, ...)` mutation failed at the new assertion; restored code and final CI are green.

## ASSUMPTIONS

- O-prime configures production `[daemon].owner_root = "/Users/jordanknight/substrate/flowspace/flowspace3"` before the first merged-binary bounce; prod currently has no `[daemon]` section. O-prime owns the merge prerequisite/runbook.
- `DatabaseConfig::DEFAULT_URL` is the production identity for this guard. Test processes must derive a child URL rather than reuse a prod-spelled anchor.
- `FS3_PROD_OWNER=1` is an explicit operator decision, not a test convenience; tests exercise non-production child URLs instead.
- Filesystem mtime is only diagnostic evidence. The test waits until it strictly advances; authentication still compares key bytes in memory.
- No migrations, dependencies, key-format changes, search/store behavior, or compatibility shims were added.

## Deferred and noteworthy

- No blocked task, unmet acceptance criterion, or new TODO/FIXME/HACK.
- Noteworthy: production configuration is a merge prerequisite because fail-closed owner designation intentionally prevents the current owner-less prod config from booting.
- Harness observations remain shared and uncleared: DL-001 (rust-analyzer reference miss) and DL-002 (shared test anchor/schema skew exposed the need for per-run child DBs).
