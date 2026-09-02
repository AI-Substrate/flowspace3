# event-stream — what the daemon says while it works

**Status**: contract frozen 2026-08-28 (plan 007, tk-a104). Types:
`crates/core/src/events.rs`. Producer: `fs3-daemon` (unit u-w). Consumers:
`flowspace3 status --watch`, `flowspace3 tui` (unit u-t), the `add` progress
indicator (unit u-r).

## Why it exists

Every interface fs3 has built so far had to invent activity, because the daemon
told nobody what it was doing (DL-045). The human-render prototype inferred
progress from poll deltas; the TUI prototype faked a feed; `flowspace3 add` sits
silent through a whole walk. One stream retires all three, and any future web UI
reads the same wire.

## The wire

NDJSON over HTTP. One JSON object per line, UTF-8, `\n`-terminated, never
pretty-printed, flushed per line. Lines are independent: a consumer that drops
one stays correct.

```
GET /events                     → 200, Content-Type: application/x-ndjson
GET /events?heartbeat_ms=5000   → same, faster liveness pulse
```

The connection stays open until the client hangs up or the daemon stops. There
is no cursor and no replay: this is a live feed, and a consumer that needs the
whole truth asks `GET /status`, which is a snapshot by design.

### Line 1 is always the hello

```json
{"stream":"fs3.events","v":1,"daemon":"0.4.0","heartbeat_ms":15000}
```

A consumer reads it before anything else and refuses a `v` it cannot parse. It
is also how a client that connected to the wrong port finds out immediately
instead of waiting for a first event that never comes.

### Every line after it is an event

```json
{"v":1,"at":"2026-08-28T03:11:04.219Z","kind":"scan_progress","root":"git:github.com/AI-Substrate/flowspace3","root_path":"/srv/api","files_seen":1200,"enqueued":900,"current":"crates/cli/src/main.rs"}
{"v":1,"at":"2026-08-28T03:11:05.001Z","kind":"job_done","job":"scan_file","subject":"src/lib.rs","ms":12,"left":455}
{"v":1,"at":"2026-08-28T03:11:20.000Z","kind":"heartbeat","seq":1}
```

`v` and `at` are on every line; `kind` selects the rest. Internally tagged, so a
line is flat and nothing has to be unwrapped — the same shape decision the
envelope made with `ok`.

## The vocabulary

| kind | fields | emitted when |
|---|---|---|
| `job_done` | `job`, `subject`, `ms`, `left` | a queued job settles — at the runner's existing completion point, beside `complete_job`/`jobs_remaining` |
| `job_failed` | `job`, `subject`, `error`, `attempts`, `terminal` | a job parks, retries or fails; `terminal: true` is the one worth alerting on |
| `queue` | `rows[{kind,state,count}]` | the queue's shape changes — a SNAPSHOT, so a late or lossy consumer is instantly correct again |
| `scan_progress` | `root`, `root_path`, `files_seen`, `enqueued`, `current?` | during the walk `add`/`scan` performs before it answers |
| `root_changed` | `change` (`added`/`rescanned`/`removed`), `root`, `root_path`, `files` | a root is registered, re-scanned or removed |
| `heartbeat` | `seq` | nothing else has happened for `heartbeat_ms` |

### Adding a kind is additive, always

`STREAM_VERSION` bumps only if the LINE framing breaks. A new kind is not a
version bump, because every consumer parses an unrecognised kind into
`EventKind::Unknown` — it keeps its `v` and `at`, and simply is not drawn. That
is what lets a shipped TUI meet a newer daemon and degrade instead of dying;
`a_kind_from_the_future_parses_instead_of_breaking_the_stream` is the test that
holds producers to it.

## Multi-subscriber

Two watchers attached at the same time see the same events. The daemon fans out
from one in-process broadcast; a slow consumer is dropped from the fan-out
rather than allowed to back-pressure the runner — indexing must never wait on a
dashboard. A dropped consumer notices at its next heartbeat gap and reconnects.

## Where the events come from

Emission mirrors the daemon's EXISTING lifecycle; no new worker loop, no second
source of truth about what "done" means.

| event | seam |
|---|---|
| `job_done`, `job_failed` | `crates/daemon/src/runner.rs` — the settle path beside `complete_job`, and the park/retry/fail branches; the batched embed path has its own completion point |
| `queue` | after any settlement that changes a depth the daemon already recomputes |
| `scan_progress` | `crates/daemon/src/roots.rs` — inside the walk, on a cadence, not per file |
| `root_changed` | `crates/daemon/src/roots.rs` (add/rescan) and `crates/daemon/src/remove.rs` (remove) |
| `heartbeat` | the stream handler itself, per subscriber |

`scan_progress` is emitted after every **256 files** or **1,000ms**, whichever
comes first, plus one final totals line. The count bound keeps a 40,000-file
walk near 157 volume-driven updates instead of 40,000 lines; the time bound
keeps a cold page cache, large file or network filesystem from looking hung.
Both are explicit because a default is a number nobody chose.

## What this stream is NOT

Not a log. `tracing` owns the daemon's narration of itself and it goes to the
log file. The stream carries work with a subject and a count — things an
interface can draw — and mixing the two would make it unreadable exactly when
someone needs it.

Not an envelope. The `/events` body is a sequence of event lines, not one
workshop-004 envelope, because the envelope is a shape for ANSWERS and this is
not an answer. `status --watch` is therefore the one place stdout carries
something other than an envelope; every other invocation, including plain
`status`, is untouched, and the byte-goldens in
`crates/cli/tests/envelope_goldens.rs` prove it.

## Consumers, and what each needs from this contract

| consumer | needs |
|---|---|
| `status --watch` | the raw lines in JSON mode (an agent gets NDJSON it can parse), a live summary in human mode |
| `tui` (u-t) | `job_done`/`job_failed` for the activity pane, `queue` for depth, `root_changed` for the roots pane; builds against a recorded fixture until composition |
| `add` progress (u-r) | `scan_progress` while the POST is in flight; the meter is written to STDERR so stdout stays exactly what it was |

## Implementation assumptions

- Consumers attach before the activity they want to draw. The stream retains
  no history; `/status` supplies current roots and queue truth after reconnect.
- One broadcast send determines ordering and timestamp once, so concurrent
  subscribers receive byte-identical work events in the same order.
- Queue rows come from the live-only `fs3_store::queue_depth` after settlement.
  The stream owns no shadow counters, cannot drift from the store, and never
  rescans completed history.
- Heartbeat sequence numbers are per connection. They prove that connection's
  liveness and are intentionally not identical between subscribers.

## Snap-in recipe

The composition root owns exactly one bounded broadcast. These are the wiring
lines in `crates/daemon/src/wiring.rs`:

```rust
events: broadcast::Sender<Event>,

let (events, _) = broadcast::channel(EVENT_CAPACITY);

Ok(Self {
    // existing services...
    events,
    // existing state...
})
```

`EVENT_CAPACITY` is 256. Producers call synchronous `send`; each HTTP response
has a separately bounded 256-line channel and closes when that channel fills or
its broadcast receiver reports lag. No producer awaits either channel.

The router hook in `crates/daemon/src/http.rs` is one line beside the existing
read routes, before shared authentication is layered over the router:

```rust
.route("/events", get(events))
```

The handler enqueues `Hello` before starting its event task, then yields one
compact serialized object plus `\n` per body frame. `status --watch` opens the
route through `DaemonClient::events`, so the same per-boot bearer key policy
applies here as to every other daemon request.
