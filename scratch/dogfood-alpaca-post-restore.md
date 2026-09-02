# fs3 dogfood after the restore — meadowlark (pij-varied-alpaca), 2026-09-02

Real questions I actually wanted answered, from the harness-engineering main checkout. Timings wall-clock.

## 1. `ask` never answers (BLOCKING for the verb)
- `ask "<question>"` unscoped: killed at **180 s**, no output.
- `ask "<question>" --repo git:github.com/AI-Substrate/harness-engineering`: killed at **100 s**, no output, exit 124.
No partial, no error, no timeout envelope. Search on the same words returns in 17 s, so retrieval is not the cost — whatever ask does after retrieval never returns. A verb with no deadline is a verdict that never arrives; it needs a bounded wall-clock and a degraded envelope.

## 2. Zero symbols for every TypeScript file (zero WITHOUT a reason)
`get el:…/harness/cli/src/acts/convo.ts` → ok, `raw_text` present, **`children: []`**. Same for `app.ts` and `services/settings/load-settings.ts`. Consequences, all silent:
- `tree harness/cli/src/acts/convo.ts` → 48 s, ok:true, `entries: [], total: 0`.
- `refs resolveConvoIdentity` → 0 results; `refs loadSettings` → 0 results. Both symbols have 5+ references in the repo.
Files are indexed; symbol extraction for TS produced nothing repo-wide. Both verbs report an honest-looking zero with no "no symbols extracted for this file" reason. 48 s to return an empty tree is its own finding.

## 3. Search: literal hit is perfect, ranking is conversation-heavy
- `search "CONVO_IDENTITY_UNRESOLVABLE_LINE"` (a constant I added today): 10 hits, top three are MY OWN conversation turns at 1.0, then `convo.test.ts` at 1.0. 17 s. Good: the merged code and my session are both current.
- `search "resolve conversation identity from env" --repo …/harness-engineering`: top 5 are all `conv:` turns; the source file that does it does not appear. With `--repo` set I expected code to lead. Suggest `--source code` be the default when `--repo` is given, or rank code above conversation for a repo-scoped query.

## 4. `ask --conversation` — my misuse, note only
It takes a `<GUID>`, not a mode. The clap error is clear; I read it late because I piped to JSON. No finding.

## Pros, so this is not one-sided
- verify verb: 0.02 s, honest negative, refuses scope flags — exactly the contract.
- `get conv:…#tN --repo all`: instant and reliable throughout the backfill.
- Backfill of 54 transcripts + 12 subagent restores + 101 conversations survived a disk-full + postgres outage intact.
