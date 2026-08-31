# w-ask-path-scope — `ask --path <GLOB>` (backlog row 116)

## Why (Jordan, twice)

`flowspace3 search` takes `--path <GLOB>`; `flowspace3 ask` rejects it
("unexpected argument '--path'"). So a caller can ask a question of a repo,
a source, or one conversation — but cannot say "answer this using only
`crates/store/**`". Jordan asked for this on 2026-08-30 and again on
2026-08-31; row 85 shipped conversation+source pinning and missed it.

## The semantic question, ruled up front

Jordan's framing was "ask/scan all conversations in a path". Conversations
are NOT path-bearing: a conversation carries a `repo` and a `worktree`, and
its turns have no file paths at all. Only code and doc elements have paths.

**RULING (o-prime, subject to Jordan's override):** `--path` is a
CODE/DOC filter. When `--path` is supplied:

- code and doc retrieval is constrained to matching paths;
- conversations are EXCLUDED, and the coverage/composition facet SAYS SO
  in words — not silently zero. The existing scoped-zero vocabulary is the
  place for it (see row 63's `path_unmatched`); add a sibling reason so a
  reader learns "conversations carry no path, so a path filter excludes
  them" rather than concluding none were relevant;
- `--path` combined with `--source conversation` is a CONTRADICTION and is
  REFUSED with `QUERY_INVALID` before any LLM tokens are spent, naming the
  right tool for the job (`--conversation <guid>` to pin one transcript,
  `--repo` to scope conversations by repository).

The conversation-scoping equivalents already exist and must be named in the
refusal text: `--conversation <guid>` and `--repo <identity>`.

## Fix contract

1. Add `--path <GLOB>` to `ask`, matching `search`'s flag exactly in name,
   type, and glob semantics — a caller who learned it on `search` must not
   have to learn it twice.
2. Bind it the way `--conversation` is bound (this is the proven pattern,
   shipped in PR #84 — READ IT FIRST): the filter is IMMUTABLE on the
   request, every model-issued search inherits it, and the model cannot
   widen it. A contradictory scope is refused, never silently broadened.
3. Honesty: reuse/extend `path_unmatched` so an unsatisfiable glob (one
   that matches NO indexed path) says so and names the repo's actual
   layout, instead of reading as "nothing relevant" — the failure roadrunner
   hit, which cost an iteration on a glob that could never match.
4. Coverage facet names the narrowed corpus (which paths, how many elements)
   the same way the conversation pin names its transcript and turn count.
5. Refusals cost ZERO LLM tokens — validate before the chat loop.

## Acceptance

- `ask "<q>" --path "crates/store/**"` answers from those paths only, and
  every citation resolves under the glob. Mutation-checked: remove the
  binding and a citation from outside the glob appears.
- `ask --path <glob> --source conversation` → `QUERY_INVALID` before any
  provider call, naming `--conversation`/`--repo` as the right tools.
- An unsatisfiable glob returns the honest unmatched reason, not empty
  silence; mutation-checked against the generic empty path.
- `--path` absent = today's behaviour, byte-for-byte. Prove it.
- Coverage names the path scope; conversation exclusion is stated, not
  implied by a zero.
- Docs updated: `crates/cli/docs/*` for ask, and the `ask` help text.
- `harness checks` green; conventional commit (`feat:`); PR into main.

## Isolation and standing rules

Own worktree, own branch, per-seat `CARGO_TARGET_DIR`, per-run test DBs.
NEVER test against prod `:7373` (read-only dogfood reproduction is fine and
encouraged). Ack-before-code: send o-prime a numbered plan and WAIT.
Report every friction via `harness observe` AND a pij message to o-prime;
LIST your observation buffer at the end, never clear it.

Prior art to read before planning, in this order: PR #84 (the
`--conversation` binding you are copying), `search`'s `--path` handling,
and row 63's `path_unmatched` work in `w-ask-honesty`.
