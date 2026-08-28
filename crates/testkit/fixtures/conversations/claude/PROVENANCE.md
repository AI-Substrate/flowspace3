source        /Users/agent/.claude/projects/-Users-agent-substrate-flowspace-flowspace3/
                a5a5588f-0979-439f-a1bf-ddf185a089c7.jsonl                                   (session A, 15262 lines)
                a5a5588f-0979-439f-a1bf-ddf185a089c7/subagents/agent-aa8ccc51dce0404a8.jsonl (45 lines) + .meta.json
                b1d6f4fb-bd8e-4a10-a018-4205f4058b8e.jsonl                                   (session B, 260 lines)
                b1d6f4fb-bd8e-4a10-a018-4205f4058b8e/tool-results/b8e9hq4my.txt
              Both sessions are flowspace3's own cwd slug; no foreign-project directory was used.
harvested     2026-08-28 by HarvestClaude (subagent of the 005-convo-ingest PM), per assets/inputs/fixture-sanitizer-spec.md
records       223 lines total across two sessions, each a single CONTIGUOUS slice (no cherry-picking):
                a5a5588f-…jsonl  148 kept of 15262  — source lines 14067-14214
                a5a5588f-…/subagents/agent-aa8ccc51dce0404a8.jsonl   40 kept of 45 — source lines 1-40, from offset 0
                b1d6f4fb-…jsonl   35 kept of   260  — source lines 1-35, from offset 0
                b8e9hq4my.txt    whole file, byte-identical to source (0 substitutions needed)
              parentUuid chains are resolvable WITHIN each file: zero interior breaks in all three jsonl files
              (verified programmatically — every record after the first has its parentUuid defined by an earlier
              record in the same file). Only each file's first record points outside, which is unavoidable for any
              slice; session B and the subagent both start at their true offset 0, so B's first record's parentUuid
              is genuinely null.
              bytes: 265276 + 110066 + 137 + 41843 + 34821 = 452143 data (+ this file) — budget 512 KB; largest
              jsonl is 148 records, budget 200.
covers        Why two sessions: no contiguous window of <=200 records in ANY single flowspace3 session covers all six
              required rows. In session A the compaction turns sit at source lines 1627/5169/9112/11825/14067, the
              only three spilled tool-results at 9266/12491/14385, and the eleven subagent spawns elsewhere again —
              the tightest span covering compaction + subagent + spill is 328 lines. Session A therefore carries five
              of the six rows contiguously, and session B (whose head happens to open with a spilled tool-result)
              carries the sixth. Spec section 5 permits harvesting a different real session for missing coverage.

              gotcha 1 — assistant message split across MULTIPLE LINES sharing one message.id (the property the
                        reader's dedupe/merge is proven against). ELEVEN such message.ids in session A alone:
                          msg_011CeU5jHaJ6XxeQCkjTq1Uc  lines 129,130,131,133,135,137   (6 lines, one message)
                          msg_011CeU5ghKhoEoBvp2KBia3T  lines 94,95,96,98,100
                          msg_011CeU5WTE6uaCEikVXuAQhT  lines 27,28,29,31
                          msg_011CeU5YuePzTrbxz2eCaPog  lines 49,50,51,53
                          msg_011CeU5bU8KNQenfMSXaptAe  lines 63,64,65 · msg_011CeU5cUa8ui8M7VjZqiFem 71,72,73
                          msg_011CeU5daa5HF1rN6cRbQTx5  lines 83,84,85 · msg_011CeU5Y1i14ExFjMEX8Je1x 34,35
                          msg_011CeU5YSnNMCYF2XmAan4BJ  lines 45,46   · msg_011CeU5faMUdz632Vmfmgf23 88,89
                          msg_011CeU5iRtpdDrkWBDTPixCz  lines 111,112
                        Plus 2 in b1d6f4fb (msg_011CeQPkGEHNMUAXLJQTw6z8 lines 21,22,23; msg_011CeQPkcM1BRjA9XFkXbT7K
                        lines 32,33,35 — note the GAP: the blocks of one message are not always adjacent) and 8 in
                        the subagent file. Note the non-adjacent case: a reader that merges only consecutive lines
                        is wrong; merge must be keyed on message.id across the whole read.
              tool_use + matching tool_result — session A holds 20 tool_use blocks and 20 tool_results, and every
                        one of the 20 ids resolves to a pair (verified). First pairs:
                          toolu_01HvxoaPBjAJriuWu7mvTZH2  Bash  use line 29 -> result line 30
                          toolu_01VdVEakkje3X4coMCGXHXVy  Bash  use line 31 -> result line 32
                          toolu_01RTYcJWaSTF6U9AjotzSocT  Bash  use line 35 -> result line 36
              bookkeeping types ingestion must SKIP by allowlist (spec asked >=2; there are thirteen distinct types
                        across the two sessions):
                          session A: attachment(5) file-history-snapshot(18) last-prompt(20) custom-title(21)
                                     agent-name(22) mode(23) permission-mode(24) atis-latch(25) pr-link(26)
                                     file-history-delta(92) queue-operation(119)
                          session B adds: ai-title(20)  [and file-history-snapshot(3), mode(1), permission-mode(2),
                                     atis-latch(8), attachment(10), last-prompt(16)]
                        (numbers in parentheses are the first line of that type in that file.)
              gotcha 5 — compaction lands IN-SESSION as a summary USER turn, must not be dropped:
                        session A line 1, uuid e41cb57f-94c1-4ca5-9fe1-2187b8f49ef4, type "user",
                        "isCompactSummary":true, content opens "This session is being continued from a previous
                        conversation that ran out of context. The summary below covers the earlier portion…".
              gotcha 6 — one session = main jsonl + N subagent jsonl, linked by parent:
                        session A line 51 (uuid 4520aa20-275e-415f-8aaf-e02ac3624f31) is an assistant record whose
                        tool_use block has id toolu_01QLsQKU2UcUKM8c1McH8Wcr and name "Agent" — exactly the
                        "toolUseId" recorded in subagents/agent-aa8ccc51dce0404a8.meta.json
                        ({"agentType":"Explore","description":"Dig per-service coder fan-out history",
                        "toolUseId":"toolu_01QLsQKU2UcUKM8c1McH8Wcr","spawnDepth":1}). Parent<->child linkage is
                        provable from committed bytes alone, in both directions.
              gotcha 9 — large tool output spilled to tool-results/, the inline record references rather than
                        contains: b1d6f4fb line 24, uuid d824b086-3d09-469a-ab7e-2b3f0b3003b9, a tool_result for
                        toolu_01GjUdzf1vmnbQZywT4XyMNT whose content is "<persisted-output>\nOutput too large
                        (34KB). Full output saved to: …/b1d6f4fb-…/tool-results/b8e9hq4my.txt\n\nPreview (first
                        2KB): …". That file IS present in this fixture (34821 bytes). Line 35 references the same
                        file again from a later assistant turn.
sanitised     home-path rewrite: 468 (386 session A · 47 subagent · 0 meta · 35 session B · 0 tool-results file) ·
              body caps: 72 string values over 2048 bytes cut on a character boundary and suffixed
              "…[fixture-truncated]" (36 session A · 27 subagent · 5 session B), touching 26 A lines, 14 subagent
              lines and 5 B lines · credential redactions: 0
notes         Credential grep (spec rule 4, MANDATORY) over all five committed data files: 917 raw hits, 0
              credentials, 0 redactions. Every hit classified: "token" 889 — usage telemetry field names only
              (input_tokens / output_tokens / cache_creation_input_tokens / ephemeral_5m_input_tokens /
              ephemeral_1h_input_tokens / thinking_tokens / total_tokens_reminder); "sk-" 17 — substrings of
              task-id, tool-use-id, task-notification, risk-triggered; "secret" 8 — flowspace3's own config prose
              ("secrets.env", "Never log secret values"), no values; "api_key" 2 — the literal "api_key_env", an
              env-var NAME shape; "Authorization" 1 — the English word in "repair authorization". A second, stricter
              sweep for secret VALUES (sk-ant-*, ghp_/gho_/ghs_/ghu_/ghr_*, github_pat_*, -----BEGIN, AWS_*,
              "bearer <10+ chars>", key/token/password/secret followed by a 24+ char literal, JWT eyJ*, key=<12+>)
              returned ZERO matches. Status: checked, clean — not "not checked".

              Nothing was synthesised, reordered, re-indented or reformatted. The sanitiser rewrites JSON string
              literals IN PLACE over the raw bytes (it locates literal spans and splices), so no record was ever
              re-serialised: key order, whitespace and escaping outside the replaced literals are the store's own
              bytes, and byte offsets remain a meaningful cursor. All 223 lines re-parse as one JSON object each;
              all 72 truncated values are <=2048 bytes before the marker and every line is valid UTF-8, so no cut
              landed mid-character.

              FINDING 1 (not in the recipe) — subagent sidecar records carry the PARENT's sessionId. All 40 records
              in subagents/agent-aa8ccc51dce0404a8.jsonl carry
              "sessionId":"a5a5588f-0979-439f-a1bf-ddf185a089c7" — the main session's uuid, not an id of their own.
              A reader that keys conversations by sessionId will silently MERGE the subagent into its parent,
              defeating ac-0004. The discriminators that DO work are "isSidechain":true (present on all 40 records)
              and the file's placement under <session>/subagents/. Recipe §1a and gotcha 6 do not mention this.

              FINDING 2 (not in the recipe) — the store writes far more bookkeeping row types than the recipe's
              "mode, permission-mode, file-history-snapshot, attachment, queue-operation, …". These two sessions
              alone add last-prompt, atis-latch, custom-title, ai-title, agent-name, pr-link, file-history-delta,
              cost-state and system — and the two sessions do not even agree with each other (A has custom-title,
              B has ai-title). This is direct evidence for the recipe's own allowlist-not-blocklist advice: a
              blocklist written from the recipe's list would have ingested seven unknown row types as turns.

              FINDING 3 (not in the recipe) — the spill is referenced by ABSOLUTE path inside the record
              ("Full output saved to: /Users/agent/.claude/projects/-Users-agent-…/tool-results/b8e9hq4my.txt").
              After the mandated rule-2 rewrite that path exists on no machine, and even unsanitised it would be
              wrong on any other host. Readers must resolve a spill by BASENAME against the sidecar tool-results/
              directory, never by following the embedded absolute path. Worth pinning as a contract-suite
              expectation.

              FINDING 4 — the inline "Preview (first 2KB)" is a verbatim prefix of the spill file: for line 24 the
              preview and b8e9hq4my.txt share their first 1811 characters exactly (measured). A reader that ingests
              the inline record AND resolves the file without dedupe double-counts that head. Also note the record's
              size notice ("Output too large (34KB)") is the store's own rounding of the source file and is not a
              checksum — where a sanitiser rewrites paths inside a spill file the two will diverge; do not assert
              the notice against the fixture's byte length.

              FINDING 5 (contiguity vs coverage, structural) — the six required properties are spread thinly enough
              in real sessions that no <=200-record contiguous window holds them all. Any future fixture refresh
              will hit the same wall; the two-session layout is the honest resolution and the contract suite should
              expect to glob MULTIPLE <session-uuid>.jsonl files under claude/, not exactly one. Note also that
              session A has a subagents/ dir but no tool-results/, and session B the reverse — real sessions carry
              partial sidecar trees, so resolve() must tolerate either subdirectory being absent.

              Residual, deliberately NOT rewritten: the bare login name "jordanknight" still appears in captured
              `ls -la` owner columns ("drwxr-xr-x@ 3 jordanknight staff …"), never as a path. Spec rule 2 defines
              exactly two substitutions (/Users/jordanknight and -Users-jordanknight-) and both were applied
              exhaustively — zero occurrences of either form remain in any data file. Widening rule 2 to the bare
              username is left as a PM ruling rather than taken unilaterally: it is not a path and does not affect
              the fixture's machine-independence.

              No cargo, rustfmt, clippy, harness check, formatter or linter was run; nothing was committed; nothing
              outside crates/testkit/fixtures/conversations/claude/ was created or modified.
