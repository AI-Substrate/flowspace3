# discovery — which files fs3 looks at

**Code**: `crates/parsers/src/discovery.rs` · **Tests**: `crates/parsers/tests/discovery_fixtures.rs` + `crates/parsers/fixtures/discovery-tree/`
**Requirements**: PRD 41 (filtered, git-ignore-aware discovery), 43 (config formats excluded, skips observable), 12/39 (relative paths, zero repo footprint), 23 (non-git folders index too).

## What it is

One function:

```rust
fs3_parsers::discovery::discover(root: &Path, settings: &DiscoverySettings) -> Result<Discovery, DiscoveryError>
```

It walks `root` and returns two sorted lists: `files` (what fs3 will scan) and
`skipped` (what it looked at and refused, with a reason). Each `DiscoveredFile`
carries the `/`-separated **relative** path, the size in bytes, the
`LanguageFamily` that routed it, and the `Language` grammar when fs3 has one.

This is the front of the pipeline: `discover` decides *whether* to look at a
file, `scan` (same crate) decides *what is in it*.

## Why it matters

The tree-sitter POC measured discovery as the dominant performance lever:
**the same 18,628 elements from 11% of the bytes, 0.394 s instead of 5.44 s —
13.8× from file selection alone**, before any parser optimisation
(`docs/plans/001-fs3-foundations/assets/poc/treesitter-results.md`). A single
18 MB `.session.json` cost 0.62 s on its own. The walk is not plumbing; it is
the scanner's budget.

## Key decisions

| Decision | Why |
|---|---|
| **Use the `ignore` crate** (ripgrep's walker) | gitignore semantics — nested ignore files, negations, parent rules, precedence — are a decade of edge cases. Re-deriving them is the reinvention the arch allow-list exists to notice. One allow-list row, `fs3-parsers → ignore`, with the justification inline. |
| **Lives in `fs3-parsers`, not `fs3-core`** | It performs IO, and core performs none (workshop 001 rule 2). It also needs `Language::for_extension` to answer "do we have a grammar for this?" — duplicating that table elsewhere is exactly the per-language code PRD req 21 refuses. The IO is confined to `discover`; every decision it makes is a pure function (`verdict`, `LanguageFamily::for_path`) unit-tested without a filesystem. |
| **Settings are injected, never read here** | `DiscoverySettings` is a plain value. `impl From<&ScanConfig>` is the whole wiring story: the composition root resolves config (machine < worktree < flag, req 40) and passes `(&config.scan).into()`. |
| **Families, not a flat extension list** | `Source` / `Document` / `Config` / `Unknown`. Req 43 excludes an entire *class* (YAML, JSON, TOML, HCL and kin), so the class is the type. `index_config_formats = true` turns it back on for a repo that wants it. |
| **Grammar table consulted first** | `LanguageFamily::for_extension` asks `Language::for_extension` before its own tables, so a file fs3 can parse can never be classified `Unknown`. The invariant is mechanical, not remembered. |
| **A standard deny list, independent of git** | `STANDARD_IGNORES` — `.cache`, `.git`, `.next`, `.venv`, `__pycache__`, `build`, `dist`, `node_modules`, `target`, `vendor`, `venv` — is refused whether or not the repo has a `.gitignore`. Git-ignore rules are the *repo's* opinion; this list is fs3's, and it has to hold when the repo has none. Found by sailfish during first-light integration: a `.gitignore`-less clone indexes `node_modules/**/*.js`, because that is real JavaScript and `js` is in the source table. |
| **Denied directories are PRUNED, not judged per file** | The deny list is a `filter_entry` prune in the walker, so `node_modules` costs one string comparison instead of a descent. Measured below: not descending is the entire saving. |
| **Denied directories are NAMED, not counted** | `Discovery::pruned` carries one row per refused directory — the directory, never its contents. Both halves of fs3 had applied the ledger-explosion argument one level too far: discovery pruned to avoid 316,609 rows and the daemon aggregated to avoid thousands, and neither noticed that the ~11 directories themselves are cheap and are the actual answer to "why is my code missing". Separating directory from contents is what makes the ledger affordable. |
| **Whole path components, never substrings** | `src/target_types.rs`, `src/node_modules_helper.rs`, `my-vendor/`, `builder/` and `build-output/` all survive. The check only ever looks at *directory* components, so a file's name cannot trip it. |
| **Denied names are ASCII-case-insensitive** | `Build/` is denied as surely as `build/`. Case sensitivity is a property of the volume, not the platform — on a case-insensitive volume they are one directory — so no `cfg!` gets it right, and denying both is the safer half of the disagreement. It also makes this prune agree with `fs3-daemon`'s watcher filter, which was already `eq_ignore_ascii_case`. |
| **The named root always wins** | The deny list applies at depth > 0 only: `flowspace3 add ./node_modules` is a deliberate instruction, not an accident. `.git` is the single exception — never walked, at any setting, by any route. |
| **A list, exposed as a bool** | `scan.standard_ignores` is a bool in TOML (that is the whole question a config file needs to answer); `DiscoverySettings.standard_ignores` is a `Vec<String>`, which is a superset — `false` is the empty list, `true` is the default list, and a caller with a reason can pass its own names without a config schema change. A custom list *replaces* the defaults, like `exclude` does. |
| **Skips are a ledger, ignores are not** | A refused file (unsupported extension, config format, too large, binary, excluded) is reported — req 43 demands "never a silent gap". A git-ignored file is *out of scope*, not refused, and stays out of both lists; otherwise every `node_modules` entry would be in the report. |
| **Force-include is a second walk** | `force_include` globs run a separate pass with ignore rules off, keeping only paths those globs name. The common case (no force-includes) stays at exactly one traversal, and the semantics stay legible: pass 1 = "what does git leave visible", pass 2 = "what did the repo insist on anyway". |
| **`exclude` outranks `force_include`** | An explicit refusal beats an explicit inclusion. Force-include overrides *git*, not judgement. |
| **Binary is decided by content** | A NUL in the first 8 KiB, the same test `git diff` uses. The PNG someone committed as `logo.md` is caught by the sniff, not by its extension. |
| **Sequential walk** | The win came from *not visiting* files, not from visiting them on more threads, and a deterministic order makes the result assertable. `ignore::WalkParallel` is a drop-in if a measurement ever asks. |

## Upgrade note — case-sensitive volumes (v0.2.0)

**If you keep first-party source in a directory whose name differs from the
deny list only by case — `Build/`, `Dist/`, `Vendor/`, `Target/` — it stops
being indexed in v0.2.0, silently.**

The deny list became ASCII-case-insensitive so it agrees with the watcher's
filter (see the four axes below). On a case-insensitive volume — macOS and
Windows by default — nothing changes: `Build/` and `build/` were always the
same directory. On a **case-sensitive volume**, typically Linux, `Build/` used
to be walked and now is not.

The symptom used to be the bad kind — no error, nothing in either file list,
just search missing code you know exists. It is now **named**: `Discovery`
reports the directory in `pruned`, and `flowspace3 add` returns those rows
unaggregated with the reason and a fix. The remedy is one key:

```toml
[scan]
# drop the standard deny list; .gitignore still applies
standard_ignores = false
```

**`force_include` is not reachable from config yet** — it is a
`DiscoverySettings` field with no `[scan]` key, so the per-directory escape
hatch this page describes is currently programmatic only, and `[scan]` refuses
unknown keys. Anyone hitting this on a case-sensitive volume has the blunt
toggle and nothing finer until the config surface carries `force_include` and
`exclude` (owner: whoever holds `crates/core/src/config.rs`; named here rather
than left as a surprise).

`flowspace3 config show` reports the effective `scan` section, which is the
first place to look when a folder is unexpectedly absent.

## Precedence

Highest first — the first rule that matches decides:

1. `exclude` globs — an explicit refusal beats everything, and is *reported*.
2. `force_include` globs — the escape hatch, reaching past git AND the deny
   list (a genuinely-vendored `vendor/` is the motivating case).
3. `STANDARD_IGNORES` — fs3's own opinion, gitignore or no gitignore.
4. Git's ignore rules — `.gitignore`, `.ignore`, `.git/info/exclude`, parents.

`.git` sits outside the ladder: it is never walked at any setting.

## Gotchas (the expensive ones)

- **`ignore` has no git index.** It matches ignore *files*, so a **tracked**
  file that matches a `.gitignore` pattern is skipped by fs3 even though git
  itself would not ignore it (git never ignores tracked files). Rare in
  practice, surprising when it bites; name `force_include` to get such a file
  back.
- **`git_global` is deliberately off.** The user's `core.excludesFile` is
  per-developer; an index whose contents depend on whose laptop ran the scan is
  not an index.
- **`require_git` is off.** A `.gitignore` in a plain, non-git folder *is*
  honoured — git would not. PRD req 23 says plain folders index through the
  same mechanism, and it would be worse for the same tree to index differently
  for having been cloned.
- **`.git` is never walked**, even with `include_hidden = true`, an emptied
  deny list, or a `force_include` naming it. It is also the one case that
  cannot be a committed fixture — git refuses to track a path named `.git` — so
  that test builds its tree in a temp directory.
- **Fixture files that are ignored had to be committed with `git add -f`**
  (`discovery-tree/build-output/generated.rs`, `app.log`, `secret-notes.md`,
  and all of `discovery-bare/target/`). They exist so the tests can prove they
  do *not* come back. If they ever vanish from a fresh clone, that is why.
  `discovery-tree/`'s nested-ignore directory is named `third_party/`, not
  `vendor/`, precisely so the deny list does not mask what that test proves.
- **The sniff costs one `open` per candidate file** and the pipeline then reads
  the file again. If that shows up in a profile, hand the sniff buffer onward
  rather than dropping it.
- **Extensionless files are `Unknown`** — `Makefile`, `LICENSE`, `Dockerfile`
  are not v1 index material, and guessing by filename is the name-matching
  PRD req 42 refuses elsewhere. They land in the skip ledger, so the gap is
  visible rather than silent.
- **With the ignore rules off, the skip ledger is enormous.** Measured on this
  repo: 57 skips with the defaults, **316,609** with both `respect_gitignore =
  false` and the deny list emptied (every `target/` artefact is a named,
  unsupported file). The ledger is sized for "what did fs3 refuse", not "what
  is on disk" — which is why a denied directory's *contents* are pruned rather
  than reported.
- **A denied directory is named in `Discovery::pruned`** — the directory, never
  its contents. That distinction is the whole reason this list is affordable:
  ~11 rows on a real repository against the 316,609 its contents would cost.
  If `vendor/` produced nothing, the prune ledger says so by name, with the
  reason and a fix; `flowspace3 add` surfaces it unaggregated in its response.
- **Git-ignored paths are still in no list at all.** `ignore`'s walker applies
  its own matchers before fs3's callback (`Walk::skip_entry` consults
  `should_skip_entry` first), so a git-ignored directory is pruned before this
  crate is asked about it and cannot be reported honestly. That half has a
  better tool than anything fs3 would be guessing at: `git check-ignore -v
  <path>` names the file and line responsible.
- **The deny list is a fixed list of names, not a heuristic.** It will be wrong
  for somebody: a Go project with a real `vendor/` directory of first-party
  code, or a Java project whose sources live under `build/`. Today the answer
  is `standard_ignores = false`; per-directory `force_include` is the better
  answer and is not typeable until the config surface carries it.

## Measured on this repo (2026-08-26, debug build)

| Settings | Files | Bytes | Skips | Wall |
|---|---|---|---|---|
| defaults | 230 | 1,827 KiB | 57 | 110 ms |
| `respect_gitignore=false`, deny list ON | 454 | 3,615 KiB | 116 | 108 ms |
| `respect_gitignore=false`, deny list OFF | 1,270 | 20,656 KiB | 316,609 | 10,711 ms |

Rows 2 and 3 are the sailfish scenario — a repository with no usable
`.gitignore`. The deny list alone accounts for **99× wall-clock, 5.7× the
bytes, and a skip ledger three orders of magnitude smaller**, with no git
involvement whatsoever. The earlier POC finding (13.8× from gitignore-aware
filtering) and this one are the same lesson twice: what a scan costs is decided
before a parser is ever handed a byte.

## How to verify it works

```bash
# The pure decisions (family tables, precedence, size window, ScanConfig seam)
cargo test -p fs3-parsers --lib discovery

# The committed fixture tree, asserted as an exact set — both lists
cargo test -p fs3-parsers --test discovery_fixtures

# The standard deny list, against a tree with NO .gitignore at all
cargo test -p fs3-parsers --test discovery_standard_ignores

# The architecture edge this added (fs3-parsers -> ignore)
cargo run -p fs3-testkit --bin fs3-arch-check
```

Both fixture trees are deliberately small and total: every file in
`crates/parsers/fixtures/discovery-tree/` and `.../discovery-bare/` appears in
an assertion, so adding a file fails the exact-set test until the expectation
is updated. The bare tree's tests all pass `respect_gitignore: false` — with
git's rules out of the picture, only the deny list can explain an absence, and
this repo's own root `.gitignore` cannot fake a pass.

## Code pointers

- `crates/parsers/src/discovery.rs` — `discover`, `DiscoverySettings`,
  `STANDARD_IGNORES`, `LanguageFamily`, `SkipReason`; the pure `verdict` is the
  per-file policy, and the `filter_entry` closure in `walk` is the deny list.
- `crates/parsers/src/lib.rs` — `Language::for_extension`, the grammar table
  discovery defers to.
- `crates/core/src/config.rs` — `ScanConfig`, the `[scan]` section
  `DiscoverySettings` converts from, including `standard_ignores` (added with
  the deny list — **note for egret**, who owns the config surface: it is a
  plain bool with `deny_unknown_fields`, defaulting to `true`, and the list
  itself deliberately lives in code rather than config).
- `crates/daemon/src/debounce.rs` — `IGNORED_DIRECTORIES`, the watcher's
  three-name pre-filter, answering a different question (when to *walk*).
  `discovery_standard_ignores.rs` pins that its names stay a subset of
  `STANDARD_IGNORES` — but see **"Delegation is to the settings, not the
  const"** below before wiring the two together.
- `crates/testkit/arch-allowlist.toml` — the `fs3-parsers → ignore` row.

## Delegation is to the settings, not the const

`STANDARD_IGNORES` is `pub`, and the obvious move — point the watcher's
`IGNORED_DIRECTORIES` at it — is **wrong today**. sawfish probed it against
`main` before touching that const and measured three divergences, of which the
names are only the first:

1. **Names** — 11 here, 3 there, a strict subset. Pinned by
   `the_list_is_sorted_and_covers_the_watchers_names`.
2. **Root-relativity** — discovery matches components *below the root*;
   `debounce::is_ignored` scans every component of the **absolute** event path.
   So a repository living under `~/target/myrepo` is already invisible to the
   watcher (`observe(...) -> Rejected(Ignored)` for every event, silently), and
   widening its three names to these eleven would do the same to `~/build/…`,
   `~/dist/…` and `~/vendor/…` — ordinary places to keep code — for roots that
   `add` indexes perfectly. Pinned from this side by
   `the_deny_list_is_root_relative_never_absolute`.
3. **The toggle** — `scan.standard_ignores = false` empties the deny list here,
   and a `const` cannot be turned off. Setting it false would make discovery
   index `build/` while the watcher still refused to walk it: indexed once by
   `add`, then never updated. That is the add-vs-watcher mismatch this deny
   list closed, running backwards, eleven names wide.
4. **Case** — `debounce::is_ignored` compares with `eq_ignore_ascii_case`;
   this prune was case-*sensitive* until sawfish named the axis, and now is
   not. Converged rather than argued: on a case-insensitive volume `Dist/` and
   `dist/` are one directory, so a case-sensitive prune would index precisely
   what the watcher refuses to walk. Pinned by
   `the_deny_list_ignores_ascii_case`. A genuine first-party `Dist/` is one
   `force_include` line.

The safe delegation is to the **settings value** —
`DiscoverySettings::standard_ignores`, matched root-relatively — so the two
filters cannot disagree on any axis rather than only on names. That is a change
to the watcher's contract (`Debouncer` threads the list, `is_ignored` takes
root + path); it belongs to **sailfish**, who owns the watcher core, and is
scheduled for after the v0.2.0 merge (o-prime ruling, 2026-08-26).

### The cross-filter fixture (landed, one half green)

`fs3_testkit::discovery_filter` holds the shared table: 12 `(root, path)`
cases, each with the `scan.standard_ignores` setting it is asked under and the
answer both filters must give. It pins the **decision**, not the data — a
subset-of-consts test reads as proof of agreement while being blind to
relativity, configurability and case (sawfish, DL-009).

- Discovery's half is green now:
  `crates/parsers/tests/cross_filter.rs`.
- The watcher's half runs the same table through `debounce::is_ignored` and
  lands with the delegation, post-merge, by sailfish. Until then the fixture
  states the contract that change must satisfy.

All four axes are represented, including the toggle — the one axis neither
side's own tests touch. Everything except the deny list is neutralised in the
run (no gitignore semantics, hidden files on, size window irrelevant), because
the watcher has none of those knobs and a fair comparison must not invent them.

One thing the fixture taught immediately: it must be built in a temp directory,
and its mixed-case cases must not collide with their lowercase siblings.
`Src/app.rs` beside `src/main.rs` silently becomes `src/app.rs` on a
case-insensitive volume — the case stops testing casing and starts testing
nothing. It is `Lib/app.rs` for that reason.
