# Ruling — commit and push as you go
**By**: Jordan (verbatim: "Make sure Gibbon is committing and pushing as we go. In fact, they all should be committing and pushing their work. We don't want to leave too much outstanding.") · **Recorded**: 2026-08-26 · pij-instant-lynx

Supersedes the "no commits — working tree only, o-prime coordinates" line in all standing worker briefs (w-azure-openai-kazimir, w-migrations-cicada, s002 brief; s001 already committed).

## Protocol (shared main tree, several writers)

1. Commit at every coherent unit — never leave finished work uncommitted.
2. Stage ONLY your fenced paths: explicit `git add <path>…`. **Never `git add -A` / `git commit -a`** — siblings have in-flight work in the same tree. **FILE-scoped, not directory-scoped, for anything shared** (root/crate Cargo.toml, Cargo.lock, lib.rs, arch-allowlist): a directory add sweeps sibling unstaged edits (it happened — 0962ba8/17878b6). Before committing a shared file, verify every hunk is yours (`git diff --cached <file>`); kazimir's hunk-audit is the exemplar. Same discipline for FORMATTING: never `cargo fmt --all` in the shared tree — fmt only your own packages/paths (`cargo fmt -p <crate>` / `rustfmt <files>`). **`harness commit "msg" -- <paths>` RE-ADDS the full worktree content of named paths and discards a surgically-staged index** (wren incident, DL-004) — for shared files where you must exclude sibling hunks, stage surgically then use PLAIN `git commit` (harness commit only for wholly-yours path sets).
3. **Push first**: commit, then `git push` (to `main`). Only if the push is REJECTED (non-fast-forward), coordinate a rebase window with the o-prime — `git pull --rebase` is all-or-nothing over the worktree and MUST NOT be run while siblings have unstaged work (never stash a sibling's edits). *(Amended per kazimir's field report 2026-08-26 — the original "pull --rebase then push" is unrunnable in a shared tree.)*
4. Shared-merge files (root Cargo.toml/Cargo.lock, providers lib.rs, arch-allowlist, roster): commit promptly after touching to shrink the collision window.
5. `.claude/**` stays uncommitted (standing exclusion, unchanged). `.harness/government/**` remains o-prime-only.
