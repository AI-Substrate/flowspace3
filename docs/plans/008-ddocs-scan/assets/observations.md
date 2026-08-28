# Observations rescued from plan 008-ddocs-scan

Recorded verbatim from each seat's `harness observe --list` **before any worktree was tidied**, so
the fleet's wording survives teardown rather than being replaced by the PM's summary of it.

**The buffer was never cleared.** Capture is every seat's job; the drain is o-prime's alone, because
clearing destroys other seats' live observations. This file is a COPY for the plan record, not a drain.

Seats: PM `pij-driving-nigel` · u1 `pij-yielding-sparrow` · u2 `pij-supreme-tapir` ·
u3 `pij-shallow-boa` · u4 `pij-sudden-pigeon`.

---

## PM observations

### DL-001 — difficulty/degrading (PM, pij-driving-nigel)

**What happened.** flowspace3 MCP tools were unavailable to all four scout subagents; each reported the miss and fell back to grep. Dogfooding is mandated by AGENTS.md but the mandated tool is not reachable from a subagent context.

**Workaround.** Scouts used grep/read and cited file:line; the seam map is accurate but was produced by the tool the repo tells agents not to use first.

**Suggested encoding.** Either make the flowspace3 MCP server reachable from subagent contexts, or tell agents in AGENTS.md to shell out to the flowspace3 CLI (which IS reachable and worked fine from the PM seat) rather than the MCP tool.

### DL-002 — difficulty/degrading (PM, pij-driving-nigel)

**What happened.** crates/cli/src/conversation.rs mint_guid() seeds the conversation guid with turns.first(), which carries an 'at' timestamp filled by now() at second resolution when the source has no explicit timestamp. Two imports of the SAME transcript straddling a second boundary therefore mint DIFFERENT guids. Observed as a real intermittent failure of the_same_transcript_mints_the_same_guid during a full cargo test --all run; 8 subsequent isolated runs passed.

**Workaround.** Re-ran; it passed. No fix attempted - out of my plan's fence.

**Suggested encoding.** Seed mint_guid from the transcript's stable content only (role/content), excluding any field filled by now(). The test's own docstring says the whole iterative loop rests on 'the same file imports to the same conversation' - today that is true only within one second.

### DL-003 — difficulty/degrading (PM, pij-driving-nigel)

**What happened.** Canary demand 'quote your pane footer model line' is unsatisfiable in a narrow tmux pane: a spawned worker stacked in a ~49-column column renders only the context gauge, never the model segment, so the seat physically cannot quote what it is asked for. Both wave-1 coders answered with the spawn argv model string instead, which proves only what was REQUESTED. Breaking the pane into its own window made the model segment render immediately.

**Workaround.** Captured both footers myself with tmux capture-pane -t <pane>, which is stronger evidence than self-report anyway, then ran tmux break-pane -d -s <pane> -n <name> so the footer renders for the seat too.

**Suggested encoding.** how-we-work 9b already says to break the pane out immediately after spawn - make that a PRECONDITION of the canary rather than a following step, and add to the canary doctrine that the VERIFIER should capture the footer directly rather than trusting a seat to quote it. A seat quoting its own argv is the default failure and it looks like compliance.

### DL-004 — difficulty/blocking (PM, pij-driving-nigel)

**What happened.** A coder seat in its own git worktree wrote file MODIFICATIONS into the PM's worktree while creating NEW files correctly in its own - twice, byte-identical, ~90 seconds apart. Signature of an edit tool replaying absolute paths captured when the seat read contract/callsite files out of the PM tree during its ack, while creates resolve against cwd. Worktree-per-coder isolation does NOT hold if the packet points a seat at absolute paths in another tree.

**Workaround.** PM checks git status in its OWN tree at every edge; captured the leaked diff, applied it to the coder's worktree, reverted the PM tree. Work preserved, zero loss, caught within 90 seconds.

**Suggested encoding.** Coder packets must state fence paths RELATIVE and forbid opening any absolute path containing another tree's name; everything a coder needs is committed at the same relative path on its own branch. Stronger: the PM should run a dirty-tree check on its own worktree as a standing edge ritual, and pij-team should ship it as a verb - an isolation breach is invisible to the coder and only the PM can see it.

### DL-005 — difficulty/degrading (PM, pij-driving-nigel)

**What happened.** pij#19 phantom alias minted MID-UNIT, hours after the canary that settled the seat's identity: pij-coloured-xoxarle fired a ready-ping with a spawnId byte-identical to the already-canaried pij-yielding-sparrow. Two spawns, two panes (%296 pid 39459, %297 pid 44918), three registry ids, all showing working/active with identical model/effort/folder in pij list.

**Workaround.** Took no action per the standing containment rule - an alias addresses the same physical PID as a live seat, so closing it would kill the real coder mid-unit. Continued addressing only the canaried id.

**Suggested encoding.** The canary is a POINT-IN-TIME proof and this defect is not point-in-time: an id that did not exist at verification can start speaking later, and nothing notifies the PM except an unexpected ready-ping. Either mint-time adoption gating upstream, or pij should refuse to register a second id for a spawnId that is already bound - and until then the containment rule needs to be stated as permanent, not as spawn-window advice.

### DL-006 — difficulty/blocking (PM, pij-driving-nigel)

**What happened.** Shared Postgres (flowspace3-db, used by every seat in this tree) crash-reinitialised at 07:09:09Z: a backend exited with code 2, all server processes terminated, ~20s of 'not yet accepting connections'. Not resource exhaustion (16/100 conns, 224MiB/31GiB, container healthy, 0 restarts). WAL redo shows a Database/DROP at the crash point; four seats plus the PM were concurrently creating and dropping throwaway test databases on one cluster.

**Workaround.** Warned all four coders to re-run any Postgres-touching verdict from that window - a PASS is more dangerous than a failure, since a suite that loses its connection mid-run can report success over work it never did. Stopped my own sandbox-daemon probe, which creates a database on that same container and was running at the crash instant.

**Suggested encoding.** Isolation for test databases is per-DATABASE but the failure domain is the CLUSTER: one backend crash takes out every seat at once, and nothing tells them. Either give each seat its own container (cheap, removes the shared failure domain entirely), or have the test harness detect connection-loss errors ('not yet accepting connections', 'expected to read N bytes, got 0 at EOF') and FAIL LOUD as infrastructure rather than letting them surface as test failures or, worse, as passes.

### DL-007 — difficulty/degrading (PM, pij-driving-nigel)

**What happened.** PM decomposition defect: the units table was complete against every INTERFACE and still left a promised CAPABILITY unowned. The plan's goals promise 'serve the inverse index - given a source file, the ddoc rows referencing it', u3 built the store query and u2 populates the refs, but NO unit exposed it to an agent. Caught mid-wave-2 by the PM auditing its own impl-guide, not by any coder or check. u4's own eval fixture 3 ('which criteria touch this source file') could never have been evaluated.

**Workaround.** Added a narrow flowspace3 refs <path> surface to u4 mid-flight, with an explicit offer to split it into a fifth unit if it would not fit cleanly.

**Suggested encoding.** Fan-out review must walk each ACCEPTANCE CRITERION to the unit that makes it OBSERVABLE, not walk each unit to its interface. The second check passes on a units table that cannot deliver the plan. Cheap to encode: a required coverage column mapping every AC id to the unit id that surfaces it, and any AC with no unit is a hole before anyone is spawned.

---

## u1 — ddoc parser and discovery admission

### DL-001 — difficulty/degrading (u1, pij-yielding-sparrow)

**What happened.** harness boot exceeded the 120-second command deadline in the u1 worktree without returning a verdict

**Workaround.** rerun with a longer bounded deadline before editing

**Suggested encoding.** boot should stream stage progress or name its expected upper bound so a timeout is interpretable

### CONF-001 — confusion/annoying (u1, pij-yielding-sparrow)

**What happened.** pij report now without positional did and next text exits E-ARG although the packet presents the bare form as a status-card command

**Workaround.** supply explicit did and next strings

**Suggested encoding.** update packet wording to show the required positional arguments

### CONF-002 — confusion/degrading (u1, pij-yielding-sparrow)

**What happened.** flowspace3 search from the assigned u1 worktree answered from other registered checkouts and warned this checkout was unregistered while status still had queued work

**Workaround.** register the exact worktree and wait for its queue before relying on search evidence

**Suggested encoding.** agents-start-here should make checkout registration provenance an explicit pre-search gate

### CONF-003 — confusion/annoying (u1, pij-yielding-sparrow)

**What happened.** pij report now rejects semantic state working even though a coder start card naturally uses that state; the valid-state list only contains terminal or coordination states

**Workaround.** omit the state and retain did and next text

**Suggested encoding.** document that active work has no semantic state or add an in-progress state

### CONF-004 — confusion/degrading (u1, pij-yielding-sparrow)

**What happened.** flowspace3 next_action told a worker to add an unregistered git worktree even though the same repository main clone was already registered, which would pollute a shared multi-worktree index with answers about ambiguous bytes

**Workaround.** unregister the worktree immediately and treat other-checkout search only as comprehension aid

**Suggested encoding.** flowspace3 add should warn when the path resolves to a git worktree whose main clone or another worktree of the same repository is already registered, naming shared-index consequences

---

## u3 — ddoc persistence, filters and the inverse index

### DL-001 — difficulty/degrading (u3, pij-shallow-boa)

**What happened.** lean-ctx-wrapped ctx_search was unavailable while locating the U3 deterministic-doc contract

**Workaround.** used the repository grep tool fallback

**Suggested encoding.** expose ctx_search in this coding harness or document the supported lean-ctx CLI search form

### CONF-001 — confusion/degrading (u3, pij-shallow-boa)

**What happened.** flowspace3 search returned a confusing envelope with seven reported errors and warned that this U3 worktree is not registered, while still returning hits from other checkouts

**Workaround.** used the hits only for orientation and grounded the contract in the local deterministic docs

**Suggested encoding.** make mixed success/error search envelopes explain which rows remain trustworthy and provide a worktree registration preflight

### DL-002 — difficulty/degrading (u3, pij-shallow-boa)

**What happened.** Rust LSP references returned no callers for exported SearchFilters and upsert_element_tree, while exact repository search found multiple callsites

**Workaround.** used exact-identifier repository search and read every found caller

**Suggested encoding.** make worktree-aware rust-analyzer routing explicit or fail references when the requested worktree is outside the active LSP workspace

### DL-003 — difficulty/blocking (u3, pij-shallow-boa)

**What happened.** Relative edit paths resolved against the PM session worktree instead of the command cwd, so four U3 source edits landed in fs3-ddocs-scan while absolute writes correctly landed in fs3-ddocs-u3

**Workaround.** stopped at the first compile mismatch; surgically revert only those edits in the PM tree, then reapply with absolute U3 paths

**Suggested encoding.** make edit paths honor an explicit cwd or reject relative edits when the task worktree differs from the session cwd

---

## Cross-seat patterns

Three findings were reported INDEPENDENTLY by more than one seat, which is what makes them
structural rather than incidental:

- **flowspace3 answers from other checkouts.** u1 CONF-002 and u3 CONF-001, independently. A
  coder worktree is never registered (a deliberate PM ruling — four checkouts of one repo would
  pollute a shared index), so semantic search is blind to every unit in flight, which is exactly
  the code a coder most needs to ask about. u4 later hit the consequence directly: a correctly
  phrased query for the inverse index missed `rows_referencing` because that function has never
  been indexed anywhere. The ruling stands; the cost is now written down rather than rediscovered
  by each seat in turn.
- **LSP `references` returns a confident empty.** u3 DL-002 and u4 DL-002, independently, on
  different symbols. A tool answering "no references" when it means "not ready yet" is the same
  failure class as everything else this plan is about.
- **Relative edit paths resolve against the spawn-time session root.** u3 DL-003, root-caused by
  the seat itself; now fleet doctrine in three templates (cd into the worktree BEFORE spawning,
  absolute paths always, and a PM dirty-tree check at every unit edge — the breach is observable
  only from the PM's side).

---

## u2 and u4

Appended when the wave-2 seats report; both were still in flight when this file was created.
