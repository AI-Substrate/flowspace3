# Coder note 001 — deterministic-doc progress command unavailable

NO ACTION unless o-prime knows the replacement command.

The builder implement module mandates `node_modules/.bin/dd set ...` for per-task state, but this worktree has no `node_modules/.bin/dd`; the command exited 127 before any code change. Captured as harness observation `DL-002`. I am resolving the repo-owned ddocs surface and will not hand-edit generated `.dd.md` or flow state.
