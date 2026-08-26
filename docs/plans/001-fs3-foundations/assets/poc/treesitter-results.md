# Spike/POC results — direct tree-sitter AST extraction in Rust

**Plan**: `docs/plans/001-fs3-foundations` · **De-risks**: base-prd reqs **2, 3, 21, 22**
**Ran by**: pij-likely-sailfish · **Date**: 2026-08-26 · **Machine**: darwin arm64, 16 rayon threads
**POC code**: `assets/poc/treesitter/` (throwaway — 517 lines of `src/main.rs`, no tests, no error polish)
**Raw captured output**: `assets/poc/treesitter/out/*.txt` (every number below is copied from those files)

```bash
cd docs/plans/001-fs3-foundations/assets/poc/treesitter && cargo build --release
./target/release/treesitter file <paths...>      # element table per file
./target/release/treesitter bench <dir>          # single-thread + rayon timings
TS_NAIVE=1 ...                                   # first-cut classifier (the L2 A/B)
TS_GIT=1 ...                                     # only git-tracked files
TS_EXTS=md,ts,py ...                             # restrict to these extensions
```

`target/` is gitignored (232 MB of build artefacts).

---

## Question

Four claims from base-prd, none of them yet proven in Rust:

1. **req 2** — tree-sitter can be used *directly* from Rust (native grammar crates), no per-language config.
2. **req 3** — a file can be split into elements (callable / type / section) with a stable address
   (raw ts kind + universal category + qualified name + line span).
3. **req 21** — broad language coverage is *adding a grammar crate*, never writing per-language code.
4. **req 22** — markdown gets first-class treatment: nested heading sections as elements.

Plus the unstated one the plan actually depends on: **is it fast enough to index a real repo?**

## What We Ran

| # | Pass | Command | Captured output |
|---|---|---|---|
| 1 | Fixture pass, naive classifier (`TS_NAIVE=1`) | `TS_NAIVE=1 treesitter file <fs2 fixtures>` | `out/01-fixtures-naive.txt` |
| 2 | Same files, refined classifier | `treesitter file <fs2 fixtures>` | `out/02-fixtures-refined.txt` |
| 2b | Broad fixture pass, 19 files / 13 grammars | `treesitter file <fs2 fixtures>` | `out/02b-fixtures-refined-full.txt` |
| 3 | Timing — fs2 fixture corpus | `treesitter bench <fs2>/tests/fixtures` | `out/03-bench-fixtures.txt` |
| 4 | Large repo — everything on disk | `treesitter bench <harness-engineering>` | `out/04-bench-harness-engineering.txt` |
| 5 | Large repo — git-tracked only | `TS_GIT=1 treesitter bench …` | `out/05-bench-he-git-tracked.txt` |
| 6 | Large repo — source files only | `TS_GIT=1 TS_EXTS=md,ts,tsx,js,py,sh,… treesitter bench …` | `out/06-bench-he-source-only.txt` |
| 7 | 5 manual spot-checks | `treesitter file <file>` vs `grep` ground truth | `out/07-spotchecks.txt` |

**Corpus**: fs2's `tests/fixtures/ast_samples/**` + `tests/fixtures/samples/**` (73 parseable files across
17 grammars — rust, python, typescript, tsx, javascript, csharp, go, java, c, cpp, ruby, bash, markdown,
hcl, json, yaml, toml). **Large repo**: `/Users/jordanknight/substrate/harness-engineering` — 11,452
walkable parseable files / 195.9 MB, of which 2,356 git-tracked and 1,937 actually source.

**Grammars wired** (20 crates, all from crates.io, zero build friction):

```
tree-sitter 0.26.13   bash 0.25.1  c 0.24.2   c-sharp 0.23.5  cpp 0.23.4   css 0.25.0
hcl 1.1.0  html 0.23.2  java 0.23.5  javascript 0.25.0  json 0.24.8  md 0.5.3
python 0.25.0  ruby 0.23.1  rust 0.24.2  toml-ng 0.7.0  typescript 0.23.2  yaml 0.7.2
```

Release binary: 19 MB. Clean `cargo build --release`: **18.7 s**.

### Sample of the real output (spot-check 1 — `harness/cli/src/app.ts`, 27.9 KB)

```
### harness/cli/src/app.ts [typescript]  27895 bytes  parse 1405µs  extract 1053µs  error=false
  ts_kind                      category       lines  qualified_name
  function_declaration         callable     88-96     jsonFlag
  function_declaration         callable    104-109    quietFlag
  …
  function_declaration         callable    389-535    buildProgram
  arrow_function               callable    463-463    buildProgram.collectorHooks
  interface_declaration        type        538-565    MainOverrides
  function_declaration         callable    592-708    main
```

Ground-truth `grep` of declaration keywords in that file returns **17 declarations at lines
88, 104, 112, 126, 148, 169, 173, 193, 221, 249, 278, 295, 338, 389, 538, 567, 592** — the extractor
returns **all 17 at exactly those start lines**, plus one nested arrow function grep cannot see.

### Markdown, nested (fs2 `ast_samples/markdown/headings_nested.md`)

```
  atx_heading  section   1-16   Main Title
  atx_heading  section   5-12   Main Title > Section One
  atx_heading  section   9-12   Main Title > Section One > Subsection 1.1
  atx_heading  section  13-16   Main Title > Section Two
```

Nesting is preserved in `qualified_name`, and each section's span runs to the next heading of
equal-or-shallower level — i.e. sections are real ranges, not points.

## Verdict

| # | Claim | Verdict | Evidence |
|---|---|---|---|
| req 2 | tree-sitter direct from Rust, native crates, no per-language config | **PROVEN** | 20 grammar crates compile and load; `language_for_ext` is a 20-line `match` and is the *only* place a language is named |
| req 3 | element model: raw kind + universal category + qualified name + line span | **PROVEN** | every table above; qualified names nest correctly through classes, impls, namespaces, closures |
| req 21 | adding a language = adding a grammar, never per-language code | **PARTIAL** | true for 15 of 20 grammars via pure pattern rules; **5 needed a rule, not code** (see Learnings L2–L4). Config languages (hcl/yaml/json/toml) parse fine but yield **0 elements** — they need a `block` element category, which is a *rule* change, not per-language code |
| req 22 | markdown first-class, nested heading sections | **PROVEN** | spot-check 2: 24/24 real headings extracted from a 43 KB doc, and **8 `#` lines inside fenced code blocks correctly rejected** — a regex splitter would have produced 32 bogus sections |
| — | fast enough to index a real repo | **PROVEN** | 1,937-file / 21.6 MB source tree fully parsed + extracted in **0.394 s** median (4,914 files/s, 54.7 MB/s, 16 threads) |

**Overall: PROVEN.** The architecture workshop's call — parser is CONCRETE, no trait — holds up.
Nothing in this POC wanted an abstraction.

## Timings

### Large repo — `harness-engineering`, three scopes

Medians of repeated runs (3 runs source-only, 2 each for the others); spread is given because it
matters for the last column.

| Scope | Files | MB | Elements | Single-thread | Parallel (16t) | Speedup | Parallel rate |
|---|---:|---:|---:|---:|---:|---:|---|
| Everything walkable | 11,452 | 195.91 | 41,632 | 17.6 s (17.5–19.8) | **5.44 s** (5.4–5.7) | 3.3× | 2,104 files/s · 36.0 MB/s |
| Git-tracked only | 2,356 | 47.08 | 18,628 | 4.55 s (4.3–4.9) | **1.00 s** (1.00–1.02) | 4.6× | 2,360 files/s · 47.2 MB/s |
| **Source files only** | 1,937 | 21.55 | 18,628 | 3.15 s (3.1–3.6) | **0.394 s** (0.39–0.45) | 8.2× | 4,914 files/s · 54.7 MB/s |

> Both passes warm the page cache and grammar init before timing the single-thread run, so the
> speedup column is thread scaling, not cold I/O. Element counts are identical single vs parallel
> — asserted in-process on every run (*"determinism check: parallel element count == single-thread ✔"*).
>
> **The speedup column is itself a finding.** Thread scaling collapses from 8.2× to 3.3× as the
> multi-megabyte JSON blobs enter the set: a single 18 MB file is one indivisible 0.6 s task, so the
> tail serialises (textbook Amdahl). Excluding data files does not just remove wasted work — it
> restores the parallelism.

### Per-language cost — source-only pass (1,937 files)

| language | files | elements | MB | cpu-ms | µs/file |
|---|---:|---:|---:|---:|---:|
| markdown | 1,205 | 12,761 | 13.14 | 1,551.0 | 1,287 |
| typescript | 689 | 5,761 | 8.27 | 786.3 | 1,141 |
| javascript | 19 | 28 | 0.05 | 5.2 | 274 |
| tsx | 9 | 53 | 0.03 | 4.1 | 457 |
| bash | 9 | 13 | 0.04 | 3.2 | 358 |
| python | 6 | 12 | 0.01 | 2.0 | 340 |

### Per-language cost — fs2 fixture corpus (73 files, 0.19 MB, 400 elements, 0.023 s single / 0.005 s parallel)

| language | files | elements | µs/file | | language | files | elements | µs/file |
|---|---:|---:|---:|---|---|---:|---:|---:|
| rust | 3 | 41 | 351 | | java | 1 | 40 | 809 |
| python | 15 | 65 | 108 | | cpp | 1 | 22 | 978 |
| typescript | 5 | 47 | 246 | | c | 1 | 13 | 935 |
| tsx | 2 | 14 | 455 | | ruby | 1 | 18 | 900 |
| go | 3 | 28 | 408 | | bash | 1 | 11 | 828 |
| csharp | 3 | 7 | 91 | | javascript | 2 | 9 | 229 |
| markdown | 18 | 85 | 134 | | hcl / json / toml / yaml | 17 | **0** | 96–410 |

### The cost outliers are data, not code

Top-5 slowest files in the everything-walkable pass:

```
790290µs   7,023,043 bytes  .harness/temp/perf/vitest-cov.json
624091µs  18,067,014 bytes  scratch/dogfood-049/…/….session.json
532682µs  16,071,237 bytes  scratch/sample-telemetry/…/….session.json
169378µs   5,083,348 bytes  scratch/dogfood-2026-06/…/….session.json
167793µs   5,083,348 bytes  scratch/spike/….session.json
```

Five JSON blobs cost **2.28 s of the ~17.6 s single-thread wall (13%)** and produced **zero elements**.
Across the whole repo, JSON is 5,844 files / 129.8 MB / 5.87 CPU-seconds / **0 elements**, and HTML is
579 files / 18.0 MB / 1.78 CPU-seconds / **0 elements** — together **66% of the bytes and 44% of the
CPU for nothing**.

## Learnings to Promote

**L1 — Parallelism is nearly free, and deterministic.** `rayon::par_iter` over a `Vec<PathBuf>` with a
fresh `Parser` per file gave 7.5–8.6× on 16 threads with byte-identical element counts. A `Parser` is
not `Sync`; constructing one per file costs ~50–500 µs and is entirely hidden by the parallelism.
**Constraint for the plan: parse fan-out is per-file, and the element count is a legitimate
determinism assertion to keep in the real test suite.**

**L2 — Substring classification alone is wrong; it must be gated on declaration shape.** fs2's
`classify_node()` maps `ts_kind` to a universal category by substring, and porting it verbatim
produced false elements immediately: Rust `struct_expression` → `type` (it is a `Self { .. }`
literal), TS `interface_body` → `type`, C++ `function_declarator` duplicating every
`function_definition` *and* naming it after the return type. The fix is one extra predicate — an
element must **also** be declaration-shaped (`_item|_declaration|_definition|_signature|_spec`, plus
`_specifier` for types only) — and it is still zero per-language code.

The A/B is re-runnable: `TS_NAIVE=1` restores the first-cut classifier.
(`out/01-fixtures-naive.txt` vs `out/02-fixtures-refined.txt`, same seven files.)

| fixture | naive elements | refined elements | what the naive pass invented |
|---|---:|---:|---|
| `rust/structs_impl.rs` | 7 | 6 | `struct_expression` → a phantom `Calculator.new.Self` type |
| `typescript/interfaces_types.ts` | 7 | 5 | `interface_body` → a duplicate anonymous type per interface |
| `go/structs_methods.go` | 6 | 4 | anonymous `struct_type` twin; methods unattached to receiver |
| `csharp/namespace_class.cs` | 3 | 3 | count same, but names lacked the `MyApp.Services.` prefix |
| `ruby/tasks.rb` | 68 | 18 | every `method_call` matched the `method` substring |
| `c/main.cpp` | **58** | **22** | `function_declarator` twins + elements named `void`, `bool`, `std::future` |
| `typescript/anonymous_callbacks.ts` | 13 | 4 | every anonymous test callback promoted to an element |

**L3 — Field-name priority matters, and `type` is a trap.** Deriving a name via
`child_by_field_name` needs the order `name → declarator → path → pattern → type`. Trying `type`
early works for Rust `impl_item` (target type) and silently breaks C/C++, where `type` is the
*return type* — producing elements literally named `void`, `bool`, and `std::future`.

**L4 — Three naming rules cover the languages a pure walk misses.** Each is generic, none is a
language branch: (a) *container-only* kinds — anything containing `namespace`, plus `mod_item` /
`module` / `package_declaration` / Go's `type_declaration` — contribute a qualified-name segment but
are not elements themselves (this is what turns `UserService` into `MyApp.Services.UserService`);
(b) a `receiver` field scopes a callable onto its type (Go `Add` → `Calculator.Add`); (c) a callable
that is the *value of a named binding* inherits the binding's name — without this, idiomatic
TS/JS `const handleClick = () => {}` is invisible, and TS is the single largest source language in
the corpus (689 files).

**L5 — Some grammars use bare kind names.** Ruby declares with `method` / `class` / `module` — no
suffix. Without a small `BARE_DECLS` exact-match list, a 6.3 KB Ruby file yields **0 elements**; with it, 18.
Expect the same for GDScript, Elixir, Lua. **The universal classifier needs both a suffix table and
an exact-word table.**

**L6 — Config/data languages parse but produce nothing under a code-shaped element model.**
hcl, yaml, json, toml, css, html all parse cleanly and yield **0 elements**, because their content
classifies as `block` / `definition`, and the fs3 element model only promotes `callable|type|section`.
fs2's taxonomy already has `block`. **Decision the plan must make explicitly: either promote `block`
to a first-class element (giving Terraform resources and YAML keys addresses), or declare these
extensions non-indexable and skip them.** Silently parsing 130 MB of JSON into nothing is the
worst of the three options.

**L7 — File selection is a bigger performance lever than the parser.** Walking everything on disk
is 11,452 files / 195.9 MB; git-tracked is 2,356 / 47.1 MB; actually-source is 1,937 / 21.6 MB.
**Same 18,628 elements from 11% of the bytes, in 0.394 s instead of 5.44 s — a 13.8× wall-clock win from file selection alone, before a single parser optimisation.** The walker must (a) be git-aware — `.claude/worktrees/`
duplicated the entire repo into the scan, and `scratch/` contributed the three slowest files — and
(b) apply a size cap: a single 18 MB `.session.json` cost 0.62 s. **Promote to the plan: git-tracked
file discovery + a per-file byte ceiling + an indexable-extension allow-list, all before the parser
is ever handed a file.**

**L8 — `has_error()` is not a skip signal.** 36 of 11,452 files parse with an ERROR node. The 43 KB
markdown doc in spot-check 2 has `error=true` and still yields **24/24 correct sections**. Error
recovery is doing its job. **fs3 should record `has_error` as metadata, never use it to reject a
file.** Only genuinely broken source loses everything: fs2's `syntax_error.py` fixture yields 0
elements, correctly.

**L9 — Markdown beats regex, measurably.** In `docs/how/harness-flow.md`, `grep '^#{1,6} '` finds 32
heading-looking lines; 8 of them are shell comments inside fenced code blocks. tree-sitter-md returns
exactly the 24 real headings. **This is the concrete argument for req 22 being AST-based**, and it is
also why heading *spans* must be synthesised: tree-sitter-md gives headings as point nodes, so the
section range (heading line → line before the next heading of equal-or-shallower level) is fs3 code,
not grammar output.

**L10 — Grammar-crate ABI skew is a non-issue in practice.** Core `tree-sitter 0.26.13` loaded
grammars published against 0.23/0.24/0.25 with **zero `set_language` failures across all 11,452
files** (`skipped=0` in every pass). The POC has an `abi-mismatch` skip path; it never fired.
Crate versions do *not* need to be aligned to the core version.

**L11 — Coverage gap to close deliberately.** fs2's corpus includes GDScript (`.gd`) and Dockerfile;
neither has a crate in this POC's set, so those files are silently *unsupported* rather than skipped.
**fs3 needs an explicit "no grammar for this extension" outcome that is observable**, or coverage
regressions will be invisible.

## Discarded

Everything in `assets/poc/treesitter/` is throwaway. Nothing here should be lifted into `parsers/`
as-is: no error handling, `expect()` on git, name derivation is a leading-identifier-run heuristic
(it renders a C++ destructor as `~Event()`), consts/statics are deliberately not elements, and the
CLI modes exist only to produce the captures in `out/`. **What survives is the eleven learnings
above and the shape of the classifier** — `classify(kind) → category` plus
`is_declaration_shaped(kind, category)` plus the three naming rules of L4. Re-derive the code in
`parsers/` against real fixture tests (workshop line 108: *"parsers | fixture files → expected
elements | none"*).
