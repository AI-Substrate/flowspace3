# Git / blob layer
**Built**: 2026-08-26 (worker pij-xenophobic-wren, w-git-blob) · **Code**: `crates/git/src/lib.rs` (`repo_identity`, `snapshot`, `blob_id`), `crates/git/src/error.rs`, pure half in `crates/core/src/git.rs` (`RepoIdentity`, `TreeSnapshot`, `ChangedSet`, `diff`) · **Tests**: `crates/git/tests/real_repos.rs` (13 real-git fixtures), unit tests in `crates/core/src/git.rs`

The git front door of the incremental pipeline (PRD reqs 5, 35, 41): it turns a worktree into "what changed, by blob SHA". Three calls —

```rust
let id       = fs3_git::repo_identity(path)?;   // the repo's primary key
let snapshot = fs3_git::snapshot(path)?;        // path -> blob id, right now
let changed  = fs3_core::git::diff(&old, &new)?; // added / modified / removed
```

— split so that everything pure lives in `fs3-core` (types, key normalisation, the set difference) and everything that touches a disk lives in `fs3-git` (gitoxide). `diff` needs no repository to run, and its tests need no fixtures.

## Key decisions
- **A crate, not a module in the daemon.** Reading git is concrete infrastructure of exactly the same shape as `fs3-store`: a hard requirement (PRD req 5), so no port, no trait, nothing to fake — but also not something to bury in the composition root, where it would be unreachable from anything but the daemon and dragged into daemon's PG-integration test surface. Ruled by the o-prime 2026-08-26; `docs/rules-idioms-architecture/fs3-architecture.md` carries the amendment.
- **gitoxide (`gix`), not libgit2.** Pure Rust, so the cross-platform build stays `cargo build` with no C compiler in the way — and index parsing, `.gitignore` semantics and blob hashing are exactly the pile of edge cases the lib-reuse rule exists to stop us re-deriving. Feature set is deliberately narrow (`sha1`, `index`, `dirwalk`, `max-performance-safe`): no network, no checkout, no diff machinery.
- **Blob ids are computed from the bytes on disk, not read from the index.** A snapshot describes what an indexer must parse: a file modified but unstaged reports its *worktree* blob, and an untracked file reports a real one, so a brand-new file indexes without `git add` (PRD req 41) and keeps the same id once committed. Trusting the index's recorded id when `stat` still matches is git's own fast path and the named next optimisation — deliberately not taken yet, because a mishandled racy timestamp returns a *stale* id, and stale ids are silent staleness in the store.
- **Identity is the remote URL, canonicalised.** `git@github.com:AI-Substrate/flowspace3.git`, `https://GitHub.com/AI-Substrate/flowspace3/` and `…/flowspace3` all key to `git:github.com/AI-Substrate/flowspace3`, which is what lets clones and worktrees share derived content (PRD req 35). Host lowercased (DNS is case-insensitive), path left alone (most forges are not), `.git` and wrapping slashes dropped. `origin` wins when several remotes exist, as it does for git itself.
- **Keys are prefixed by their source.** `git:…` vs `path:…` — the fallback key space and the remote key space share a column, and `/srv/git/fs3` as a local remote must not collide with `/srv/git/fs3` as a folder on this machine.
- **`diff` refuses to cross repositories.** Diffing two unrelated snapshots would report every file as both added and removed — a plausible caller mistake that would look exactly like a full re-index. It returns `Error::SnapshotMismatch` instead.
- **The commit id is provenance, never a key.** It rides on the snapshot for traceability; nothing is keyed by it. `--allow-empty` commits produce an empty `ChangedSet`, which is the whole point of blob keying.
- **Not the discovery filter.** The file set here is git's answer — tracked plus untracked-but-not-ignored. The extension allow-list, size ceiling and force-includes of PRD reqs 41/43 filter it downstream (`docs/services/discovery.md`). Two owners, one boundary: git says what exists, discovery says what is worth indexing.
- **Not the non-git path.** A plain folder has no snapshot; `snapshot` says so by name (`Error::NotAWorktree`) and PRD req 23's content-hash path handles it. `repo_identity` still answers, with a `path:` key.

## Gotchas learned
- **`gix` with `default-features = false` and no `sha1` feature fails to compile as `gix-hash`**, with a `compile_error!` plus a cascade of `#[derive(Default)] on enum with no #[default]` and "expected `Kind`, found `()`" — errors that read like a broken dependency rather than a missing feature. The one line that matters is `Please set either the sha1 or the sha256 feature flag`.
- **`cargo fmt` and `cargo clippy` do not run the same toolchain as `cargo` here.** `cargo` resolves to Homebrew's 1.95, but the `cargo-fmt`/`cargo-clippy` subcommand binaries resolve through `~/.cargo/bin` rustup shims pinned to 1.85 — which has no `rustfmt` component installed (`'cargo-fmt' is not installed for the toolchain '1.85.0-…'`) and **rejects let-chains** (`if let … && let …`) as unstable even in edition 2024. Consequence for this crate: no let-chains, and format with `/opt/homebrew/bin/cargo-fmt fmt`. Anything that compiles under `cargo check` can still fail `harness checks`.
- **clippy's `result_large_err` bites on gix error types.** `gix::worktree::open_index::Error` alone is ≥128 bytes, so `Error::Index` and `Error::Walk` are boxed. Boxing on the cold path costs nothing and keeps every `Result` in the crate small.
- **`dirwalk` only counts an index entry as *tracked* if it carries the `UPTODATE` flag**, which a freshly-read index does not have — so a walk alone can report tracked files as untracked. The file set is therefore the **union** of index entries that still exist on disk and the untracked walk; because every file is hashed from disk anyway, the classification never has to be right.
- **Adding a crate to `arch-allowlist.toml` breaks 8 of the 13 `arch_drift` tests until the three committed `cargo-metadata` fixtures learn about it.** An allow-list entry with no crate in the graph is a `StaleAllowlistEntry` violation, and the fixtures are hand-maintained JSON (`crates/testkit/fixtures/arch/*.json`) — add the package to `workspace_members` *and* `packages` in each.
- **`harness commit "msg" -- <paths>` re-stages the full worktree content of every path it is given**, discarding a surgically prepared index. In this shared tree that swept a sibling's in-flight `Cargo.toml`/allow-list lines into `c7670cd`. Filed as harness observation DL-004.
- **Symlinks, submodules and conflicted index entries are excluded.** Git stores a symlink's target as a blob, but there is no source in that to parse and following it would double-count the target; a conflicted path has no single content to index. Non-UTF-8 paths are skipped rather than lossily renamed — a lossy key is a wrong key.
- **A fresh `git init` has no commit and that is not an error.** `snapshot.commit` is `None` and the worktree is still full of files to index.

## Verify
```bash
cargo test -p fs3-git                 # 13 real-git fixtures: init, commit, modify,
                                      # untracked, ignored, subdir, linked worktree,
                                      # remoteless, plain folder, empty commit
cargo test -p fs3-core --lib git      # the pure half: key normalisation + diff
cargo clippy -p fs3-git -p fs3-core --all-targets -- -D warnings
cargo test -p fs3-testkit --test arch_drift   # the crate graph, incl. this crate's edges
```
The load-bearing assertion is blob-id equivalence with git itself — `an_untracked_file_gets_the_id_git_hash_object_would_print` compares against `git hash-object` on the same file, and `a_committed_file_hashes_to_the_blob_git_recorded` against `git rev-parse HEAD:<path>`. If those ever diverge, every incremental decision downstream is built on sand.

Smoke it against a real repository (no binary yet — the daemon wiring is a later packet):
```bash
cd /tmp && cargo new fs3gitsmoke && cd fs3gitsmoke
cargo add --path <repo>/crates/git --path <repo>/crates/core
# main.rs: print repo_identity(".."), snapshot("..").len(), and a diff
cargo run
```

## Code pointers
| Concern | Where |
|---|---|
| Identity, snapshot, blob hashing | `crates/git/src/lib.rs` |
| Failure modes, each naming its path | `crates/git/src/error.rs` |
| Types + key canonicalisation + `diff` | `crates/core/src/git.rs` |
| Real-git fixtures | `crates/git/tests/real_repos.rs` |
| Allow-list row + justification | `crates/testkit/arch-allowlist.toml` (`[crates.fs3-git]`) |

## Known gaps (named, not hidden)
- **No commit-tree snapshot.** `snapshot` reads the worktree; indexing an arbitrary branch or commit without checking it out (`snapshot_at(rev)`, reading the tree object directly) is a natural next call and is not built. PRD req 5 wants it eventually.
- **No stat-cache fast path.** Every snapshot hashes every discoverable file. That is what a cold `git status` does too, but it is the obvious optimisation when the daemon starts snapshotting on every watcher event.
- **`core.autocrlf` / clean filters are not applied.** Blob ids match `git hash-object` on a repository without content filters, which is every fs3 target today. A Windows checkout with `autocrlf=true` would hash the worktree bytes, not the normalised ones, and disagree with the index — the stat-cache fast path above is also the fix for this.
- **Nothing is wired into the daemon yet.** The composition root does not call any of this; that is the indexing-pipeline packet's job.
