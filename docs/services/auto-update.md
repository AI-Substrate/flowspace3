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
  └── claim_check(interval)          conditional UPDATE in Postgres
        ├── not due  → no network at all
        └── due      → probe → compare → download → verify → lock → swap
  └── update_state → desired_messages(running) → sync_messages("update")
```

1. **Claim.** `update_state.last_checked_at` is advanced by a conditional
   `UPDATE`. Only a caller whose `UPDATE` matched a row runs a check, which
   makes the interval survive restarts and makes two daemons on one store
   incapable of both checking.
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
7. **Say so.** The state row becomes messages, and the messages ride on every
   envelope.

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
| `key` | The producer's stable identity (`update:installed:0.3.1`). Pushing twice updates one row. |
| `source` | The feature. A producer owns every row under its own source and none under anyone else's. |
| `severity` | `info`, `warning`, `error`. |
| `text` | What happened, in the user's terms. |
| `next_action` | What to do. **NOT NULL** — a message a user cannot act on is a log line. |
| `created_at` / `updated_at` | Set by Postgres, never by a producer. |
| `acked_at` / `expires_at` | The two clears a producer cannot do for itself. |

### Clearing without a clear-condition engine

There is no stored predicate, and this is the point. Every pass, a producer
declares the messages its source **should** have right now; `sync_messages`
deletes the rest of that source in the same transaction. An update that succeeds
simply stops declaring "restart me" once the running version matches, and the
message disappears. Nothing evaluates a rule, because there is no rule to
evaluate.

`fs3_core::update::UpdateState::desired_messages` is that declaration, and it is
pure — which is why the whole clearing story is unit-tested with no database and
no network.

### How a message reaches you

- **Daemon-served verbs** (`add`, `scan`, `status`, `search`): attached in ONE
  place. `answer::ok`/`failed` take `&AppState`, so an endpoint physically
  cannot build an envelope without the queue — a compile error rather than a
  review comment.
- **`doctor`**: the one local verb holding a pool, so it carries the queue even
  with the daemon down, and shows a `messages` row.
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
flowspace3 doctor            # `update` row: current / waiting-on-restart / blocked
                             # `messages` row: what the queue is currently saying
flowspace3 doctor upgrade    # force it now, ignoring the interval
```

`doctor upgrade` drives the same engine on the same state row, so a manual
upgrade clears the same message an automatic one would. It ignores the interval
because a person typing it has already decided it is time.

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
| Schema | `crates/store/migrations/0008_user_messages.sql`, `0009_update_state.sql` |
| Persistence | `crates/store/src/messages.rs`, `crates/store/src/updates.rs` |
| Probe, verify, version guard, swap, lock, supervisor | `crates/daemon/src/update.rs` |
| Envelope attach point | `crates/daemon/src/answer.rs` |
| `doctor upgrade` | `crates/cli/src/upgrade.rs` |
| doctor rows | `crates/cli/src/doctor.rs` |
| Publishing `SHA256SUMS` | `.github/workflows/release.yml` |
| Keeping the binary's version truthful | `Cargo.toml` annotation · `release-please-config.json` · `.github/workflows/release-please.yml` · preflight leg B2 |
| Proof | `crates/daemon/tests/auto_update.rs` |

## Open, and named rather than hidden

- **The queue has two producers** (`update` here, `schema` in
  [`schema-skew.md`](schema-skew.md), added req-0061). The second was chosen
  deliberately as the seam test, because its lifecycle is the opposite of this
  one's — the condition arrives from ANOTHER process at any instant and can
  clear without a restart — and `sync_messages(source, desired)` carried both
  with no change. `one_producer_declaring_does_not_retract_another_producers_message`
  is the proof that per-source ownership holds. Disk pressure and provider
  misconfig are still unwritten.
- **No `ack` verb on the CLI.** `ack_message` exists in the store and nothing
  calls it, because no message today outlives its cause. The first message that
  needs dismissing brings the verb with it.
- **Offline verbs carry no messages.** Named above; revisit only if a real
  scenario shows someone missing news because they only ever run `docs`.
- **`latest_seen` is only written on a check.** A machine that has never
  successfully probed reports "never checked", which is honest but means the
  doctor row cannot distinguish "no network, ever" from "brand new install".
