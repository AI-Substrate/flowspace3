# Worker brief — git/blob layer: repo identity + commit→blob diffing · (seat at canary, pane %40)
**From**: pij-instant-lynx (o-prime) · 2026-08-26 · one bounded task

## The job
The git front door of the incremental pipeline (PRD reqs 5, 35): turn a worktree into "what changed, by blob SHA".

1. **New module** (`fs3-core` types + a git-facing crate home you justify — likely `fs3-parsers` is WRONG; propose `fs3-store`? No: recommend a new module in `fs3-daemon` OR core-pure types + `gitoxide` IO behind a small function set; stop-and-ask me with a one-liner if unsure where it lands re: the arch allowlist).
2. API shape (pure data out): `repo_identity(path) -> RepoIdentity` (git remote URL as primary key, req 35's fallback for remoteless repos — deterministic path-derived id); `snapshot(path) -> TreeSnapshot` (commit id + map of tracked file → blob SHA, plus untracked-but-discoverable files hashed the same way git would — `git hash-object` semantics so untracked files get real blob ids, PRD 41's new-file concern); `diff(old: &TreeSnapshot, new: &TreeSnapshot) -> ChangedSet` (added/modified/removed by blob comparison).
3. Use **gitoxide (`gix`)** (pure Rust, cross-platform, no libgit2 C dep) — lib-reuse rule; allowlist row + justification.
4. Fixture-tested: build throwaway git repos in tests (init, commit, modify, untracked file, no-remote repo) — real git fixtures, no mocks; assert exact snapshots/diffs incl. the untracked-file blob-id equivalence to `git hash-object`.
5. `docs/services/git-blob.md` per convention when done.

## Rules & fence
- Architecture authority: `docs/rules-idioms-architecture/fs3-architecture.md`. No new ports (this is concrete infrastructure, like the store). No mocks.
- Fence: your new module + tests, `crates/testkit/arch-allowlist.toml` (one row), `docs/services/git-blob.md`. Sibling mollusk is mid-refactor of `crates/core/src/element.rs` — don't touch element/classify; if the tree doesn't compile, gate per-package until its cutover lands.
- Commit+push per unit, scoped adds only, push-first (ruling 2026-08-26-commit-push-as-you-go.md).
- Report to pij-instant-lynx: claim · files · gate output · observations. Deviations = stop-and-ask.
