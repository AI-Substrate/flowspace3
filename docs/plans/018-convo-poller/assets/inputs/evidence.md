# Evidence — why plan 018 exists (measured 2026-09-05/06 on prod, o-prime)

1. **Nothing schedules conversation ingest.** `crates/daemon/src/convo_ingest.rs:249`
   says ingest "is fired from HARNESS HOOKS"; no hook anywhere calls it
   (`~/.claude/settings.json` hooks: pij bind, git-ai checkpoint, harness-hook,
   chainglass — none mention fs3). `crates/cli/src/conversation.rs:6`: "the live
   git-ai/harness submitter is a separate future packet" — never built. Every
   conversation in the index got there because a human typed the verb.
2. **Measured rot.** pij-minor-unicorn's session `cbe08128`: indexed at 22 turns
   (last_turn_at 2026-09-01T23:50) while its jsonl grew to 984 lines over three
   days; one manual ingest → 316 turns in ~30s. grim-gurgeh's omp session
   `01a06e89-dd16…`: indexed at 110 turns on 09-04 22:35; file now 13.1 MB
   (09-06 10:31) and still 110 turns. Fleet: 106 claude session files touched in
   7 days; 109 conversations indexed in TOTAL, ever; newest before intervention
   dated 09-02.
3. **The metrics-db route opens a file that does not exist.**
   `convo_ingest.rs:538` → `~/.git-ai/metrics.sqlite3`; git-ai writes
   `~/.git-ai/internal/metrics-db` (4.9 GB, live). Proved: ingest ACCEPTED, then
   the job failed `FS3-E-QUERY-INVALID … cannot open … read-only`. This is the
   ONLY store copilot sessions exist in (`conversation_join.rs:89`). Fixture
   provenance names the real path (`testkit/fixtures/conversations/metrics_db/PROVENANCE.md:4`).
4. **A caller-supplied id can name a session with no file yet.** pij rewrites
   `harness_session` on every omp session switch and omp persists lazily
   (unicorn, source-backed, 09-06). fs3 `verify` then answers
   `FS3-E-QUERY-CONVERSATION-NOT-FOUND` with `fix: run ingest` — the wrong
   instruction; there is nothing to ingest. The poller must key on FILES.
5. **git-ai is not a substitute** (assets/inputs/review-git-ai-fidelity.md):
   high-fidelity when it captures, but a 10-day-old prime session silently
   stalled (watermark < size, errors 0), a 13-tool-call session never captured,
   omp dead since 09-02, and the sweep never visits `~/.claude/projects`.
   Native stores are primary; git-ai is the copilot store + a cross-check.
6. **Existing machinery to reuse, not rebuild:** `Reconcile` trait + `run_forever`
   (`reconcile.rs`, five-second shared cadence, tick-counting for slower loops as
   in `retention.rs`); `convo_ingest::submit_after` (idempotent enqueue, dedupe
   key = address@folder); `discover_folder`/`cwd_of` (session file → workspace);
   `tail.rs` durable byte-offset cursor via `fs3_store::ingest_cursors`; readers
   `ClaudeSource` (main + `subagents/` sidecars) and `OmpSource::from_home`.
