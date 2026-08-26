# Ruling — PR-workflow cutover (Jordan, 2026-08-26)
Once the current work drains and the v0.2.0 release is done: main gets BRANCH PROTECTION and all
work moves to PRs from then on. At that moment (one cutover, deliberate): CI trigger moves from
push-to-main to pull_request-only, the green check becomes a merge requirement, and the push
trigger is removed (ox holds the mechanical change; noted in docs/services/ci-release.md).
Until cutover, the push-to-main gate + commit-push-as-you-go ruling stand unchanged.
