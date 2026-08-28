# Integration proof — plan 008-ddocs-scan

Run by the PM on the composed branch, against **this repo's own ddoc corpus**, using the
**branch-built binary** (`target/debug/flowspace3`) — never the PATH-installed one, which is
o-prime's build of a different main and reports the same version string.

**Isolation:** own database `fs3_pm_proof` on the shared container (never `compose up`), own
config dir `/tmp/pmmsg/proofcfg` with `kind = "fake"` providers so the whole stack ran offline,
own port 7391.

## Corpus admitted

```
flowspace3 add .        -> {"ok":true,"data":{"files":351}}
queue drained          -> scan_file 351 done · embed 3317 done · summarize 2832 done · 0 errors
```

| assertion | measured |
|---|---|
| ddoc rows indexed | **529** `kind='row'` elements |
| `.dd.json` admitted | **36** |
| `.dd.md` indexed | **0** — the generated face contributes nothing |

Row addresses are dd's positional form, including a deep dynamic-key trail:

```
crates/parsers/fixtures/ddoc/plain.dd.json#acceptance_criteria/ac-0001
crates/parsers/fixtures/ddoc/dynamic.dd.json#done_when/tk-0001/assertions/required/dw-0002
```

## Address resolves in BOTH tools (ac-0003)

```
flowspace3 get "docs/plans/008-ddocs-scan/plan.dd.json#acceptance_criteria/ac-0004"
  -> {"ok":true,"address":"…#acceptance_criteria/ac-0004","kind":"row","name":"ac-0004"}
ddocs --json get "…#acceptance_criteria/ac-0004/claim"
  -> {"status":"ok","value":"Chunk identity is (file, section, id): reordering rows …"}
```

## Typed edges attached to the owning row (ac-0009)

Rel classes present after a healthy scan: `derives` 53 · `satisfies` 53 · `pressure` 24.
77 rows carry edges; 44 carry derived state. Each edge keeps its rel, target address and
JSONPath location:

```json
{"rel":"satisfies","kind":"document",
 "target":"docs/plans/001-fs3-foundations/plan.dd.json#acceptance_criteria/ac-0001",
 "location":"$.sections[tasks].value[0].satisfies[0]"}
```

## THE REORDER PROOF (ac-0004) — the plan's central invariant

Reversed the id-bearing rows of an indexed `.dd.json`, rescanned, counted the **embeddings
table**, not the job queue and not element rows:

```
embeddings_1024 BEFORE : 7225
embeddings_1024 AFTER  : 7225      <- ZERO new vectors
```

Element rows DID double (each id now present at `sibling_order` 0 and 1) because the reordered
file is new bytes, hence a new blob, hence a fresh parse whose rows differ in `span_start` — which
is part of the elements unique key. That is expected and is exactly why the assertion is on the
vector count: enrichment is keyed on `raw_hash`, so identical row text pays for no embedding.
Fixture restored afterwards; count still 7225.

## Inverse index (ac-000b)

```
flowspace3 refs crates/core/src/ddoc.rs
  -> {"ok":true,"results":[],
      "next_action":"no indexed ddoc rows reference that source path — this is a successful empty answer"}
```

Empty is the CORRECT answer here, verified rather than assumed: `ddocs --json graph` over this
corpus returns 175 edges, all `kind:"document"`, **zero** `kind:"file"`. The committed file-link
fixture corpus declares schema `render/filelinks`, which does not resolve from this repo's root
(`E401`), so its link is not typed as a file link here. The empty result is the state of the world,
not a defect — and it does not error.

## Degradation, both halves (ac-000d)

`ddocs` REMOVED from the daemon's PATH:

```
{"ok":true,"n":2,
 "next_action":"the `ddocs` binary is unavailable: rows are indexed and searchable, but link
  edges, gate-terminal membership and derived state are unavailable until `ddocs` is on PATH …"}
```

`ddocs` RESTORED, identical query:

```
{"ok":true,"n":2,"mentions_ddocs_binary":false}
```

Rows are served in both cases. The notice appears only when degraded and is **silent when
healthy** — the half that catches the real bug, since a warning that never stops is a warning
people learn to ignore.

## What this proof exposed — see DL-008

Bumping `PARSER_VERSION` did **not** re-index the corpus, contradicting its own doc comment.
`flowspace3 scan .` reported `enqueued: 0` across 351 files, because `roots.rs:197` decides what to
enqueue by comparing ONLY the stored path→blob map; `parser_version` is consulted later, inside
`scan::run`'s skip, which never runs because nothing was enqueued. `remove` + `add` forced it
(`enqueued: 351`), and only then did rows re-parse under `fs3-parsers@2` with edges and derived
state attached.

This is pre-existing and outside the plan's fence, but it is load-bearing for shipping: as written
the knob LOOKS like an invalidation mechanism and silently is not, so this plan's ddoc support
would never reach an already-indexed corpus until each file's bytes happened to change.
