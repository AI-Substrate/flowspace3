# Bug: pij records a `harness_session` id that no omp session file has

**Found by** pij-binding-magpie (flowspace3 o-prime), 2026-09-04 ~22:35Z
**Seat affected** `pij-grim-gurgeh` (omp, pane %2302, folder `/Users/jordanknight/pi-hacking/pij`)
**Severity** silent — nothing errors; a consumer just gets nothing, forever.

## What pij recorded

```
sqlite3 ~/.pij-rs/pij.sqlite \
  "select id,harness,pid,proc_start,harness_session from seats where id like '%gurgeh%';"

id              = pij-grim-gurgeh
harness         = omp
pid             = 7107
proc_start      = 20260905082842      -- 2026-09-04T22:28:42Z
harness_session = 01a06e8c-696a-7000-a209-a02cb4d884c0
```

## What is actually on disk

**No file with uuid `01a06e8c-…` exists anywhere in the omp store.**

```
$ find ~/.omp/agent/sessions -name '*01a06e8c*'
(nothing)
```

The seat's real, live session is a DIFFERENT uuid:

```
$ ls -lt ~/.omp/agent/sessions/-pi-hacking-pij/ | head -2
-rw-r--r--  862376  5 Sep 08:35  2026-09-04T22-28-43-926Z_01a06e89-dd16-7000-8256-fa53ca812629.jsonl
```

Started `22:28:43.926Z` — **one second after** grim-gurgeh's `proc_start` of
`22:28:42`. That is the session, and it is 862KB and still growing.

The two ids differ only near the end: `…e8c-696a…` (recorded) vs `…e89-dd16…`
(real). One apart in the timestamp-ordered prefix — consistent with an id
**minted at spawn time** rather than read back from the session omp actually
opened. Same failure class as req-0046 (pre-minted seat ids that never bound).

## Why it matters downstream

flowspace3 indexes agent conversations as first-class content. The lookup is by
session id, and `harness_session` is the obvious field to drive it from. Using
what pij recorded:

```
$ flowspace3 conversation verify --harness omp --session 01a06e8c-696a-7000-a209-a02cb4d884c0
FS3-E-QUERY-CONVERSATION-NOT-FOUND: conversation 00a9d599-… is not indexed
```

That error is **misleading in a costly way**: it reads as "not indexed yet, go
ingest it", when the truth is "this session does not exist". An operator (or an
automated poller) reading `harness_session` would ingest nothing, be told it
simply hadn't been ingested, and never learn the id was wrong.

Using the real uuid instead works immediately:

```
$ flowspace3 conversation ingest --harness omp --session 01a06e89-dd16-7000-8256-fa53ca812629
$ flowspace3 conversation verify --harness omp --session 01a06e89-dd16-7000-8256-fa53ca812629
turns = 110, last_turn_at = 2026-09-04T22:35:14Z
address = conv:97cab781-f11f-8068-a461-09443b3f014d
```

grim-gurgeh's transcript is now indexed and searchable. I got there by matching
`proc_start` against file mtimes **by hand** — there is no supported way to do
that, which is the part that should not stay true.

## What I am NOT claiming

- I have not checked whether this affects claude-harness seats too, or whether
  it is omp-specific. `pij-minor-unicorn`'s own claude session id (`cbe08128-…`)
  DID match a real file, so the claude path looked correct in that one case.
- I have not read pij's spawn/bind code. The "minted at spawn" reading is
  inferred from the id shape and the one-second offset, not from source.
- I do not know whether the recorded id is meaningful in some other namespace
  and simply is not the omp session filename. If so, the bug is that nothing
  says which namespace it is in.

## Suggested shape of a fix (yours to judge)

Read the session id back from the harness after it opens, rather than recording
what was minted before. Failing that, make the field's namespace explicit, so a
consumer knows it is not an omp session filename and does not go looking for one.

## Repro

1. Spawn an omp seat via pij.
2. `select harness_session from seats where id = '<seat>'`.
3. `find ~/.omp/agent/sessions -name "*<that uuid>*"` → nothing.
4. `ls -lt ~/.omp/agent/sessions/<slug>/` → the real file, ~1s after `proc_start`.
