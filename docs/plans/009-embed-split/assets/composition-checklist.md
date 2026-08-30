# Composition checklist — 009-embed-split

PM-owned (pij-associated-owl). Written BEFORE the units land, deliberately: a
checklist authored after the fact only records what you happened to remember.
Merge order is `u1 -> u2 -> u3` onto `009-embed-split`, re-gating after each.

## Why this file exists

Tenet 15: composition is where emergent defects appear, because one unit's
success can create the very population that exposes a sibling's gap. This plan
has that shape by construction — u2's chunking creates the N-vectors-per-element
population that u3's collapse exists to handle, and neither unit's own suite can
see the interaction. Tenet 17: a capability without a proven trigger is
unshipped. Two items below (C3, C4) are wiring that no coder owns and that would
otherwise ship unproven.

## Before merging anything

- [ ] `git status --short` on the PM worktree is clean. An unexplained
      modification while coders run is a breach, not noise: stop, attribute,
      patch across, revert. (PM packet i12.)
- [ ] Baseline `harness checks` on `009-embed-split` with no unit merged is
      recorded, so a later red is attributable to a unit rather than inherited.
- [ ] Each done coder has been asked EXPLICITLY what it assumed about the
      composition it does not own. A live seat volunteering its assumptions
      beats discovering them while wiring. (PM packet i5.)

## C1 — u1 lands first

- [ ] Merge `u1-store`. Re-gate.
- [ ] Confirm migration ordinal is still free against `origin/main` (0021 at
      authoring time; `0020_one_file_root_per_blob.sql` already exists on this
      branch, and the claim-index packet has renumbered once before).
- [ ] Confirm the `truncated` column still exists and still carries its data.
      Its write path dies in u2; the column is the deferred-backfill inventory
      the plan's non-goals preserve. Dropping it is a silent scope breach.
- [ ] ATOMICITY, the question u1's own tests cannot answer: can a multi-chunk
      write be observed half-finished? If yes, `existing_embedding_hashes`
      (crates/store/src/embeddings.rs:120-146) reports a partially-embedded
      hash as done and the tail is lost forever with no error anywhere. Read
      u1's answer in its done report, then verify it in the merged code.

## C2 — u2 lands second

- [ ] Tell u2 to merge `origin/009-embed-split` forward into `u2-enrich` first,
      so it gates against the real `NewEmbedding.chunk_no` rather than a stub.
      This is the named u2->u1 build dependency; it is PM-owned by design.
- [ ] Merge `u2-enrich`. Re-gate.
- [ ] EMPTY MUST MEAN THE SAME THING ON BOTH SIDES: u1's heal predicate and
      u2's mint filter must agree (trim-then-is-empty). If the heal retires
      rows the filter would still mint, the poison returns on the next ingest
      and the receipt count looks like success. Read both predicates side by
      side in the merged tree — this is a cross-unit defect with no owner.
- [ ] Confirm `EmbedJob.items` is still `Vec<(String,String)>` and the dedupe
      key at crates/daemon/src/enrich.rs:107-112 is unchanged. Chunks are
      inputs within one job, never N jobs — `ON CONFLICT (dedupe_key) SET
      payload = EXCLUDED.payload` means N chunk jobs would make siblings
      replace each other's payloads.
- [ ] Confirm batch.rs `TOKEN_BUDGET` / `split_to_budget` / `SOLO_FROM_ATTEMPT`
      are untouched (frozen seam).
- [ ] Re-run u2's two mutation checks MYSELF on the merged tree. A mutation
      check that only ever ran in its author's worktree proves that worktree.

## C3 — WIRE THE HEAL (PM-authored; nobody else's fence)

u1 ships `retire_empty_embed_jobs` plus a snap-in recipe as a doc comment. The
call site is `crates/daemon/src/boot.rs:462-477`, immediately BEFORE
`requeue_failed` — order matters: retiring after the sweep leaves the poison
revived for a full cycle.

- [ ] Paste the recipe verbatim; do not re-derive it.
- [ ] Emit the count at INFO. A heal with no receipt cannot be audited later.
- [ ] PROVE THE TRIGGER, not the mechanism: boot the daemon against a database
      seeded with a poison job and assert the job is still failed, still
      terminal, and was never re-queued. u1's unit test proves the function;
      only this proves that anything calls it. This is the exact shape of the
      four defects tenet 17 was written from.

## C4 — u3 lands third, then the composed proof

- [ ] Tell u3 to merge `origin/009-embed-split` forward first, so its fixtures
      run against real chunked data rather than hand-seeded rows.
- [ ] Merge `u3-read`. Re-gate.
- [ ] `git diff --stat` names NO `crates/daemon/src/search.rs`. If the collapse
      drifted into `fuse`, lexical-channel (#74) fusion changed silently.
- [ ] Confirm the nearest CTE still carries every original admission predicate:
      `model_key`, the source_kind filter, the distance threshold, the EXISTS
      clause. A rewrite that drops a scope guard is a leak the tests may not
      see (tenet 16 — check what the CALLER received).
- [ ] Run the ac-0006 live isolated-daemon fixture MYSELF: ephemeral port,
      disposable database, torn down after. NEVER prod :7373.
- [ ] Adversarial run beyond the fixtures: search the composed daemon for
      content that exists in both a whale turn's tail and a small element, and
      confirm the small element is still reachable. The crowd-out leg proves k
      elements come back; this asks whether the RIGHT ones do.

## C5 — before the PR

- [ ] Re-check the migration ordinal against `origin/main` one final time and
      renumber cleanly if taken (precedent: claim-index 0018 -> 0019).
- [ ] Every AC checked in `plan.dd.json` with evidence, ac-0007's post-merge
      observation leg explicitly handed to o-prime/krill rather than claimed.
- [ ] Backlog row 92 (pre-existing raw+smart duplicate hits): state whether the
      collapse closed it. An observation, not a claim of scope.
- [ ] Reviewer verdict recorded; every finding fixed or refuted with evidence.
- [ ] ONE PR into main. I do not merge it.
