# Ruling 2026-09-02 — sol codes, Claude reviews; sequential under an unstable pij

Jordan, verbatim: "have sol code, claude review please." Preceded by
"condier pij state too which is still unstable" and
"5.6-sol-fast-1m please, check it."

In force for rows 117 and 119:
- Coders: `pij spawn --harness pi --bin omp --model
  github-copilot/gpt-5.6-sol-fast-1m --effort high` (checked: the model is
  offered under omp only — "GPT-5.6 Sol Fast (Internal only) (1M)").
- Reviewer: Claude (`github-copilot/claude-opus-5`, effort high), cross-model
  by design; spawned only when a PR is actually up.
- Sequential: 117 first; 119 only after 117's ack lands THROUGH pij. The wire
  is treated as untrusted: every brief names a file channel
  (`.harness/temp/agent/<packet>-ack.md` / `-report.md`) as the defined
  fallback; delivery is proven by content, never by receipt; `pij spawn`
  only, never adopt or omp-extension boot; generation recorded at canary.
