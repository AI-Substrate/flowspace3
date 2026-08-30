# w-parser-reindex — PARSER_VERSION bump must re-index (backlog row 61)

**Ship-blocking for 008's rollout** (ddoc support never reaches an
already-indexed corpus) and latent for every future parser improvement.
Found and measured by the 008 PM (nigel), outside its fence by correct
refusal; evidence below is its, verified.

## The defect, measured

Bumping PARSER_VERSION does NOT re-index an existing corpus. Measured:
bump to `fs3-parsers@2`, daemon restart, `flowspace3 scan .` → enqueued 0
of 351 files, nothing re-parsed. Only remove+add forced it (enqueued=351).

Mechanism: `roots.rs:197` decides what to enqueue by comparing ONLY the
stored path→blob map; `parser_version` is consulted later inside
`scan::run`'s skip (`scan.rs:142`) — which never runs, because nothing was
enqueued. The doc comment at `scan.rs:198-201` ("Bumping it re-mints every
element row and costs nothing in the content layer") is FALSE as written:
a knob that looks like an invalidation mechanism and silently is not
(tenet 17's shape).

Compounding defect (008 review): the scan skip also bypasses ddoc
enrichment entirely, so a row indexed while `ddocs` was absent stays
degraded FOREVER — "a rescan, not an install" is currently a false remedy.

## The fix

The enqueue decision treats a file parsed under a DIFFERENT parser_version
as changed: consult parser_version alongside the blob at roots.rs's
enqueue decision (the same comparison scan::run's skip already knows how
to make, one layer earlier — the mechanism exists, the trigger does not
use it).

If that shape proves wrong in the code, the fallback ruling is: delete the
false doc-comment claim and document remove+add as the required migration —
but the primary fix is strongly preferred; argue in your ack if you land
on the fallback.

## Proof requirements

- A test that fails without the fix: index a corpus at version A, bump to
  version B, scan, assert re-enqueue happens (and at version A→A, assert
  NOTHING is enqueued — the dedupe half must survive; do not trade one
  defect for a full-rescan-every-time regression).
- The 008 compose case as a second test if cheap: content indexed while a
  tool was absent must become enrichable after the tool appears + rescan
  (coordinate the exact assertion with the 008 PM — the ddoc degradation
  half may be fixed on their side; your half is the version-aware enqueue).
- Doc comment corrected to say what is now true.
- Cost note in the PR body: what a version bump now COSTS on this
  machine's corpus (rough job count) — Jordan watches queue volumes today.

## Rules (standard, hardened today)

Worktree fs3-w-parser-reindex, absolute paths always, never add your
worktree to the index, private test DB (pre-mint the base), sandbox-only
daemon testing (SIGTERM leak: drop minted DBs by hand), harness checks
green, one harness commit (never amend after), PR to o-prime
pij-instant-lynx. LIFO note: claim ordering changed today (#61) — your
tests must not assume FIFO service order.
