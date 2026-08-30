# Worker brief — watcher ignore hole + embed spend guard · (seat at canary)

**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-28 · one bounded fix packet.

## The job

Root-caused by pij-stingy-swallow (read-only investigation, full evidence with
measured counts: `.harness/temp/w-daemon-churn/FINDINGS.md` — READ IT FIRST).
Jordan's ruling: fix it (autonomy ruling 2026-08-28; o-prime dispatches).

The defects, verbatim from the findings:

1. **Watcher indexes gitignored trees** (DL-035): `roots.rs` discovery walks
   from the WORKTREE ROOT so `scratch/` (trailing-slash directory pattern) is
   correctly excluded; `watch.rs:276` walks discovery rooted at the SETTLED
   DIRECTORY — when that directory sits INSIDE an ignored tree, the pattern
   never gets a directory entry to match, so every file is accepted and
   `watch.rs:344` record_walk merges them into worktree_files. A later full
   walk reaps them (ping-pong). Measured blast: 886 gitignored files indexed,
   4,436 paid raw vectors + 222 paid summaries orphaned.
2. **Asymmetric spend guard** (DL-036): summarize is protected —
   `enrich.rs:389` raw_hash_is_referenced early-returns via
   `held_by_a_live_root!` (store/src/roots.rs:246). Embed is NOT:
   `enrich.rs:553` existing_embedding_hashes dedupes by hash only, so a NEW
   hash for UNREFERENCED content is paid. The 886-file burst paid for embeds
   only because there is no reference guard on that path.
3. **Stale doc-comment** (CONF-011): the `scan.rs` module header claims the
   content-addressed skip "enqueues no enrichment", while `scan.rs:103-108`
   deliberately re-emits enrichment (asserted by
   first_light.rs::a_scan_whose_parse_already_landed_still_enqueues_its_enrichment).
   Fix the sentence to describe the real behaviour.

Deliverables (numbered):

1. Watcher fix: the settled-directory walk must make the SAME ignore decision
   the worktree-root walk would (anchor the ignore matcher at the worktree
   root, or resolve ignore state for the path prefix before walking). A
   regression test: an fs event under `<root>/<gitignored-dir>/...` results in
   ZERO worktree_files rows for that tree (mutation-checked: break the fix,
   watch it fail).
2. Embed spend guard: filter embed batches through the same
   held-by-a-live-root predicate before the provider call, mirroring the
   summarize guard. Test proves an unreferenced item is dropped from the
   batch BEFORE the provider is invoked (fake provider counts calls).
3. The scan.rs doc-comment corrected.
4. OPTIONAL if trivial, else name-and-defer: a `--json` count of skipped
   unreferenced embed items in the enrich log line so the guard is observable.

Out of scope: GC of the existing 5,960 orphan elements / 4,436 orphan vectors
(they age out via the normal three-level GC — verify by citation, don't build),
the 3 failed embed jobs (report what they are if cheap to see, touch nothing).

## Rules & fence

- Worktree `../fs3-watcher-ignore`, branch `w-watcher-ignore` off main.
  Conventional commits (`fix:`). `harness commit`.
- Fence: `crates/daemon/src/watch.rs`, `crates/daemon/src/enrich.rs`,
  `crates/daemon/src/scan.rs`, their test files, `crates/store` ONLY if the
  predicate needs a new query helper (one, mirroring the existing macro use),
  `docs/services/watcher.md` + `docs/services/enrichment.md` updates. Nothing else.
- Gate: `harness checks` green in YOUR worktree before declaring done.
- DOGFOOD: flowspace3 search for meaning-shaped questions first.
- `harness observe` frictions; list, never clear.
- Open a PR into main when green; DO NOT MERGE — o-prime coordinates (Jordan
  gets a Telegram ping before merges).

## Report back

claim · files · gate output · the two mutation-check transcripts · PR number ·
observations. Deviations = stop-and-ask. Ack via pij send to pij-instant-lynx
with your read + numbered plan before coding.
