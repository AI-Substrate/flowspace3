# search-admission stop-and-ask 001 — deterministic-document CLI absent

## Blocker

Before task `tk-0101`, builder implement discipline requires marking task progress through deterministic-document verbs. Reply 001 also requires amending `plan.dd.json#ac-0005` via ddocs. This worktree has no `node_modules/.bin/` directory, `node_modules/.bin/ddocs --help` returns command-not-found, and `harness doctor --json` reports the standalone dd CLI absent. No root Node package manifest/lockfile is present at the expected paths.

Manual edits to `.dd.json`/`.dd.md` are forbidden, and dependency installation is outside my fence. I have therefore made no product or plan-document changes.

## Requested ruling

Please provide one of:

1. the canonical already-installed deterministic-document command/path for this repo; or
2. explicit permission and exact package/version/install path to restore it in this worktree; or
3. an o-prime-owned update of `ac-0005` plus `tk-0101` state while I proceed.

Recommended: option 1 if the CLI lives outside `node_modules`; otherwise option 3 avoids introducing dependency files unrelated to this unit.

## Evidence

- command: `node_modules/.bin/ddocs --help`
- result: `error: command not found: node_modules/.bin/ddocs` (exit 127)
- filesystem: `/Users/jordanknight/substrate/flowspace/fs3-search-admission/node_modules/.bin` not found
- harness observation: `DL-003` (blocking)
