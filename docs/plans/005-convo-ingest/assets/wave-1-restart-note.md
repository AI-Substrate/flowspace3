# Wave-1 restart note — read this BEFORE your packet

Written 2026-08-28 by **pij-pale-silkworm**, PM3 for plan 005-convo-ingest.
It applies to the three re-dispatched seats u1a, u1b and u1d. It is short on
purpose: everything else you need is in your packet, which is unchanged and
still binding.

## 1. Your PM is not the one your packet names

Your packet says to ack and report to **pij-traditional-piranha**. That seat is
dead. Every `pij send` in your packet — ack, stop-and-ask, done report — comes
to **pij-pale-silkworm** instead. Nothing else in the packet changes.

Chain of custody, so you can trust the packet's rulings: PM1 (narwhal) froze
most of phase 1, PM2 (piranha) closed and committed it and wrote your packets,
I am PM3. The rulings recorded IN the packets are in force; I have re-affirmed
them and will not re-litigate them with you.

## 2. Why you exist: the machine ran out of disk, and it killed your predecessor

At 2026-08-28T01:04–01:07Z the machine's disk hit 100% and the sweep that
followed killed the PM seat and three of the four coder seats — including
yours. This was not a work failure and it says nothing about your unit.

**Your predecessor left NOTHING.** I inventoried your worktree with git before
re-dispatching you: zero commits past the fan-out commit `460883d`, a clean
tree, and not one file touched since dispatch. There is no half-written state
to inherit, no partial file to treat with suspicion, no delta to reconcile.
Start from your packet, clean. (An earlier handover note claimed u1b and u1d
had partial work surviving; that was wrong, prime has accepted the correction,
and this note is the record.)

## 3. Disk: the failure signature you must recognise

If a build dies with

```
rustc-LLVM ERROR: IO failure on output stream
```

or any `No space left on device`, **that is the disk, not your code.** Do not
debug it as a compile error and do not start deleting things to make room.
Stop and tell me; reclaiming space outside your own worktree is prime's call
and prime is actively doing it (~40G+ headroom expected).

Two standing measures:

- **Build with `CARGO_INCREMENTAL=0`** — incremental artefacts are the bulk of
  a coder `target/` and no seat here benefits from them.
- **Report free disk in every status update to me** (`df -h /System/Volumes/Data`,
  the Avail column). Cheap, and it is the sensor that was missing last time.

A shared `CARGO_TARGET_DIR` was considered and **rejected by prime**: four
coders would serialise on the target lock, which defeats the fan-out. Your
`target/` is your own.

## 4. Your worktree already exists — do not create it

Your packet's scope says to create your worktree yourself. It is already there,
on the right branch, at the fan-out commit:

| unit | worktree | branch | scratch db |
| --- | --- | --- | --- |
| u1a | `../fs3-convo-u1a` | `005-convo-u1a` | `fs3_convo_u1a` |
| u1b | `../fs3-convo-u1b` | `005-convo-u1b` | `fs3_convo_u1b` |
| u1d | `../fs3-convo-u1d` | `005-convo-u1d` | `fs3_convo_u1d` |

`cd` into yours and work there. `git worktree add` will fail; that is expected,
not a problem to solve.

## 5. Four things that will bite you, already paid for by someone else

1. **`export PIJ_SESSION_ID=<your pij id>` in every shell you send from.**
   From a worktree pij cannot infer your seat and your message is silently
   lost. Get the id from `pij whoami` — which also answers "what was my spawn
   task"; a previous seat burned 302 seconds grepping `~/.pij` for that.
2. **NEVER `docker compose up`.** Postgres is already up for the whole fleet as
   `flowspace3-db` on `127.0.0.1:5433`; `container_name` is pinned, so a second
   `up` can take the fleet's database down. Use your scratch db above via
   `FS3_TEST_DATABASE_URL`.
3. **`harness boot --json` will report `degraded` / `service "db" is not
   running`** in your worktree even though the database is healthy. Boot is
   looking for a compose service in a worktree that never ran compose. This is
   a known false negative — see item 2, and do not "fix" it.
4. **`pij` may mint phantom seat ids** under parallel subprocesses (defect
   pij#19). If you see a stray ready-ping or tombstone that is not you, ignore
   it. I confirm seat identity by asking the seat, never by assuming.

## 6. What I want from you, in order

1. **CANARY** — `pij send pij-pale-silkworm` with: your pij id, your `pij whoami`
   folder, the unit you believe you are (u1a / u1b / u1d), and CANARY-OK. If the
   unit in your spawn task differs from what I said, contradict me — pij#19 is
   live and I verify identity rather than assume it.
2. **Read** your packet at `docs/plans/005-convo-ingest/packet-coder-u1<x>.dd.md`,
   the impl-guide (BINDING — especially the AMENDED / RULING paragraphs in its
   architecture section, which correct the recipe with measurements), and the
   frozen contract at `crates/core/src/conversation_source.rs`.
3. **ACK with a NUMBERED plan** before any code. I rule it by number.
   Corrections to your packet are welcome and expected — bring evidence; your
   measurement outranks my prose. The last seat to do this got two packet
   defects overturned in its favour.
4. Then code, per your packet.

## 7. The contract is frozen — filling it is your job, changing it is not

`fs3_core::conversation_source` was frozen and committed at `a3bbfd2` before
any of you were dispatched. A method it does not have, a field you wish
existed, a signature you would rather have: **stop and ask me.** Three other
units compile against the same shape, so changing it is not a refactor, it is a
re-plan. After the freeze a contract change is a defect, and it is mine and
prime's to rule, not yours to work around locally.

Line framing is **already built and tested** in
`crates/providers/src/conversation_sources/tail.rs`. Do not write a fourth
copy — spend your unit on your dialect.

## 8. Capture friction the moment it bites

```bash
harness observe "<what happened>" --kind difficulty|confusion \
  --workaround "<what you did>" --suggested-encoding "<how to fix it for the next agent>"
```

**LIST, never CLEAR** — the buffer is shared across the whole fleet and
clearing destroys your siblings' observations. Tell me anything surprising
while you are still in context, not at the end. Half this note is made of
frictions previous seats reported; that is the mechanism working.
