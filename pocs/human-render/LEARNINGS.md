# LEARNINGS — human-render

What building the human skin taught us about the crates, about the frozen
envelope, and about what promoting this into `fs3-cli` would actually take.

Written for the o-prime and for workshop 004. The middle section is the
important one: **every place the renderer had to guess is a place the CONTRACT
is thin**, and that feedback only exists because someone tried to consume it.

---

## 1. Crate choices

| Crate | Version | Why this one |
|---|---|---|
| `comfy-table` | 8, `default-features = false`, `+custom_styling` | Dynamic content arrangement — the element column gives way on a narrow terminal instead of the table blowing past the edge. `custom_styling` measures cell width with ANSI *stripped*, which is what allows the colour to come from `owo-colors` rather than from comfy-table's own palette. |
| `owo-colors` | 4 | Zero-alloc styling wrappers on anything `Display`, plus first-class `Style` objects for the per-segment styling `theme::spans` needs. |
| `anstream` | 1 | Owns the "should this have colour at all" decision — `NO_COLOR`, `CLICOLOR_FORCE`, `TERM=dumb`, not-a-tty, Windows consoles — and strips at WRITE time. Its `adapter::strip_str` is also how every test asserts on layout without asserting on escape codes. |
| `textwrap` | 0.16, `default-features = false`, `+unicode-width +smawk +terminal_size` | Wrapping payload prose to a width the renderer chose. `termwidth()` is used in the demo BINARY only. Default features drop `unicode-linebreak`, which pulls the whole ICU segmenter in to hyphenate prose this renderer never hyphenates. |
| `fs3-core` | path dep | The thesis as a dependency: the renderer reads the REAL `Envelope`/`Failure`. Costs serde, serde_json, sha2, toml, thiserror — the functional core has no tokio, no sqlx, no HTTP. |

### Three gotchas worth carrying forward

1. **`custom_styling` implies `tty`.** Turning comfy-table's default features off
   does not remove crossterm — `custom_styling = ["dep:ansi-str", "dep:console",
   "tty"]`. Left alone, comfy-table would sniff the terminal and form its own
   opinion about styling, invisible from here and free to disagree with
   anstream's. Every table in `theme.rs` calls `force_no_tty()`, so there is
   exactly one decision-maker and a table renders identically in a pipe and on a
   terminal.
2. **comfy-table 8 renamed the preset API.** `load_preset(...)` +
   `apply_modifier(UTF8_ROUND_CORNERS)` became
   `load_style(presets::UTF8_FULL_CONDENSED.with_rounded_corners())`, and the
   `modifiers` module is gone. Any snippet found online is for v7.
3. **Nested styles eat their parent.** `format!("{}", "a `b` c".bright_black())`
   with an inner cyan span emits the inner span's reset, and everything after it
   loses the dim. `theme::spans` styles each segment independently instead —
   which is also what lets backticks be treated as markup (the punctuation
   becomes colour, and the command left on screen is copy-pasteable).

<a id="indicatif-declined"></a>
### `indicatif`: considered, declined

A pure `&Envelope -> String` renderer has no draw loop and no tick thread, which
is the entire value of `indicatif`. Queue progress is a static meter built from
block characters (`theme::meter`) — correct for a screen that is printed once.
`indicatif` becomes the right answer the moment a promoted CLI grows
`status --watch` or a live `add`, and at that point it belongs in the CLI's
loop, not in the renderer. Deferring it cost nothing and kept the renderer pure.

### Also rejected

`colored` (String-returning API and global state), `tabled` (larger API surface
for the same table), `crossterm` used directly (comfy-table already carries it;
raw mode is not wanted), `ratatui` (full-screen TUI — the wrong shape: fs3's
output must survive in a scrollback buffer and in a CI log).

---

## 2. What the envelope contract made EASY

- **`ok` as the only discriminator (D1).** Dispatch is six lines and never sniffs
  a payload. A failure renders through the failure surface whatever verb
  produced it, so the error screen looks the same from `search` and from `add` —
  the reader learns the shape once.
- **`fix` mandatory in the TYPE (D3).** The error screen's whole design — the fix
  framed, bright, and below the message as the conclusion — is only possible
  because `fix` cannot be absent. A renderer over an *optional* fix would need a
  fallback layout for its most important screen, and that fallback would be the
  one people actually saw.
- **`command` travelling in the envelope.** The title comes from the payload, not
  from what the caller thinks it asked, so an envelope replayed out of a log
  renders as the verb it actually answered.
- **Additive payload evolution.** Every DTO in `views.rs` is `#[serde(default)]`
  and unknown fields are ignored, so an older renderer against a newer daemon
  degrades field-by-field instead of failing whole. An unrecognised `command`
  falls through to a generic dump — tested, never a blank screen.
- **`next_action` (PRD req 44).** A gift for human UX: every screen gets a
  natural footer that says what to do next, in the daemon's words rather than
  the renderer's guesses.

---

## 3. What it made AWKWARD — feedback for workshop 004

### 3.1 The payload DTOs are unreachable (the one real blocker)

`SearchResults`/`Hit` live in `fs3-daemon` (axum + sqlx + tokio);
`DoctorReport`/`Step` live in `fs3-cli`. A renderer that wants the real types has
to depend on a web server to draw a table. So `views.rs` mirrors them by hand,
field for field — which is exactly the drift risk the one-envelope decision
exists to eliminate.

**Ask:** move the per-verb payload DTOs into `fs3-core` beside the envelope (they
are plain serde structs; the functional-core rule is untouched). The human skin
and the daemon then cannot disagree, and `views.rs` deletes itself.

### 3.2 The doc and the code already disagree about `meta`

Workshop 003's result envelope specifies `meta.total`, `meta.showing`,
`meta.mode`, `meta.rank`, `meta.folders`, `meta.filters_applied`, and a `lang`
on each hit. The shipped daemon builds `SearchResults { results }` and attaches
no meta at all (`crates/daemon/src/search.rs`, `crates/daemon/src/http.rs`), and
`Hit` has no `lang`. The renderer is the first consumer to have to care, and it
found the gap by being written against the doc.

The prototype's fixture follows the DOC (it is the frozen contract), and the
surfaces treat every meta field as optional — `a_search_with_no_meta_at_all_
still_renders_its_rows` proves the today-shape works too. But one of the two
needs to move.

**Ask:** either implement `meta` as specified, or amend workshop 003. And when it
is implemented, type it: `meta` is `Value` on the wire by design, but a
`SearchMeta` struct in the same place as `SearchResults` would let the renderer
stop guess-parsing.

### 3.3 Fields the renderer WISHED it had

| Want | Why the human screen needs it | Status |
|---|---|---|
| `meta.total` + `meta.showing` | the header can only say "6 hits" when the honest answer is "6 of 143" | spec'd in 003, not implemented |
| `lang` on a hit | the kind column shows `function`/`fn`; a human scanning mixed-language results wants `rust` | spec'd in 003, not in `Hit` |
| envelope-level `took_ms` | every screen wants to say how long it took; today only search's meta could carry it | not spec'd |
| a CLOSED enum for `Step.outcome` | the renderer special-cases the strings `ok`/`repaired`/`failed` and must guess at anything else (it degrades to a neutral glyph — tested) | open string today |
| a severity/area hint travelling with `error.code` | `Failure::http_status()` looks the code up in the catalog and returns 500 for one it does not know, which is exactly the case an older CLI hits against a newer daemon. A hint in the envelope would let an unknown FUTURE code still be coloured and grouped correctly | not spec'd |
| a place for non-fatal WARNINGS | workshop 003 says `--since`/`--role` are "no-ops with a warning" — but `ok: true` has nowhere to put one. Untyped `meta` is the only home | **gap** |

The warnings gap is the one worth deciding on soon: it is a whole class of
message the envelope cannot currently express, and every consumer will invent
its own convention if the contract does not.

### 3.4 Smaller notes

- `error.details` is a `BTreeMap`, so it renders alphabetically: `attempts`
  sorts above `host`. A human wants "host, port, then the diagnostics". Not
  worth a shape change — noted so nobody re-discovers it as a bug.
- `Hit` carries `repo` and `path` as `Option`s, and the address already embeds
  both. The renderer uses the address alone. Harmless redundancy, but if the row
  is ever slimmed, those two are the candidates.
- `Failure::render()` (`code: message\nfix: …`) exists in core and is exactly the
  right two lines for a log. It was NOT used here — the human screen needs the
  three parts separately so it can weight them. Both are correct; they are
  different renderers, and core's should stay.

---

## 4. What promotion into `fs3-cli` would take

Shape, not hours.

1. **Prerequisite:** §3.1 — payload DTOs into `fs3-core`. Everything else is a
   move; this is the only design decision.
2. `theme.rs` + `surfaces/` + `render.rs` move into `fs3-cli` as a `render`
   module. No new workspace crate, so `fs3-arch-check`'s crate-count assertion is
   untouched. (A separate `fs3-render` crate only earns its place if the daemon
   ever wants to serve pre-rendered text. It does not today.)
3. `mode.rs` becomes the CLI's output boundary: ~15 lines in `main.rs` plus
   `--json` / `--human` / `--color` on the root command. Today the CLI prints the
   envelope; after promotion it asks `Presentation::for_stdout(...)` first.
4. `arch-allowlist.toml` gains `comfy-table`, `owo-colors`, `anstream`,
   `textwrap` for `fs3-cli` only — the daemon and the core stay clean.
5. **One contract test comes with it:**
   `the_json_path_is_the_same_bytes_not_a_reserialisation`. The moment the human
   path and the machine path stop agreeing on the bytes, the human skin has
   started carrying truth of its own, and that test is the tripwire.
6. **Deliberately NOT decided here** (they are promotion decisions, not shelf
   decisions): whether `--human` exists at all or piped-rich is simply
   unsupported; whether `--width` is exposed to users or only to tests; whether
   `get`'s reading view is a fifth surface or a pager; whether `status --watch`
   arrives with `indicatif`.

---

## 5. Proof

`cargo test` — 46 tests: 39 beside the surfaces (what each screen SAYS) and 7
end-to-end through the real binary (the TTY strategy, byte-identical JSON
passthrough, `NO_COLOR`, non-zero exit on a rendered failure, a 60-column canvas
that still fits). `cargo clippy --all-targets -- -D warnings` is clean.
`transcript.ansi` is a captured run of all four screens.
