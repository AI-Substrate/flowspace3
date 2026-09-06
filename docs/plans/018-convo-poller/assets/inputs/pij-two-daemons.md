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

**Worse than it first looked (2026-09-02, pij o-prime's contract answer):** rs
rows carry **no inner session id at all**, so an rs seat cannot currently be
resolved to a native session by anyone, from either side. Filed upstream as pij
req-0033. If your harness is claude, `CLAUDE_CODE_SESSION_ID` is in your shell
and `flowspace3 conversation ingest --session <that> --harness claude` works
today with no pij dependency — use it.

**pi/omp seats have NO equivalent env var** (confirmed by pij o-prime,
2026-09-02) and therefore no escape hatch at all until req-0033 lands. That is
OUR fleet: our standing spawn shape is `--harness pi --bin omp`. So for us the
mitigation is not a fallback, it is knowing which store a seat landed in.

## The one check to run at canary time

Right after the canary, before you trust a seat with work:

    pij sessions --json | grep -c '"pijId":"<seat-id>"'    # 1 = legacy, ingesting
    pij-rs list | grep -c '"id":"<seat-id>"'               # 1 = rs, NOT ingesting

`pij spawn` is legacy-routed, so a spawned seat is normally fine. A seat that
booted through the **omp extension**, or that ran `pij adopt`, lands in **rs** —
and for pi/omp that means its whole conversation is lost to the index with no
error. Record the generation in the roster row. If it is rs, either respawn it
through `pij spawn` or accept the loss knowingly and say so in the row.

## If you hit any of this

`harness observe` it AND tell o-prime. Four of the `ask --path` coder's
observations (two BLOCKING) were this defect seen from a seat that had no idea
two daemons existed. It cost that seat its ability to report at all — it left
its ACK plan in a file for me to find. You should never have to do that twice.


## Reaching your seats across the split (weasel, 2026-09-02, checked against daemon source — req-0034)

**Legacy prime → rs child — WORKS:**

    pij-rs send --from pij-instant-lynx --to <child-id> --msg-id "$(uuidgen)" --body "<text or file path>"

rs does not validate `--from`; `--msg-id` is mandatory or it prints usage and
sends NOTHING (req-0032). Long bodies: write a file, send the path.

**rs child → legacy prime — NO pij path exists.** o-prime's standing choice is
**file + poll**: the child writes `.harness/temp/agent/<packet>-ack.md` /
`-report.md` / stop-and-asks in its worktree and STOPS; o-prime reads them and
replies with `pij-rs send` + numbered `-prime-reply-NNN.md`. o-prime declined
the alternative (adopting its own pane into rs for a second identity) — that
splits the prime's name across two stores and cuts off legacy peers. Workers
write files only; **never type into a prime's pane.**

`pij spawn --bin omp` ALWAYS lands the child in rs today — expect this on every
sol coder, not as a fluke. Filed upstream as pij **req-0034** (spawn must
register the child where its parent is or refuse; E-RS must name the route,
never say "adopt").

## o-prime has TWO names now (2026-09-02, after weasel cleared the gate)
- **legacy: `pij-instant-lynx`** — for legacy peers (pij-telegram, meadowlark). From o-prime's pane, legacy sends are `PIJ_DAEMON_GENERATION=legacy pij send <legacy-seat> "<text>"` (verified: Telegram received it).
- **rs: `pij-binding-magpie`** — minted by `pij-rs adopt --harness claude --parent none %21`. **Every rs-resident seat (all our sol coders and Claude reviewers) can `pij send pij-binding-magpie "<text>"` and it lands as a real turn.** The file channel stays the durable record; the rs send is the instant pointer.
- pij plan 129 (`migrate-seat`) will fold both under one id; until then, two names, one seat, one pane.
- **WARNING for o-prime's own hands:** after the rs registration, a PLAIN `pij send` from pane %21 routes to **rs** (`pij generation` says so). Every send to a legacy-only seat — `pij-telegram` (the merge-veto channel), `pij-massive-meadowlark`, `pij-forward-worm` — MUST carry `PIJ_DAEMON_GENERATION=legacy`. The merge-train incantation is updated accordingly; `pij report now` still routes legacy on its own.

## LEGACY DAEMON SHUTDOWN — 2026-09-02 (Jordan, via helper seat pij-surprising-dove)
The legacy TS daemon is being turned OFF today (CPU cost). Consequences for this fleet:
- o-prime's live name is **`pij-binding-magpie`** (rs). `pij-instant-lynx` stops resolving when legacy stops. All children: `pij send pij-binding-magpie`.
- The `PIJ_DAEMON_GENERATION=legacy pij send …` path in this doc dies with the daemon. rs refuses legacy targets; a peer that has not moved is unreachable.
- omp/pi seats: restart omp — the extension registers into rs at boot. Claude/Copilot/Codex seats: `pij-rs adopt "$TMUX_PANE" --harness <h>` — NEVER `pij adopt` (unrouted verb, served by legacy).
- **Open question, asked of pij-surprising-dove: does `pij-telegram` (the merge-veto channel) get an rs id?** Until answered, no merge train runs after the daemon stops.
- Peer rs ids (2026-09-02): meadowlark → **pij-varied-alpaca**; roadrunner (chainglass) → **pij-lonely-antelope**; forward-worm → already rs; weasel → pij-still-weasel; dove (helper) → pij-surprising-dove.

**SUPERSEDED THE SAME HOUR — HOLD (Jordan, via weasel, via dove):** the legacy daemon STAYS UP until pij plan 129 (`migrate-seat`) lands. Primes do not adopt; Telegram untouched. o-prime had already adopted (dual-present: `pij-instant-lynx` legacy + `pij-binding-magpie` rs) — ruled a 129 preservation question, not a loss; do NOT un-adopt. Practical state: merge notices keep going to `pij-telegram` over legacy (`PIJ_DAEMON_GENERATION=legacy pij send`); rs children send to `pij-binding-magpie`; legacy peers that already moved (alpaca, antelope) are reached by `pij-rs send`. The open telegram question is closed by the hold.
