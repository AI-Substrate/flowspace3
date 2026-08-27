# get / tree

Search finds an address. `get` reads what is AT it, and `tree` browses what is
around it. Together they are why an agent using fs3 does not need to shell out
to `cat` and `ls` after a search.

```bash
flowspace3 search "how does the queue avoid two workers taking the same job"
flowspace3 get el:git:github.com/org/repo/crates/store/src/jobs.rs::claim_job
flowspace3 tree el:git:github.com/org/repo/crates/store/src/jobs.rs
```

## Addresses

```text
el:<repo>/<path>::<container>::<name>    an element
el:<repo>/<path>                         a whole file
el:<path>::<name>                        repo-less: resolved where you stand
conv:<guid>                              a conversation — not in this version
```

Every search hit carries the address to `get` it with. Copy it; do not compose
one by hand. Addresses survive re-parses because they are path-and-name shaped
rather than line-numbered.

A repository identity contains slashes (`git:github.com/org/repo`), so the
boundary between the repo and the path is resolved against the repositories
that are actually indexed — longest match wins. That is also why a repo-less
address works: whatever is left is a path, resolved in the repository you are
standing in.

## get

| flag | effect |
|---|---|
| `--depth N` | levels of children to outline (default 1, 0 for none) |
| `--span <line>` | pick one of several elements sharing an address |
| `--repo <identity>` | resolve a repo-less address in this repository, or `all` |

Returns the element's own `raw_text` — for a file address, the whole file as
indexed, served from the file element rather than stitched together from its
children — plus its summary and tags when it has them, the `parents` chain up
to the file, and a `children` outline you can `get` next.

`meta.parser_version` names the parse that answered. Elements are keyed by
`(blob, parser_version)`, so after a parser upgrade the previous version's rows
still answer until a re-scan replaces them.

## tree

| flag | effect |
|---|---|
| `--depth N` | levels to show (default 2) |
| `--limit N` | files to list before reporting a count instead (default 500) |
| `--repo <identity>` | browse this repository, or `all` |

```bash
flowspace3 tree                          # where you are standing, or the index
flowspace3 tree crates/store             # a directory, in the current repo
flowspace3 tree el:<repo>/src/lib.rs     # one file's declarations
flowspace3 tree /abs/path/to/checkout    # an absolute path works too
```

Directories are **derived from the paths that are indexed**, not read from
disk: a directory full of files fs3 was told to ignore does not appear. That is
the honest answer for a browser over an index.

## One address, two elements

`struct Rect` and `impl Rect` are two elements at one address — by design, and
the store keys elements on `(address, span_start)` for exactly that reason. So
`get` on such an address refuses rather than flipping a coin:

```json
{ "ok": false, "error": { "code": "FS3-E-QUERY-INVALID-AMBIGUOUS",
  "details": { "candidates": ["container Rect lines 2-5 (--span 2)",
                              "container Rect lines 7-12 (--span 7)"] },
  "fix": "pick one with `--span <line>`: 2, 7" } }
```

## Scope: what "here" means

A bare `search`, `get` or `tree` is about the repository you are standing in.
The CLI sends your working directory and the daemon resolves it to a registered
root; `meta.scope` always reports what happened:

| `scope.source` | meaning |
|---|---|
| `cwd` | narrowed to the repository your directory belongs to |
| `flag` | you named `--repo <identity>` |
| `all` | every indexed repository — asked for with `--repo all`, or nothing better was known |

`scope.warnings` is where the two awkward truths are said out loud, and they
also lead `next_action` so a consumer reading only that field still sees them:

- **you are in a checkout that was never added** — but its repository IS
  indexed from another checkout. The scope narrows to the repository, and the
  warning names the checkout whose content actually answered.
- **you are somewhere fs3 has never been told about** — the answer comes from
  every other repository, and the warning names `flowspace3 add <path>`.

Silence on either is what makes a search from a fresh worktree answer, in full
confidence, from an unrelated repository.

## Errors

| code | means | status |
|---|---|---|
| `FS3-E-QUERY-INVALID-ADDRESS` | not an address at all | 400 |
| `FS3-E-QUERY-INVALID-AMBIGUOUS` | matches several elements or checkouts | 400 |
| `FS3-E-QUERY-NOT-FOUND` | nothing answers to it — `details` names what IS nearby | 404 |
| `FS3-E-QUERY-NOT-IMPLEMENTED` | a `conv:` address; conversations are not stored yet | 501 |

Not in this version: conversation storage, so `conv:` addresses parse and are
refused as not-yet rather than as errors in your typing.
