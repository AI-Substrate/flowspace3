# Conversations — turns as first-class indexed content

Agent and human conversations are stored turn by turn, anchored to a repository,
enriched through the same summarize/embed lanes as code, and queried through the
standard surface. Workshop 005 (`docs/plans/prd/workshops/005-conversations.md`)
is authoritative for every design decision here; this page is what was BUILT and
what building it taught.

The user-facing guide is bundled in the binary: `flowspace3 docs get
conversations`. This page is the engineering one.

## Shape

```
CLI                    daemon                     store
conversation import →  POST /conversations     →  conversations + turns
                       (shape, then append)       + elements(kind='turn')
                       enqueue delta only      →  summarize / embed lanes
search --source conversation → kind filter     →  embeddings_1024
get conv:<guid>#t<n>   → window()              →  turns, by primary key
tree conv:<guid>       → outline()             →  turns, lean columns
```

Two ref-layer tables (migration 0013) and ONE join key. `turns.blob_sha` is the
content address of a turn's canonical stored form, and the same value is the
`blob_sha` of an `elements` row of kind `turn`. Everything expensive hangs off
that element exactly as it does for code: one summary, one pair of vectors, one
spend guard, one collector.

## Key decisions, and why

**Turns are elements.** There is no parallel conversation pipeline. Search, the
spend guard and GC all root at `elements`, so a turn that is not an element is a
turn nothing can find, enrich or collect.

**The canonical stored form is ONE text**, and it carries content only — no
timestamp, no role, no `head_sha`. It is what the summariser reads, what the raw
vector embeds, what the content hash addresses, and what a below-gate turn
displays. Excluding metadata is what makes two byte-identical turns hash
identically and share one paid enrichment; agents repeat themselves constantly,
so that is most of the saving.

**The anchor is a pointer with NO foreign key.** Workshop 005's DDL sketch had
`repo_id BIGINT REFERENCES repos(id)`, and it could not hold:
`roots::remove_root` deletes the repos row once its last worktree goes, so a
single anchored conversation would have turned every `flowspace3 remove` of that
repository into a foreign-key violation. Storing `repo_identity` as TEXT also
re-links the anchor for free when a repository is re-added, and answers for
repositories fs3 was never asked to index. The cost — no referential integrity
on the anchor — is the price of a pointer, and it is the same bargain the
content layer already makes with `blob_sha`.

**Turn elements use a reserved `parser_version`, `conversation/1`.** They are
rootless, and `get_elements` refuses a `(blob_sha, parser_version)` pair without
exactly one file root — so a canonical form that happened to hash equal to a
source file's blob would have turned that file's next scan into a corruption
error.

**The size gate is BYTES, not lines.** A turn occupies exactly one position in a
sequence, so a line floor cannot tell a five-word "ship it" from the same turn
carrying a 4KB tool result. `indexing.turn_summary_min_bytes`, default 256.

**Conversation is a content-source filter, not a vector space.** Default search
ranks code, document, and conversation elements together. `--source code`,
`doc`, or `conversation` narrows by element KIND; `all` is the explicit default.
Raw and smart remain internal vector spaces, and `match_field` says which won.
The pre-limit scored set also produces `composition`, so threshold-matching
turns remain visible as a count when file hits occupy the returned top-k.

## Gotchas discovered

**A deduped turn resolves to its code twin.** The search resolver takes the
lowest-id element carrying a raw hash, so a turn quoting a line of code — which
the dedupe makes common — resolves to the CODE element unless the kind predicate
is bound in the resolution join AS WELL AS the candidate CTE. Nothing errors;
the answer is quietly the wrong shape, with an `el:` address for a turn.
Mutation-checked by `a_hit_on_text_shared_with_code_resolves_to_the_turn`.

**A repo filter would have erased conversations entirely.** The search CTE
reaches a repository through `elements → worktree_files → worktrees → repos`. A
turn element has no `worktree_files` row and never will, so `--repo` returned
NOTHING for conversations, silently, while workshop 005 promises anchor filters
compose. The CTE now has a second leg through `turns → conversations`.

**A stored turn must be a ROOT of reference.** The same absent `worktree_files`
row means GC's level-1 predicate reads every turn element as unreferenced. All
five reference sites in `store/src/roots.rs` gained a second leg, written once
as a macro `concat!` splices at compile time — five copies is five chances to
drift, and drift there deletes paid LLM output for content that is still live.

**Removing by blob would take a twin with it.** `delete_conversation` matches
turn elements by ADDRESS prefix, because `blob_sha` is shared by construction.

**A 512-byte cut lands mid-character eventually.** `String::truncate` panics
there, and in intake that loses the whole batch rather than one tool result.

## How to verify it works

```bash
export FS3_TEST_DATABASE_URL=postgres://…/your_scratch_db   # never the default
cargo test -p fs3-store  --test pg_conversations
cargo test -p fs3-daemon --test conversation_intake
cargo test -p fs3-daemon --test conversation_query
```

End to end, against a running daemon:

```bash
flowspace3 conversation import ./transcript.jsonl
flowspace3 status                       # wait for the queue to drain
flowspace3 search "<a moment you remember>" --source conversation
flowspace3 get conv:<guid>#t<n> --before 5 --after 5
flowspace3 tree conv:<guid>
flowspace3 conversation list
flowspace3 conversation remove <guid> && flowspace3 gc
```

## Code pointers

| what | where |
|---|---|
| tables, kind CHECK, indexes | `crates/store/migrations/0013_conversations.sql` |
| domain types, canonical form | `crates/core/src/conversation.rs` |
| store flows | `crates/store/src/conversations.rs` |
| the reference predicate | `crates/store/src/roots.rs` (`held_by_a_live_root!`) |
| kind + anchor filters | `crates/store/src/embeddings.rs` (`search_elements`) |
| intake, payload policy, identity | `crates/daemon/src/conversations.rs` |
| enqueue for turns | `crates/daemon/src/enrich.rs` (`enqueue_for_turns`) |
| window and outline arms | `crates/daemon/src/read.rs` |
| the `--source` mapping | `crates/daemon/src/search.rs` |
| the importer and its dialects | `crates/cli/src/conversation.rs` |

## Not built (by ruling)

Automatic capture — the live git-ai/harness submitter is a separate packet
against the same endpoint · conversation-level rollup summaries · thinking
blocks · element splitting of long turns.
