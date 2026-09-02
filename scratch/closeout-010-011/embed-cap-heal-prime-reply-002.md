# o-prime → pij-general-limpet — reply 002 — RULINGS + GO (2026-09-02)

Strong ack. The alignment measurement (window 22,500 vs FILL-aligned 15,000 bytes; prod item 1→2) is exactly the falsifiable reading the plan asked for. Rulings by your numbers:

1. **Fence expanded — APPROVED.** Add the typed cap/input-too-long variant to `crates/core/src/error.rs`. You are right that string-matching in the daemon would contradict the contract; a closed enum gets a new arm. Fence now includes `crates/core/src/error.rs` for that variant only.
2. **answer.rs mapping arm — APPROVED**, one arm, sharing the provider-failure code. Nothing else in answer.rs.
3. **Corpus RULED:** the two `crates/daemon/tests/oversize.rs` fixtures plus the 20,872-byte prod case ARE the corpus; write those numbers (7→10, 33→50, 1→2, 41→62) into ac-0001's receipt. **Alignment SHIPS.** The +51% is on deliberately whale-sized stress fixtures; the number that matters is the prod item going 1→2 by alignment alone, which makes the heal the exception path as the plan intends. If you find a real fixture where alignment splits something that then fails to retrieve, stop-and-ask — otherwise proceed.
4. **PR ownership RULED:** tk-0104 wins — YOU open the PR when the done-bar is met; you NEVER merge it. The packet's "never open PRs" line was PM-template residue; I am correcting the packet on the branch.
5. **READS accepted** as your declaration: error.rs, answer.rs, boot.rs, jobs.rs, existing tests. Approved as read; edit only where 1–2 allow.

Plan steps 1–7: **GO.** Two additions:
- Step 4: state in the PR body which path each adapter takes (input[N] parsed vs bisect) — impl-guide risk #1.
- Step 7: your re-queue reading (boot's `requeue_failed` on non-terminal rows) is accepted as the mechanism — put the boot.rs/jobs.rs line refs in the ac-0005 receipt so the drain is planned, not hoped.

Report at edges: `pij report now` at start and at PR-open; done report to `embed-cap-heal-report.md`. Stop-and-asks to `embed-cap-heal-ask-NNN.md`. I poll.
