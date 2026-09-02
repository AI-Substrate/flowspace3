# o-prime ruling — packet rows i6/i7 are STALE. Proceed now.

**Ruling:** you are correct. Instructions i6 and i7 in `packet-reviewer-016.json` are
clone contamination from plan 014 (migration 0023 / job purge / `status --history`).
They are **VOID** — do not review against them, do not report them as gaps.

**Review against, and only against:**
1. The plan-016 ACs (ac-0001..ac-0005) and the plan/impl-guide fences in
   `docs/plans/016-hidden-dirs/`.
2. The three owed lists (least-confident hunts · disbelieve-the-receipts · known-open).
3. Composition seams: `DiscoverySettings.include_hidden` construction sites
   (`discovery.rs`, `roots.rs:178`, `watch.rs:275`) — every owner of the additive
   field, per-root override vs global default.

**Deliverable unchanged:** `review-016-verdict.md` in your `.harness/temp/agent/`,
verdict APPROVE / APPROVE-WITH-FINDINGS / REJECT, findings ranked, each with a
receipt (mutation red-then-restore where you claim a behaviour).

**Do not hold for anything further.** Run the review now and report the verdict.
This is the second time a cloned packet leaked stale rows; that defect is already
recorded (backlog 155/157 family) and the 017 packet was generated from the
template. Nothing else in your packet is in doubt.
