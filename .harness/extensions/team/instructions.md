# team — pij-team delivery lifecycle

Two verbs, one lifecycle:

| verb | does |
|---|---|
| `harness team new <slug>` | creates the worktree, plan branch and next-ordinal plan folder, and seeds the four pij-team templates inside it |
| `harness team tidy <slug>` | rescues the worktree's observation buffer, then removes the worktree, its branch and its docker volumes when the plan lands |

Both preview with the same flag: `--propose` (and, on `tidy`, its `--dry-run`
alias) computes the same answer and touches nothing.

`new` is the first step of the flow in `.agents/skills/pij-team/SKILL.md`: prime
runs this, then writes the plan, then the impl-guide, then fans out. `tidy` is
the last step, after the PR lands.

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
| `E_BRANCH_EXISTS` | `<ord>-<slug>` already exists — the usual cause is a **hand**-tidy that removed the worktree but not the branch. `harness team tidy <slug>` clears both (and the stale registry entry) in one call; it is the reason this refusal should now be rare |
| `E_TEMPLATES_MISSING` | `.agents/skills/pij-team/templates/` is absent (it is tracked; this means a stale branch) |
| `E_CORE_TOO_OLD` | the core exposes no write-side filesystem capability; `--propose` still works |
| `E_SCAFFOLD_FAILED` | something failed *after* the worktree was created — it is removed again, and `details.rolled_back` says whether that succeeded |

Every refusal that can be made before mutation is made before mutation. A
half-built worktree is worse than none.

## `harness team tidy <slug>` — the teardown counterpart

The mirror of `new`. It exists because its absence was already costing us: the
ordinal scan above stats every path precisely because a hand-tidy leaves the
registry naming trees that are gone, and `E_BRANCH_EXISTS` names the orphaned
branch as its usual cause. Both are scars from work being minted with one
command and removed with four.

```bash
harness team tidy conversation-ingest --dry-run   # or --propose
harness team tidy conversation-ingest             # refuses if anything is at risk
harness team tidy conversation-ingest --force --remote
```

### What it does, in order

1. **Rescues the observation buffer — before anything else.**
2. Removes `../fs3-<slug>` and prunes the worktree registry.
3. Deletes the local `<ord>-<slug>` branch (`--remote` also deletes
   `origin/<ord>-<slug>`).
4. Drops **zero-link** `fs3-<slug>_*` docker volumes, by name.

`target/` needs no special handling: it lives inside the worktree, so it goes
with the tree. **Verified, not assumed** — `git worktree remove` does *not*
refuse over gitignored files (tested with a 2 MB `target/debug/` artifact in an
otherwise-clean tree: exit 0, no `--force` needed).

### The buffer rescue — why it is step one

`<worktree>/.harness/temp/agent/session-buffer.md` is the one thing in a
worktree that is neither committed nor regenerable: it is gitignored by
construction, so removing the tree destroys a seat's observations silently.
That is **DL-027**, and it is encoded here rather than remembered:

- the rescue runs **before any mutation and before any refusal that would
  abort** — a tidy that refuses on a dirty tree has still saved the buffer,
  because the next thing the operator does is re-run with `--force`;
- the bytes are **sha256-verified** on both ends; a mismatch aborts the whole
  tidy with `E_RESCUE_FAILED` and removes nothing;
- it **never clobbers** a previous rescue — a second rescue for the same slug
  lands under a timestamped name;
- the destination is reported in `data.rescued.to` and as evidence, on every
  path including `--dry-run` (as `would_rescue`).

Rescued buffers land in the main clone at
`.harness/temp/agent/<slug>-observations.md` — beside the live buffer, inside
the same self-gitignored area.

> **Known limitation.** A rescued file is *saved*, not *drained*:
> `harness observe --list` reads `session-buffer.md` per bucket and will not see
> it. It is deliberately not merged into the live buffer, because that risks ID
> collisions in a file the CLI owns. Read it by hand at drain time. The clean
> fix is an `harness observe --import <file>` verb — captured, not built here.

### What it refuses, and why

Every refusal names **what would be lost**, never a bare count, and all of them
are made before any removal.

| code | when |
|---|---|
| `E_BAD_SLUG` | the slug is not kebab-case |
| `E_LINKED_WORKTREE` | run from a linked worktree — tidy removes trees *beside* the main clone, and cannot remove the one it is standing in |
| `E_NOTHING_TO_TIDY` | no worktree, no registry entry and no `NNN-<slug>` branch — already tidy |
| `E_WORKTREE_DIRTY` | uncommitted changes; the files are listed |
| `E_UNPUSHED_COMMITS` | a clean tree whose commits exist nowhere else; the commit subjects are listed. Distinct from the above on purpose — a clean-but-unpushed tree is not "dirty", and a refusal that says it is sends you hunting for files that do not exist |
| `E_BRANCH_NOT_MERGED` | the branch is not merged into `main` — **and squash merges count as merged**, see below |
| `E_RESCUE_FAILED` | the observation buffer could not be copied out verifiably — **nothing is removed** |
| `E_CORE_TOO_OLD` | the core has no write-side filesystem capability, so the buffer cannot be rescued |

`--force` overrides the three at-risk refusals — and still lists what it is
discarding first.

### Merged-detection: squash merges are the NORMAL case here

This repo **squash-merges every PR**. A squash rewrites history, so a correctly
landed branch's tip is *never* an ancestor of `main` and `git branch --merged`
never lists it. Merge detection therefore has **two legs**, and the second is
not optional:

1. **Ancestor** — `git branch --merged <base>`. Cheap, and correct for a real
   merge commit or a fast-forward.
2. **Patch-equivalence** — `git cherry <base> <branch>`. One line per commit on
   the branch: `-` when an equivalent patch is already upstream, `+` when it is
   not. **All `-` means merged**, which is exactly what a squash looks like from
   the branch's side.

```
$ git cherry main w-team-tidy
- a83cb335d0f76ad0b789e7e0f15032225cb51669     # squashed into main as bb10474
```

**The ancestor check ALONE was the original bug — do not "simplify" the second
leg away.** (DL-049, found by tidying tidy's own worktree the day it merged.)
With only leg 1, *every properly merged packet branch in this repo* hit
`E_BRANCH_NOT_MERGED` and had to be removed with `--force` — which also silences
the dirty-tree and unpushed-commit refusals. A safety rail that trains people to
`--force` past **all** the checks is worse than the gap it was guarding.

**It fails closed.** Anything unproven reports *not* merged:

| situation | verdict |
|---|---|
| no commits ahead of base | merged |
| every commit patch-equivalent upstream (`-`) | merged — the squash case |
| any `+` line, even alongside `-` lines | **not merged** — a half-upstream branch still holds work that exists nowhere else |
| `git cherry` fails, or no base resolves | **not merged** |

The cost of guessing "merged" is deleting commits that exist nowhere else, so
the uncertain answer is always the refusal.

### Docker volumes: namespaced, by name, never a prune

Compose derives its project name from the directory, so a worktree at
`fs3-<slug>` owns `fs3-<slug>_*` and nothing else. Tidy drops **only** that
namespace, **only** volumes with zero attached containers, and **only** by
explicit name. It never runs `docker volume prune`, never `-a`, and reports
in-use volumes as kept rather than removing them.

If the engine is unreachable the verb returns **`degraded`**, not an error: the
worktree and branch are still tidied and the envelope says the volumes were
never checked. Tidy works on a machine with no engine running.

**Out of scope — for a human, not this verb.** These are orphans from other
projects and from agent runs that predate tidy. Tidy will never touch them;
remove them by hand when you want the space back:

```bash
# ~35 GB — dind-in-docker state from dead agent runs (all zero-link)
docker volume ls -q | grep '^dind-var-lib-docker-' | xargs -r docker volume rm
# per-branch chainglass volumes for finished branches (check links first)
docker volume ls -q | grep -E '^chainglass_(node_modules|dot_next)_' | xargs -r docker volume rm
```

Check `docker ps -a --filter volume=<name> -q` is empty before removing any of
them; a non-empty result means something is still attached.

### Tidy closes the ordinal loop

After a tidy the ordinal is genuinely **free again** — the scan finds no folder,
no branch and no registry entry, so `harness team new <slug> --propose` stops
counting it. Verified end-to-end: mint `005`, tidy, and the next proposal is
`005` once more. Tidy does not punch a hole in the sequence.

## Related

- `.agents/skills/pij-team/SKILL.md` — the flow this verb starts
- `.agents/skills/pij-team/TENETS.md` — the doctrine the impl-guide instantiates
- `scratch/team-new-poc/validation.md` — the POC this was ported from, and the
  extension notes that shaped it
