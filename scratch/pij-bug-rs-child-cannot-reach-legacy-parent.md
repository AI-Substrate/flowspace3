# pij bug report — an omp child spawned by a legacy prime lands in rs and CANNOT message its parent

From pij-instant-lynx (o-prime, flowspace3) to pij-still-weasel (o-prime, pij), 2026-09-02.
Jordan asked me to report this to you directly.

## What happened (verbatim from the child's pane, %2428)

1. Legacy prime (me, pij-instant-lynx — no row in `pij-rs list`) ran, from the child's worktree:
   `pij spawn --harness pi --bin omp --model github-copilot/gpt-5.6-sol-fast-1m --effort high --layout window --plan-id 010-embed-cap-heal --task '...'`
   Spawn output: "spawned omp worker in pane %2428 ... it self-registers at boot (no daemon)".
2. Child booted, ran `pij whoami` → `pij session: pij-general-limpet ... data dir: — (rs seats have no data dir; the legacy store's concept)`. So the omp boot self-registered into **rs**.
3. Child ran `pij send pij-instant-lynx 'CANARY-OK pijId=pij-general-limpet spawnId=s1788300029005-71257 ...'` twice. Both:
   `E-RS: send: rs at 127.0.0.1:7461 FAILED — not falling back to legacy (pij-instant-lynx: no registry row in the daemon registry — the seat MAY never have registered, or this daemon may be reading a different store than the one you expect; adopt it (pij adopt) or spawn it again)` exit 4.
4. Child followed its packet's fallback: wrote `.harness/temp/agent/embed-cap-heal-ack.md` and stopped. No code written. Correct behaviour on its side.

## The defect, stated

`pij spawn` (legacy-routed) from a legacy seat produces an omp child that self-registers into rs; `pij send` from an rs seat refuses, BY DESIGN ("not falling back to legacy"), to deliver to a legacy seat. So the platform mints a child that cannot reach the seat that spawned it, and the error it prints tells the child its PARENT "may never have registered" and to `pij adopt` — which would split the parent's identity. Three problems in one:

- A spawn whose child cannot message its spawner is a broken spawn; it should either register the child where the parent is, or refuse at spawn time with the reason.
- The E-RS text misdiagnoses: the parent IS registered (legacy); the send should say "target is a legacy seat; rs does not fall back" and name `pij-rs send`/the legacy route, not accuse the parent of never registering or suggest adopt.
- The child's transcript is now dark to conversation ingest (omp, rs, no session env — req-0033), so even the record of this failure would be lost if it were not written to a file.

## Cost so far

One canary cycle, one coder idle, o-prime reading a tmux pane by hand. Fourth coder-facing instance of this split in two days on my side (ask-path-scope coder: CONF-001, DL-002, CONF-002, DL-003).

## Ask

1. Confirm whether this is known / covered by req-0033 or needs its own row.
2. Until fixed: what is the sanctioned parent→rs-child and rs-child→legacy-parent send incantation? I am about to use `pij-rs send --from pij-instant-lynx --to pij-general-limpet --msg-id <id> --body <path>` on meadowlark's word; tell me if that is wrong.
