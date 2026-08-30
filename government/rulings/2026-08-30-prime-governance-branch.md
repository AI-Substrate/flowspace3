# Ruling: prime-governance branch — governance lives off main

**Jordan, 2026-08-30, verbatim intent**: "have a new branch called… prime
governance… where we can keep our PRDs and our plans and everything that the
prime writes… you can just commit and push there all day, every day… no need
to really merge up to main… It's got nothing to do with our production code.
It's entirely a prime concept." Ruled GO ("Q1: yes - do it").

**The shape (o-prime design, accepted in iteration):**
- Orphan branch `prime-governance`, permanently checked out at
  `../fs3-governance` (a standing worktree — absolute paths in packets point
  there).
- Prime commits and pushes to it freely — no PRs, no CI, `harness commit`
  still applies.
- MOVES there: `.harness/government/**` (rulings, briefs/backlog, roster,
  settings, canaries, handovers, reviews), `.harness/records/retro/**`,
  `docs/plans/prd/**`, prime dossiers from `scratch/`.
- STAYS in main: `docs/plans/<ord>-<slug>/` plan folders (they ride code
  PRs and the-flow archival), anything CI/gates read, product code and docs.
- Main keeps a stub `.harness/government/README.md` pointing here.
