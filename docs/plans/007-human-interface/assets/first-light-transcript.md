# First light — transcript

Exit evidence for plan 007. Recorded against the LIVE daemon on :7373 (7 roots,
9,339 files, ~23,600 jobs queued) with the composed branch binary at
`target/debug/flowspace3`.

**Part 1 — the output contract — recorded 2026-08-28.**
**Part 2 — the stream and the TUI — recorded 2026-08-28 on the composed branch.**

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
`1ce572b` — before any of this plan's code existed.

Two golden files, `stdout/docs-list.stdout` and `stdout/agents-start-here.stdout`,
were DELETED by the PM in this branch, and the byte assertion for those two verbs
was replaced with a shape assertion. That was a ruled, deliberate change, not a
drift: their payload is the bundled documentation, which ac-0005 rewrites on
purpose and which main's authentication work rewrote again underneath us, so the
byte alarm misfires there by design (o-prime ruling, 2026-08-28; the reasoning and
the loss it accepts are written up in `crates/cli/tests/goldens/PROVENANCE.md`).

Stated precisely, because an earlier draft of this line was not: **no golden was
ever RE-CAPTURED, and no unit modified one.** Twelve of the fifteen captured
files still hold their original bytes; two were removed with a ruling and a
replacement assertion; the fifteenth (`usage-error-prints-no-envelope.stdout`,
0 bytes) is untouched. The distinction matters — re-capturing would have hidden a
change, and removing with a ruling records one.

## Part 2 — the stream, the meter, and the TUI

Recorded 2026-08-28 on the fully composed branch, against an ISOLATED daemon —
minted database `fs3_pm_firstlight`, an empty `FS3_CONFIG_DIR`, unique port
7740, in-tree logs — per the DL-004 rule. The boot line was verified BEFORE
anything indexed:

```console
INFO fs3_daemon::boot: fs3 daemon starting config=/tmp/fs3-pm-firstlight.QCkTAI
  embedder=fake summarizer=fake repos=0
INFO fs3_daemon::http: fs3 daemon listening bound=127.0.0.1:7740
```

`embedder=fake summarizer=fake` — no live provider was used and nothing was
spent. The database was dropped and the config directory removed at the end.

### 7. Two concurrent watchers see the same events

Both attached, then a real `scan` of the 310-file main clone ran underneath
them:

```console
$ flowspace3 status --watch --json > a.ndjson &
$ flowspace3 status --watch --json > b.ndjson &
$ flowspace3 scan /Users/jordanknight/substrate/flowspace/flowspace3

lines a=1352 b=1352
IDENTICAL WORK EVENTS ACROSS BOTH WATCHERS
```

Compared after excluding the per-connection `Hello` and `heartbeat` lines, which
are per-subscriber by design. ac-0004's multi-subscriber half, proven on live
data rather than in a fake.

### 8. The stream is NDJSON, line by line, and it stops politely

```console
$ flowspace3 status --watch --json | head -4
{"stream":"fs3.events","v":1,"daemon":"0.4.0","heartbeat_ms":15000}
{"v":1,"at":"2026-08-28T05:09:40.138Z","kind":"job_done","job":"embed","subject":"9 x raw","ms":217,"left":1871}
{"v":1,"at":"2026-08-28T05:09:40.140Z","kind":"queue","rows":[{"kind":"embed","state":"done","count":560},…]}
{"v":1,"at":"2026-08-28T05:09:40.151Z","kind":"job_done","job":"scan_file","subject":"pocs/human-render/tests/surfaces.rs","ms":249,"left":1870}
flowspace3: writing the event stream: Broken pipe (os error 32)
```

The `Hello` arrives first, lines are flushed as they happen (`head -4` returned
immediately, mid-scan), and a consumer that hangs up gets a named message rather
than a panic. An agent parses this with `while read line`.

### 9. `scan_progress` fires on the ruled cadence

From the same run, during `add` of the main clone:

```json
{"kind":"scan_progress","root":"git:github.com/AI-Substrate/flowspace3","files_seen":256,"enqueued":0,"current":"docs/plans/prd/workshops/003-query-surface.md"}
{"kind":"scan_progress","root":"git:github.com/AI-Substrate/flowspace3","files_seen":310,"enqueued":295,"current":"pocs/human-render/LEARNINGS.md"}
{"kind":"scan_progress","root":"git:github.com/AI-Substrate/flowspace3","files_seen":310,"enqueued":310}
```

The first line is the 256-file COUNT pulse; the second is the 1000ms TIME pulse
that the PM added at ack (count-only cadence goes silent exactly when a walk is
slowest, which reads as a hang); the third is the final totals event. Both
halves of the ruled cadence, observed in one walk.

### 10. `add` at a terminal shows a human answer, and the queue it created

```console
$ script -q /dev/null flowspace3 add /Users/jordanknight/substrate/flowspace/fs3-hi-goldenbase
  …
  directories not walked
   crates/parsers/fixtures/discovery-bare/node_mod  index it anyway with `[scan] standard_ignores =
   ules                                             false`
  → 305 scan jobs queued — poll flowspace3 status until the queue is empty, then search
```

Skips and pruned directories are shown with the fix for each, and the footer is
the daemon's own `next_action` rather than the renderer's guess.

### 11. The TUI, live, during a real scan

`flowspace3 tui` in a real terminal, captured from the pane while the scan ran:

```text
╭ DASHBOARD ────────────────────────────────────────────────────────────────╮
│ flowspace³   LIVE · 0s ago  activity live · daemon 0.4.0                   │
╰───────────────────────────────────────────────────────────────────────────╯
╭ ROOTS · LIVE ────────────╮╭ OPERATIONS · LIVE ───────╮╭ ACTIVITY · LIVE ──────────────╮
│ROOT / PATH        FILES  ││KIND        STATE   DEPTH ││05:11:17  summarize finished ·│
│…/flowspace3       310    ││embed       done    3981  ││05:11:17  embed finished · 1 x│
│…/fs3-goldenbase   305    ││scan_file   done    615   ││05:11:17  summarize finished ·│
│                          ││summarize   pending 943   ││05:11:17  embed finished · 1 x│
╰──────────────────────────╯╰──────────────────────────╯╰──────────────────────────────╯
Tab panes · ↑↓ select · Enter search · Ctrl-P/N history · Esc leave search · q quit
```

Every activity line is a real `job_done` from the stream. There is no mock tag
anywhere, because there is nothing mocked. Search focus makes the results pane
dominant (the POC verdict's behaviour, kept); `Esc` returns to this dashboard.

### 12. The TUI when the daemon goes away

The daemon was stopped underneath the running TUI:

```text
│ flowspace³   STALE · 12s old · retrying every 2s · FS3-E-DAEMON-UNAVAILABLE: cannot reach the fs3 daemon at http://127.0.0.1:7740 …
╭ ROOTS · STALE · 12s old ─╮╭ OPERATIONS · STALE · 12s old ─╮╭ ACTIVITY · DISCONNECTED ─╮
```

Every pane says what it is: STALE with an AGE, the retry cadence, the actual
error code, and the activity feed marked DISCONNECTED rather than frozen and
pretending. `q` exited cleanly and restored the terminal.

## Verdict

ac-0001 (§1-4), ac-0002 (§6), ac-0003 (§11-12), ac-0004 (§7-9) and Jordan's
add-progress ruling (§9-10) are all shown here on live data. ac-0005 and ac-0006
are proven by the gate and the diff rather than by transcript: `harness checks`
is green on the composed branch, and the agent-path docs carry the output rules
with a test asserting they do.
