# Brief: w-embed-oversize — oversized inputs must embed, not fail forever (Jordan ruled 2026-08-27)

**Seat**: (fill at canary — fresh seat; the enrichment/embedding pipeline becomes your
domain, inherited from sawfish, closed — roster has its history). PR-era done-bar:
own worktree + branch off main, conventional commits (`fix:`), harness checks green
(seven gates), PR, report the number, never self-merge. Read AGENTS.md first (dogfood +
observe duties bind). The production-database ruling binds you: tests NEVER touch the
default 5433 — the testkit gate will refuse you anyway; `flowspace3_test` is the
canonical FS3_TEST_DATABASE_URL target.

## The defect (live on Jordan's index right now)

59 of ~4,000 elements in a real repo fail embedding PERMANENTLY: each exceeds the
embedding model's per-input token cap and Azure answers
`400 Invalid 'input[0]': maximum input length is 8192 tokens` — retried 3x, failed,
correctly not poisoning batchmates, but the content is forever unsearchable. fs2 had
special handling for this; fs3 has NO per-input size guard anywhere in the embed path.
(The batch token BUDGET exists but governs the SUM, not any single input.)

## Ruling

**Truncate now; split later.** V1 truncates an oversized input to fit the model cap —
the tail of a huge element is low-value for search relative to never embedding it at
all. Splitting one element into multiple vectors is explicitly a FUTURE upgrade: leave
a short design note where the split would slot in, and nothing more.

## Deliverables

1. **Per-input guard in the embed path**: before an input joins a request, if it
   exceeds the model's per-input cap, truncate it to fit WITH MARGIN. Token counting:
   do not guess your mechanism blind — either a real tokenizer dep (justify its weight)
   or a conservative bytes-per-token estimate with a documented safety margin (Azure
   counts tokens, we must never exceed; undershooting by a few hundred tokens is free,
   overshooting is the bug back again). Read how the existing 300k batch budget counts
   tokens FIRST and stay consistent with it — one counting convention, not two.
2. **Cap is model-aware config, not a magic number**: per-model/per-provider cap with
   8192 as the shipped default for the current embedding models; document in
   docs/reference/configuration.md if you surface it as config (drift test will insist).
3. **Keying + honesty**: the embedding row stays keyed by the ORIGINAL source_hash
   (dedup/re-emission semantics unchanged — a truncated embedding is still THE
   embedding for that content). Record that truncation happened (the smart_content /
   extras mechanism or a marker column — your judgment, document it) so search results
   and future audits can know the vector covers a prefix.
4. **Summarize side**: check whether the summarize path has the same cliff (LLM context
   limits are larger but not infinite; a 50k-token element will hit something). If it
   does, apply the same guard shape there; if it provably cannot, say why in one
   comment. Do not leave it uninvestigated.
5. **Recovery for the already-failed**: the 59 failed jobs (attempts exhausted) must
   have a real path back: cheapest honest mechanism — e.g. `flowspace3 scan` re-emission
   picks them up because the pre-check sees no stored vector, or a targeted retry of
   failed embed jobs post-upgrade. Prove the story with a test: a job failed under the
   old behaviour succeeds after the guard exists without hand-SQL.
6. **Tests**: fake-server test asserting an oversized input arrives at the provider
   UNDER the cap (assert on the request the fake receives — the tester's
   assert-the-actual-surface lesson); truncation marker recorded; batch budget
   interaction (one huge element + several small ones in one claim set); the recovery
   test from (5). No live provider calls in CI.

## Out of scope

Element splitting into multiple vectors (design note only). Changing scanner element
boundaries (upstream fix debated separately — an element that big may be a scanner
smell, but that is a different packet).
