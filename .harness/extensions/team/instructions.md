# team — pij-team delivery scaffolding

`harness team new <slug>` creates the worktree, plan branch and next-ordinal
plan folder for a new pij-team plan, and seeds the four pij-team templates
inside it. `--propose` computes the same answer and creates nothing.

It is the first step of the flow in `.agents/skills/pij-team/SKILL.md`: prime
runs this, then writes the plan, then the impl-guide, then fans out.

## What it computes deterministically

1. **The next ordinal**, from every place an ordinal can hide:
   - this clone's `docs/plans/`,
   - every registered worktree's `docs/plans/` — stat-ed first, because
     `git worktree list` is a registry, not a filesystem oracle: it names paths
     that have already been removed,
   - every **local branch head**, via `git ls-tree`. A plan folder committed on
     a branch nobody has checked out is real, claimed, and invisible to a
     filesystem walk. Without this leg two seats mint the same ordinal days
     apart and discover it at merge.
   - one level into any `archive*/` folder in each of those, so a retired
     ordinal is never reissued.

   `next = max + 1`, zero-padded to three. The envelope reports
   `scanned_worktrees`, `scanned_branches` and `highest_existing`
   (with `source: worktree | branch`), so the answer is auditable rather than
   asserted. If NO ordinal is found anywhere the verb refuses (`E_NO_ORDINALS`)
   instead of minting `001` — a repo with no plans is more likely the wrong
   repo than the first day.

2. **The worktree and branch**: `git worktree add ../fs3-<slug> -b <ord>-<slug>`
   from the main clone.
3. **The plan folder**: `harness plan new <slug> --ordinal <ord>` inside the new
   worktree, giving `docs/plans/<ord>-<slug>/` with empty plan ddocs.
4. **The templates**: `impl-guide.dd.json` and the three packets copied from
   `.agents/skills/pij-team/templates/`, each built to its `.dd.md` sibling.

## `--propose` — ask without minting

`harness team new <slug> --propose` scans, computes, and prints
`would_create` (worktree, branch, ordinal, plan folder, and every document and
rendered sibling the real run creates) — then stops. Nothing is created.

The data keys match the real run's, so the two are directly comparable: the
proposal shows exactly what will happen.

**A proposed ordinal is advisory, not reserved.** Nothing is written, so nothing
is held: between the proposal and the mint, another seat can take that ordinal —
and the mint does **not** trust the proposal. It re-scans from scratch and may
therefore mint a *different*, higher ordinal than the one you were shown. That
is the correct behaviour: reserving an ordinal would mean writing state that a
propose run is defined not to write, and a stale reservation from an
investigation that never happened is worse than a re-scan.

Use it when the answer is the deliverable — sizing an investigation, deciding
whether work deserves a plan at all. An investigation should not burn an
ordinal to ask a question.

Envelope shape: `status` is `ok` (a read-only query that answered is not
degraded), and `data.mode` is `propose` with `reserved: false`. The kernel's
status vocabulary is `ok | degraded | unconfigured | error` and is not this
extension's to widen, so the proposal says what it is in `mode`.

## Branch conventions — both are correct, do not "fix" one into the other

| branch | for | created by |
|---|---|---|
| `<ord>-<slug>` e.g. `005-convo-ingest` | a **plan** branch: the worktree a whole pij-team plan is delivered on | this verb |
| `w-<packet>` e.g. `w-team-extension` | an ordinary **packet** branch: one worker's bounded change | by hand, per `AGENTS.md` |

The ordinal prefix ties a plan branch to its `docs/plans/<ord>-<slug>/` folder,
which is what makes the ordinal scan above meaningful. Packet branches have no
plan folder and deliberately keep the `w-` prefix.

## `ddocs build` runs with cwd = the worktree root

Not a style choice. `ddocs` resolves schemas from **CWD**, not from the
document's path ancestors:

```
$ cd docs/plans/005-x && ddocs build impl-guide.dd.json
E401 schema "pij-team/impl-guide" was not found in any discovery root
$ ddocs build docs/plans/005-x/impl-guide.dd.json     # same file, from the root
ok
```

So the verb always shells `ddocs build <path-relative-to-worktree>` with
`cwd` set to the new worktree. If you refactor this, keep that invariant or the
templates land with no rendered siblings. (Captured as CONF-009.)

## What it refuses, and why

| code | when |
|---|---|
| `E_BAD_SLUG` | the slug is not kebab-case — it becomes a branch name and a folder name |
| `E_LINKED_WORKTREE` | run from a linked worktree: `../fs3-<slug>` would land beside *that* tree instead of beside the main clone |
| `E_NO_ORDINALS` | no `NNN-` plan folder anywhere — the first ordinal is created by hand |
| `E_WORKTREE_EXISTS` | `../fs3-<slug>` already exists |
| `E_BRANCH_EXISTS` | `<ord>-<slug>` already exists — the usual cause is a tidy that removed the worktree but not the branch |
| `E_TEMPLATES_MISSING` | `.agents/skills/pij-team/templates/` is absent (it is tracked; this means a stale branch) |
| `E_CORE_TOO_OLD` | the core exposes no write-side filesystem capability; `--propose` still works |
| `E_SCAFFOLD_FAILED` | something failed *after* the worktree was created — it is removed again, and `details.rolled_back` says whether that succeeded |

Every refusal that can be made before mutation is made before mutation. A
half-built worktree is worse than none.

## Related

- `.agents/skills/pij-team/SKILL.md` — the flow this verb starts
- `.agents/skills/pij-team/TENETS.md` — the doctrine the impl-guide instantiates
- `scratch/team-new-poc/validation.md` — the POC this was ported from, and the
  extension notes that shaped it
