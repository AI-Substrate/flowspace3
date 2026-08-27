# Auto-update

**Owner**: pij-strange-edeard (w-auto-update packet) · **Requirements**: PRD
req 54 (auto-update), req 59 (user messages queue), req 58 (config reference).

The daemon keeps the installed `flowspace3` binary current by itself, and tells
you about it through a channel any feature can use. Two subsystems, built
together because the first one needed the second and bolting it on ad hoc was
exactly what the ruling forbade.

## What happens, in order

```
reconcile pass (shared 5s cadence)
  └── claim: boot → always · after that → claim_check(interval)
        ├── not due  → no network at all
        └── due      → probe → compare → download → verify → lock → swap
  └── boot or check → read the binary AT the install path (--version)
  └── update_state(path) → desired_messages(running) → sync_messages("update", path)
```

Everything below is scoped to ONE install path — this daemon's own resolved
binary. See "Update truth is per-install" below for why that is the unit.

1. **Claim.** `update_state.last_checked_at` is advanced by a conditional
   `UPDATE` for this install path. Only a caller whose `UPDATE` matched a row
   runs a check, which makes the interval survive restarts and makes two
   daemons on one install incapable of both checking. **At boot the claim is
   unconditional**: every daemon start re-evaluates within seconds, whatever
   the interval says.
2. **Probe.** `GET https://github.com/AI-Substrate/flowspace3/releases/latest`
   with redirects disabled; the `Location` header names the newest tag. No
   GitHub API call, so a fleet on a daily cadence spends **zero** API quota
   (fleet retro DL-018).
3. **Compare.** Semver, numerically. Same version is a no-op and a lower one is
   a downgrade; the updater performs neither, so a yanked release replaced by a
   lower tag cannot walk an installation backwards.
4. **Verify.** The release's own `SHA256SUMS` asset is fetched and the
   downloaded binary's sha256 is compared against the line for this triple. A
   mismatch, or a release with no `SHA256SUMS`, installs nothing.
5. **Interrogate.** The staged binary is RUN — `--version` — and the swap is
   refused unless it reports the version being installed. See below; this is
   what makes a self-reinstalling loop structurally impossible.
6. **Swap.** Stage a temp file **in the install directory**, `fsync`, `chmod
   0755`, `rename()` over the target.
7. **Reconcile against disk.** At boot and after every check, the binary at the
   install path is asked what it is. That reading — not what the updater
   remembers doing — is what the row records.
8. **Say so.** The state row becomes messages, and the messages ride on every
   envelope built by an install they concern.

## Update truth is per-install, verified against disk, refreshed at boot

Ruled by Jordan on 2026-08-27, after three defects that were all the same
defect: the row described something other than the thing the user was holding.

**An install is a PATH, not a store.** `install.sh` picks `/usr/local/bin` or
`~/.local/bin` depending on permissions, so one person who has ever installed
both ways has two installs against one database. The old singleton row thrashed
last-writer-wins between them: root's daemon carried an unprivileged user's
"not writable" message about a path root does not use, and later a "restart the
daemon" for a user who has no daemon. `update_state` is now keyed by install
path, `doctor upgrade` writes only its own binary's row, and the queue scopes
both ownership and visibility (below).

**Boot is a tick.** A standing message is level-triggered, but the level was
only re-read on the producer's 24h cadence and boot did not tick it. Observed:
a debug daemon run from a dev worktree wrote `update:blocked` naming its own
`target/debug` path; the production daemon restarted the next morning on a
current binary and went on serving that fossil, because the interval a dead
process had claimed suppressed the one pass that would have retracted it. Every
boot now claims a check unconditionally.

The check and the *re-evaluation* are deliberately two clocks. The release
check reaches the network and may install, so it stays behind `auto = true`.
The disk reconcile and the message re-declaration run at boot whatever `auto`
says — neither needs the network, neither installs anything, and hanging them
off `auto` would leave exactly the users who opted OUT of unattended updates
carrying fossils forever.

**`installed_version` is a cache of disk, not a memory.** It used to record
"we swapped 0.3.1 in", and nothing could ever unset it — `record_clear` only
cleared the block. So a pinned reinstall at an older tag left a permanently
false "restart to pick up 0.3.1" against a path holding 0.3.0, and with no
rollback verb the only escape was hand-written SQL. The value is now read from
the file by running its `--version`, the same interrogation the pre-swap guard
performs, so a swap this daemon made and a change somebody made behind its back
are the same question with the same answer. A path that holds nothing runnable
reads as "nothing installed", which RETRACTS the restart steer rather than
leaving it to outlive its cause.

One `fork`/`exec` per check — at boot and then on the cadence — against a
process whose `--version` is a `clap` string. Cheaper than the HTTP probe
standing beside it.

**Recovery is the migration, not a repair verb.** Migration 0012 drops the
store-keyed row and deletes every `update`-source message, because a row keyed
to a store names no install that can honestly be re-homed onto. The fix arrives
as a new binary, a new binary migrates at boot, and the boot check re-derives
the truth seconds later. Nothing for a user to run (the w-embed-oversize
precedent: the recovery IS the new default).

## What auto-update needs to reach

Two hosts, and an allowlist that names only the first gets an unexplained
`UPDATE_UNREACHABLE`:

| Host | What for |
|---|---|
| `github.com` | the `releases/latest` redirect and the `releases/download/...` request |
| `release-assets.githubusercontent.com` | where that download is redirected to — the bytes and `SHA256SUMS` |

`objects.githubusercontent.com` is **no longer used** for this: the standing
Linux tester blackholed that host on 2026-08-27 and the download sailed through
anyway (finding 13). Do not add it to an allowlist expecting it to matter, and
do not treat blocking it as a way to disable updates — `auto = false` is that.

## The five design decisions worth knowing

### The install path is canonicalised before anything touches it

A real install is often a symlink — on the machine this was built,
`/usr/local/bin/flowspace3` points into a build tree. Renaming over the LINK
would replace it with a regular file and leave what it pointed at stale.
`install_path()` resolves it first, so the swap lands on the real binary and the
symlink keeps working.

### The staging file lives beside the target, not in `/tmp`

`rename()` across filesystems fails `EXDEV`, and `/tmp` is a different
filesystem often enough that the bug would only ever appear on somebody else's
machine. `tempfile::NamedTempFile::new_in(install_dir)` makes it structural.

### The running daemon keeps its old inode — that is the design

Nothing ever opens the running binary for writing (`ETXTBSY`). `rename()` is
atomic, so a concurrent `exec` sees either the whole old binary or the whole new
one. The live process goes on executing the inode it started with, which is
precisely why it has to be told to restart, and why the message exists.

Proven, not asserted: `crates/daemon/tests/auto_update.rs`'s
`a_running_binary_can_be_replaced_underneath_itself` spawns a process, swaps the
file out from under it, and checks both that the old process finishes happily
and that the path holds the new bytes.

### Not writable means notify-only, never a failing loop

If the install directory cannot be written — root-owned `/usr/local/bin` is the
common case — nothing is downloaded and nothing fails. The state row records the
reason and the queue carries a message naming the path, the reason,
`flowspace3 doctor upgrade`, and the reinstall one-liner.

Writability is probed by trying, not by reading permission bits: with ACLs,
read-only mounts and containers in play, the bits are only part of the answer,
and the question is precisely "would the rename work".

### The downloaded binary is asked what it is, before it is installed

A binary that lies about its own version is not a cosmetic bug for an updater —
it is a permanent loop. The comparison is `env!("CARGO_PKG_VERSION")` against
the newest published tag, so a build whose compiled-in version is stale is
*permanently* older than every release: download, swap, restart, still stale,
repeat, once per check interval, forever. The restart message can never clear,
because restarting does not change the answer.

Not hypothetical. **v0.2.0 shipped reporting 0.1.0** — release-please's `simple`
strategy bumped its own manifest and never touched `[workspace.package]
version`. Fixed in req-0060; see `docs/services/ci-release.md` for the three
mechanisms that now hold that shut on the release side.

This is the defence on the *client* side, and it is deliberately positioned
**before** the swap. Detecting it afterwards means the bad binary is already
installed and the daemon has to argue with itself about what it is; refusing
beforehand means the install never happens and the user gets one actionable
message.

Running it is not extra trust: its sha256 has already been checked against the
release's own `SHA256SUMS`, and executing `--version` is strictly less dangerous
than installing it. The probe also catches two classes a version comparison
never would — an asset built for the wrong triple, and a binary that cannot
`exec` at all.

## The user messages queue (req 59)

One table, `user_messages`, and one rule: **no feature invents its own envelope
side-channel.** A producer pushes; the envelope builder carries; the queue
clears.

| Field | Meaning |
|---|---|
| `key` | The producer's stable identity (`update:installed:0.3.1:/usr/local/bin/flowspace3`). Pushing twice updates one row. It is the PRIMARY KEY, so a scoped producer names its install here or two installs silently share one row. |
| `source` | The feature. |
| `install_path` | WHICH installation the message concerns. `NULL` means every installation on this store. |
| `severity` | `info`, `warning`, `error`. |
| `text` | What happened, in the user's terms. |
| `next_action` | What to do. **NOT NULL** — a message a user cannot act on is a log line. |
| `created_at` / `updated_at` | Set by Postgres, never by a producer. |
| `acked_at` / `expires_at` | The two clears a producer cannot do for itself. |

### Ownership is (source, install path)

A producer owns every row under its own source **and its own scope**, and none
under anyone else's. One source, one producer was one dimension short: a store
is shared by every installation pointed at it, so "the update producer" is not
one producer, it is one per install path, and they were retracting and
overwriting each other's rows.

`schema` and `logging` scope their messages `NULL` — a schema skew is a fact
about the store and an unwritable log directory is a fact about the host, so
both are news for every installation. `update` scopes to one path.

### Clearing without a clear-condition engine

There is no stored predicate, and this is the point. Every pass, a producer
declares the messages its source and scope **should** have right now;
`sync_messages` deletes the rest of that source *within that scope* in the same
transaction. An update that succeeds simply stops declaring "restart me" once
the running version matches, and the message disappears. Nothing evaluates a
rule, because there is no rule to evaluate.

`fs3_core::update::UpdateState::desired_messages` is that declaration, and it is
pure — which is why the whole clearing story is unit-tested with no database and
no network.

### How a message reaches you

- **Daemon-served verbs** (`add`, `scan`, `status`, `search`): attached in ONE
  place. `answer::ok`/`failed` take `&AppState`, so an endpoint physically
  cannot build an envelope without the queue — a compile error rather than a
  review comment. Scoped to the DAEMON's install path, because the daemon is
  what built the envelope.
- **`doctor`**: the one local verb holding a pool, so it carries the queue even
  with the daemon down, and shows a `messages` row. Scoped to the CLI's own
  install path — `doctor` is the verb that speaks for the binary you just typed.
- **Offline verbs** (`docs`, `config show`): no queue. They must work with the
  daemon and the store both absent, and paying a round-trip to tell you about an
  update while you are reading documentation offline would be a worse trade.
- **Humans**: `emit` prints each message to stderr, before any failure — a
  standing condition is often the explanation for the error under it.

## Configuration

```toml
[update]
auto = true               # ON BY DEFAULT (Jordan, 2026-08-27)
check_interval_hours = 24
```

`auto = false` means the daemon never reaches the network for a release and
never swaps a binary. `flowspace3 doctor upgrade` still works: a human asking
for an update is not the same thing as one happening unattended.

Full table: [`docs/reference/configuration.md`](../reference/configuration.md).

## Operating it

```bash
flowspace3 doctor            # `update` row: current / waiting-on-restart / blocked,
                             #   for the install path THIS binary lives at
                             # `messages` row: what the queue is currently saying
flowspace3 doctor upgrade    # force it now, ignoring the interval
```

`doctor upgrade` drives the same engine and writes the state row for its OWN
install path, so a manual upgrade clears the same message an automatic one
would — and, on a machine with two installs, clears the right one. It ignores
the interval because a person typing it has already decided it is time. It also
re-reads the binary on disk, which is what makes it the right verb after a
reinstall by hand.

## What is deliberately NOT here

| Not built | Why |
|---|---|
| **Phase 2: the daemon `exec()`ing itself** | Ruled a separate step (Jordan, 2026-08-27). Update-and-notify proves itself first; a process that restarts itself is a much sharper edge and deserves its own packet. |
| Windows | Out of scope for this packet. `TARGET_TRIPLE` is `None` there, so the updater degrades to notify-only rather than downloading something that cannot run. |
| musl | Excluded explicitly in `TARGET_TRIPLE`: a musl build handed a gnu binary would not run. No published musl release exists (ort-sys has no prebuilt runtime). |
| Delta updates | The binary is one file and the download is once a day. |
| Signing beyond sha256 | Out of scope. `SHA256SUMS` closes the "the bytes are not what the release published" gap; it does not close "the release itself is hostile". |
| The `self_update` crate | It probes `api.github.com` (rate-limited quota, on a cadence, shared with everything else on the machine) and verifies nothing beyond TLS. Both fight requirements this packet had. Verdict recorded at the top of `crates/daemon/src/update.rs`. |
| The `self-replace` crate | On POSIX the swap IS `fs::rename`; that crate exists for the Windows case, which is out of scope. |

## Where the code is

| Concern | File |
|---|---|
| Domain: state, desired messages, version compare | `crates/core/src/update.rs` |
| Domain: message + severity types | `crates/core/src/messages.rs` |
| Envelope's `messages` field | `crates/core/src/envelope.rs` |
| Schema | `crates/store/migrations/0008_user_messages.sql`, `0009_update_state.sql`, `0012_update_state_per_install.sql` |
| Persistence | `crates/store/src/messages.rs`, `crates/store/src/updates.rs` |
| Probe, verify, version guard, swap, lock, supervisor | `crates/daemon/src/update.rs` |
| Envelope attach point | `crates/daemon/src/answer.rs` |
| `doctor upgrade` | `crates/cli/src/upgrade.rs` |
| doctor rows | `crates/cli/src/doctor.rs` |
| Publishing `SHA256SUMS` | `.github/workflows/release.yml` |
| Keeping the binary's version truthful | `Cargo.toml` annotation · `release-please-config.json` · `.github/workflows/release-please.yml` · preflight leg B2 |
| Proof | `crates/daemon/tests/auto_update.rs`, `crates/store/tests/pg_migrations.rs` |

## Open, and named rather than hidden

- **The queue has three producers** (`update` here, `schema` in
  [`schema-skew.md`](schema-skew.md) added req-0061, `logging` added the same
  day). The second was chosen deliberately as the seam test, because its
  lifecycle is the opposite of this one's — the condition arrives from ANOTHER
  process at any instant and can clear without a restart — and
  `sync_messages(source, scope, desired)` carries all three with no change.
  `one_producer_declaring_does_not_retract_another_producers_message` is the
  proof that ownership holds, in both dimensions. Disk pressure and provider
  misconfig are still unwritten.
- **An envelope carries the messages of the install that BUILT it.** A CLI at
  `~/.local/bin` talking to a daemon at `/usr/local/bin` sees the daemon's
  messages, not its own, because the answer came from the daemon. `doctor` is
  the verb that reports on YOUR install. Carrying both would mean the CLI
  telling the daemon its path on the wire — a protocol change, deliberately not
  taken here, and worth revisiting the first time somebody is confused by it
  rather than pre-emptively.
- **A row for an install path that no longer exists is never deleted.** "Missing
  here" is not "missing everywhere" when one database serves several hosts, and
  a laptop must not retract a server's message. So the leak is ROWS, not
  messages: nothing can see a scope it does not occupy, and a reinstall at that
  path overwrites the row with truth on its first boot. There is no GC verb, and
  writing one honestly would need a host identity this schema does not have.
- **A pre-0012 binary run against a post-0012 store would write unscoped update
  messages again**, which every install would then see. Nothing stops that at
  the schema level; what covers it is the `schema` producer, whose entire job is
  to shout when a binary is older than its database.
- **No `ack` verb on the CLI.** `ack_message` exists in the store and nothing
  calls it, because no message today outlives its cause. The first message that
  needs dismissing brings the verb with it.
- **Offline verbs carry no messages.** Named above; revisit only if a real
  scenario shows someone missing news because they only ever run `docs`.
- **`latest_seen` is only written on a check.** A machine that has never
  successfully probed reports "never checked", which is honest but means the
  doctor row cannot distinguish "no network, ever" from "brand new install".
