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
| **Skips are a ledger, ignores are not** | A refused file (unsupported extension, config format, too large, binary, excluded) is reported — req 43 demands "never a silent gap". A git-ignored file is *out of scope*, not refused, and stays out of both lists; otherwise every `node_modules` entry would be in the report. |
| **Force-include is a second walk** | `force_include` globs run a separate pass with ignore rules off, keeping only paths those globs name. The common case (no force-includes) stays at exactly one traversal, and the semantics stay legible: pass 1 = "what does git leave visible", pass 2 = "what did the repo insist on anyway". |
| **`exclude` outranks `force_include`** | An explicit refusal beats an explicit inclusion. Force-include overrides *git*, not judgement. |
| **Binary is decided by content** | A NUL in the first 8 KiB, the same test `git diff` uses. The PNG someone committed as `logo.md` is caught by the sniff, not by its extension. |
| **Sequential walk** | The win came from *not visiting* files, not from visiting them on more threads, and a deterministic order makes the result assertable. `ignore::WalkParallel` is a drop-in if a measurement ever asks. |

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
- **`.git` is never walked**, even with `include_hidden = true`.
- **Fixture files that are ignored had to be committed with `git add -f`**
  (`build-output/generated.rs`, `app.log`, `secret-notes.md`). They exist so
  the tests can prove they do *not* come back. If they ever vanish from a fresh
  clone, that is why.
- **The sniff costs one `open` per candidate file** and the pipeline then reads
  the file again. If that shows up in a profile, hand the sniff buffer onward
  rather than dropping it.
- **Extensionless files are `Unknown`** — `Makefile`, `LICENSE`, `Dockerfile`
  are not v1 index material, and guessing by filename is the name-matching
  PRD req 42 refuses elsewhere. They land in the skip ledger, so the gap is
  visible rather than silent.

## How to verify it works

```bash
# The pure decisions (family tables, precedence, size window, ScanConfig seam)
cargo test -p fs3-parsers --lib discovery

# The committed fixture tree, asserted as an exact set — both lists
cargo test -p fs3-parsers --test discovery_fixtures

# The architecture edge this added (fs3-parsers -> ignore)
cargo run -p fs3-testkit --bin fs3-arch-check
```

The fixture tree is deliberately small and total: every file in
`crates/parsers/fixtures/discovery-tree/` appears in an assertion, so adding a
file to it fails the default test until the expected set is updated.

## Code pointers

- `crates/parsers/src/discovery.rs` — `discover`, `DiscoverySettings`,
  `LanguageFamily`, `SkipReason`; the pure `verdict` is the whole policy.
- `crates/parsers/src/lib.rs` — `Language::for_extension`, the grammar table
  discovery defers to.
- `crates/core/src/config.rs` — `ScanConfig`, the `[scan]` section
  `DiscoverySettings` converts from.
- `crates/testkit/arch-allowlist.toml` — the `fs3-parsers → ignore` row.
