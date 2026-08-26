# ci-release — CI gate + release machinery

**Status**: live (plan 004). **Owner**: pij-impressive-ox.

## What it is

The shipping pipeline: a Linux quality gate on every main push, release-please
semver releases from conventional commits, cross-platform binary attachments
on each Release, a curl installer, Dependabot.

## Key decisions and why

- **Linux-only gate**: every main push runs fmt / clippy -D warnings /
  `cargo test --workspace` / arch-drift — the same commands as local
  `harness checks`, plus a pgvector service mirroring docker-compose.yml
  (host port 5433 so the daemon's shipped default address holds; readiness is
  observed manually because the image defines no HEALTHCHECK).
- **macOS/Windows never gated**: mac builds are release-only and native on
  macOS runners (Apple SDK licensing forbids Darwin builds in Linux
  containers); windows binaries are produced-not-run.
- **release-please `simple` type**: the repo's root Cargo.toml is a virtual
  workspace (`[workspace.package]`) which the `rust` strategy can't drive;
  version truth lives in `.release-please-manifest.json` + git tags.
- **Conventional commit subjects are BINDING on main** (`feat:`/`fix:`/`perf:`
  bump versions; everything else is chore). Non-conforming subjects are simply
  ignored by release-please.
- **Single binary per platform** (req 51): one asset per target triple;
  asset names freeze at one constant in `install.sh` / `install.ps1`.
- **mac tier scope (Jordan ruling 2026-08-26, binding)**: mac jobs prove the
  BINARY on real hardware — build, smoke (`--version`/help), engine-independent
  tests. NO docker/compose installs and NO live-engine assertions: macOS
  runners have no engine and no nested virt, so `doctor_daemon.rs`
  (engine-present findings semantics) is skipped on that tier only
  (`-- --skip '^doctor_'`); it stays green in the Linux gate. Simplify, don't
  accrete — an earlier brew docker + compose attempt was removed.

## Gotchas


- **`scratch/**` is shared mutable ground — READ and WRITE sides both count**
  (DL-012 + addenda). Mechanism: `scratch/` is GITIGNORED, so the exposure is
  exactly the moment you DISABLE gitignore filtering in a sweep (hunting build
  output, dotfiles, `.env`) — sibling worktrees then join your results wearing
  main relative paths. Corollary: cargo gates are structurally IMMUNE
  (workspace members are path-listed; no manifests under `scratch/`) — never
  distrust a green gate on this account. Worktrees are seat-scoped
  (`scratch/verify-<seat> <sha> --detach`) and never force-removed by anyone
  but their owner — `git worktree list` shows paths, not owners, so skipping
  the seat naming disarms the rule for everyone. **Mutation checks**
  (disable-the-fix-watch-it-fail) run in your seat-scoped worktree only — in
  the shared tree a deliberately-broken state fails OTHER seats' tests while
  naming YOUR file.
- The pgvector image has no HEALTHCHECK → GitHub service health probes fail
  with "map has no entry for key Health". Observe readiness yourself.
- Port discipline matters: tests spawn the real daemon against the SHIPPED
  default address (127.0.0.1:5433), so the CI service must map 5433:5432,
  not 5432.
- release-please hard-fails on nothing but logs every unparseable legacy
  subject; it still only counts conforming ones for versioning.

> **Named cutover (ruling 2026-08-26-pr-workflow-cutover.md)**: the
> push-to-main trigger is INTERIM — we have no PR flow yet. When work drains
> and v0.2.0 ships: main gets branch protection requiring the green `ci`
> check, workflows move to `pull_request` triggers, and the push trigger is
> removed. Sequencing: release first, cutover after — deliberately, not by
> drift. Mechanical change owned by pij-impressive-ox.
- Repo-level "Dependabot alerts" toggle is an admin setting separate from
  version updates; alerts were off at setup time.

## How to verify

```bash
# NEVER pin by "--limit 1": on a busy push release-please (41s) finishes
# last and a green can be misread as the gate (fleet rule DL-012). Resolve
# the run id for YOUR sha, then watch it, and report workflow+sha together.
gh run watch "$(gh run list --workflow ci --commit "$(git rev-parse HEAD)" --json databaseId -q '.[0].databaseId')" --exit-status
gh run view <run-id> -q '.headSha + " " + .workflowName'   # quote both with the verdict

gh pr list --state open            # rolling release PR ("chore(main): release …")
gh run list --workflow=release     # release builds after a tag lands
gh api repos/AI-Substrate/flowspace3/releases/latest -q .assets[].name   # 4 triple-named assets
curl -fsSL https://raw.githubusercontent.com/AI-Substrate/flowspace3/main/install.sh | sh
flowspace3 --version
```

## Pointers

- `.github/workflows/ci.yml`, `release-please.yml`, `release.yml`
- `release-please-config.json`, `.release-please-manifest.json`
- `.github/dependabot.yml`
- `install.sh`, `install.ps1` (ps1 unvalidated by stance)
