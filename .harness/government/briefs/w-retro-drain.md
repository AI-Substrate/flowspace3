# Worker brief — retro drain + top-5 review · (seat at canary; reviewer model)

**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-28 · Jordan-ordered
("use a pij sub-agent with GPT 5.6 Sol to do the review and have it give five
top items that we should be implementing; do drain them though and make sure
they're stored as retros properly").

## The job

The fleet's observation buffers hold ~45+ frictions from two days of intense
multi-agent work (releases, a production-DB incident, the pij-team pipeline
prototype, a mass seat death, watcher/embed defects). Your job: turn the
SNAPSHOT of them into durable retro records, and rank the top five things we
should actually implement.

Input — SNAPSHOT ONLY (never touch live buffers): six files at
`/Users/jordanknight/substrate/flowspace/flowspace3/.harness/temp/retro-snapshot-2026-08-28/`
(main-session-buffer.md 601 lines is the bulk; five smaller per-worktree/rescued
files). These are append-logs of `harness observe` entries: id, kind, severity,
description, workaround, suggested encoding.

Deliverables (numbered):

1. **Retro record(s)**, properly stored: in YOUR worktree run
   `harness record retro` to scaffold into `.harness/records/retro/`, read one
   existing retro record there FIRST and match its structure exactly. Group
   observations by theme (not chronology), preserve every observation id
   (DL-*/CONF-*) so entries stay traceable, name counts honestly, and mark
   which items already have fixes landed/in-flight (e.g. DL-032 fixed by PR
   #35, DL-035/036 in-flight w-watcher-ignore) vs still open.
2. **TOP 5 items we should implement**, ranked, each with: the evidence (which
   observation ids, how many seats hit it), the cost it caused, the concrete
   encoding (a command/check/verb/template change — not "document it"), and
   where it should live (flowspace3 code, harness, pij platform, pij-team
   skill). Top-5 goes in the retro record AND as a plain section in your
   report back.
3. PR into main with the record(s), conventional commit (`docs(retro): ...`),
   gate green first (`harness checks`), DO NOT MERGE — o-prime coordinates.

## Rules & fence

- Worktree `../fs3-retro-drain`, branch `w-retro-drain` off main.
- Fence: `.harness/records/retro/**` ONLY. Do NOT edit code, briefs,
  government files, or the skill folder. Do NOT clear or edit ANY observation
  buffer — the clear is o-prime-owned and happens after your PR merges.
- Read the snapshot via the absolute path above (it is gitignored temp — it
  will not appear in your worktree).
- You are a REVIEWER-model seat: judgment and synthesis are the deliverable;
  be blunt in the ranking, and refuse to pad — if two items are the same root
  cause, merge them and say so.
- `pij send` from your worktree needs `export PIJ_SESSION_ID=<your id>`.

## Report back

claim · retro record path(s) · the top-5 list inline · PR number · anything in
the buffers you judged NOT worth recording (say what you dropped and why).
Ack via pij send to pij-instant-lynx with your read + numbered plan first.
