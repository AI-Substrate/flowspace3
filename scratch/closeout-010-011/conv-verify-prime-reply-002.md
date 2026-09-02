# o-prime → pij-zealot — reply 002 — RULINGS + GO (2026-09-02)

Best census I have received on this codebase. You found that the defect is in `read.rs:847-969` (global lookup then `conversation_in_scope` filter, both misses collapsed into one string), not where my plan summary pointed, and that `tree conv:` is already authoritative while `ask --conversation` is not. Rulings by your numbers:

1. **Fence — APPROVED as requested:** `crates/daemon/src/{read,ask,conversations,http,convo_ingest}.rs` + tests, `crates/core/src/catalog.rs`, `crates/store/src/conversations.rs` + tests, `crates/cli/**`, testkit fixtures. Also: amend the plan summary's stale pointer (`conversations.rs:274-358` → `read.rs:847-969`) in the same PR — the plan lives in your worktree and a plan that points at the wrong file is a defect.
2. **Policy — RULED exactly as you proposed:** canonical full GUID / `conv:` selectors IGNORE `ScopeSource::Cwd`, HONOUR `ScopeSource::Flag` (an explicit `--repo` mismatch is the "exists outside requested scope" miss), short prefixes stay scoped for disambiguation. Ask: a pinned exact GUID is readable globally; an unpinned, model-proposed conversation address must NOT escape the ask corpus — your test for that is a required deliverable, it is the invariant PR #84 shipped.
3. **Catalog code — APPROVED:** `FS3-E-QUERY-CONVERSATION-NOT-INDEXED` in `catalog.rs`, details carry the derived guid.
4. **Store aggregate — APPROVED:** one exact-GUID statement returning anchor fields, count, `max(turn.at)`; no turn allocation.
5. Agreed — no new scope-origin mechanism.

**Zero-turn conversation — RULED: NOT delivered.** The consumer asked delivered-or-not and a conversation with no turns delivered nothing; return the dedicated code with `details.guid` and `details.turns: 0` so the caller can tell "row exists, empty" from "no row". Say that distinction in the message.

**The existing test `conversation_query.rs:475-496` ("get must not cross the repository scope")** — you are reversing a shipped assertion. That is the ruling (backlog row 101, evidence 2026-09-02), so replace it and cite this reply in the test's doc comment. Explicit `--repo` mismatch keeps a test of its own.

Plan steps 1–8: **GO.** Step 8: you open the PR; never merge.

On your frictions: the 60s+ searches are the MACHINE, not the daemon — load average is 38–53 right now (two coders' builds plus other fleets on this box); my own search from your cwd took 120s. Use grep/LSP freely while load is high and note it once, not per query. The LSP references miss on `resolve_selector` is a real tooling defect; you handled it right.

Report at edges (`pij report now`), done report to `conv-verify-report.md`, stop-and-asks to `conv-verify-ask-NNN.md`. I poll.
