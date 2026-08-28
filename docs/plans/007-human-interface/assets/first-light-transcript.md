# First light — transcript

Exit evidence for plan 007. Recorded against the LIVE daemon on :7373 (7 roots,
9,339 files, ~23,600 jobs queued) with the composed branch binary at
`target/debug/flowspace3`.

**Part 1 — the output contract — recorded 2026-08-28.**
**Part 2 — the stream and the TUI — pending u-w's composition.**

A terminal was allocated with `script -q /dev/null <cmd>`, which is how a
non-interactive session proves the TTY leg honestly: the CLI's own probe
(`std::io::stdout().is_terminal()`) sees a real pty, exactly as it would for a
person. The `^D` prefix on those lines is `script`'s own echo, not output.

## 1. A terminal, no flags → HUMAN

```console
$ script -q /dev/null flowspace3 status
  ▍ status  7 roots · 9339 files · 23635 queued

  ╭────────────────────────────────────────────┬───────────────────────────────────────────┬───────╮
  │ repo                                       ┆ root                                      ┆ files │
  ╞════════════════════════════════════════════╪═══════════════════════════════════════════╪═══════╡
  │ git:github.com/AI-Substrate/chainglass     ┆ /Users/jordanknight/substrate/chainglass  ┆  4183 │
  │ git:github.com/AI-Substrate/flowspace3     ┆ /Users/jordanknight/substrate/flowspace/f ┆   309 │
  │                                            ┆ lowspace3                                 ┆       │
  …
```

The element column gives way on a narrow terminal instead of the table running
past the edge — the reason `comfy-table` was chosen (LEARNINGS §1).

## 2. A pipe, no flags → JSON, with no flag required

```console
$ flowspace3 status | head -4
{
  "ok": true,
  "command": "status",
  "v": 1,
```

This is the property v1 had and the plan was not allowed to spend:
`flowspace3 … | jq` keeps working with nothing added to the command line.

## 3. A terminal with `--json` → JSON

```console
$ script -q /dev/null flowspace3 status --json | head -4
{
  "ok": true,
  "command": "status",
  "v": 1,
```

## 4. A pipe with `--human` → the rendered screen

```console
$ flowspace3 status --human | head -3
  ▍ status  7 roots · 9339 files · 23806 queued

  ╭────────────────────────────────────────────┬───────────────────────────────────────────┬───────╮
```

## 5. The agent check — risk r2, retired here rather than assumed

The risk: agents run inside tmux PTYs, so the terminal probe says "human" and is
WRONG. This command ran inside a live pij seat — an agent in a tmux pane, with a
pty allocated, which is precisely the shape that would break every agent in the
fleet if the override did not work:

```console
$ FS3_OUTPUT=json script -q /dev/null flowspace3 status | head -4
{
  "ok": true,
  "command": "status",
  "v": 1,
```

A harness exports `FS3_OUTPUT=json` once and stops thinking about it. Without
that override this same invocation renders (case 1 above), which is what makes
this a proof rather than a coincidence.

## 6. The byte check

```console
$ cargo test -p fs3-cli --test envelope_goldens
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Twelve verbs byte-identical to the goldens captured from the binary built at
`1ce572b` — before any of this plan's code existed — and the two documentation
verbs shape-asserted (see `crates/cli/tests/goldens/PROVENANCE.md`). No golden
file was modified by any unit at any point in this plan.

## Part 2 — pending

`status --watch` on the live stream, two concurrent watchers, `add` showing real
progress from `scan_progress`, the TUI live during a scan, and the TUI degrading
when the daemon is unreachable. All five need u-w composed; the runbook for them
is `first-light.md`.
