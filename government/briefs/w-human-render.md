# Worker brief — human-render prototype · (seat at canary, pane %46)
**From**: pij-instant-lynx (o-prime) · 2026-08-26 · one bounded task · PROTOTYPE (fast-follow shelf, not wired)

## The job
Jordan: JSON-only v1 stands, but "let's get a coder prototyping [the human version]… I like Python rich and things like that, so really nice output for humans. Have that ready in case we want it as a fast follow." (PRD req 50.)

Standalone prototype at `pocs/human-render/` — own cargo project, **NOT a workspace member** (empty `[workspace]` in its Cargo.toml, .gitignore for target/; the arch check's 8-crate assertion must stay green).

1. **A pure renderer over the FROZEN envelope contract**: input = envelope JSON (the real shapes from `crates/core/src/envelope.rs` + catalog — read them; depend on fs3-core by PATH if convenient, or mirror the shapes and say so), output = rich terminal rendering. The renderer consumes ONLY public envelope fields — that constraint is the whole architecture (two skins, one truth).
2. **Rust's python-rich ecosystem**: `comfy-table` (tables), `owo-colors`/`anstream` (color with NO_COLOR + non-TTY degradation), `textwrap`; `indicatif` only if you show a progress concept. Lib-reuse rule: never hand-roll ANSI.
3. **Render the four surfaces that matter**, from realistic sample envelopes you author as fixtures: (a) search results (address/score/kind/span/tags/snippet as a ranked table + the meta.folders steer), (b) doctor (found→did checklist with ✓/✗/fixed coloring), (c) an error envelope (code + message + the `fix` line made VISUALLY primary — the fix is the star), (d) status (roots + queue-depth table). A demo bin prints all four; screenshot-grade.
4. **TTY strategy demonstrated**: auto-detect (human at terminal → rich, piped → raw JSON passthrough), `--json` forces envelope; show both paths in the demo.
5. **LEARNINGS.md**: crate choices + why, what the envelope contract made easy/awkward (any field the renderer WISHED it had = feedback to workshop 004), what promotion-to-fs3-cli would take (est. shape, not estimate hours).

## Rules & fence
- Fence: `pocs/human-render/**` only. No workspace crates, no docs elsewhere. Scratch `.harness/temp/w-human-render/**`.
- `harness checks` stays green (you're outside the workspace; verify you didn't break the crate-count assertion or docs links).
- Commit+push per unit, scoped adds, push-first (ruling 2026-08-26-commit-push-as-you-go.md).
- Report to pij-instant-lynx: claim · demo transcript/screenshots · LEARNINGS highlights. Deviations = stop-and-ask.
