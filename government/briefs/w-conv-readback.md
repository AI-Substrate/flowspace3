# w-conv-readback — conversations listed but not readable (P1, backlog row 100)

## Symptom (reproduced in two repos, 2026-08-30)
- `flowspace3 conversation list` — healthy: guids, titles, repos, turn counts (37 conversations).
- `flowspace3 get "conv:<guid>#t<n>"` — FS3-E-QUERY-NOT-FOUND "no conversation <guid> is indexed", for EVERY conversation, including ones readable yesterday.
- `flowspace3 search "<any>" --source conversation` — composition conversation:0 always; unscoped search healthy for code+doc.
- `flowspace3 ask ... --repo git:github.com/AI-Substrate/pij` — WORKS (retrieved turns with citations today, 179s run). Storage is intact.

## Prime suspect (verify first, do not assume)
#80 (conversations in default mixed search + composition facet + honest body-less get) added repo/worktree scoping to conversation reads. `conversation list` renders repo as `github.com/AI-Substrate/dd`; query scope meta carries `git:github.com/AI-Substrate/...`. If the new filter joins on mismatched formats (stored vs scope-normalized), every conversation fails the scope predicate — list (unscoped) still shows them, ask's explicit --repo path may normalize differently and still work.

## Fix contract
1. Root-cause with a failing integration test FIRST: fixture conversation ingested, then get + scoped search + unscoped search all return it (three assertions that fail today).
2. Normalize repo identity ONCE at a named seam (store or scope resolution) — do not sprinkle string fixups at call sites.
3. [CORRECTED 2026-08-30 after coder stop-and-ask — the original clause here was a prime paraphrase error] #80's shipped law is preserved byte-for-byte for foreign repos: scoped get REJECTS a foreign-repo conversation with its explanatory envelope (test: get_rejects_foreign_conversations_and_explains_body_less_turns). The fix scope is ONLY the repo-string normalization so SAME-repo conversations admit again. Matrix: scoped get same-repo ADMIT / foreign REJECT-with-explanation; scoped search same-repo only; unscoped search all. Making get address-authoritative is a product decision for Jordan, not this hotfix.
4. Mutation check: with the normalization removed, the new test goes red.
5. `harness checks` green; conventional commit (fix:); PR into main; note in PR that this unblocks row 100 and every fleet's conversation recovery.

## Isolation
Own worktree fs3-conv-readback, branch w-conv-readback, per-seat CARGO_TARGET_DIR, per-run test DBs (post-#70). NEVER prod :7373 for testing; read-only dogfood get/search against prod is allowed for reproduction only.
