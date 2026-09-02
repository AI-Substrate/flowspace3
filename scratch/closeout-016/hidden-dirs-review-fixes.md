# hidden-dirs review fixes — pushed

New head: `aa1abb61d9d427a1138fd8ce3622834d5b4468b1`

PR: https://github.com/AI-Substrate/flowspace3/pull/107

## f-16a1

Deny-listed dot directories are classified before hidden policy. `.venv`, `.cache`, and `.next` now report `standard-ignore` plus the `standard_ignores = false` fix in both hidden modes. Plain `.hidden` keeps `hidden` plus `--include-hidden`. The exact fixture exercises both policy modes and then executes the advertised standard-ignore fix.

Mutation: gated the deny-list branch on `include_hidden`, recreating hidden-first behavior. `hidden_directory_prunes_name_the_effective_rule` failed `left: "hidden", right: "standard-ignore"`; restored implementation passed.

## f-16a2

File-tree policy now resolves from the selected `IndexedFile.root_path`; ambiguous explicit repository/directory targets return absent rather than borrowing cwd policy. The two-root delta test proves opposite policies in both cross-address directions, plus plain tree in each root, absolute other-root file, and `--repo <other>` controls. Renderer test covers `hidden yes`, `hidden no`, and absent.

Mutation: restored cwd-based file policy lookup. `tree_policy_tracks_the_resolved_root_in_both_directions` failed with root A's `true` returned for root B's expected `false`; restored implementation passed.

## Targeted proof

- `cargo test -p fs3-parsers --test discovery_standard_ignores` — 14 passed
- daemon `hidden_files_are_discovered_only_for_an_opted_in_root` — passed on `:5434`
- daemon `tree_policy_tracks_the_resolved_root_in_both_directions` — passed on `:5434`
- CLI `tree_title_agrees_with_the_resolved_root_policy` — passed
- `cargo fmt --all --check` — passed
- `ddocs validate .../assets/reviews/review-016.dd.json` — 0 errors, 0 warnings

Committed review record: `docs/plans/016-hidden-dirs/assets/reviews/review-016.dd.json` and generated `.dd.md`.

Full harness gate intentionally not run; cod holds the exclusive slot. Branch pushed; no merge performed.
