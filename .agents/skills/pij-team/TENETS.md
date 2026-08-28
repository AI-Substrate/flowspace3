# pij-team — core tenets of the work

**TECH-AGNOSTIC.** This doctrine works in any repo, any language. Concrete
mechanisms below marked "(Rust example)" or "(this repo)" are illustrations of
a tenet, never the tenet itself — a new repo re-instantiates them in its own
stack (its impl-guide names the local equivalents).

**LIVING DOC — iterate it.** Every run of this pipeline must leave this file
better: when a tenet is confirmed, contradicted, or missing, edit it (log the
why in `EXPERIENCES.md`). Destination: once proven here, this doctrine
graduates into the harness and pij as first-class substrate (verbs, templates,
checks) — this file is the staging ground, not the final home.

The doctrine behind the prime→pm→coders→reviewer pipeline. Packets and
templates CITE this file by path; they do not restate it. Evidence base: the
2026-08-26 flowspace3 rebuild — one human + one o-prime + ~16 seats, empty
repo to released v0.2.0 in a day — reconstructed in `scratch/reconstruct/`
(esp. `04-architecture-enables-parallelism.md`,
`07-manifesto-ways-of-working.md`), plus `.harness/government/how-we-work.md`
and the retro record (132 drained observations).

## The prime tenet

1. **Architecture is the multiplier — you cannot parallelize what you have
   not decomposed.** Parallel agents don't create speed; the arch split
   creates the *possibility* of parallel agents, and orchestration keeps it
   from collapsing. Decomposition is an architecture activity, not a
   management activity — which is why the impl-guide, where prime does that
   decomposition, is the most important document in the pipeline. Ten agents
   in a monolith produce merge conflicts at machine speed; ten agents behind
   two frozen traits produced FlowSpace in six hours. The fleet is only ever
   as parallel as the architecture lets it be.

## Architecture tenets — what makes a unit coder-ready

2. **Freeze the seams before you spawn anyone.** Interfaces are settled in
   the impl-guide BEFORE fan-out. A frozen seam converts a design
   conversation into a work order — and a work order is the only thing you
   can hand to an agent that can't attend your design conversations. Coders
   fill contracts, never widen them: "a third port is stop-and-ask." Freeze
   the contract WITH its proof: the shared contract suite and the golden
   fixtures ship in the contracts phase, so every fanned-out unit's done is
   mechanical from its first minute.
3. **Engineer the collision surface down to one line.** Shape units so two
   coders never touch the same file except a trivial-merge point (Rust
   example: one `pub mod` + re-export line in lib.rs; other stacks: a barrel
   export, a registry entry, one line in an index). If two units must edit
   the same file, redesign until they don't.
4. **Snap-in recipe, not snap-in.** Workers ship the integration *recipe*
   (Rust example: the exact config variant + composition-root match arm as a
   doc comment; generally: the precise wiring an integrator will paste),
   never the integration itself. That deletes the one file every parallel
   worker would fight over; serialization moves from edit time to
   convergence time, where one mind (the PM) sequences it.
5. **Make "done" mechanical so acceptance scales.** Every unit's done is the
   same checkable predicate: the repo's quality gate green · standalone
   offline tests with fakes · a shared contract suite where units implement
   one seam · a service page (this repo: `harness checks`, testkit fakes
   with mocking crates refused by name, `docs/services/<name>.md`). The
   reviewer reads a diff against a contract, not a philosophy.
6. **Design change to be additive.** New rows over migrations, extras-first
   wire fields, concurrency declared not defaulted ("a default is a number
   nobody chose") — so units need no cross-team coordination over time
   either, not just across seats.

## Orchestration tenets

7. **Structure over trust.** Replace trust in agent judgment with structure
   that makes the wrong thing impossible and the right thing checkable —
   enforcement over documentation (this repo's examples: dependency
   allowlist checked in CI, contract suites, ddocs validation — any stack
   has equivalents: lint rules, import boundaries, schema validators).
   Contracts over conventions.
8. **The packet is the interface to a worker.** Fixed anatomy, delivered by
   path pointer, current-state written to be FALSIFIABLE in one read,
   constraints carrying their reasons so a worker can spot a dead premise.
   The ack-with-numbered-plan is the real control point: defects surface
   there, before the diff exists. Rule the ack by number/letter, fast.
9. **One brain per domain; context is a first-class asset.** Work returns to
   the seat that owns the domain; a new domain gets a fresh seat. The
   orchestrator's core skill is deciding whose context work belongs in.
   Closed seats stay revivable; the roster is the ownership authority.
10. **Isolate at the tree, keep convergence serial.** Worktree-per-coder
    removes edit-time hazard (the retro's largest pain cluster — ~35 of 132
    observations — was shared-tree sweeps and untrustable greens). What
    isolation does NOT remove: merge order, shared credentials/quotas, one
    database, one observation buffer, and the SHARED GLOBAL TOOLCHAIN — all
    parallel units read one installed binary, so one unit upgrading it
    invalidates the others retroactively and invisibly (they stay green and
    become wrong; worse than the named hazards because it has no edit-time
    signal). Rule: the toolchain version is pinned in wave 0, no unit may
    change it, and every done-receipt carries the version it actually
    observed (meadowlark's bun-run finding, 2026-08-28). Convergence stays
    governed by one mind. Green is necessary; merged is a decision. Fences partition WRITE
    intent, not the build — waves must respect build dependencies.
11. **Evidence, never conclusions — verify then relay.** Cite the command
    and its output. A worker's measurement outranks the brief and the
    orchestrator's diagnosis. Re-run before forwarding any claim.
12. **The human is the taste function.** Prime owns product truth, defers
    rulings up one-at-a-time (one sentence context, one sentence ask), and
    puts the product in the human's hands the moment it does anything —
    their live corrections are the highest-value dispatches of the day.
13. **The fleet builds the machine that builds the product.** Every friction
    is captured the moment it bites (`harness observe` + `EXPERIENCES.md`)
    and drained into structure — a template edit, a rule, a check — the same
    day, so dispatch N+1 inherits everything run N learned. This is why the
    pipeline is templated: templates are the improvable substrate.
14. **A measurement has preconditions, and a predicate must REFUSE when they
    fail.** A misleading zero is worse than a refusal: a leak probe that
    reads 0 under fake embeddings is measuring garbage ranking against
    garbage, and a must-be-0 gate over a population that can never be
    nonzero passes forever. Probes check their own preconditions (the right
    provider, a population that can exercise the claim, a control that
    proves the instrument works) and emit `unmeasurable-<reason>` instead of
    a number. Same law as three-valued eval rows: missing instrumentation
    never masquerades as a result. (Added 2026-08-28, leopon/006.)
15. **Composition is where EMERGENT defects appear — run the composed
    artifact.** Units can each be correct against their own assumptions and
    the pair still wrong: one unit's success can create the very population
    that exposes a sibling's gap (auto-registration made many-checkouts
    normal; many-checkouts broke scoped resolution). Neither unit's tests
    can see it, so composition includes an adversarial RUN on a realistic
    corpus — three defects in one day were caught only by running the
    composed artifact, all three invisible to passing tests.
    (Added 2026-08-28, leopon/006 + DL-004 + flea/008.)
