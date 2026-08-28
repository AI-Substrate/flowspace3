# Worker brief — flowspace TUI mock-up POC · (seat at canary)

**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-28 · Jordan-ordered POC, scratch-only.

## The job

Jordan (verbatim intent): "a mock-up of what an awesome looking TUI could look
like if we were to add a TUI verb to our flowspace thing that shows our
database operations, what it's currently doing, maybe has some basic search
capability, shows which things are registered, turn it into a bit of an
interface. I just want to see what this looks like in Rust, otherwise we might
just go for some sort of Rust backend, React front-end style split, hosted out
of the daemon."

This is a LOOK-AND-FEEL POC, not a product feature. The decision it feeds:
Rust TUI (ratatui) vs Rust-backend+React-frontend served by the daemon. Make
it BEAUTIFUL — this succeeds or fails on visual impression.

Deliverables (numbered):

1. A standalone cargo project at `scratch/tui-poc/` (own Cargo.toml, NOT a
   workspace member — scratch is gitignored, keep it fully self-contained;
   ratatui + crossterm recommended, your call on extras like tui-textarea for
   the search box). `cargo run` from that dir launches it.
2. The interface, four regions minimum (layout/design yours — impress):
   a. REGISTERED ROOTS: the indexed repos (identity, path, file count).
   b. LIVE OPERATIONS: what the daemon is doing now — queue depths by
      kind/state (scan/summarize/embed × pending/running/done), ideally as a
      sparkline/gauge over time while the TUI runs.
   c. ACTIVITY FEED: recent operations scrolling (scanned X, summarized Y,
      embedded Z...).
   d. SEARCH: a text box that runs a query and renders ranked results
      (score, address, snippet) in-pane.
3. REAL DATA where it is cheap, mocked where it is not: `flowspace3 status`
   (JSON envelope) polled for roots + queue; `flowspace3 search <q>` (JSON)
   for the search pane; the activity feed may be mocked/derived from queue
   deltas — label mocked panes subtly (e.g. dimmed "mock" tag). READ-ONLY:
   no DB writes, no daemon control, do not touch the database directly —
   go through the CLI only.
4. Evidence for Jordan: run it in a real terminal, capture 2-3 states
   (idle, searching, busy) — `tmux capture-pane` text captures committed to
   scratch/tui-poc/captures/ are fine (color is lost; also note how to run it
   live: cd scratch/tui-poc && cargo run).
5. A one-page verdict note scratch/tui-poc/VERDICT.md: effort spent, what was
   easy/painful in ratatui, what a web split would do better/worse, and your
   recommendation for the real `flowspace3 tui` verb (or against it).

## Rules & fence

- Fence: `scratch/tui-poc/**` ONLY. No commits (scratch is gitignored). No
  workspace edits, no crates/ edits, no Cargo.toml outside your dir.
- Disk is recovering from a full-disk incident: build with
  `CARGO_INCREMENTAL=0`, and if a build dies with `rustc-LLVM ERROR: IO
  failure` that is DISK, not your code — report it, do not debug it.
- The daemon is LIVE on this machine and mid-scan sometimes — a working
  status poll should show real numbers. If the CLI surprises you, that is
  product feedback: `harness observe` it (list, never clear).
- `pij send` needs no export in main (you run in the main clone).

## Report back

claim · how to run it · capture paths · VERDICT.md summary (3 lines) ·
observations. Ack via pij send to pij-instant-lynx with your read + numbered
plan (incl. your layout sketch in ASCII) before coding. Deviations = stop-and-ask.
