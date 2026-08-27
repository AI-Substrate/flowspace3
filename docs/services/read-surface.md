# The read surface — `get` and `tree`
**Built**: 2026-08-27 (worker pij-clumsy-tick, w-get-verb) · **Authority**: [workshop 003](../plans/prd/workshops/003-query-surface.md) (get/tree, D6), [workshop 004](../plans/prd/workshops/004-envelopes-and-errors.md) (envelope, codes) · **Code**: `crates/core/src/address.rs`, `crates/store/src/read.rs`, `crates/daemon/src/{read,scope}.rs`, `crates/cli/src/main.rs` · **Tests**: `crates/daemon/tests/read_surface.rs`, `crates/store/tests/pg_read.rs`, unit tests in `address.rs` and `read.rs`

Search answers *what is nearest to this question* and returns lean rows.
This is the other half: **fetch by address**. `get` reads what is AT an
address; `tree` browses what is around it. The reason the packet exists, in
Jordan's words: "in the old flowspace you can pull individual files — it's not
just about search". Without it, an agent that just found the perfect hit has to
leave fs3 and go `cat` a file it has to guess the checkout of.

```bash
flowspace3 search "how does the queue avoid two workers taking the same job"
flowspace3 get el:git:github.com/org/repo/crates/store/src/jobs.rs::claim_job
flowspace3 tree el:git:github.com/org/repo/crates/store/src/jobs.rs
```

## The shape

```text
CLI verb ──► GET /get   ──► daemon::read::get  ──┐
                                                 ├─► fs3_store::read (ref layer → content layer)
CLI verb ──► GET /tree  ──► daemon::read::tree ──┘
                    │
                    └── daemon::scope::resolve  (D6: which repository is this about)
```

One service function per verb, wrapped by the HTTP route, wrapped by the CLI —
so an MCP surface later calls the same function with the same parameters
(workshop 003's parity property) rather than reimplementing the resolution.

`sqlx` never leaves `fs3-store`; the daemon gets typed reads. No new tables and
no migration: everything here is a different question asked of the schema
workshop 002 already built.

## Addresses are resolved, not parsed

```text
el:git:github.com/AI-Substrate/flowspace3/crates/store/src/lib.rs::migrate
   └────────── repository identity ───┘ └────── element address ──────┘
```

A repository identity contains `/`, so **nothing in the text says where the
boundary is**. `fs3_core::address` therefore splits the scheme purely (that is
unit-testable with no database) and hands the remainder to a resolution step
that matches it against the identities the store actually holds, longest first.
A prefix only matches on a segment boundary, so `git:github.com/org` never
claims an address belonging to `git:github.com/org-labs`.

The repo-less form (`el:<path>::<name>`) is legal and must be: search itself
emits one for content no live path holds any more, and `get` has to eat what
search emits. A repo-less address resolves in the repository you are standing
in, which is also what makes an address copied out of a log usable.

`fs3_core::element_address` is the ONE renderer. Search prints with it and the
read surface accepts with it, so what is printed and what is accepted cannot
drift apart.

## Where the content comes from (the honest mechanism)

A whole-file `get` returns the file **whole**, out of the file-root element's
`raw_text`. The scanner stores the entire source on that root
(`fs3_parsers::file_element`) and migration 0004 keeps `raw_text` inline, so
nothing is stitched together from children.

That distinction is not cosmetic. A reconstruction from child elements would
silently drop everything BETWEEN declarations — imports, module docs, the
comment explaining the hack — and the result would look like a file while being
a different file. The test asserts byte equality with what is on disk.

A named element is served the same way, from its own row.

## Two ways an address is legitimately not one thing

**One address, two elements.** `struct Rect` and `impl Rect` share one address
by design — workshop 002 keys elements on `(address, span_start)` precisely
because the scanner emits both. `get` refuses rather than flipping a coin, and
the failure lists every candidate with its kind and span:

```json
{ "code": "FS3-E-QUERY-INVALID-AMBIGUOUS",
  "details": { "candidates": ["container Rect lines 2-5 (--span 2)",
                              "container Rect lines 7-12 (--span 7)"] },
  "fix": "pick one with `--span <line>`: 2, 7" }
```

`--span <line>` is the disambiguator (granted at ack time, 2026-08-27). It is a
flag rather than new address syntax: the address grammar is workshop 003's and
is shared with search, MCP and conversations, while a flag is local to the verb
that needs it.

**One path, two checkouts.** The same repo-relative path exists in every
checkout that holds it. Identical bytes are not a choice — every candidate
answers the same thing. Different bytes ARE, and the only non-arbitrary
tiebreak is where the caller is standing; failing that, the candidates are
reported rather than one being picked silently.

## Parser versions: no silent cliff after an upgrade

Elements are keyed by `(blob_sha, parser_version)`. The instant `PARSER_VERSION`
is bumped, the new version has no rows until a re-scan — so a reader that only
ever asked for the current version would answer `NOT FOUND` for **every address
in the index** during that window, with the data sitting right there.

So the current version is preferred, the most recently written one is the
fallback, and `meta.parser_version` (+ `parser_version_current`) always says
which parse answered. The store returns versions ordered by `max(id)`, which is
"most recently written", not lexical order.

## D6: what "here" means

Workshop 003 D6 says a bare `search` scopes to the repository the caller is
inside. It was not implemented, and the miss was measured rather than imagined:
a search run from a flowspace3 worktree returned five of five hits from an
unrelated older index, with **nothing in the envelope saying the current
repository was absent** (reproduced 2026-08-27, CONF-001/CONF-004 class).

The daemon cannot see the caller's directory — it has one of its own and it is
never yours. This is the same trap the CLI already closes for `add` by sending
an absolute path, so `cwd` is now a parameter on `search`, `get` and `tree`.

Resolution order, in `daemon::scope::resolve`:

| situation | scope | what is said |
|---|---|---|
| `--repo all` | every repository | nothing |
| `--repo <identity>` | that identity | a warning if it is not indexed at all |
| cwd inside a registered root | that root's repository | nothing — the healthy case |
| cwd in a checkout of an indexed repository | that repository | **which checkout actually answered**, plus `add <cwd>` |
| cwd somewhere unindexed | every repository | that it is unindexed, plus `add <cwd>` |
| no cwd sent | every repository | nothing |

The ancestor match is its own store function: `find_worktree` is exact-path (a
`scan` needs the root registered at exactly that path), while scoping needs the
root a caller is standing *somewhere inside*. Longest root wins, so a nested
registration beats the outer one. The boundary test is
`left(path, length(root) + 1) = root || '/'` rather than a `LIKE`, because a
root path containing `_` or `%` would otherwise behave as a wildcard —
`/srv/repo` must never claim `/srv/repo-two`.

**Warnings lead `next_action` as well as living in `meta.scope.warnings`.**
Workshop 004 says a consumer ignoring `meta` still works, which is exactly why
a warning cannot live there alone: the miss is invisible in `data`, so a
consumer reading only `data` and `next_action` would never learn of it.

Scoping also selects the MODEL, not just the rows — `embedder_for` is resolved
per repository, and vectors are only comparable within one model's space. A
search inside a repository with its own embedder is answered by that embedder,
whether the repository was named or inferred.

## Errors

| code | means | status |
|---|---|---|
| `FS3-E-QUERY-INVALID-ADDRESS` | not an address at all | 400 |
| `FS3-E-QUERY-INVALID-AMBIGUOUS` | matches several elements or checkouts | 400 |
| `FS3-E-QUERY-NOT-FOUND` | nothing answers to it | 404 |
| `FS3-E-QUERY-NOT-IMPLEMENTED` | a `conv:` address | 501 |

Every `NOT FOUND` names what WAS found nearby: an unknown element carries the
addresses the file does hold (`details.found_here`), an unknown path carries
its neighbours (`details.nearby`), and a path that exists in a repository the
scope excluded says so and names `--repo all`. A 404 that only says "no" makes
the caller guess, and guessing is what search exists to stop.

`-NOT-IMPLEMENTED → 501` is a new arm on workshop 004's mechanical status
mapping (approved 2026-08-27). The mapping stays mechanical — status comes from
the code's spelling and an endpoint author still chooses nothing — but a valid
`conv:` address answered with 500 would say "fs3 broke" when the truth is "that
feature is not built yet".

## The `conv:` arm

Conversation addresses PARSE (`conv:<guid>`, `conv:<guid>#t42`) and are refused
as not-yet, with the guid in `details`. The conversations packet fills this arm
in; it does not have to invent the dispatch, and until then "not yet" is
distinguishable from "you typed it wrong".

## Gotchas discovered

- **Two byte-identical fixtures are ONE fixture as far as vectors go.** A test
  that indexed two repositories with the same content and expected hits from
  both got hits from one: enrichment and embeddings are keyed by the hash of
  the text (workshop 002 D2), so the shared content has one vector, resolved to
  one representative path. Fixtures that need to be two repositories must
  differ in their bytes.
- **A folder with no remote gets a `path:` identity**, so "this is a checkout
  of `path:/tmp/thing` which is not indexed" is a sentence that teaches nobody
  anything. Only a remote-derived identity is worth naming in that warning.
- **`Query<T>` on axum means every parameter is a string**, so the verbs take
  `Option<String>`/`Option<u32>` and validate in the service function, where
  the failure can carry a catalog code instead of axum's own rejection.
- **Directories are derived, not stored.** The ref layer holds paths; a
  directory is a prefix several of them share. So `tree` shows what is INDEXED,
  and a directory full of ignored files simply is not there — the honest answer
  for a browser over an index rather than a filesystem.

## Verify

```bash
docker compose up -d
export FS3_TEST_DATABASE_URL='postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_test'
cargo test -p fs3-core --lib address                 # address parsing/resolution
cargo test -p fs3-store --test pg_read               # the ref-layer reads, prefix boundaries
cargo test -p fs3-daemon --test read_surface         # get/tree/D6/conv, through the real router
harness checks
```

Mutation-checked at build time: neutering the cwd scope (`repo: None`) fails
`a_search_inside_a_registered_root_scopes_to_that_repository`; making ambiguity
return the first match fails `one_address_two_elements_is_ambiguous_and_span_picks_one`;
truncating the returned text fails `get_on_a_file_address_returns_the_file_as_indexed`.

## What is deliberately not here

Conversation storage and windowed turn fetch (a separate packet builds on the
`conv:` dispatch arm). An MCP server — parity is a design constraint here, not
a deliverable. Human-readable rendering: JSON only, per workshop 003 D5.
