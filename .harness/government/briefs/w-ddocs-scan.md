# w-ddocs-scan — first-class scanning of deterministic documents (ddocs)

**Ruled in by Jordan 2026-08-28.** flowspace3 should understand ddocs natively
when indexing a repo — semantic rows, addressable citations — not treat them
as opaque JSON/markdown.

## Domain authority

- **Read FIRST**: `ddocs agents-start-here` (baked into the dd binary — the
  maintained canonical page; if anything below disagrees, the tool is right).
- **Domain brief** (indexer-specific commentary, all 8 interview answers,
  file:line citations, measured against dd main 2e60ab9):
  `/Users/jordanknight/substrate/dd/scratch/ddocs-indexer-brief-fs3.md`
- **Living authority**: pij-mental-dajeil (dd o-prime). Ask it DIRECTLY on
  link/schema semantics rather than inferring from a corpus — its explicit
  standing offer: "I would rather answer twice."

## The four load-bearing answers (dajeil, verbatim intent)

1. **THE CHUNK IS THE ROW**, not the section, never the document. An element
   earns an id when it carries state or is a link target (`ph- tk- ac- bp-
   lg- dw- fn- fd- vd-` + four hex digits, pattern-enforced). Key chunks on
   (file, section, id). Ids are born once, reused never; **row order is not
   identity** — a reorder must not re-embed the corpus.
2. **Index the `.dd.json`, never the `.dd.md`.** The markdown is generated
   and can be arbitrarily stale with nothing in-file saying so. Skip
   predicate: the literal GENERATED banner on line 1 (constant at dd
   `src/render/renderer.ts:23`). Detection: suffix `.dd.json`
   (`src/links/scan.ts:6`). No content sniffing on either side.
3. **Cite with dd's own address**: `file#section/id` — `ddocs get` resolves
   it, humans can paste it. Do NOT write our own link resolver:
   `ddocs --json links <path>` hands inbound+outbound edges with a JSONPath
   location per edge — attach those to the chunk.
4. **THE TRAP**: a row's typed `state` and the summary DERIVED from its
   done_when assertions are separate claims dd does not reconcile — a task
   can read `checked` over an all-unchecked assertion list and validate
   raises nothing. **Believe the derived summary** (computed from rows) or
   search becomes confidently wrong about the one number everyone asks for.

## Design-for, don't code-against

- dd **PR #12 (file edges — ddoc→ordinary-file links, kind:'file') is OPEN,
  NOT MERGED**. Design the schema to receive it; dajeil will announce when
  it lands. It is the highest-value future edge: "which AC claims
  src/foo.ts" becomes answerable, and fs3 is the only holder of both halves.
- **Dajeil's ask back — the INVERSE index**: given a source file, the ddoc
  rows referencing it. dd answers forward-only; fs3 holds the code side.
  Design it in, not bolt it on.

## Repo context for the PM

- This repo's own corpus is a live fixture: `docs/plans/**/*.dd.json`,
  `.harness/government/settings.dd.json`, pij-team packet templates.
- Existing scanner/parser seams: tree-sitter parsers per language, watcher →
  enrich pipeline, per-chunk metadata already carried to pgvector rows.
- Query shapes to serve (eval-fixture candidates): "which AC covers X",
  "what state is task tk-NNNN", "what links to this plan row".
- Related backlog rows: 31 ("how is this tested" query class), 34 (bundled
  SKILL.md front door — a ddocs answer surface once ask knows rows).

## Process

Standard pipeline: PM (opus-5 medium) → impl-guide with frozen seams →
coder packets (gpt-5.6-sol-fast-1m high, worktree-per-coder, fs3-<slug>
naming per backlog row 33) → reviewer (gpt-5.6-sol high, DL-011 pinned SHA).
Templates at `.agents/skills/pij-team/templates/`; TENETS/EXPERIENCES cited
by path. PRs into main, telegram via o-prime.
