# daemon-logs — the daemon's log file, rotation, and panic capture

**Status**: live. **Owner**: pij-shiny-keech (daemon observability).

## What it is

The daemon's durable evidence trail: a rolling log FILE beside the stdout the
daemon has always written, panics routed into it, a hard ceiling on the disk it
consumes, and a `flowspace3 doctor` row naming the active path.

## Why it exists

On 2026-08-27 the summarize lane died silently mid-run — 9,411 jobs pending,
zero claims — and the only copy of the panic was the scrollback of the terminal
Jordan happened to be running the daemon in. The daemon logged to stdout and
nowhere else. Two consequences fall straight out of that incident:

1. **A file, not a terminal.** Phase-2 self-restart (the daemon exec'ing
   itself) has no terminal at all, so a log file is mandatory rather than nice.
2. **Panics have to reach it.** A panic inside a `tokio::spawn`ed task unwinds
   inside that task; nothing in the task's own code observes it. The panic HOOK
   is the only thing that can, so `logging::init` installs one that logs through
   `tracing` and then calls the previous hook (stderr keeps behaving normally).

## Where things are

| Thing | Where |
|---|---|
| Subscriber construction, panic hook, `Roller` | `crates/daemon/src/logging.rs` |
| Path resolution, message shape, file names | `crates/core/src/logging.rs` (pure) |
| `[daemon] log_*` keys and their validation | `crates/core/src/config.rs` |
| Boot wiring, the startup line, the queue push | `crates/daemon/src/boot.rs` |
| The `logs` doctor row | `crates/cli/src/doctor.rs` |
| Option rows (drift-tested) | `docs/reference/configuration.md` |

## Key decisions and why

- **The subscriber is built in `boot`, not in `main`.** The log path, the size
  caps and the filter are all configuration, so a subscriber installed before
  the config is read can honour none of them. `crates/cli/src/main.rs` now hands
  `flowspace3 daemon` straight to `fs3_daemon::run`.
- **`Effective::has_file` carries the missing-config fact as DATA.** The loader
  used to log "no config file: running on defaults" itself. It now runs before
  any subscriber exists, so the fact travels back and `boot` says it once
  logging is up. Nothing was lost; the ordering just became honest.
- **One path convention, not two.** `log_dir` defaults to the literal string
  `~/.local/state/flowspace3/logs` and is expanded by `fs3_core::resolve_log_dir`,
  which takes `home` as an ARGUMENT (core performs no effects, so a default that
  read `$HOME` would be a lie about the crate). The brief suggested the
  `directories` crate; o-prime accepted the override on 2026-08-27 — the repo
  already hand-rolls `~/.config/flowspace3` in both `fs3-cli` and `fs3-daemon`,
  and a second path convention beside the first is how config drift starts.
- **Rotation is hand-rolled, deliberately.** `tracing-appender`'s rolling writer
  rolls by CALENDAR only — hourly, daily, never — with no size cap and no
  retention sweep. It cannot state the invariant this exists to keep, so
  `Roller` rolls on size and deletes the oldest generation:
  `log_max_bytes × log_max_files` is a hard ceiling (40 MB by default).
  Generations are numbered (`flowspace3.log`, `.1`, `.2`, oldest highest) rather
  than timestamped, so names are deterministic and two rolls in one second
  cannot collide.
- **The file is never coloured; stdout is coloured only on a TTY.** Two layers,
  one subscriber. Redirecting the daemon's stdout used to produce a file full of
  escape sequences (the standing Linux tester's finding).
- **Reopening APPENDS.** A restart continues the same file rather than
  destroying the evidence of why the previous process stopped.
- **A bad log path degrades; it never crashes.** No file means stdout alone, a
  `warn` on the first line, and one `logging:unwritable:<dir>` message in the
  user-messages queue (`fs3_core::LOGGING_SOURCE`). Declared once at boot rather
  than from a reconcile pass, because unlike schema skew the condition cannot
  change while the process runs — but still level-triggered, so a healthy start
  declares an empty set and retracts the previous run's complaint.
- **Doctor reports, never repairs.** The `logs` row names the active file and
  proves writability by actually writing (a permissions bit is not proof — a
  directory can be mode 755 and owned by root). It never creates the directory
  it reports on: that would be doctor reporting on its own handiwork.

## The invariant

**No unbounded disk growth, ever.** At most `log_max_files` files exist in the
log directory, each rolled at `log_max_bytes`. The honest edge: a single event
larger than the whole cap is written anyway rather than dropped, so a file's
true ceiling is `log_max_bytes + one event` — rolling first would spin producing
empty files, and at an 8 MB default the case is academic.

## How it is proved

| Claim | Proof |
|---|---|
| Rotation rotates, retention holds | `logging::tests::writing_past_the_cap_rolls_and_never_exceeds_the_retention` |
| Generations are ordered newest-first | `logging::tests::the_newest_events_are_in_the_active_file_and_older_ones_behind_it` |
| A restart does not truncate | `logging::tests::reopening_appends_rather_than_truncating` |
| **A spawned task's panic lands in the file** | `crates/daemon/tests/logging_file.rs` (mutation-checked: it FAILS with the hook install removed) |
| The file carries no escape sequences | same test — asserts ABSENCE, strips nothing |
| An unwritable path degrades and raises a message | `crates/daemon/tests/logging_degraded.rs` |
| The real binary writes a file and names it | `crates/cli/tests/daemon_logging.rs` — runs `flowspace3 daemon`, no database needed |
| Config overrides, both layers | `config::tests::the_log_destination_is_configurable_from_the_file_and_the_environment` |
| Doctor's row | `doctor::tests::*_log*` |

## Operating notes

- Where are the logs? `flowspace3 doctor` — the `logs` row. Or the daemon's
  first line, which names the path it is using (or says `stdout only`).
- Want everything for one run? `RUST_LOG=debug flowspace3 daemon` — `RUST_LOG`
  still beats `[daemon] log_level`, so debugging never means editing a file.
- Both destinations share one filter. If that ever needs to differ per
  destination, it is two `EnvFilter`s and a per-layer filter, not a redesign.
