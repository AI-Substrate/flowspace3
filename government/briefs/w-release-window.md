# Brief: w-release-window — no more assetless releases (cheetah finding 8, Jordan ruled 2026-08-27)

**Seat**: (fill at canary — fresh seat; release engineering becomes your domain, inherited
from ox, closed). PR-era done-bar: own worktree + branch off main, conventional commits
(`fix:` — this SHOULD bump the patch version: the resulting v0.3.1 is deliberately the
target of the fleet's first live auto-update-hop proof), harness checks green (seven
gates), PR, report the number, never self-merge. Read AGENTS.md fully first.

## The defect (reproduced LIVE by the standing Linux tester, 2026-08-27 ~07:50Z)

release-please publishes the GitHub Release at TAG time; `.github/workflows/release.yml`
only then builds (~6 min) and uploads assets + SHA256SUMS. For that whole window,
`releases/latest` resolves to a release with ZERO assets, and `install.sh` — the
README's documented one-liner — 404s for EVERY user (curl exit 22, raw noise, no binary,
no explanation). The v0.2.0 campaign's nine tag cycles opened this window nine times.
Secondary defect: the installer's failure mode is raw curl output, not a real message.

## Deliverables

1. **Draft-first releases**: release-please creates the release as a DRAFT
   (`release-please-config.json` supports a draft flag — verify against the pinned
   version's actual schema by READING it, the way edeard's source-read caught the
   release-type-rust trap; do not trust docs). `release.yml`'s upload job undrafts as
   its FINAL step after binaries + SHA256SUMS attach: `gh release edit $TAG
   --draft=false --latest`. Result: `latest` can NEVER resolve to an assetless release.
2. **Tag-cycle compatibility**: cycling a tag re-drafts the release (observed v0.2.0
   behaviour) — under this design that is now CORRECT (the rebuild re-undrafts at the
   end). State this in a comment where the undraft happens, and update the release
   runbook section in docs/services/ci-release.md: the "verify isDraft=false" manual
   step becomes an automated final step, but the manual check stays documented as the
   post-release verification.
3. **Installer honesty**: `install.sh` (and install.ps1's equivalent path if trivial)
   detects the missing-asset 404 and prints a REAL message: the release is likely
   mid-publish, retry in a few minutes — plus the direct releases URL. No raw curl
   noise as the only output. Keep it POSIX sh.
4. **The recursion-guard note**: GITHUB_TOKEN-pushed tags do not trigger release.yml
   (why o-prime hand-cycles tags). NOT this packet's job to fix, but the draft-first
   design must not make the manual cycle worse — verify the sequence
   (merge release PR → tag exists, release DRAFT, invisible to latest → cycle →
   build → undraft) and document it as the canonical flow in ci-release.md.
5. **Verification**: preflight cannot exercise GitHub's release lifecycle, so state
   the manual acceptance in the PR body: the NEXT release (v0.3.1 — this very packet's
   own release) must show latest resolving to v0.3.0-with-assets during its entire
   build window, then flipping to v0.3.1 only when assets are attached. O-prime runs
   that check at cut time; write the exact commands for it.

## Out of scope

The daemon update-supervisor's behaviour during the publish window (already handled:
missing SHA256SUMS/assets = Blocked; cheetah proves it). The first-user compose cliff
(separate packet). Fixing the recursion guard itself.
