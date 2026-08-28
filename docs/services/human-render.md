# human-render — terminal presentation over the envelope

**Status**: unit u-r implementation for plan 007. Boundary:
`fs3_cli::render::render(&Envelope<Value>) -> Option<String>`.

## Contract

The JSON envelope is produced and serialized before rendering. The renderer has
no store, daemon, or filesystem access: every fact on screen comes from that
envelope. `None` declines a command or payload shape and the caller prints the
already-serialized JSON instead.

Output selection is centralized in `fs3_core::output::resolve`:

- terminal: human output;
- pipe, file, CI capture, or agent subprocess: JSON with no flag;
- `--json`: JSON anywhere;
- `--human`: human output anywhere;
- `FS3_OUTPUT=json`: pins JSON for harnesses whose PTY looks human.

Covered commands are search, status, doctor, add, scan, get, tree, remove, gc,
conversation list, docs list, agents-start-here, plus every failed envelope.

## Snap-in recipe

1. Keep `pub mod render;` in `crates/cli/src/lib.rs`.
2. In `crates/cli/src/main.rs::emit`, keep `serde_json::to_string_pretty`
   first. For `OutputMode::Human`, deserialize those bytes as
   `Envelope<Value>`, call `render::render`, and write `Some(screen)` through
   `anstream::stdout()`. On `None`, print the serialized JSON unchanged.
3. In only the `Add` and `Scan` match arms, when output mode is human, wrap the
   existing POST future with
   `render::progress::while_pending(client.base_url(), future)`. JSON mode calls
   the existing client method directly.
4. Keep the four `fs3-cli` dependency edges and matching
   `crates/testkit/arch-allowlist.toml` rows:
   `anstream`, `comfy-table`, `owo-colors`, `textwrap`.

Do not copy envelope production into the renderer, move `emit`, or add a second
output-mode decision.

## Add/scan meter composition assumptions

The live stream is `GET /events` on the same base URL as the POST. Its first
line is `fs3_core::events::Hello`; later NDJSON lines are
`fs3_core::events::Event`. Only `EventKind::ScanProgress { root, root_path,
files_seen, enqueued, current }` is drawn.

The subscription and POST are polled concurrently. Stream connection failure,
404, malformed data, or slow acceptance is silent and never retried. The stream
never gates the POST. Progress is written only to stderr in human mode; its
single terminal line is erased when the POST settles, before the envelope is
printed. JSON stdout is therefore byte-identical whether `/events` exists or
not.

Unit tests use `crates/cli/tests/fixtures/scan-progress.ndjson`. Composition must
re-run the real-binary output matrix and `envelope_goldens` after u-w's live
endpoint is merged.

## Known contract feedback

Search `meta.total`/`showing` and hit `lang` are not in the current typed payload;
the renderer does not invent them. Non-fatal warnings also have no typed place
in an `ok: true` envelope. These are envelope decisions, not renderer-side
fetches.
