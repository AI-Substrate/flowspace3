# review-017 ACK — cross-model reviewer (Claude), plan 017 daemon-key-after-bind

Seat: `pij-unexpected-hyena`. Counterpart: o-prime `pij-binding-magpie`.
Worktree: `/Users/jordanknight/substrate/flowspace/fs3-review-017`.
SHA under review: `6d6637d8eac60ec61a200338fb4b061b98040caf` — confirmed detached
here (`git rev-parse HEAD`); working tree carries only my own packet render +
`.harness/temp/`, no code drift.

Packet contract accepted: the three owed lists are present (owed-1 least-confident,
owed-2 disbelieve-the-receipts, owed-3 known-open), so I am not refusing to start.

## Fence I am holding

Read-only on code. Writes only to `.harness/temp/agent/` and
`docs/plans/017-daemon-key-after-bind/assets/reviews/` in THIS worktree.
Databases: `postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test`
only, with per-run scratch databases I create and drop. Never `:5433`, never
`:7373`, never a daemon with the default config dir, never
`~/.config/flowspace3/daemon.key` (recorded mtime 16:57:45 — will re-assert at
close-out). No `harness checks` (not my slot) — targeted `cargo test` only.

## Numbered plan, per acceptance criterion

0. **Orient** — read `plan.dd.md`, `impl-guide.dd.md`, `assets/backpressure.dd.md`,
   `assets/inputs/evidence.md`, then the FULL diff `689ac27..6d6637d`. Read the
   author's receipts (`daemon-key-t*.md`, `daemon-key-closeout.md`,
   `daemon-key-pr-body.md`) once, to know what is CLAIMED — then re-derive every
   number independently. Re-read PR #108 CI run 33610456864 myself rather than
   trusting the dispatch line. Dogfood `flowspace3 search` for consumer discovery;
   log misses as friction via `harness observe`.

1. **ac-0001 — a losing daemon never touches the key.**
   Re-run the author's `boot_contract` A/B tests, then build my OWN reproduction:
   scratch `FS3_CONFIG_DIR`, per-run scratch DB on `:5434`, ephemeral port; daemon
   A up; daemon B same config → assert (a) B exits non-zero, (b) `daemon.key`
   bytes AND mtime unchanged (hash computed by me, not quoted), (c) no staging
   residue in the config dir, (d) A's client still authenticates. Repeat for
   `--json`. Then the pinned cause: prove the DUAL-FAMILY case specifically —
   A on one loopback family, B on the other — that B cannot half-bind and publish.

2. **ac-0002 — publish is impossible without a bind proof.**
   Enumerate EVERY construction site of the `BoundListener` proof and every caller
   of `StagedAuth::publish` across the workspace (grep + lsp references, not just
   the diff), including tests, sandbox, `--json`, and any `#[cfg(test)]` or
   builder-style constructor. A proof constructible without a real bound listener
   is a CRITICAL finding. Confirm a failed bind returns the bind error and never
   reaches publish, on every boot path. Re-run the author's mutation (remove the
   proof → compile/red) plus one mutation of my own.

3. **ac-0003 — the 401 hint is truthful.**
   Verify `key_newer_than_daemon` semantics directly: strict-newer true, equal
   false, older false. Hunt the false-positive the author flagged: a NORMAL
   restart (key mtime == boot, or key written microseconds before bind) must not
   claim an overwrite. Check the time source (mtime wall-clock vs boot instant)
   for clock-skew and coarse-mtime-granularity hazards. Exercise the real CLI 401
   rendering against a live SCRATCH daemon, not a unit stub. Reverse-comparison
   mutation → red.

4. **ac-0004 — designation refuses the prod DB from a foreign cwd.**
   Test the guard's decision table myself: foreign cwd + prod URL → refuse with
   `FS3-E-PROD-NOT-DESIGNATED`, naming `FS3_PROD_OWNER` and the `:5434` test URL;
   `FS3_PROD_OWNER=1` → proceed; cwd inside the owner root → proceed; config-named
   owner path → proceed. Then the regression o-prime cares about: o-prime's own
   bounce from the main clone and `bin/daemon-restart` must STILL work — I will
   check that path by reading the script and reasoning against the guard, without
   ever executing it against prod. Bypass mutation → red.

5. **ac-0005 — the real-usage transcript reproduces.**
   Re-run the whole transcript end-to-end on MY scratch setup (`:5434` + per-run
   database + ephemeral port + scratch `FS3_CONFIG_DIR`): A healthy → B from a
   foreign cwd refuses → A's `ping` still authorized → key mtime + hash unchanged
   → no residue → database dropped. Independently verify the PR body's production
   bounce sequence is the one that actually works under the new guard.

6. **Adjudicate and deliver.** Judge each of ac-0001..0005 true/false with cited
   evidence; both boot paths examined for the proof (done-bar d2). Findings only
   if material, each with severity / exact location / claim / proof / smallest
   fix; `no_material_findings` is a legitimate verdict and I will use it if the
   evidence says so. Zero findings spent on owed-3 known-open rows (164/165/167,
   per-instance keys, migration 0024, the CI `FS3_TEST_DATABASE_URL` collision
   in backlog 179, rows 177/178, viewer truncation row 152).

7. **Record.** Write `assets/reviews/review-017.dd.json`, run BOTH `ddocs build`
   and `ddocs validate` from the worktree root, then `review-017-verdict.md` and a
   one-line mirror in `review-017-status.md`, and pij one line to
   `pij-binding-magpie` with the path + verdict.

8. **Close-out as what I did NOT touch** (instruction i11): shared `:5434`
   database root count before and after, scratch resources created AND destroyed,
   the untouched set (`:7373`, `:5433`, `~/.config/flowspace3/daemon.key` mtime
   re-asserted), and the gates I deliberately did not hold.

**Blocking protocol accepted:** the instant I am blocked I pij `pij-binding-magpie`
FIRST, before writing any status file.
