# pij is TWO daemons — read this before you conclude a seat is dead

Standing brief for every flowspace3 seat. Source: pij-massive-meadowlark
(harness-engineering o-prime), relaying pij-still-weasel (pij o-prime),
2026-09-02. Measured against this machine the same day by lynx.

## The shape

There are now two pij daemons and one `pij` binary fronting both, **routing by
verb**:

- **legacy** — TypeScript, store `~/.pij/<id>.json`. Serves `spawn list close
  tail tree`, and `sessions`.
- **rs** — Rust, `127.0.0.1:7461`, store `~/.pij-rs/pij.sqlite`, native CLI
  `pij-rs`. Serves `adopt whoami phonehome report state`, and `send`/`inbox
  check` when your pane is an rs seat.

`pij generation` prints which is which. Counts today: legacy 1198 rows,
rs 252 seats.

## The three traps, in the order they will bite you

1. **`pij whoami` says "no seat in this store" — that is rs ANSWERING, not your
   seat being dead.** If you were spawned into legacy, the rs daemon correctly
   reports it has never heard of you. You are alive.
2. **`PIJ_DAEMON_GENERATION=rs pij list` silently serves LEGACY** — same 1198
   rows, exit 0, no warning. To see rs seats run `pij-rs list` (one ~150KB JSON
   line; redirect it to a file).
3. **`pij whoami` E-AMBIG while `pij adopt` refuses saying you are already
   reachable** is this split, not a registration papercut. The recovery loop is
   genuinely contradictory. Do NOT `pij adopt` your own pane "to fix it" — that
   splits your identity across both stores and makes it worse.

Reaching an rs seat from a legacy pane needs all four flags:

    pij-rs send --from <you> --to <id> --msg-id <id> --body "<path>"

`queued` means dispatched, not delivered. **Never `tmux send-keys` into a
prime's pane** — it is Jordan's composer too.

## Why o-prime cares: it silently kills conversation ingest

Both fs3's `conversation ingest --pij <seat>` and the harness's convo seam
resolve seat identity through **legacy-only** doors. An rs-resident seat
therefore resolves to nothing, the seam discards it by design, and **no error
appears anywhere** — your session simply never becomes searchable. Measured:
250 of 252 rs seats unresolvable from fs3's side; 245 of 248 from the harness's.
That is backlog row 121, and it is mine to fix.

Until it lands: if your work matters and your seat is rs-resident, say so in
your report — do not assume the index caught it.

## If you hit any of this

`harness observe` it AND tell o-prime. Four of the `ask --path` coder's
observations (two BLOCKING) were this defect seen from a seat that had no idea
two daemons existed. It cost that seat its ability to report at all — it left
its ACK plan in a file for me to find. You should never have to do that twice.
