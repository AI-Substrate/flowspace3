# embed-cap-heal final coder report

## Delivery

- PR: https://github.com/AI-Substrate/flowspace3/pull/92
- merged main: `7fdf6fc` — `fix: heal embedding cap rejections and align chunk windows (#92)`
- production bundle: `4788c9d` (#93 + #92)
- fold-in head reviewed before merge: `3606c1397f78564716b0d640f8bfaf59f599b402`
- closeout receipt commit: `0475655c918100a1fbf3f0dc2b91084a822e906b` on recreated remote branch `010-embed-cap-heal`; o-prime must carry/cherry-pick this docs-only commit because PR #92 was already merged

## What shipped

1. `chunk_plan` and `fit_to_cap` share the FILL-aligned byte-budget helper.
2. OpenAI, Azure OpenAI, and OpenAI-compatible embedders classify embeddings-route cap 400s as `Error::InputTooLong`; the provider-reported cap is parsed and unrelated 400s remain generic.
3. The daemon re-splits a named `input[N]`, or bisects an unnamed/invalid-index failed call until isolated.
4. One heal round reaches the one-byte/token floor. Exhaustion is terminal and names source hash, original bytes, and actual window/token numbers.
5. Overlap clamps inside `chunk_plan_bytes`, so a tuned later round such as `(468, 600)` cannot panic.
6. All vectors remain in memory until one atomic write; final per-source chunk numbers are contiguous and collision-free.
7. `docs/services/enrichment.md` describes the live contract. `impl-guide.dd.json` conforms to the current schema.

## Deterministic evidence

- provider `cap_rejection`: 7 passed across OpenAI, Azure, and OpenAI-compatible stubs
- chunk planner: 7 passed; ruled corpus unchanged at 7→10, 33→50, 1→2, total 41→62
- round-2 ratio regression: 1 passed
- oversize suite: 12 passed with `--test-threads=2`
- mutation proofs: typed heal arm removal made all 3 heal cases retryable failures; compat classifier removal failed its stub; hardcoded 8192 failed the 4096 stub; clamp removal panicked; integer ratio rendering failed with false `1 byte/token`
- `harness checks`: status `ok` with `RUST_TEST_THREADS=2`
- PR CI: green; delta cross-model review accepted the fold-ins
- final `harness plan validate`: 0 errors, 0 warnings, 0 open completable items

## AC-0001 — alignment

- `oversized` 136,000 bytes: 7→10
- `request_whale` 710,000 bytes: 33→50
- production case 20,872 bytes: 1→2
- aggregate: 41→62

The 20,872-byte production item splits by alignment alone; the production drain proves recovery, while dense-token fixtures prove the heal path.

## AC-0005 — production drain

Before bounce: embed failed=5; jobs 1314967, 1315244, 1316706, 1323215, and 1344012 were failed, attempts=3, terminal=false, with original cap-400 errors captured.

Precondition repair: jobs 1316706 and 1323215 shared the `043365…` key and would collide on `jobs_live_dedupe_idx`. O-prime terminally retired 1316706 as `duplicate-of:1323215` before bounce.

After bounce at 09:53:57 on `7fdf6fc`:

- 1314967 — done, attempts=1
- 1315244 — done, attempts=1
- 1316706 — failed, terminal=true, named duplicate residue
- 1323215 — done, attempts=1
- 1344012 — done, attempts=1
- status — embed done=494958, failed=1 named residue

AC-0005 passes.

## AC-0006 — amended real-usage proof

Original premise disproven: `conv:recovery` is a placeholder default-provider identity for cross-content recovery batches (`enrich.rs:485-496`), not a conversation job. Job 1344012 contains five non-empty document-section hashes plus the empty hash. O-prime amended AC-0006 to the actual recovered content type.

Command:

`flowspace3 search --source doc --repo all 'pij verb usage ranking' --json`

Receipt: exact-name, score-1.0 hit at:

`el:git:github.com/AI-Substrate/pij/docs/plans/112-verb-usage/report.md::pij verb usage ranking`

The snippet contains `Stores overlap, so values are never summed or averaged`; its recovered payload hash is `c74f70755adff59c9e30261e232389b537ca4614e56e180bdf86d80de161f2f4`. AC-0006 passes.

## Deviations and rulings

- Fence expanded by o-prime for `core/error.rs`, one `daemon/answer.rs` arm, `openai_compat.rs`, the enrichment service page, and impl-guide schema repair.
- PR ownership template conflict ruled in favor of the task: coder opened PR, never merged it.
- The undefined fixture corpus was ruled as the two oversize fixtures plus the 20,872-byte production case; alignment explicitly shipped.
- Duplicate failed-key repair was correctly kept outside plan 010 and applied by o-prime in production.
- Conversation-search AC premise was amended rather than falsely satisfied by an unrelated semantic hit.

## Assumptions

- Provider cap bodies retain the stable `maximum input length is ` prefix with a decimal count.
- Route=`embeddings` plus HTTP 400 remains the exact false-positive boundary.
- One input byte is the lower bound for one token, so the 7,500-byte heal window is safe against an honest 8,192-token provider cap.
- Provider adapters normalize vector order before the daemon receives it.
- `put_embeddings` remains the atomic complete-set write.
- O-prime's row-123 repair is the authoritative history for the duplicate production row.

## Harness observations — listed, not cleared

`DL-001`, `DL-002`, `CONF-001`, `DL-003`, `DL-004`, `DL-005`, `CONF-002`, `DL-006` remain in the shared buffer for o-prime drain. Highest-leverage encodings: expose the active `harness checks` stage; serialize FreshDatabase create/drop; make recovery-job provenance and search scope explicit so `conv:recovery` and cwd-scoped zero results cannot be misread.
