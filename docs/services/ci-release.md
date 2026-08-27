# ci-release — CI gate + release machinery

**Status**: live (plan 004). **Owner**: pij-relieved-goat (release engineering;
inherited from pij-impressive-ox, closed 2026-08-27).

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
  macOS runners; windows binaries are produced-not-run. **Intel Macs are not
  supported** (Jordan, 2026-08-26) — the matrix is Apple Silicon only:
  three shipped binaries (aarch64-apple-darwin, aarch64/x86_64-linux-gnu).
- **release-please `simple` type**: the repo's root Cargo.toml is a virtual
  workspace (`[workspace.package]`) which the `rust` strategy can't drive;
  version truth lives in `.release-please-manifest.json` + git tags.
- **Conventional commit subjects are BINDING on main** (`feat:`/`fix:`/`perf:`
  bump versions; everything else is chore). Non-conforming subjects are simply
  ignored by release-please.
- **Single binary per platform** (req 51): one asset per target triple;
  asset names freeze at one constant in `install.sh` / `install.ps1`.
- **Draft-first releases (w-release-window, 2026-08-27)**: release-please
  creates the Release as a DRAFT and `release.yml` undrafts it as its FINAL
  step, after the binaries and SHA256SUMS are attached. Drafts are invisible to
  `releases/latest`, so `latest` can never resolve to an assetless release —
  which is what the documented `install.sh` one-liner was 404-ing on for ~6
  minutes of every release (nine times during the v0.2.0 campaign). A failed
  release now stays a draft: invisible, rather than broken-and-latest.
  `force-tag-creation` rides with it and is load-bearing — GitHub does not
  create a draft release's git ref until publish, so without it there would be
  no tag push, no `release.yml` run, and no release at all. Both options were
  verified against the version the action ACTUALLY bundles: `@v4` is action
  4.4.1, whose package-lock pins release-please **17.3.0**, not the newest.
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
- **`/releases/latest/download/<asset>` 404s for ~30s after a release
  publishes** until GitHub's latest-pointer propagates — an installer run in
  the same minute as the release can fail spuriously. Retry before believing
  it; the tagged URL (`/releases/download/<tag>/<asset>`) works immediately.
  Since draft-first, this is the only installer-facing 404 window left, and it
  opens AFTER the assets exist. `install.sh` no longer dies on it silently: it
  branches on `%{http_code}` and prints a mid-publish message plus the releases
  URL. Note the status code is the ONLY usable signal — asset downloads redirect
  to object storage, so a 404 surfaces as curl exit **56**, not the 22 that
  `-f` documents.
- **Cycling a tag flips the release back to DRAFT.** Still true; since
  draft-first it is CORRECT rather than a defect. The re-triggered release run
  rebuilds, re-uploads and re-undrafts at the end, so a cycle needs no manual
  repair. Previously this looked identical from outside to the propagation
  delay above (a 404) and had to be repaired by hand.
- **The shipped binary can lie about its version, and v0.2.0 did** (req-0060,
  fixed 2026-08-27). release-please's `simple` strategy bumps
  `.release-please-manifest.json` and the changelog and *nothing in the Rust
  manifests*, so `[workspace.package] version` sat at `0.1.0` while tags marched
  on. The v0.2.0 release binary self-reported `0.1.0`. Nothing caught it: the
  release smoke step RUNS `--version` and never reads what it prints.

  It stopped being cosmetic the moment auto-update landed. The updater compares
  its own `env!("CARGO_PKG_VERSION")` against the newest published tag, so a
  binary that under-reports is permanently "older" than every release — it would
  re-download and re-swap once per check interval **forever**, raising a restart
  message that restarting cannot clear.

  Three things now hold it shut, and it is worth knowing why each exists:

  1. **`Cargo.toml` carries an annotation**: `version = "…" #
     x-release-please-version`, with a typed `{"type": "generic", "path":
     "Cargo.toml"}` entry in `release-please-config.json`'s `extra-files`.
     `release-type: rust` is NOT usable here — release-please's own
     `src/updaters/rust/cargo-toml.ts` throws on any manifest without a
     `[package]` table, and our root is a virtual workspace. The typed
     `generic` form is deliberate too: a bare `"Cargo.toml"` string additionally
     runs `GenericToml('$.version')`, which targets a top-level `version` we do
     not have.
  2. **`release-please.yml` syncs `Cargo.lock`** on the release branch, because
     the generic updater cannot regenerate a lockfile and the lock carries a
     `[[package]] version` for all eight members. Annotating those is not an
     option — cargo rewrites the lock from scratch whenever resolution changes
     and drops every comment, so the annotations would vanish silently. The step
     asserts `cargo metadata --locked` afterwards, so a sync that did not take
     fails the job rather than the tag.
  3. **Preflight leg B2** compares the built binary's `--version` against
     `.release-please-manifest.json`. The manifest is the oracle, not
     `Cargo.toml`: a binary is always built FROM `Cargo.toml`, so comparing
     those two compares a thing with itself and can never fail. The drift that
     actually happened was between the manifest and the manifest-that-ships.

## Release runbook — NO TAG CYCLE WITHOUT PREFLIGHT GREEN

Jordan, 2026-08-26, after 8 tag cycles in one day (at least 5 locally
catchable): every release-job command is reproducible on this machine, so
reproduce it BEFORE spending a cycle.

```bash
./docker/scripts/release-preflight.sh      # ~13s warm; PREFLIGHT_ARM=1 adds the emulated arm64 leg
```

It replicates, verbatim and mapped 1:1 onto the release job names:

| leg | replicates | catches (real incidents) |
| --- | --- | --- |
| A | `cargo build --locked --release -p fs3-cli` | v1: lock not resolvable under `--locked` |
| B | the smoke block | binary that builds but will not run |
| C1 | mac fast tier, normal env | ordinary test breakage |
| C2 | mac fast tier, **runner simulation** (docker masked out of PATH, db pointed at a dead port) | v2–v4 docker-absence, v6 skip-filter (`--skip` is substring, not regex), v7 live-Postgres integration tests |
| D | linux x86_64 via the plan-002 build container + smoke | container-leg breakage; wrong-loader mistakes |

Only when it prints **GREEN** do you cycle the tag:

```bash
git push origin :refs/tags/vX.Y.Z && git tag -f vX.Y.Z <sha> && git push origin vX.Y.Z
```

### Canonical release flow (draft-first, since w-release-window 2026-08-27)

1. **Merge the release PR.** release-please creates `refs/tags/vX.Y.Z`
   (`force-tag-creation`) and the Release as a **DRAFT**. The draft is invisible
   to `releases/latest`, so `latest` still resolves to the PREVIOUS release,
   with its assets. Installer users are unaffected from this moment on.
2. **The tag push does not trigger `release.yml`.** The ref was written with
   GITHUB_TOKEN, and GitHub's recursion guard suppresses workflow triggers for
   token-pushed refs. This is unchanged by draft-first and is why the cycle
   below is manual; fixing the guard itself is a separate job.
3. **Cycle the tag** (command above, preflight GREEN first). This is the push
   that starts the build. The cycle re-drafts the release — harmless now.
4. **`release.yml` builds (~6 min)**, verifies the artifact shapes and the
   3-binary count, generates SHA256SUMS, uploads, and **undrafts as its final
   step** (`gh release edit "$TAG" --draft=false --latest`). If any earlier
   step fails, the release stays a draft and `latest` never moves.
5. **Verify** — the check below is no longer a repair step, it is the
   post-release proof that the automated undraft ran:

```bash
gh release view vX.Y.Z --json isDraft,assets -q '{draft: .isDraft, assets: [.assets[].name]}'
```

Expected: `draft: false` with four asset names (three binaries + SHA256SUMS).
A release still showing `isDraft: true` means the build did not reach its last
step — read the run, do not hand-publish it, because hand-publishing a release
whose assets never attached recreates exactly the defect this design removed.

Single-watcher rule (shared gh auth): ONE `gh run watch -i 60` per run,
fleet-wide; parallel 3s polls exhausted the API mid-release once already.

### Queued for the 0.3.0 cycle (not now)

- ~~**Native ARM runners**~~ DONE early (o-prime, Jordan word): evaluate `runs-on: ubuntu-24.04-arm` (free for
  public repos) to replace the QEMU-emulated aarch64 container leg — QEMU
  costs 20–40 min per release, native lands around 5 and removes the last
  slow leg. Preflight leg E (`PREFLIGHT_ARM=1`) maps onto it directly.
- **Version stitch (wart, seen in preflight)**: the binary reports the
  workspace version (`0.1.0`) while releases are tagged `v0.2.0` —
  release-please's `simple` type never touches `Cargo.toml`. Two options:
  (a) annotate the version line (`# x-release-please-version`) and list
  `Cargo.toml` under `extra-files` in `release-please-config.json` — boring,
  no build machinery, keeps `--version` truthful; (b) derive the version at
  build time from the tag. Recommendation: (a).

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

# Draft-first acceptance, run DURING a build window (the negative check first):
#   latest must still be the PREVIOUS release, and still downloadable.
gh api repos/AI-Substrate/flowspace3/releases/latest -q .tag_name
curl -sIL -o /dev/null -w '%{http_code}\n' \
  https://github.com/AI-Substrate/flowspace3/releases/latest/download/flowspace3-aarch64-apple-darwin
# expect: the previous tag, and 200 — never the tag being built, never 404.
gh release view vX.Y.Z --json isDraft -q .isDraft   # expect true while building
curl -fsSL https://raw.githubusercontent.com/AI-Substrate/flowspace3/main/install.sh | sh
flowspace3 --version
```

## Pointers

- `.github/workflows/ci.yml`, `release-please.yml`, `release.yml`
- `release-please-config.json`, `.release-please-manifest.json`
- `.github/dependabot.yml`
- `install.sh`, `install.ps1` (ps1 unvalidated by stance)
