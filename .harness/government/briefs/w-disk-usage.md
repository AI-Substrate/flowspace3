# Worker brief — home-directory disk-usage investigation · (seat at canary)

**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-28 · Jordan-ordered, READ-ONLY.

## The job

The machine hit 100%-full mid-fleet today (LLVM IO failures killed builds;
likely killed seats). Jordan is cleaning interactively with GrandPerspective;
your job is the AGENT-LEGIBLE map of what is eating space under `~/` so the
cleanup hits the right things and we can encode prevention.

Deliverables (numbered):

1. TOP CONSUMERS under ~/: the top ~20 directories and top ~20 individual
   files by size (du/find; stay on this volume; skip permission-denied
   quietly). Include hidden dirs (~/Library, ~/.cargo, ~/.rustup, ~/.docker,
   ~/.omp, ~/.claude, ~/.pij, ~/.git-ai).
2. THE SNEAKY CLASSES Jordan named — things that don't show as single large
   files: node_modules folders (count + total), cargo target/ dirs (per
   worktree/repo, total), git worktrees and their duplication, Docker/OrbStack
   disk (images, volumes, builder cache — `docker system df` and OrbStack's
   data dir), browser/IDE caches, old simulator/Xcode junk if present,
   package-manager caches (npm/pnpm/yarn/pip/brew), and agent session stores
   (~/.claude/projects, ~/.omp/agent/sessions, ~/.pij) with sizes.
3. CLASSIFY each top item: SAFE-TO-DELETE (regenerable cache/build output) ·
   RECLAIM-WITH-CARE (worktree targets, docker volumes — name what breaks) ·
   KEEP (data). Note anything the fleet needs (main clone target/release is
   Jordan's live daemon binary — KEEP).
4. VMs AND VIRTUALIZATION, system-wide (Jordan addition): Docker/OrbStack VM
   disk images and data dirs (~/OrbStack, ~/Library/Group Containers dev.orbstack*,
   docker system df incl. builder cache), any UTM/Parallels/VMware/Lima/Colima
   VM images, Linux VM disks (the release rig!), Xcode simulators
   (xcrun simctl list + ~/Library/Developer/CoreSimulator), and Time Machine
   local snapshots (tmutil listlocalsnapshots /) — snapshots can pin hundreds
   of GB invisibly and would explain df saying 99% while du finds less.
5. One-line prevention suggestions (e.g. harness boot free-space check —
   already observed today as DL-class; shared CARGO_TARGET_DIR trade-offs).

## Rules & fence

- READ-ONLY: no deletions, no docker prune, nothing. Jordan deletes.
- du can be slow on ~/Library — use depth-limited passes and sample rather
  than hanging; note what you skipped.
- Report as a compact ranked table with sizes, to a file:
  scratch/disk-usage-report-2026-08-28.md in the MAIN clone, then pij send
  the path + top-5 lines to pij-instant-lynx.

Ack with numbered plan via pij send first (reply USING PIJ TOOLING).
