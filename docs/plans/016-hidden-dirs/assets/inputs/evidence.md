125. **dot-directories are excluded at INDEX time, so `--path .pi/**`
    (and any hidden tree) is unsearchable — and the honest
    `path_unmatched` hides the real cause** (pij coder via weasel, plan
    124 DL-002, 2026-09-02; characterised read-only by lynx). Measured
    against the pij root: `worktree_files` rows with path LIKE '.pi/%'
    = **0**, `tree .pi` → NOT-FOUND, while `.pi/extensions/` holds real
    source on disk (file-watch-notify, image-see, minih-workbench…). So
    the glob is not the defect — the scanner never indexed the tree.
    `.pi/extensions` is pij's actual product surface for extensions;
    `.harness/` (our governance + extensions), `.agents/skills/`,
    `.github/workflows/` are the same class: dot-prefixed directories
    that ARE the codebase. Blanket hidden-dir exclusion is wrong for
    agent repos. ENCODING: (a) index dot-directories by default, keeping
    an explicit deny-list (`.git`, `target`, `node_modules`, and
    `.gitignore`-derived); (b) when a --path glob matches nothing, the
    `path_unmatched` detail should say WHETHER the prefix exists on disk
    but is excluded by an index rule — "not indexed (hidden dir rule)"
    vs "no such path" — row 119's two-messages principle again; (c) a
    `flowspace3 tree` row / doctor line listing what the index rules
    skipped for this root. Detail file:

## Code facts (o-prime, 2026-09-02, main 82f60ec)
- DiscoverySettings.include_hidden: discovery.rs:222 (field), :251 (default false), :829 (.hidden(!include_hidden) on the walker).
- Only two constructions, both from the GLOBAL config: roots.rs:178 and watch.rs:275 — DiscoverySettings::from(&state.config.scan). No CLI flag, no per-root storage. worktrees table: id, repo_id, root_path, ref_name, added_at.
- pij repo: 667 .ts files, 100 outside dot-directories, 563 under .pi/. Prod worktree 263 for pij holds 100 TS paths. Row-147 receipt: TS elements are searchable once indexed; the extensions are simply never discovered.
