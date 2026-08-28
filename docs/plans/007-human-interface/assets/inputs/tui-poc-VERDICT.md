# tui-poc verdict — ratatui mock of a `flowspace3 tui` verb

**Seat**: pij-mammoth-ox · 2026-08-28 · brief `w-tui-poc` · scratch-only, read-only, CLI-only.

## How to run it

```bash
cd scratch/tui-poc
cargo run            # live data: status polled every 2s, search on Enter
cargo run -- --demo  # synthetic busy daemon (sparklines + mock feed) for screenshots
```

Keys: type + `Enter` to search (search focused on launch), `Tab` cycle panes,
`/` jump to search, `↑/↓` select results, `Esc` leave search, `q` quit.
Captures (plain text, color lost): `captures/1-idle.txt`, `2a-searching.txt`,
`2-search-results.txt`, `3-busy-demo.txt`.

## Effort spent

~2.5 hours end to end, of which the working UI was roughly the first hour —
one file, ~700 lines, 3 deps (ratatui, serde, serde_json). It compiled clean
on the first real build and there was **zero** debugging of the UI itself.
The rest was polish, captures, and this note. Real data throughout: `status`
and `search` JSON envelopes parsed straight off the CLI's stdout; only the
per-file activity feed is mocked (tagged in-pane), because the CLI exposes
queue *counts*, not an event stream.

## What was easy in ratatui

- **Layout is the star.** Constraint-based split (percent/length/min) made the
  "search steals the screen when focused" behaviour a 3-line `if`. This kind
  of state-driven relayout is genuinely easier than CSS.
- Sparklines, gauges, styled spans, rounded borders — all built in, all look
  good with near-zero effort. The busy screen dances.
- The whole app is one synchronous draw loop + two worker threads over an
  mpsc channel. No async, no runtime, no build step, starts instantly, ~4MB
  binary that would ride along inside the existing `flowspace3` binary as
  just another verb. Distribution cost: zero.

## What was painful

- **Text is manual.** Truncation, padding, alignment, wrapping snippets — you
  do it all by hand in character counts. A results card that a browser gives
  you for free (flexbox + ellipsis) is fiddly arithmetic here.
- The search input is hand-rolled append/backspace; a real one (cursor
  movement, selection, history) means pulling `tui-textarea`.
- No mouse, no links: an address you can't click or copy-select per-field is
  a real loss for *this* product, whose output is addresses.
- Color depth and glyph fidelity depend on the user's terminal; captures for
  sharing lose color entirely (tmux capture-pane is text-only).

## What a web split would do better / worse

Better: clickable addresses that jump to code, text selection/copy, real
typography and density for snippets, shareable URLs, richer charts, mouse.
Worse: a second stack (React build, asset serving from the daemon, port +
auth story), a browser tab that isn't where agents and terminal people live,
and 10–100x the dependency surface for what is, today, a status page. The
daemon would also need a real event/SSE endpoint either way — the CLI-poll
trick is POC-only.

## Recommendation

**Ship a modest `flowspace3 tui` verb in ratatui — and treat it as the front
half, not instead of, an eventual web surface.** The TUI at POC quality is
already demo-worthy and costs almost nothing to carry (3 crates, one module,
same binary, works over ssh where the daemon lives). What it needs from the
product to graduate from mock to real is the same thing a web UI would need
first anyway: an activity/event stream from the daemon (even a `status
--watch` NDJSON tail) instead of derived queue deltas. Build that seam next;
defer React until someone actually asks to click.
