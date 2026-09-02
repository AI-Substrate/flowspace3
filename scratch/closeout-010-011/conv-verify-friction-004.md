# conv-verify friction 004

The builder implement contract mandates `node_modules/.bin/dd set/get` for task state, but this worktree has no `node_modules/.bin/dd`; the first `dd get` failed with command-not-found before any code change. Captured with `harness observe`. I am locating the repo-supported deterministic-document mutation surface and will not hand-edit generated `.dd.md`/`.dd.json` files.
