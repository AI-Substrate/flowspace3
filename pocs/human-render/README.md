# human-render — the human skin over the frozen fs3 envelope

A standalone PROTOTYPE (shelf item, not wired into flowspace3). v1 answers in
JSON only — workshop 003 D5, Jordan's call. This is the fast-follow made ready
in advance: the same envelope, rendered for a person at a terminal, in the
spirit of Python's `rich`.

**Two skins, one truth.** The renderer's input is an `Envelope` and nothing
else. Everything on screen is derivable from the bytes an agent already
receives, which means the human view cannot drift ahead of the machine view. A
fact a human needs and the envelope does not carry is a gap in the CONTRACT, not
a thing to fetch on the side — see [LEARNINGS.md](LEARNINGS.md) for the three
this prototype found.

That constraint is enforced by the dependency graph, not by discipline: this
crate depends on `fs3-core` (by path, for the real `Envelope` and `Failure`) and
on nothing else from flowspace3. It cannot reach a database if it wants to.

## Run it

```console
cd pocs/human-render

cargo run --bin demo                          # rich: you are at a terminal
cargo run --bin demo | jq .command            # JSON: something is consuming this
cargo run --bin demo -- --json                # JSON, always
cargo run --bin demo -- --surface search      # one screen
cargo run --bin demo -- --human --width 100   # rich into a pipe or a file
cargo run --bin demo -- --file - --human      # an envelope off stdin

cat transcript.ansi                           # the four screens, in colour
```

`transcript.ansi` is a captured run (`--human --color always --width 100
--explain`); `cat` it in a terminal to see exactly what a user would.

## The four surfaces

| Screen | What it is for | The judgement in it |
|---|---|---|
| `search` | ranked hits + `meta.folders` steer | addresses printed in FULL (they are the input to `get`), summary preferred over snippet, folder steer ordered by count |
| `doctor` | the found→did checklist | `repaired` is its own glyph, not a green tick — doctor CHANGED your machine |
| error | any `ok: false` envelope | the `fix` is framed and bright; code and message are dim above it |
| `status` | roots + queue depth | the queue is pivoted to one row per kind, with a completion meter |

An unknown verb, or a payload shaped differently from what this build expects,
falls through to a generic titled dump — never a blank screen, never a panic.

## The TTY strategy

```text
--json given?  ────yes──▶ JSON (the envelope, verbatim)
     │no
--human given? ────yes──▶ RICH
     │no
stdout a tty?  ────no───▶ JSON (a pipe, a file, a CI log, an agent)
     ▼yes
   RICH
```

Piped means JSON, not rich-without-colour: `flowspace3 search … | jq` must keep
working with no flag, which is the property v1 has today and must not lose. The
JSON path is a byte-for-byte passthrough — `tests/surfaces.rs` asserts it, and
that assertion is the thesis of this prototype in one line.

Colour is a separate question, answered once, by `anstream` (`NO_COLOR`,
`CLICOLOR_FORCE`, `TERM=dumb`, Windows consoles). The renderer always emits
ANSI; the stream decides whether it survives. No surface anywhere in this crate
branches on whether colour is wanted.

## Layout

```text
src/lib.rs          the rule, and the crate's shape
src/mode.rs         rich-vs-JSON, and the colour policy
src/render.rs       dispatch on (ok, command)
src/theme.rs        palette, glyphs, meters, tables — one visual vocabulary
src/views.rs        per-verb payload DTOs (deserialize-only, all defaulted)
src/surfaces/       search · doctor · failure · status · generic
src/bin/demo.rs     the boundary: decides presentation, reads bytes, writes
fixtures/*.json     four realistic envelopes, in the frozen shapes
tests/surfaces.rs   end-to-end, through the real binary
```

## Not in scope

No live progress rendering (`indicatif` was considered and declined —
[LEARNINGS.md](LEARNINGS.md#indicatif-declined)), no pager, no `--watch`, no
theming config. Those are decisions for the promotion, not for the shelf.
