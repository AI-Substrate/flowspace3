# Phase 1 execution log — fs3-foundations

Orchestrator-maintained (worker packet excluded `docs/plans/**`; PM s001 owns this file).
Run: flow-pair `2026-08-26T02-07-44Z-github.com-AI-Substr` · delegation dlg-0002 · commit `b812d4d`.

## Task outcomes (tk-0001..tk-000e)

| task | outcome | evidence |
|---|---|---|
| tk-0001 workspace scaffold | DONE | root Cargo.toml + 7 crates; `cargo build --workspace` green |
| tk-0002 core types + classify | DONE | 17 unit tests, zero doubles in core; declaration-shape gate proven by reviewer Dim-0 mutation (RED→restore→GREEN, sha 66369848…) |
| tk-0003 port traits | DONE | `#[async_trait]`, dyn-safe; doc-tests compile `Arc<dyn Embedder/Summarizer>` |
| tk-0004 testkit fakes + contract | DONE | embedder_contract green incl. determinism across instances and dyn seam (review findings rev-0001 #7 on slot coverage → FIX) |
| tk-0005 providers stub | DONE | compiles; real contracts `#[ignore]`d (2 ignored by design) (review finding: Debug leaks api_key → FIX) |
| tk-0006 parsers rust+md | DONE | fixture tables exact incl. fenced-code trap (review finding: `<anonymous>` elements, setext corruption → FIX) |
| tk-0007 compose stack | DONE | compose up + pg_isready verified; README documents up/down (review finding: quick-start lacks --wait → FIX) |
| tk-0008 store migration 0001 | DONE | round-trip + upsert idempotent + pgvector probe + CHECK constraint; absent-PG fails ~6s naming compose cmd |
| tk-0009 config | DONE | fake provider selection proved via determinism; FS3_CONFIG_DIR override |
| tk-000a daemon | DONE | GET /health 200 {status:ok,embedder:fake}; lazy pool serves without DB (review finding: main.rs bypassed by test, bind not loopback-checked → FIX) |
| tk-000b cli ping | DONE | real binary vs real daemon: "healthy - fs3 daemon 0.1.0 …"; stopped daemon exits 1 naming doctor |
| tk-000c drift check | DONE | arch-check green (7 crates, 50 edges); RED proven via drifted-metadata.json tests + one hand-run live-graph violation |
| tk-000d harness extensions | DONE | checks = fmt/clippy/test/arch 4/4; boot = 5 stages ok; compose stage negatively verified (db stopped → degraded) (review finding: docs-link check missing → FIX) |
| tk-000e docs | DONE | README quick-start followed end-to-end; links docs/how/architecture.md |

## Gate receipts (PM-run, 2026-08-26T02:40Z)

- `harness checks` → status ok, 4/4 gates (fmt, clippy -D warnings, cargo test --all, arch)
- `harness boot --json` → ok; toolchain/crate/build/compose/checks all true
- `cargo test -p fs3-testkit --test arch_drift` → 5 passed (drift RED cases included)
- `cargo tree -p fs3-core` → only async-trait, serde, thiserror, toml (purity holds)
- Reviewer independent verification: build/test/boot/checks re-run clean; real daemon + real ping smoke healthy

## Decisions recorded (coder, none contradicting workshop 001)

1. ProviderConfig = Fake | OpenAi only (local/ort deferred; no not-implemented stub arm).
2. `Element.text` added so Summarizer signature stays verbatim.
3. Drift checker lives in fs3-testkit with `[[bin]]` (no xtask crate; negative proof is a normal cargo test).
4. sqlx runtime queries (no offline cache needed); `macros` feature only for migrate!.
5. Lazy PgPool so /health answers without PG; ping distinguishes daemon-down vs db-down.
6. acquire_timeout 5s (fast stopped-stack verdict).

## Review trail

- rev-0001 (gpt-5.6-sol high): **FIX_REQUIRED** — 1 critical (this log's absence), 8 high, 4 medium. Artifact `.harness/temp/s001/dlg-0002-review.json`; Dim-0 empirical mutation on core/src/classify.rs RED→GREEN verified.
- Critical resolved by this file (orchestrator-owned). Code findings → fix delegation.

## fix-0002 (coder, 2026-08-26) — rev-0002 findings addressed

Appended, not rewritten: the sections above are the orchestrator's record.
Corrections to them are dated here rather than edited in place.

### Corrections to the record above

- 2026-08-26 — tk-0007 row: the quick start now runs `docker compose up -d
  --wait`, so the readiness race is closed. Verified from a genuinely clean
  start (`docker compose down` first): `--wait` returned only on `Container
  flowspace3-db Healthy`, 5.9s.
- 2026-08-26 — tk-000d row: `harness checks` is now **5** gates, not 4. The
  docs-link gate runs first, because it needs no compiler.
- 2026-08-26 — tk-000a row: the daemon's HTTP surface is now loopback-enforced
  at startup, and the integration tier exercises the real binaries.
- 2026-08-26 — "Gate receipts (PM-run)": superseded by the receipts below.

### Findings and what changed

| Finding | Change | Why it satisfies the finding |
| --- | --- | --- |
| HIGH `Debug` leaked the API key (`providers/src/openai.rs`) | A `Secret(String)` newtype with a hand-written `Debug` printing `Secret(<redacted>)`; `OpenAiClient.api_key` is now `Secret`, readable only via `expose()`. | The redaction lives in the *type*, so all three structs keep `#[derive(Debug)]` and a field added later cannot re-open the leak. `debug_never_prints_the_api_key` asserts the key is absent from `{:?}` and `{:#?}` of client, embedder and summarizer, and that "redacted" is present. |
| HIGH `bind_address` accepted `0.0.0.0` (`daemon/src/main.rs`) | Parse the authority (bracketed IPv6 included) and `ensure!` the host is loopback; unknown names are refused, not resolved. | PRD req 17 / AC-0005 is now a startup failure rather than a silent exposure. `bind_address_refuses_every_non_loopback_host` covers `0.0.0.0`, `[::]`, an RFC1918 address and a public name; `bind_address_accepts_every_loopback_spelling` keeps `127.0.0.2`, `[::1]` and `LocalHost` working. |
| HIGH AC-0005 proof bypassed `main.rs` (`daemon/tests/health.rs`) | Added `the_real_binaries_agree_through_a_discovered_config`: writes a config with a random port into a temp `FS3_CONFIG_DIR`, spawns the real `fs3-daemon` binary, polls `/health` until it answers, then runs the real `flowspace3 ping` with **no `--daemon-url`**. | The port matches only if both shipped binaries discovered and honoured the same file. Mutating main to `Config::default()` now fails the suite; before, nothing noticed. Readiness is observed, and a daemon that exits early is reported instead of waited on. |
| HIGH no docs-link step (bp-0008) | `harness checks` gained a first, in-process `docs` gate: the README-to-`docs/how/architecture.md` signpost must exist and be linked, and every relative markdown link in `README.md` and `docs/how/*.md` must resolve (fenced samples excluded). | Deleting the guide or breaking the link is now red. Both failure modes were mutation-verified against `harness checks`. |
| HIGH `FakeEmbedder` carried no shared signal (`testkit/src/fakes.rs`) | Replaced whole-string-per-dimension hashing with signed token feature hashing (`FAKE_DIMENSIONS` 8 to 32); an all-zero vector is impossible by construction. | Shared tokens now produce shared components, so `related_text_ranks_above_unrelated_text` asserts related outranks unrelated (and exceeds 0.5), and `a_shared_token_is_the_unit_of_similarity` asserts word order does not destroy it. The vacuous "different text differs" assertion is no longer the only claim. |
| HIGH embedder contract checked only slot 0 (`testkit/src/contract.rs`) | Every slot is compared against an independently obtained single-item embedding, plus a reversed-batch assertion that output order follows the request. | An implementation swapping slots 1 and 2 now fails the contract; it passed before. Verified by mutating `FakeEmbedder::embed` to swap exactly those slots. |
| HIGH nameless / empty-named elements (`parsers/src/source.rs`) | Two guards: `walk` skips a classified node with no name while still walking its children, and `first_identifier_text` returns `None` for blank text. | The second guard was the real defect. `impl<T> {` yields an `impl_item` whose `type` field is a **zero-width MISSING node**, so the name arrived as `Some("")`, not `None`, and the name guard alone did not catch it. The fixture is that witness. `no_element_is_nameless_or_anonymous` rejects both shapes; `children_of_a_nameless_node_are_still_found_and_cleanly_named` proves a skipped node leaves no gap in the qualified names beneath it. |
| MEDIUM duplicate embedding index accepted (`providers/src/openai.rs`) | Extracted a pure `order_embeddings`; slots are `Option`, and duplicate, out-of-range or missing indexes are errors. | A duplicate used to overwrite one slot and return the other as an empty vector under `Ok`. Four pure boundary tests, no HTTP. |
| MEDIUM blank summary text or tags returned as `Ok` (`providers/src/openai.rs`) | Extracted a pure `parse_summary`: trim the text and reject it when blank, drop blank tags, enforce the 1-5 band, then validate the whole `Summary`. | Nothing leaves the boundary that the shared contract harness would reject. `a_repaired_summary_satisfies_the_shared_contract` asserts precisely the harness's own properties. |
| MEDIUM `trim_end_matches('#')` corrupted `C#` (`parsers/src/markdown.rs`) | The title now comes from the grammar's `inline` node; ATX closing sequences are stripped per CommonMark (a `#` run preceded by a space, or the entire content). Setext headings never reach that path. | `C#` survives as both setext and ATX heading, while `## Title ##` still loses its closing run. Five tests, including the fenced-block case. |
| MEDIUM quick start raced Postgres readiness (`README.md`) | `docker compose up -d --wait`. | Matches dw-0007's readiness step; verified from a clean start. |
| CRITICAL missing execution log | Orchestrator-owned; this section appends to it. | - |

### Gate receipts (coder, 2026-08-26T03:20Z)

- `harness checks` → status ok, **5/5** gates: docs (4 links checked), fmt,
  clippy `-D warnings`, `cargo test --all`, arch drift.
- `harness boot` → ok, 5 stages.
- Mutation gate → **12/12 killed**, at least one per guard touched. Script with
  the exact before/after anchors: `/tmp/fs3-mutations.py`.

### Decisions added by this fix

7. A `Secret` newtype rather than a hand-written `Debug` per struct: the leak is
   closed at the type, so it cannot be reopened by adding a field.
8. Provider response handling split into pure functions (`order_embeddings`,
   `parse_summary`), so the boundary is testable without HTTP and without a mock.
9. `is_loopback` refuses unknown hostnames instead of resolving them: a name that
   points at loopback today is not a local-only guarantee.
10. The `docs` gate is in-process TypeScript rather than a shelled-out script.
    The packet's allowed scope has no room for a new script file, and the check
    is a dozen lines of `existsSync`.
11. The daemon integration test locates `flowspace3` from the target directory
    rather than `CARGO_BIN_EXE_*` (which only covers the current package), and
    fails loudly naming `cargo build --workspace` when it is absent.
