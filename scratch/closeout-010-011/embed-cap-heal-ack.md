# embed-cap-heal coder ACK

CANARY-OK

- pij id: `pij-general-limpet`
- spawnId: `s1788300029005-71257`
- model: `github-copilot/gpt-5.6-sol-fast-1m`
- worktree root: `/Users/jordanknight/substrate/flowspace/fs3-embed-cap-heal`
- branch: `010-embed-cap-heal`
- delivery generation: rs; replies to o-prime use files in this directory per reply 001
- code state: no code written

## Read of the acceptance criteria

1. **AC-0001 — alignment.** `fit_to_cap` applies private `FILL=(2,3)` through `budget_bytes`; `chunk_plan` currently does not. It directly computes `window_tokens * BYTES_PER_TOKEN`, so its production window is `7,500 * 3 = 22,500` bytes instead of the FILL-aligned `7,500 * 2 = 15,000` bytes. Measured over the two existing integration-test source fixtures in `crates/daemon/tests/oversize.rs`, plus the measured 20,872-byte production case: `oversized` (136,000 bytes) is 7→10 chunks; `request_whale` (710,000 bytes) is 33→50; prod 20,872 is 1→2; aggregate 41→62 (+21, +51.2%). The production item would split by alignment alone.
2. **AC-0002 — self-heal.** A daemon integration test will use a local fake embedder whose real count is denser than the shared bytes/3 estimate. It will return the typed cap error for the offending input, then accept the tighter re-plan; assertions cover multiple lossless chunks, contiguous chunk numbers, and no duplicate stored rows. The mutation receipt will remove the typed-error heal arm and demonstrate this same test failing.
3. **AC-0003 — bounded/named terminal failure.** A second fake rejects every ratio. After `MAX_HEAL_ROUNDS`, `embed_items` will return one `Failure.retryable(false)` naming source hash/item, original byte length, and final ratio. The test will execute through the runner and assert one failed job with `terminal=true`, then invoke boot recovery and prove it is not revived.
4. **AC-0004 — exact adapter classification.** OpenAI and Azure OpenAI stub tests will assert that status 400 plus the provider's `Invalid 'input[N]': maximum input length is 8192 tokens` shape returns the distinct typed error (including parsed `N` when present), while an unrelated 400 remains `Error::Provider`. Test names include `cap_rejection`, matching `cargo test -p fs3-providers cap_rejection`.
5. **AC-0005 — production drain.** The bounce is the re-queue mechanism; no repair SQL is required. `boot::recover_enrichment_jobs` calls `fs3_store::requeue_failed(db, &[SUMMARIZE, EMBED])`. Its query changes non-terminal failed rows to `pending`, resets `attempts=0` and `parks=0`, sets `not_before=now()`, and skips a key with a live duplicate (`crates/daemon/src/boot.rs:176-194`, `crates/store/src/jobs.rs:506-534`). Thus the five current cap failures are eligible on o-prime's bounce. I will provide pre/post `flowspace3 status --json` plus the five dedupe keys and final states.
6. **AC-0006 — real usage.** After o-prime bounces and the queue drains, I will select a phrase from recovered conversation `c5a6be2d`, run `flowspace3 search --source conversation <phrase> --json`, and retain the hit address in the receipt.

## Corrections / rulings needed

1. **Fence expansion is required for a genuinely distinct error.** Both adapters return `fs3_core::Result`, whose closed error enum is `crates/core/src/error.rs::Error`; today it only has generic `Provider` and `RateLimited` provider variants. The declared fence names `tokens.rs` but not `error.rs`. I request approval to add a cap/input-too-long variant in `crates/core/src/error.rs`. Otherwise AC-0004's “distinct error the caller can match” is impossible without string matching in the daemon, which contradicts the interface contract.
2. **One daemon mapping arm should accompany that variant.** `crates/daemon/src/answer.rs:98-126` currently maps only `Error::Provider` to `PROVIDER_FAILED`; its wildcard maps other variants to `STORE_QUERY_FAILED`. I request approval for the new typed cap error to share the provider-failure arm, preventing misclassification if it reaches another caller. `embed_items` catches it locally on the planned path.
3. **“Fixture corpus” is undefined in plan 010.** Repository search finds no plan-010 corpus declaration beyond the AC/task wording. The measurement above uses the two existing embedding-oversize integration fixtures and the production-sized case. Please rule whether that is the intended corpus. The +51.2% aggregate is driven by deliberately huge stress fixtures; per impl-guide risk #3, please also rule whether alignment still ships or heal-only is preferred.
4. **PR ownership conflict.** `tk-0104` says coder opens the PR; the packet scope says “Never ... open PRs — o-prime merges.” I will gate and commit only, then hand the branch/head and PR body evidence to o-prime unless ruled otherwise.
5. **READS declaration is absent.** The packet's i11 says the PM fills consumed files and owners, but no declaration was supplied. To ground this ACK I read `crates/core/src/error.rs`, `crates/daemon/src/answer.rs`, `crates/daemon/src/boot.rs`, `crates/store/src/jobs.rs`, and existing tests; I will not edit any of those outside the explicit amendment requested above.

## Numbered implementation plan

1. Apply the ruled scope/corpus decision. Keep all edits inside the amended fence and use absolute paths. Before any exported-symbol modification, run LSP references; use the existing `fs3_core::Error` and daemon `Failure` conventions rather than introducing an adapter-only error stack.
2. Complete **tk-0101** in `crates/core/src/tokens.rs` and `crates/daemon/src/enrich.rs`: expose one core byte-budget helper that owns `FILL`, make both `fit_to_cap` and `chunk_plan` call it, preserve UTF-8/overlap behavior, and add a `chunk_plan` measurement test printing the ruled corpus's before/after counts and production-case result.
3. Complete **tk-0102** in both provider adapters: recognize only HTTP 400 cap-rejection bodies, parse optional `input[N]`, construct the shared typed error, and leave all other status/body paths unchanged. Add OpenAI and Azure stub tests for positive and negative 400 classification; run `cargo test -p fs3-providers cap_rejection`.
4. Complete **tk-0103** in `embed_items`: retain each prepared chunk's source identity/original bytes and current ratio; on the typed rejection, select the named input when available, otherwise bisect the rejected call, regenerate the affected original source at a tighter ratio, and re-budget/re-issue without writing rows early. Bound with `MAX_HEAL_ROUNDS`; on exhaustion return a non-retryable named failure. Preserve the existing single `put_embeddings` transaction so failed attempts cannot create duplicate/partial chunk rows.
5. Add daemon integration tests defending the observable contracts: dense-count rejection heals to N>1 stored chunks; removal of the heal arm makes that test red; always-reject exhaustion records item/bytes/ratio and `terminal=true`; boot recovery leaves it failed; stored chunk numbers are contiguous and unique. Run the exact bp-0002/bp-0003 tests plus `cargo test -p fs3-daemon chunk_plan -- --nocapture`.
6. Run targeted diagnostics/tests, then `harness checks` in this worktree. Confirm tree-resident evidence, create conventional commits via `harness commit`, and prepare the mutation statement, measurements, assumptions, and branch/head handoff for o-prime. Do not open or merge a PR unless reply 002 changes the packet ruling.
7. With o-prime after deploy: capture pre-bounce status and five keys, let boot's automatic non-terminal requeue run on bounce, read back all final states/status, prove one recovered conversation hit, and persist the final done report at `.harness/temp/agent/embed-cap-heal-report.md`.

## Environment evidence

- `harness boot --json`: build passed; overall degraded only because compose service `db` is not running. No code was changed.
- Rust LSP: configured and operational; symbol/definition/reference requests succeeded.
- flowspace3 dogfood: `agents-start-here`, `docs list`, and meaning-shaped searches succeeded against this worktree.
- rs→legacy `pij send` failure captured as harness observation `DL-001`; o-prime channel is the agreed file path.

STOPPED pending `.harness/temp/agent/embed-cap-heal-prime-reply-002.md`.
