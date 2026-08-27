# Brief: w-update-truth — update state must be true per-install, and fresh at boot (Jordan ruled 2026-08-27)

**Seat**: (fill at canary — fresh seat; this packet lives in the auto-update domain
pij-strange-edeard built — read its living page docs/services/auto-update.md and the
messages-queue design FIRST; edeard is adopted-idle, not to be disturbed, its docs
speak for it). PR-era done-bar: own worktree + branch off main, conventional commits
(`fix:`), harness checks green (seven gates — NOTE: first run against a brand-new
FS3_TEST_DATABASE_URL can go red at the test gate, re-run once before believing it;
canonical value `postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_test`),
PR, report the number, never self-merge. AGENTS.md binds (dogfood + observe). The
production-database ruling binds: tests never touch the default 5433 database.

## The defects (all reproduced live today, 2026-08-27)

**A — Fossil messages survive restarts for up to 24h.** A debug daemon run from a
dev worktree YESTERDAY wrote `update:blocked` (naming its own throwaway
target/debug path and v0.2.0-era text) into the shared DB. Jordan's production
daemon restarted TODAY on a current binary — and his next `add` still carried that
fossil, because the update supervisor starts at boot with `every_hours=24` and does
NOT check at startup: nothing re-evaluates, so nothing retracts. A standing message
is level-triggered by design — but the level is only re-read on the producer's
cadence, and boot does not tick it.

**B — Update state is keyed to the STORE; an install is keyed to a PATH** (standing
Linux tester, finding 12, reproduced with root + non-root sharing one DB): two
installs against one database thrash the update row last-writer-wins. Root (current,
healthy) carried alice's "not writable" blocked message about a path root does not
use — unactionable, on a surface whose next_action is NOT NULL precisely to
guarantee actionability. Root later carried alice's "restart the daemon" message;
alice has no daemon. NOT exotic: install.sh itself picks /usr/local/bin OR
~/.local/bin depending on permissions, so one person who ever installed both ways
has exactly this.

**C — The row is never reconciled against disk** (finding 12's tail): after an
out-of-band binary change (pinned reinstall at an older tag), the row claimed 0.3.1
was installed at a path holding 0.3.0. Combined with no rollback story, any
out-of-band change leaves a permanently false "restart to pick up X".

**D — Egress surface undocumented** (finding 13): auto-update needs github.com AND
release-assets.githubusercontent.com (NOT objects.githubusercontent.com any more —
the tester blackholed the old host and the download sailed through). Anyone behind
an allowlist gets an unexplained UNREACHABLE.

## Ruling

Update truth is **per-install-path, verified against disk, refreshed at boot**.

## Deliverables

1. **Check at supervisor start**: the update supervisor runs one check immediately
   when it starts (then the configured cadence). Every daemon boot therefore
   refreshes or retracts update messages within seconds. Keep the existing
   config semantics — this is "first tick now", not a cadence change.
2. **Key update state by install path**: the supervisor's state row (and the
   standing messages it produces) carry the install path they describe — the
   daemon's own resolved binary path. A daemon declares/retracts ONLY messages for
   its own path; message keys become path-scoped (the existing per-source
   self-retraction design extends naturally — read the messages-queue design and
   stay inside its doctrine rather than inventing a second mechanism). Decide and
   document what happens to rows for paths that no longer exist on THIS host
   (sketch: retract on check when the path is gone — but the multi-host-one-DB
   case means "path missing here" ≠ "path missing everywhere"; name your choice
   and its limits honestly rather than pretending the ambiguity away).
3. **Reconcile against disk before claiming**: before declaring "X installed at
   path, restart to pick it up", confirm the binary at that path IS X (the staged
   pre-swap exec probe precedent: interrogate `--version` of the on-disk file, or
   a cheaper honest mechanism you justify). A mismatch retracts/rewrites the
   message to the truth.
4. **Migration path for existing fossils**: existing keyed-by-store rows (like
   Jordan's yesterday-fossil) must die on first boot of the fixed binary — the
   fix arriving as a new binary should be enough, no hand-SQL (the w-embed-oversize
   recovery precedent: DEFAULT-false-is-the-recovery style beats repair commands).
5. **Egress docs**: name both hosts in docs/services/auto-update.md, with the
   tester's evidence note (old objects.githubusercontent.com host is dead for this
   purpose). One paragraph, not an essay.
6. **Tests, mutation-checked**: boot-check fires without waiting for the interval;
   a message produced under path A is not visible/actionable under path B's daemon;
   a disk-mismatch is corrected not amplified; a pre-fix store-keyed fossil row is
   retracted at boot. Fake providers only, explicit test DB.

## Out of scope

Rollback verb (finding 11 — separate). Minutes-granularity check cadence /
`doctor upgrade --watch` (tester finding 9 — queued separately). The first-run
compose cliff (finding 2). Phase-2 self-restart.
