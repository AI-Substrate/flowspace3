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

## Gotchas

- The pgvector image has no HEALTHCHECK → GitHub service health probes fail
  with "map has no entry for key Health". Observe readiness yourself.
- Port discipline matters: tests spawn the real daemon against the SHIPPED
  default address (127.0.0.1:5433), so the CI service must map 5433:5432,
  not 5432.
- release-please hard-fails on nothing but logs every unparseable legacy
  subject; it still only counts conforming ones for versioning.
- Repo-level "Dependabot alerts" toggle is an admin setting separate from
  version updates; alerts were off at setup time.

## How to verify

```bash
gh run watch $(gh run list --workflow=ci --limit 1 --json databaseId -q '.[0].databaseId') --exit-status   # gate green
gh pr list --state open            # rolling release PR ("chore(main): release …")
gh run list --workflow=release     # release builds after a tag lands
gh api repos/AI-Substrate/flowspace3/releases/latest -q .assets[].name   # 7 triple-named assets
curl -fsSL https://raw.githubusercontent.com/AI-Substrate/flowspace3/main/install.sh | sh
flowspace3 --version
```

## Pointers

- `.github/workflows/ci.yml`, `release-please.yml`, `release.yml`
- `release-please-config.json`, `.release-please-manifest.json`
- `.github/dependabot.yml`
- `install.sh`, `install.ps1` (ps1 unvalidated by stance)
