# Process feedback — pij-team run #3 (plan 007), written as it happened

The prototype-improvement duty (packet i9), kept as a running log rather than
reconstructed at the end, because the second half of a run never remembers what
the first half cost. PM seat: `pij-near-carp`. Prime: `pij-instant-lynx`.

## What the templates got RIGHT, and should not be touched

1. **The ack as the control point.** Both real defects in this plan surfaced at
   an ack, before a line of code existed: the payload-DTO fence violation (mine,
   to prime) and the fixture-to-live swap seam (u-t's, to me). Neither would have
   been cheap to find in a diff. The template's insistence on "ack with a
   NUMBERED plan, no code first" is the highest-value line in it.
2. **Current state written to be falsifiable.** The PM packet's claims were
   checkable in one read (`worktree exists; plan committed at f3cc640`), and one
   of them — the impl-guide's zero-shared-files claim — turned out to be FALSE
   and was caught in twenty minutes because it was stated precisely enough to
   test.
3. **Constraints carrying their reasons.** Every constraint I inherited told me
   why, so when the "seeded testkit store" constraint met a repo with no
   deterministic store, I could satisfy the REASON (determinism for a byte
   witness) rather than the letter, and say so with evidence. A reasonless
   constraint would have produced either a bad golden or a stall.

## What was MISSING and cost time

1. **The impl-guide claimed zero shared files without checking the types.** It
   named the unit paths correctly but not the payload DTOs the renderer must
   read, and those lived inside another unit's fence
   (`crates/daemon/src/search.rs`). Suggested encoding: the impl-guide template
   should require a line per unit naming **what the unit READS that it does not
   own** — consumption is where fences actually collide, not production.
2. **No template line about pre-existing PROOF that must survive.** The plan's
   invariant was "the envelope must not move", but nothing in the packet
   template asks a coder to name the check that proves the invariant still
   holds. I added it by hand to all three coder packets ("if
   `envelope_goldens` goes red you changed the agent contract, STOP"). That
   should be a template field: *the tripwire, and what a red one means*.
3. **The canary asks a seat for an identity it cannot see.** All three fresh
   seats mis-stated their own pij id — two said "unknown", one gave its worktree
   name as `pijId` and its branch as `spawnId`. `pij whoami` exists and is the
   canonical answer (prime, 2026-08-28); the canary instruction should say "run
   `pij whoami` and quote its output" rather than "tell me your id".
4. **Nothing in the templates says where SEARCH resolves.** Every coder works in
   a worktree; the index covers the main clone; so every hit resolves to a path
   outside the seat's tree, and one coder lost a search to it before reporting.
   Until plan 006 lands, packets should carry the two-step explicitly: use the
   hit to find the FILE, then read the same relative path in your own worktree.

## Frictions captured this run (`harness observe`, buffer NOT cleared)

| id | what |
|---|---|
| DL-001 | `harness boot` reports compose "not running" in every worktree — `docker compose ps` is cwd-scoped, the shared db is up. Trains seats to ignore a red stage. |
| DL-002 | `pij report now` rejects a >280-char field only after the command runs, with no length in `--help`. |
| DL-003 | A test stub server copied from the established `ping.rs` pattern blocks forever in `accept()` when the case never connects — cost a 900s and a 240s timeout. Encode a bounded stub helper in `fs3-testkit`. |
| DL-004 | Seats cannot state their own pij id (see above). |
| CONF-001 | `ddocs` schema discovery does not walk up to the repo root; `ddocs set` fails from inside a plan folder and works from the root. |
| (coder) | `lean-ctx ls` on a Cargo registry path returned the repo tree instead. |
| (coder) | LSP `references` on a freshly-added exported symbol returned none while a real callsite existed. |

## Decisions worth stealing for the next run

- **Freeze the proof, not just the interface.** The byte-goldens were captured
  from the PRE-PLAN binary through the SAME harness that later asserts them. A
  witness minted by the code it polices is not a witness, and the harness
  refuses to let a capture run report success.
- **Write the collision map into every packet, identically.** Three coders in
  one crate, each told the same line-level map of who may touch which region of
  `main.rs` and `Cargo.toml`. Nobody had to guess what a sibling was doing.
- **Name the DECLINED options in a grant.** u-r's allowlist grant lists the four
  crates it may add AND the ones already considered and rejected, with reasons,
  so a crate choice cannot be silently re-litigated mid-unit.
- **A criterion checked before its risk has passed is a criterion nobody
  re-checks.** ac-0002 was provable at the end of phase 1 and was deliberately
  left unchecked, because it must hold after the renderer lands.
