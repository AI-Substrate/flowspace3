# Brief: w-get-verb — fetch by address: the missing half of the query surface (Jordan ruled 2026-08-27)

**Seat**: (fill at canary — fresh seat; the query/read surface becomes your domain).
PR-era done-bar: own worktree + branch off main, conventional commits (`feat:`),
harness checks green (seven gates — a brand-new FS3_TEST_DATABASE_URL can fail the
test gate on its FIRST run only, re-run once; canonical value
`postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_test`), PR, report the
number, never self-merge. AGENTS.md binds (dogfood + observe). Production-database
ruling binds: tests never touch the default 5433 database.

## What Jordan ruled (2026-08-27, near-verbatim)

"In the old flowspace you can pull individual files — it's not just about search,
you can actually grab individual files." fs3 has no such verb; Jordan ruled it a
packet of its own ("a different thing altogether" from the conversations feature),
new coder.

## Doctrine to read FIRST

`docs/plans/prd/workshops/003-query-surface.md` is AUTHORITATIVE and already
designed this verb — you are implementing its contract, not designing a new one:

- **`get`** = depth on ONE address (an element with its children). Addresses are
  the universal currency: `el:<repo>/<path>::<container>::<name>`; conversation
  addresses exist in the doc but conversation STORAGE does not exist yet — build
  the dispatch so `conv:` addresses fail with an honest "not yet" error, not a
  parse error (the conversations plan will fill that arm in; do NOT build it).
- **`tree`** = structure browse (files/containers under a path/repo).
- JSON-only envelopes (workshop 004 owns the envelope shape — one envelope, `ok`
  discriminator, non-null next_action). Same surface on CLI and (when it exists)
  MCP — build the service function once, verb wraps it.

Also read `docs/services/` pages for store schema (elements/raw text live in PG —
workshop 002 put raw_text inline; a file's content is reconstructible from its
elements, or served whole if the store holds it whole — read the schema and pick
the honest mechanism, stating it in the service page).

## Deliverables

1. **`flowspace3 get <address>`**: full content of the addressed element plus its
   children (per workshop 003); flags for depth control if the doc names them.
   A whole-file ask (`el:<repo>/<path>` with no container/name) returns the file's
   content as indexed. Errors are honest: unknown repo, unknown path, ambiguous
   address each say what WAS found nearby (next_action non-null).
2. **`flowspace3 tree <address-or-path>`**: the structure outline (files under a
   root, containers/elements under a file) — the navigation companion.
3. **Bare-search cwd-scoping (workshop 003 D6) — fix or implement**: bare `search`
   is specified to scope to the current repo when cwd is inside a registered one;
   TODAY it returns cross-repo results with nothing saying the cwd repo is absent
   (reproduced twice: a search from the fs3 worktree answered entirely from the
   OLD fs2 index). Implement D6, and when the cwd is inside a repo that is NOT
   registered, say so in the envelope (a warning naming the miss + how to add) —
   that silence cost us a real confusion today (CONF-001/CONF-004 class).
4. **Docs**: service page for the read surface; agents-start-here and bundled docs
   gain get/tree the moment they exist (they change the recommended agent loop:
   search → get, not search → shell-cat).
5. **Tests, mutation-checked**: get on element/file/unknown addresses; tree on
   repo and file; D6 scoping (cwd inside registered repo narrows; unregistered
   cwd warns); conv: address answers the honest not-yet error. Fake providers,
   explicit test DB.

## Out of scope

Conversation storage/windowed turn fetch (separate packet builds on your `conv:`
dispatch arm). MCP server (surface parity is a design constraint, not a deliverable).
Human-readable rendering (JSON-only per D5).
