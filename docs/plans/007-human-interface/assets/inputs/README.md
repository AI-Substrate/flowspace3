# Inputs — what plan 007 was built from, pinned

Every input to this plan, with a sha256 so a coder can prove they read the same
bytes the plan was written against. Refs must resolve **where seats live**: two
of these live outside any worktree's tracked tree, so they are copied here.

## Copied in (the source is gitignored — the ref would not resolve for a coder)

| file | sha256 | origin |
|---|---|---|
| `tui-poc-VERDICT.md` | `bd36fa6b5938bf22a4e64d97b0021abe96175c002d024974a570ec6ecadb6649` | `scratch/tui-poc/VERDICT.md` in the main clone (`/Users/jordanknight/substrate/flowspace/flowspace3`), pij-mammoth-ox, 2026-08-28 |

The verdict is unit u-t's brief in the seat that wrote it: what ratatui made
easy, what it made painful, and the pain list (truncation, input handling) that
becomes u-t's polish checklist. The POC's own code stays scratch-only reference
— product code is fresh in-crate.

## Pointed at (in this tree, so pinned rather than copied)

| path | sha256 | why it matters |
|---|---|---|
| `pocs/human-render/LEARNINGS.md` | `107f6d251b522170b5f51bd8afcb4be5dd6d371734f1535c1e88c6493938972b` | the authoritative promotion shape: §1 the crate choices and their justifications (the only allowlist rows u-r may add), §3.1 the payload-DTO move, §4 the port order, §5 the byte-identity test that had to come with it |
| `pocs/human-render/README.md` | `c464d38c754bb202ef8854a566c9ae10102d39ea41d54e4ee62c6d2466a547e4` | the TTY strategy diagram, the four surfaces, and what each screen's judgement is |
| `pocs/human-render/fixtures/search.json` | `dab21a87c475f6bee2de473a9744854591f20b1c6ec80861abbaf1ce5f3dbbe8` | the search screen's input |
| `pocs/human-render/fixtures/doctor.json` | `1701c2ef65ae9a2a014eb0e8fcf4e78d3ae1055a28493b6016f7676e5390d2fa` | the doctor screen's input |
| `pocs/human-render/fixtures/error.json` | `033426b3d3dc1eb6ac64521d95b9ce4764bac64ea03dff6a4f70baf4b5e5f0d8` | the failure screen's input |
| `pocs/human-render/fixtures/status.json` | `2278cddfcdc7e3fb15626b9084b5d28be1a8c6326803c7e853e1d55a8b7d9164` | the status screen's input |

## What the POC already proved, so u-r cites rather than re-derives

1. **The renderer's only input is an `Envelope`.** Everything on screen is
   derivable from bytes an agent already receives, enforced by the dependency
   graph rather than by discipline.
2. **The TTY strategy** — `--json` → JSON, `--human` → rich, otherwise a tty
   decides — with piped meaning JSON rather than rich-without-colour. Now
   implemented once, in `fs3_core::output::resolve`, with `FS3_OUTPUT` added for
   harnesses whose terminal probe lies.
3. **The crate set and the reasons**: `comfy-table` (+`custom_styling`, with
   `force_no_tty()` so exactly one thing decides styling), `owo-colors`,
   `anstream` (the single colour decision: `NO_COLOR`, `CLICOLOR_FORCE`,
   `TERM=dumb`, Windows), `textwrap` (default features off). `indicatif`,
   `colored`, `tabled`, raw `crossterm` and `ratatui` were considered and
   declined for the RENDERER, with reasons — `ratatui` is u-t's crate, not
   u-r's.
4. **Three gotchas** that cost the POC time and would cost u-r the same:
   `custom_styling` implies `tty` and pulls crossterm; comfy-table 8 renamed the
   preset API (every snippet online is v7); nested styles eat their parent, so
   segments are styled independently.
5. **The payload DTOs had to move core-ward first** (§3.1) — done in this plan
   as tk-a106, before fan-out, because the types straddled the u-r/u-w fence.
6. **Contract gaps the renderer found by being written**: `meta.total`/`showing`
   and `lang` are specified in workshop 003 but not implemented; `Step.outcome`
   is an open string the renderer must guess at; there is nowhere in an `ok:
   true` envelope to put a warning. These are FEEDBACK, not work for u-r: a
   renderer that fetches a missing fact on the side is the drift the
   one-envelope decision exists to prevent. Report them, render honestly
   without them.
