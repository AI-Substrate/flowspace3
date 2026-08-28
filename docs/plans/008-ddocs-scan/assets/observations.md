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

## u2 — ddocs adapter, edges, gate semantics and degradation

Five of these are severity **blocking** and four are findings about `ddocs` itself rather than about
flowspace3. Every one arrived with a verbatim envelope and a refusal to invent the missing part;
each forced a PM ruling that amended the frozen contract.

### CONF-001 — confusion/degrading · target `flowspace3` (u2, pij-supreme-tapir)

**What happened.** flowspace3 search from fs3-ddocs-u2 warned that this checkout is unregistered, returned sibling-worktree hits, and truncated the next_action while the global queue remains pending

**Workaround.** Used repository-scoped paths and verified source files in this worktree before trusting hits

**Suggested encoding.** Register active pij worktrees automatically or provide a complete actionable next_action that names the exact add command

### CONF-002 — confusion/annoying · target `pij` (u2, pij-supreme-tapir)

**What happened.** I mistook the tmux status bar for the OMP pane footer when asked to quote the rendered footer; the PM had to capture the real OMP footer directly

**Workaround.** Accepted the PM-captured OMP footer and will state uncertainty instead of guessing between rendered status lines

**Suggested encoding.** Expose a pij verb that prints the current OMP footer distinctly from tmux status

### DL-001 — difficulty/degrading · target `harness` (u2, pij-supreme-tapir)

**What happened.** harness boot reported degraded because compose service db is not running, but this unit is explicitly required to use the already-healthy shared flowspace3-db on port 5433 and forbidden to start compose

**Workaround.** Use the packet-mandated isolated database on the shared flowspace3-db container and do not follow boot's compose start advice

**Suggested encoding.** Teach harness boot to detect the configured shared FS3_TEST_DATABASE_URL/container before requiring this worktree's compose service

### DL-002 — difficulty/blocking · target `ddocs` (u2, pij-supreme-tapir)

**What happened.** Real ddocs 0.1.0 schema show envelope contains only section name/required/type and omits nested field declarations, so parse_schema_show(json) cannot populate DdocSchemaFacts prose_fields or string_fields as the frozen u2 contract assumes

**Workaround.** Stopped before implementing or inventing schema semantics; reported measured envelope shape to PM for ruling

**Suggested encoding.** Extend ddocs schema show JSON payload with resolved field shapes or amend the adapter contract to read the resolved schema path returned by the envelope

### CONF-003 — confusion/blocking · target `ddocs` (u2, pij-supreme-tapir)

**What happened.** Observed ddocs 0.1.0 graph envelope contradicts the packet: all 175 live edges carry kind=document, so the supposed real graph-without-kind primary fixture cannot be recorded from the installed binary

**Workaround.** Preserved the verbatim live envelope and stopped before relabeling or editing it; reported versioned evidence to PM

**Suggested encoding.** Expose build provenance in ddocs version and update the consumer contract/fixtures when PR #12 edge kind ships

### DL-003 — difficulty/blocking · target `ddocs` (u2, pij-supreme-tapir)

**What happened.** Real ddocs graph edges identify only source document absolute path plus numeric JSONPath location; they do not carry the source row id/address, so edges_for(graph,file) cannot produce the stable source row address needed to attach outbound relations or DdocFileRef without also receiving the parsed tree

**Workaround.** Stopped before guessing source identity from array position; reported the exact edge shape to PM for interface ruling

**Suggested encoding.** Have ddocs graph emit source_address, or amend edges_for to consume the parsed document/tree and map JSONPath to a stable row address

### DL-004 — difficulty/blocking · target `ddocs` (u2, pij-supreme-tapir)

**What happened.** Recorded PR #12 graph has one structured file edge address src/library.ts and one nested Markdown file edge address ../handbook.md while normalized to is docs/plans/handbook.md; raw address is not universally repo-relative and cannot key the inverse index

**Workaround.** Kept normalized edge.to as target_path and reported the two-edge counterexample before applying the address-based ruling

**Suggested encoding.** Define file-edge target_path semantics explicitly in ddocs graph or emit a dedicated repo_relative_target field

### DL-005 — difficulty/degrading · target `flowspace3` (u2, pij-supreme-tapir)

**What happened.** After the worktree became auto-registered, flowspace3 search scoped to the newly committed crates/daemon/src/ddoc.rs returned FS3-E-QUERY-NO-INDEX: no content for this checkout/path, so the live watcher did not make the new adapter findable before closeout

**Workaround.** Used source-grounded tests and LSP diagnostics; did not widen search to another checkout

**Suggested encoding.** Have worktree auto-registration enqueue existing/new files immediately and expose registration-vs-indexing lag in status

---

## u4 — agent-facing surface

Note the pair CONF-001 / CONF-002: the seat recorded a finding, the PM measured it and found the
diagnosis wrong, and the seat then recorded the CORRECTION beside the original rather than editing
it away. The superseded entry is kept deliberately — a buffer that only holds conclusions that
survived is a worse record than one that shows what was believed and why it changed.

### DL-001 — difficulty/degrading (u4, pij-sudden-pigeon)

**What happened.** lean-ctx ctx_read command unavailable while reading required pij skill

**Workaround.** fell back to native read per policy

**Suggested encoding.** expose ctx_read tool or document valid lean-ctx CLI read syntax

### DL-002 — difficulty/degrading (u4, pij-sudden-pigeon)

**What happened.** rust-analyzer references returned no references for exported SearchRequest and Address despite exact callsites existing

**Workaround.** used flowspace3 semantic search plus exact text search to enumerate callsites

**Suggested encoding.** make LSP report index readiness or retry after workspace load instead of a false empty result

### DL-003 — difficulty/degrading (u4, pij-sudden-pigeon)

**What happened.** flowspace3 search for inverse-index ddoc rows referencing a source path missed the exact rows_referencing function in crates/store/src/ddoc.rs and returned unrelated read/root helpers

**Workaround.** opened the known store ddoc module from the PM-provided contract

**Suggested encoding.** index or ranking check that semantic queries mentioning ddoc inverse refs surface rows_referencing

### CONF-001 — confusion/degrading (u4, pij-sudden-pigeon)

**What happened.** worktree-built flowspace3 rejected documented global --json on add even though installed flowspace3 and bundled agent guide advertise it

**Workaround.** reran with FS3_OUTPUT=json and no --json flag

**Suggested encoding.** CLI contract test that every verb accepts the documented global output flags

### CONF-002 — confusion/degrading (u4, pij-sudden-pigeon)

**What happened.** CORRECTION to the finding above: the documented --json came from the INSTALLED current-main flowspace3 while the worktree-built branch binary legitimately lacks it; both report 0.4.0 so version does not expose build provenance

**Workaround.** smoked target/debug/flowspace3 explicitly and stopped using the bare PATH binary as branch evidence

**Suggested encoding.** require worktree agents to invoke the branch-built binary explicitly; add commit or dirty provenance to the reported version so same-semver builds are distinguishable

---

## What this record cost, and why it was worth keeping

Five seats, 29 observations, none cleared. Three findings were reported independently by more than
one seat, which is what promoted them from incidental to structural. Four of u2's are findings about
`ddocs` rather than about flowspace3, and every one of those forced an amendment to a contract the
PM had frozen — which is the strongest evidence in this plan for the rule that a brief's job is not
to be right, it is to be CHEAP TO CONTRADICT.

The buffer is SHARED across every seat in this tree. Capture is everyone's job; the drain is
o-prime's alone. This file is a copy taken before any worktree was tidied, so the fleet's own
wording survives teardown.
