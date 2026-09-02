# Scanner
**Built**: 2026-08-26 (worker pij-plain-mollusk, w-scanner-v1) · **Code**: `crates/parsers/src/{lib,source,markdown}.rs`, model in `crates/core/src/element.rs` + `crates/core/src/classify.rs` · **Tests**: `crates/parsers/tests/fixture_elements.rs`, `crates/parsers/fixtures/sample.{rs,py,md}`

The front half of indexing: **one pure function** that turns a file's bytes into its element tree.

```rust
fs3_parsers::scan(path: &Path, bytes: &[u8]) -> Result<ElementTree, ScanError>
```

It opens nothing, writes nothing, and knows nothing about a database, a queue or a clock — `path` is used for addressing and grammar selection only. That is enforced mechanically: the architecture allow-list refuses `tokio` and `sqlx` in `fs3-parsers`, so the purity is a gate, not a comment.

The returned `ElementTree` is an owned value with a `file` element at the root and everything the file declares nested beneath it. Rust, Python and Markdown are the exemplar grammars; every other file still scans.

## The model

One node type, self-parented (`crates/core/src/element.rs`):

| field | what it is |
|---|---|
| `kind` | CLOSED enum: `File \| Container \| Function \| Section` |
| `subkind` | the grammar's own kind verbatim (`impl_item`, `class_definition`, `atx_heading`), or the language on a `File` root |
| `name` | the declaration's own short name — `scan`, not `Indexer::scan` |
| `address` | stable identity: `src/foo.rs::Indexer::scan` |
| `span` | inclusive 1-based `start_line`/`end_line` |
| `raw_text` | the element's exact source slice |
| `raw_hash` | sha-256 of `raw_text` — **the dirtiness key**, derived, never set by hand |
| `sibling_order` | 0-based position among siblings, source order |
| `children` | declarations nested directly inside, source order |

`path`, `blob` and `has_error` live once on `ElementTree`, not on each node.

## Key decisions
- **A closed kind enum plus a free `subkind`.** Adding a language must never grow the enum. `class`, `impl`, `trait`, `mod`, `struct` are all `container` with the grammar's word in `subkind`, which is exactly the axis a query wants to filter on loosely (`kind`) or precisely (`subkind`).
- **File-level facts live on the tree, not on every node.** `path`/`blob`/`has_error` are one value per file. Copying them onto each of a few hundred elements is a few hundred `String` clones carrying no information. The cost of the decision is one extra parameter at the store boundary — `upsert_element(pool, tree, element)`.
- **`raw_hash` is derived at construction and readable only through `Element::raw_hash()`.** The field is private and computed in `Element::new` from `raw_text`. "The hash changed" therefore *means* "the text changed", by construction rather than by discipline.
- **Modules are container elements, not invisible scopes.** s001 treated `mod_item` as a naming scope that contributed a segment without being an element. In a tree a module is the parent of what it holds, so it is a node. This is what makes `impl`-in-`mod` and `def`-in-`def` parent correctly rather than flattening onto the file.
- **A missing grammar is a tree, not an error.** `ScanError` has exactly one variant (`Unparseable`), and there is deliberately no `NoGrammar`. An unknown extension yields a one-element tree whose `language()` is `unknown`; non-UTF-8 bytes yield one whose `language()` is `binary` and whose `raw_text` is empty. PRD req 43's "never a silent skip" is served by an observable value.
- **Nameless declarations are skipped; their children are not.** A classified node with no name is error-recovery debris. It is spliced through — its real declarations attach to the nearest named ancestor rather than to a ghost.
- **Addresses are not unique, and that is deliberate.** `struct Rect` and `impl Rect` share `sample.rs::geometry::Rect`: they are one logical entity seen in two pieces. `(address, span.start_line)` identifies a *node*; `address` identifies a *thing*. The store's primary key already agrees (`blob, qualified_name, start_line`). Inventing `Rect#2` would have made method addresses read `Rect#2::new`.
- **Markdown section ranges are fs3 code, not grammar output.** tree-sitter-md reports headings as point nodes, so a section runs from its heading line to the line before the next heading of equal-or-shallower level (POC L9). Those ranges nest exactly, which is what makes a heading hierarchy a real tree.
- **The whole tree is the assertion.** Fixture tests render each element as `<kind> <subkind> <address> #<sibling_order> <span>`, indented by depth, and compare the entire list. A classifier regression shows up as an *extra* row far more often than a missing one, and a parenting regression shows up as the same rows at the wrong depth — a spot-check catches neither.

## Gotchas learned
- **`cargo fmt` and `cargo clippy` are broken on this machine and exit 1 without running.** `/opt/homebrew/bin/cargo-{fmt,clippy}` resolve to a rustup shim for toolchain `1.85.0-aarch64-apple-darwin`, which is not installed. Work around it with `rustfmt --edition 2024 <files>` and `/opt/homebrew/bin/cargo-clippy --all-targets -- -D warnings`. A fmt gate that is red for environmental reasons costs every worker who assumes it is judging their code.
- **Never pipe a gate into `head` and then read `$?`** — you get `head`'s exit code. That is how "cargo fmt exits 0" was believed for an hour.
- **Siblings share ONE working tree, so `git add <dir>` is not scoped.** `git add crates/parsers` stages whatever another worker's unstaged edits happen to be at that instant. This landed a `crates/git` workspace member for an untracked crate (hard build failure from a clean checkout) and a `ignore` dependency whose allow-list entry had not landed (arch drift). Stage **files**, not directories, and read `git diff --cached --name-only` before committing.
- **To un-sweep someone else's line without disturbing them:** write the corrected content, commit those paths, restore the working-tree copies immediately. Committing does not touch the working tree, so their file is untouched on disk and lands with their own commit.
- **A green `cargo test --workspace` in the shared tree proves nothing about your commit.** Verify in a detached worktree pinned to your sha: `git worktree add .harness/temp/<you>/verify <sha> --detach`.
- **`mod` as a classifier substring is a near-miss magnet.** It catches `mod_item`, `module`, `module_declaration` — and would catch Java/C# `modifiers`, which the declaration-shape gate rejects. The gate is what makes cheap substring hints safe; `crates/core/src/classify.rs` proves the near-miss in a test.
- **Python's `decorated_definition` must not be promoted.** It wraps the real `function_definition`; classifying it would twin every decorated function. It is spliced through, so a decorated `def` appears once and its span is the `def` — the same way a Rust doc comment sits outside the declaration it documents.
- **tree-sitter's error recovery inserts zero-width MISSING nodes.** `impl<T> {` yields an `impl_item` whose `type` field is *present and empty*, so a name lookup returns `Some("")` rather than `None`. Blank names must be refused explicitly or they become elements nobody can address.
- **`has_error` is metadata, never a skip signal** (POC L8): a 43 KB markdown doc parses with an ERROR node and still yields all 24 correct sections.
- **Element text loses its trailing newline in markdown sections**, which are rebuilt with `lines().join("\n")`. Sections hash their own joined body; the `file` root hashes the file verbatim, so `tree.root.raw_hash() == tree.blob` for any UTF-8 file.

## Verify
```bash
cargo test -p fs3-parsers                       # 17 unit + 10 whole-tree fixture tests
cargo test -p fs3-core                          # model + classifier
cargo test -p fs3-store --test pg_round_trip    # needs `docker compose up -d`
```
The two tests worth reading first, because they are the contract:
- `rust_fixture_yields_the_expected_tree` — the entire tree as one table.
- `a_one_character_edit_rehashes_only_the_elements_containing_it` — a one-character edit inside `Rect::area` re-hashes the file, the module, the impl block and the method, and **nothing else**. If a sibling's hash moved, incremental indexing would re-embed whole files on every commit.

## Adding a language
1. Add the grammar crate to `[workspace.dependencies]` and to `crates/parsers/Cargo.toml`.
2. Add the crate name to `[crates.fs3-parsers].external` in `crates/testkit/arch-allowlist.toml` — one deliberate line, or the drift check bites.
3. Add the extension(s) and the `tree_sitter::Language` in `Language::for_extension` / `Language::grammar` (`crates/parsers/src/lib.rs`).
4. Bump `PARSER_VERSION` in `crates/daemon/src/scan.rs`. Element trees are keyed by `(blob_sha, parser_version)`; without a bump, already-stored blobs keep the old grammar result and a normal scan correctly reuses it.
5. Add a fixture under `crates/parsers/fixtures/` containing nesting and at least one construct that a naive substring classifier would invent an element for.
6. Add the whole-tree expectation plus an `invents_nothing` negative to `crates/parsers/tests/fixture_elements.rs`.

There is normally **no step 7**: extraction is generic. Only reach into `crates/core/src/classify.rs` if the grammar's declaration kinds are shaped unlike every other grammar's — a bare-word declaration (Ruby's `method`/`class`) earns a `BARE_DECLS` entry, and a genuinely new declaration suffix earns a `DECL_SUFFIXES` entry. A per-language branch is never the answer (PRD req 21).

## Not done here
- **File discovery** — `scan` is handed bytes. Choosing which files to hand it is the discovery worker's job, and POC L7 says it is the bigger lever: git-tracked selection took the same 18,628 elements from 11% of the bytes, 13.8× faster.
- **Storing the tree shape.** `0001_elements.sql` is flat and has no column for `sibling_order` or a parent link, so a store round-trip returns a source-ordered list, not a tree (`sibling_order` is re-derived as position; `raw_hash` is re-derived from the body and is therefore exact). Migration `0002_element_kinds.sql` only rewrites the `kind` CHECK to the new spellings. The real ref layer is workshop 002.
- **Parallel fan-out.** POC L1 measured 7.5–8.6× on 16 threads with byte-identical element counts, one `Parser` per file. `scan` is pure and `Send`-friendly, so this is the caller's to add.
