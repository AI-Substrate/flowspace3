# Worker brief — Azure OpenAI provider adapter · pij-sure-kazimir
**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-26 · first live run of the `add-provider` skill

## The job (one bounded task)

Build the **Azure OpenAI** adapter for fs3 — BOTH ports (Summarizer for LLM, Embedder for embeddings) — by following the repo skill **`.agents/skills/add-provider/SKILL.md`** to the letter. The skill is your packet: frozen contract, file list, steps, done checklist. Your roster row: `docs/plans/prd/providers-roster.md` (Azure OpenAI, status 🔨).

Azure specifics:
- Auth BOTH modes: api-key header AND Entra/AAD bearer token; deployment-based URL scheme + `api-version` query param.
- **fs2 is the prior art for how Azure credentials/endpoints work** — read-only at `/Users/jordanknight/substrate/fs2/flow_squared` (search its provider/config code for the Azure shapes). Prior art for the adapter pattern itself: `crates/providers/src/openai.rs`.

## Non-negotiable

The architecture rules in the skill's frozen-contract section bind absolutely: two ports only, code in `crates/providers/` only, no mocking crates, do NOT touch `fs3-core::config` or daemon wiring (snap-in recipe as a doc comment instead), arch allowlist extended only for your adapter's real deps. Deviations are stop-and-ask (message me), never improvised.

## Fence & mechanics

- Touch set: exactly the skill's "Files you create/touch" list. Scratch: `.harness/temp/w-azure/**`.
- Hard exclusions: everything else — especially `.harness/government/**`, `.claude/**`, `docs/plans/00*/**`, core/daemon/store/cli crates, any the-flow/.dd.md by hand.
- Do NOT commit — working tree only; s001 is landing commits on main, I coordinate.
- Gates before reporting: `harness checks` + `cargo test -p fs3-providers`. Keyed contract run: only if Jordan supplies credentials — ask HIM directly if you want them, otherwise state it was not run.
- Report to pij-instant-lynx: claim · files · gate outputs · keyed-run status · skill-gap observations (this is the skill's first live run — what was missing/unclear in it is a deliverable too).

Ack this brief by pij message, then go.
