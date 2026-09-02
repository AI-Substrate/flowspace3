# conv-verify coder acknowledgement

- pij id: `pij-zealot`
- spawnId: `s1788300509654-6142`
- model: `github-copilot/gpt-5.6-sol-fast-1m`
- cwd: `/Users/jordanknight/substrate/flowspace/fs3-conv-verify`
- branch: `011-conv-verify`

CANARY-OK

## Ordered-read receipt

Read in the required order:

1. `docs/plans/011-conv-verify/packet-coder.dd.md`
2. `docs/plans/011-conv-verify/plan.dd.md`
3. `docs/plans/011-conv-verify/impl-guide.dd.md`
4. `docs/plans/011-conv-verify/assets/tasks/phase-1/tasks.dd.md`
5. `docs/plans/011-conv-verify/assets/backpressure.dd.md`
6. `docs/plans/011-conv-verify/assets/inputs/pij-two-daemons.md`

Worktree root for every file operation: `/Users/jordanknight/substrate/flowspace/fs3-conv-verify`.

Environment evidence: `harness boot --json` built all targets successfully, then reported degraded because the compose `db` service is stopped. `flowspace3 doctor --json` reported the production stack healthy at `:7373`; status showed this worktree indexed and no active queue. This seat is rs-resident, so its conversation is not ingestible under req-0033. No code has been written.

## Exact `conv:` resolution-path census

Exact grep covered `crates/{cli,core,daemon,store}`; LSP references confirmed all nine callers of `read::get`. Paths that resolve or constrain a `conv:` selector/address:

1. **Parse once:** `crates/core/src/address.rs:40-44,116-117,225-258` owns the `conv:`/`#t` grammar and returns `Address::Conversation`.
2. **Standalone `get`:** `crates/cli/src/main.rs:652-669` sends the explicit address, optional `--repo`, and always `cwd`; `crates/daemon/src/http.rs:780-816` resolves that into `Scope` and calls `read::get`; `crates/daemon/src/read.rs:145-160` dispatches to `conversation_window`.
3. **The defective lookup:** `crates/daemon/src/read.rs:847-873` first queries the GUID globally, then filters the result through `conversation_in_scope`; `read.rs:935-969` collapses both a true absence and a scope rejection into `no conversation <guid> is indexed`. The existing test `crates/daemon/tests/conversation_query.rs:475-496` explicitly asserts the now-reversed behaviour (`get must not cross the repository scope`).
4. **`tree conv:`:** `crates/daemon/src/read.rs:253-260,977-996` resolves the GUID globally and does not apply caller scope. It is already address-authoritative.
5. **`ask --conversation`:** `crates/cli/src/main.rs:640-650` sends selector plus repo/path/source/cwd; `crates/daemon/src/ask.rs:267-297` calls `conversations::resolve_selector`; `crates/daemon/src/conversations.rs:284-324` applies both `scope.repo` and `scope.worktree`. Therefore an exact full GUID or `conv:` address is **not** currently authoritative from a foreign cwd; impl-guide risk #1's expectation is false.
6. **Ask's internal `get`:** `crates/daemon/src/ask.rs:577-610,637-659` guards source/path/pinned-GUID constraints, then calls the same `read::get` with its immutable ask scope. Making standalone `get` global without preserving this boundary would let an unpinned model-proposed conversation address escape the ask corpus. A pinned exact conversation must remain readable after global resolution.
7. **Search/list/remove are separate shapes:** conversation search uses anchor filters; `conversation list` is intentionally scope/filter shaped (`conversations.rs:234-264`); `conversation remove` validates a canonical GUID and deletes globally (`conversations.rs:364-386`). Neither is another explicit-address read path.
8. **Store layer:** `fs3_store::list_conversations` is the shared prefix/exact query. `ConversationSummary` has `started_at` and `turns` but no `last_turn_at` (`crates/store/src/conversations.rs:79-110,457-495`). `window`/`outline` fetch turns after resolution.

The LSP itself exposed one tooling defect during the census: references for `conversations.rs::resolve_selector` returned none although grep found `ask.rs:294`; references for `read::get` then returned nine correct sites. Recorded as `CONF-001` and `conv-verify-friction-003.md`.

## Corrections requiring o-prime ruling

1. **The packet's daemon fence misses the actual bug.** The false verdict is in `crates/daemon/src/read.rs:847-969`, not the plan summary's cited `conversations.rs:274-358` / `http.rs:304-346` list/ask-selector path. `read.rs` and its tests must be added to my fence.
2. **Ask needs an explicit invariant-preservation change.** Exact `--conversation` currently remains cwd-scoped, and changing `read::get` alone risks widening unpinned ask tool reads. Add `crates/daemon/src/ask.rs` to the fence. Recommended policy: canonical full GUID and `conv:` selectors ignore `ScopeSource::Cwd`, honor `ScopeSource::Flag`; short prefixes stay scoped for disambiguation. Post-resolution ask guards continue enforcing an unpinned corpus, while a pinned exact GUID is readable globally.
3. **The distinct verify code follows the central catalog convention.** Every `Code::new` is in `crates/core/src/catalog.rs`; ac-0003 therefore requires that file outside the stated fence. Proposed code: `FS3-E-QUERY-CONVERSATION-NOT-INDEXED`.
4. **The required success shape cannot be produced cheaply from current summaries.** `ConversationSummary` has no `last_turn_at`; deriving it via `outline()` allocates every turn. Add `crates/store/src/conversations.rs` and tests to the fence, with one exact-GUID aggregate query returning anchor fields, count, and `max(turn.at)`.
5. Existing `ScopeSource::{Flag,Cwd,All}` at `crates/daemon/src/scope.rs:42-72` already distinguishes explicit `--repo` from implicit cwd. No new scope-origin mechanism is needed.

Requested expanded fence: `crates/daemon/src/{read,ask,conversations,http,convo_ingest}.rs`, their tests, `crates/core/src/catalog.rs`, `crates/store/src/conversations.rs` and its tests, plus the already-authorized `crates/cli/**` and testkit fixtures.

## Numbered implementation plan

1. **Lock the ruling and tests first.** Add cross-worktree and cross-repo `get conv:<guid>#t1` cases, an explicit `--repo` mismatch case, and assertions for the two distinct messages/details. Replace the existing opposite-scope assertion. Capture the required mutation receipt by reverting only the lookup change and showing the cross-scope test red.
2. **Make exact addresses authoritative without erasing explicit filters.** In `read::conversation_window`, resolve the GUID globally; ignore cwd-derived scope for an explicit address, but when `ScopeSource::Flag` names a repository, classify a global hit outside that explicit scope separately from a globally absent GUID. Keep the two failures on distinct messages and details shapes.
3. **Preserve ask's corpus boundary.** Make canonical full GUID / `conv:` pin resolution ignore cwd-derived scope while retaining explicit-flag intersection and short-prefix disambiguation. Add tests proving a foreign-cwd exact pin succeeds and an unpinned ask tool cannot read a foreign conversation merely by proposing its address.
4. **Add the exact verification read model.** In the store, query one derived GUID index-wide and return `{guid, turns, repo, worktree, last_turn_at}` in one statement; no turn-vector allocation. Define the dedicated catalog code for the absent result.
5. **Implement daemon verify.** Reuse `convo_ingest::conversation_guid()` for `--harness/--session`; factor/reuse the existing `pij sessions` join for `--pij`. Return `{guid,address,turns,repo,worktree,last_turn_at}` on success. For a missing rs seat, report that the join is legacy-only and name `pij req-0033`; for a derived but absent GUID, return the dedicated code and include the GUID in details.
6. **Wire the unscopable CLI surface.** Add `conversation verify` plus client/HTTP route. Its clap struct contains only the mutually exclusive identity forms and daemon URL—no `repo`, `path`, or cwd-derived scope parameters. Add positive, absent, fake-join, and clap-rejection tests.
7. **Update help and bundled conversation/get docs.** State address-authoritative lookup, explicit-scope miss semantics, verify's exact consumer shape, and the legacy-only pij join limitation.
8. **Run every named pressure command.** Run the focused get/miss/verify/clap/fake-join tests, the mutation check, then `harness checks`; preserve exact receipts. Commit with `harness commit`, open the PR requested by tk-0104, report assumptions/deviations, and hold for o-prime's prod bounce and ac-0006/ac-0007 read-backs.

Open semantic question for the ruling: treat a zero-turn conversation row as not delivered (dedicated not-indexed result) or as success with `turns: 0` and `last_turn_at: null`. I recommend the former because the consumer asks delivered-or-not and the success contract names a last turn.

ACK-PLAN-READY — awaiting `conv-verify-prime-reply-002.md`; no code before that ruling.
