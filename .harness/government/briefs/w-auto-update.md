# Brief: w-auto-update — daemon self-update + envelope steering (req-0054) + config reference (req-0058)

**Seat**: pij-surprising-sailfish (revived — you own the reconcile substrate; the update
checker is a reconcile loop). **First PR-era packet** — read AGENTS.md § Working model
before anything: main is BRANCH-PROTECTED, you work in your OWN worktree on a branch,
CI runs on your PR, o-prime merges.

## What Jordan ruled (2026-08-26 + 2026-08-27, binding)

1. **Auto-update is ON BY DEFAULT**, with a config opt-out (`[update]` section).
2. The **daemon does the updating**: it periodically checks GitHub Releases for a newer
   published build, downloads the matching asset, and **replaces the installed binary
   itself** — no human action.
3. The daemon records that the swap happened; from then **every CLI command envelope**
   carries update-installed steering: new version waiting, restart the daemon.
4. Restart is in place (same config/stack). **Phase 2 — NOT this packet**: the daemon
   drains and exec()s itself. Build update+notify first; self-restart comes after it proves.
5. `flowspace3 doctor upgrade` remains as the manual force-it-now path (shares the same
   update engine); doctor explains update state.

## Current state (falsify in one read)

- No update code exists anywhere. req-0054 text in `docs/plans/prd/base-prd.dd.json` is
  the authority (just reshaped — reread it).
- Releases: `https://github.com/AI-Substrate/flowspace3/releases`, assets named
  `flowspace3-<target-triple>` (3 triples), version tags `vX.Y.Z` from release-please.
  **No checksums file is published today** — you add one (see deliverable 1d).
- Reconcile substrate: your own `Reconcile` trait + runner in the daemon (cadence/nudge).
  The update checker should be a `Reconcile` implementor registered at the composition root.
- Envelope: every CLI response goes through the shared envelope builder (sawfish's
  crates/core envelope code) — the steering field rides there, not per-command.

## Design constraints (best practice, discussed with Jordan — encode these)

- **Atomic swap**: download to a temp file IN THE SAME DIRECTORY as the install path
  (rename across filesystems fails EXDEV), set exec bit, `rename()` over the target.
  Never open the running binary for write (ETXTBSY). The running daemon keeps its old
  inode — that is the design, not a problem. Prefer the `self_update` crate (GitHub
  Releases support, asset-by-triple) over hand-rolling; `self-replace` if you only want
  the swap primitive.
- **Not-writable fallback**: if the install path isn't writable (root-owned /usr/local/bin),
  degrade to NOTIFY-ONLY — envelope says update available + how to install; never a
  failing loop.
- **Quota-free version probe**: HEAD `releases/latest/download/…` and read the redirect
  (or `releases/latest` Location) — NO GitHub API calls on a cadence (fleet retro DL-018:
  rate-limited shared resources; state your interval — daily default is fine, config-able).
- **Concurrency**: lock file around the swap so daemon + manual `doctor upgrade` can't race.
- **Integrity**: release.yml publishes a `SHA256SUMS` asset (deliverable 1d); updater
  verifies the downloaded asset's sha256 against it before swap. TLS+GitHub alone is not
  verification.
- **Version compare**: semver against the running binary's version; never downgrade;
  same-version = no-op.

## Deliverables

**1. Auto-update (req-0054)**
   a. `[update]` config: `auto = true` (default), `check_interval_hours` (default 24),
      documented like every other option.
   b. UpdateSupervisor as a `Reconcile` implementor: probe → download → verify → atomic
      swap → record swap state (in PG, per our state-in-postgres doctrine — the envelope
      steering must survive daemon restarts of the OLD binary).
   c. Envelope steering: when a newer binary is installed than is running, every command's
      envelope carries it (shape yours; steer to restarting the daemon). Doctor row shows
      update state (current / downloaded-waiting-restart / notify-only + reason).
   d. release.yml: generate + upload `SHA256SUMS` in the upload job.
   e. `doctor upgrade`: manual trigger of the same engine (kept from the old req shape).
   f. Tests: version-compare, probe parsing, swap-under-running-binary (integration:
      swap a dummy binary while executing it), not-writable fallback, envelope presence.
      NO live GitHub calls in CI — fake the release server (testkit pattern).

**1g. USER MESSAGES QUEUE (req-0059 — Jordan ruled 2026-08-27, build it in THIS packet
as the vehicle).** Do not bolt update-steering onto the envelope ad hoc. Build the
centralized concept: a daemon-side **user messages queue** (PG table, per state-in-postgres)
that ANY feature can push messages onto; the envelope builder drains-or-carries the queue
into EVERY CLI command's response; messages clear when their condition resolves (update
succeeds → its messages clear) or by explicit ack/expiry. The update feature is the first
producer: "new version installed — restart the daemon"; and for the locked/not-writable
case the message must be actionable: "update not possible at <path>: <reason>. Run
`flowspace3 doctor upgrade` — or reinstall: <install one-liner>". Design the message shape
(id, source, severity, text, next_action, created, clear-condition) so future features
(disk pressure, schema drift, provider misconfig) just push to it. Doctor shows the queue.

**2. Complete config-option reference (req-0058)**
   Extend `docs/how/configuration.md` (or a linked sibling reference) with a table of
   EVERY option the binary reads — walk fs3-core's config structs so nothing is missed:
   key, type, default, effect, env override. Include your new `[update]` section.

## Done-bar (PR era)

- Own worktree + branch (suggest `w-auto-update`), conventional commits
  (`feat:` — this bumps the version, correctly).
- `harness checks` green in YOUR worktree; fake-server tests green; no live-network tests.
- PR into main with a description that maps commits → deliverables; CI green on the PR.
- Do NOT merge — report the PR number to o-prime (pij-instant-lynx).
- `harness observe` every friction the moment it bites; you are also dogfooding
  flowspace3 per AGENTS.md while you work.
- Stop-and-ask on any design surprise (your forever-rescan stop-and-ask is the model).

## Out of scope

Phase-2 self-restart (exec). Windows anything. Delta updates. Signing beyond sha256.
