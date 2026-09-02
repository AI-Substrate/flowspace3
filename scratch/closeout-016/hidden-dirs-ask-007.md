# hidden-dirs stop-and-ask 007 — current pij corpus cannot meet stale ≥500 TS threshold

Scratch daemon proof is running safely with `FS3_CONFIG_DIR=/tmp/fs3-hidden-dirs.gis4B3`, test DB `:5434/flowspace3_test`, and free port `63624`.

Measured current `/Users/jordanknight/pi-hacking/pij`:

- Default explicit `--no-include-hidden`: `.pi/**` search returns `path_unmatched`; stored `include_hidden=false`; `.pi/%` mapped rows = 0.
- Opt-in `--include-hidden`: report `include_hidden=true`, `files=2463`, `unchanged=1974`, `enqueued=489`.
- `git -C /Users/jordanknight/pi-hacking/pij ls-files '.pi/**/*.ts' | wc -l` = **379** tracked TypeScript files.
- Store after opt-in: `.pi/%` rows = **402**, of which `.ts` = **378**.

Therefore AC-0005's inherited `≥500 TypeScript files under .pi/ scanned` cannot be true for today's corpus; there are only 379 tracked candidates. The earlier evidence's 563 count is stale relative to this checkout. I will finish the named-function search receipt, but need a ruling to amend the quantitative criterion to the observable current invariant (all 378 eligible tracked `.pi/**/*.ts` paths indexed, with the one exclusion named if needed) before checking T5 or opening the PR.
