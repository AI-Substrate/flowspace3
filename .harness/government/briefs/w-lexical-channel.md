# w-lexical-channel — plain-text search with direct hits first

**From**: pij-instant-lynx (o-prime) · 2026-08-29 · Jordan's ruling (verbatim
intent): "Search should be also plain text. It should do direct hits first as
well." Closes backlog rows 64 (+ the 21-26/CONF-003 lexical family anchor).

## The defect (measured)

A phrase existing VERBATIM in three indexed elements (SQL-confirmed against
raw_text) returns none of them; top hits unrelated at 0.31 (leopon, 006 run
eight). Pure-vector retrieval has no fallback for exact-string lookup, so an
agent searching for a symbol/error-code/identifier it just wrote gets nothing.
Compounds with row 72 (no freshness introspection): absence is undiagnosable.

## Prior art — BOTH ancestors, measured via flowspace3 ask (receipts in
o-prime transcript 2026-08-29)

- **Old flowspace (mcaps)**: dual search — TextMatcher + embedding run in
  parallel, exact text hits scored 1.0 above any cosine, merge, dedupe by
  id keeping max score, sort, top-k. Fusion by merge-and-dedupe, not RRF.
- **flow_squared (current fs2)**: deliberately NO fusion — mode-dispatched
  (`SearchMode`: TEXT/REGEX/SEMANTIC/AUTO); AUTO routes regex-metachar
  patterns to regex, else semantic, and silently FALLS BACK to TEXT when
  embeddings are absent; within the lexical channel exact node-ID = 1.0 >
  partial-ID 0.8 > context 0.6 > content 0.5. Hybrid was an explicit
  spec non-goal, parked as future extension (weighted RRF sketch in
  docs/plans/010-search/research/hybrid-search-scoring.md).

Jordan's ruling goes further than flow_squared: plain text is not a fallback
mode, it runs ALONGSIDE semantic with direct hits ranked first.

**fs2's own hybrid research** (flow_squared
docs/plans/010-search/research/hybrid-search-scoring.md, read 2026-08-30) is
the design homework already done for this packet: BM25 as the lexical
standard (Sourcegraph measured ~20% ranking improvement, with STRUCTURAL
boosting — a match on a function/identifier NAME outranks a body match);
RRF (k=60) as the proven method for combining rankings when modes mix.
Adopt both insights under Jordan's ruling: exact/identifier hits are PINNED
above the fused list (ruling wins over pure RRF), structural boosting applies
within the lexical leg (element-name match > body match), and RRF may order
the remainder. Postgres mapping: pg_trgm for substring/identifier, tsvector
ranking for term queries; do not build literal BM25 unless measurement says
ts_rank misranks the anchor fixtures.

## The job

1. **Lexical leg in the store**: Postgres text search over the already-stored
   raw text — pg_trgm (substring/identifier-friendly) and/or tsvector —
   scoped by the same repo/worktree/path/kind filters as the vector leg.
   Choose by measurement on the real corpus (identifiers like
   FS3-E-DAEMON-UNAUTHORIZED and snake_case symbols must hit); document the
   choice. Index cost measured and stated.
2. **Fusion, direct-hits-first**: run both legs; exact/lexical hits rank
   above semantic hits (old-flowspace shape: lexical wins ties and tops the
   list; keep scores honest — a lexical hit's score says WHY it ranked
   (exact-substring) rather than faking a cosine).
3. **Envelope honesty**: each hit names its channel (lexical | semantic |
   both); zero-lexical + nonzero-semantic and the reverse are both visible.
   The empty_because vocabulary stays coherent with #67's path_unmatched.
4. **The anchor regression**: leopon's case as a fixture — a verbatim phrase
   present in N elements MUST return all N ranked first. Mutation-checked
   (remove the lexical leg, fixture fails).
5. **Perf guard**: fused search stays within budget on the prod-sized corpus
   (state the measured p95 before/after); lexical leg must not run the
   whole-table scan shape the throughput review just outlawed.

## Fence

- IN: store search SQL + indexes (one migration), daemon search-path fusion,
  envelope fields, CLI rendering of channel tags, fixtures/tests, docs.
- OUT: ask-loop changes beyond consuming the new envelope; ranking ML;
  RRF tuning (merge-dedupe-lexical-first is the ruled shape); freshness
  introspection (row 72, separate).

## Rules

Worktree ../fs3-lexical-channel, branch w-lexical-channel; absolute paths;
per-seat CARGO_TARGET_DIR + test DB (post-#70 the gate minta its own);
never test against prod :7373; numbered plan-of-attack to pij-instant-lynx
before code; harness checks; harness commit; PR into main.
