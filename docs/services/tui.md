# Terminal dashboard

`flowspace3 tui` is the human, interactive view of the same daemon data exposed
to agents. It renders four surfaces in one terminal: indexed roots, queue depth
and history, the real activity stream, and semantic search.

## Run

```console
flowspace3 tui
flowspace3 tui --daemon-url http://127.0.0.1:7373
```

The dashboard works in a plain terminal and over SSH. It owns the alternate
screen only while running and restores the screen, cursor, and raw-mode state
on normal return, error, and panic unwind.

## Keys

| Key | Action |
|---|---|
| `Tab` | Cycle roots, operations, activity, and search |
| `/` | Focus search |
| typing, `Enter` | Edit and submit a search |
| `Left`/`Right`, `Home`/`End`, `Backspace`/`Delete` | Edit by Unicode scalar, never by byte |
| `Up`/`Down` | Select a search result |
| `Ctrl-P`/`Ctrl-N` | Older/newer submitted query |
| `Esc` | Leave search |
| `q` | Quit while search is not focused |

Search takes over the content area while focused. Result addresses are clipped
by their terminal rectangle, and the selected result's context wraps in its own
pane; no UTF-8 string is byte-sliced to fit a terminal cell count.

## Data and failure semantics

Snapshots come from `fs3_cli::DaemonClient::status` and
`fs3_cli::DaemonClient::search`. The dashboard polls status every two seconds.
A failed refresh preserves the last snapshot for diagnosis but labels every
snapshot-backed pane `STALE`, including the snapshot's age and the daemon's
first actionable error line. A later successful refresh clears the stale state.
No stale value is styled as live.

Activity comes only from `GET /events`, decoded as the frozen
`fs3_core::events::{Hello, Event, EventKind}` NDJSON contract. Heartbeats and
unknown future event kinds keep the connection alive but do not create feed
entries. An empty feed says `No activity yet.`; it never manufactures activity.
EOF, a partial line, an HTTP failure, or a two-heartbeat silence marks the pane
`DISCONNECTED`; the worker reconnects every two seconds without blocking input
or drawing.

## Snap-in recipe

The unit consists of these exact composition hooks:

1. `crates/cli/src/lib.rs`: `pub mod tui;`
2. `crates/cli/src/main.rs`: add `Command::Tui { daemon_url }`, build the normal
   `DaemonClient`, and call `fs3_cli::tui::run(client).await?`; do not call
   `emit()`, because an interactive screen is not an envelope.
3. `crates/cli/Cargo.toml`: direct dependencies `ratatui = "0.29"` and
   `crossterm = "0.28"`; enable reqwest's existing `stream` feature for chunked
   NDJSON consumption.
4. `crates/testkit/arch-allowlist.toml`: add only `ratatui` and `crossterm` to
   `crates.fs3-cli.external`.

### Named event-source composition seam

`crates/cli/src/tui.rs::event_stream_request` is the one source seam. **This is
the line the composer changes** if u-w exposes a concrete request helper:

```rust
http.get(format!("{base_url}/events"))
```

The base URL is copied from the same `DaemonClient` used for snapshots. The
NDJSON decoder, heartbeat timeout, reconnect loop, state transitions, and UI
must not change during composition. The recorded fixture at
`crates/cli/tests/fixtures/tui-events.ndjson` is test input only; production
always targets the live endpoint.

## Assumptions at unit handoff

- `GET /events` follows the frozen service contract: HTTP 200, a `Hello` first,
  then newline-terminated `Event` records, with heartbeats at the cadence in
  `Hello.heartbeat_ms`.
- A stream is live-only and has no replay cursor. Status remains the complete
  snapshot after reconnect.
- Additive event kinds deserialize as `EventKind::Unknown`; the dashboard skips
  them without dropping the stream.
- The daemon endpoint needs no authentication beyond the existing local daemon
  contract.
