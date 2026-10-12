# conv-atlas

Visual and analytical atlas of the conversations flowspace3 has indexed. It has two parts:

- **`export/`**: a read-only, incremental exporter (Python). It turns the fs3 Postgres store into immutable **snapshots** with two layers:
  - an analysis layer (Parquet tables, vectors and a `datapackage.json` descriptor) for agents and data science;
  - a view layer (JSON) for the app.
- **`app/`**: a long-lived single-page app (Vite + React + TypeScript). It loads snapshots, notices new ones, and plays the corpus back through time. Views:
  - galaxy, similarity graph, seat network;
  - overview, timeline, session river, composition, tags.

Agents write findings back as **insights**. The open app shows them on the map, the timeline and in a guided tour.

The data format is frozen in [`contract/`](contract/README.md). Its authorities are `export/src/conv_atlas/contract.py` (tables) and `contract/bundle.schema.json` (view JSON).

## Data never enters this repo
Snapshots hold real conversation summaries and labels, so they live outside the repo:
`$CONV_ATLAS_HOME`, default `~/.local/share/flowspace3/conv-atlas/`, with `bundles/`, `insights/` and `cache/`.
Every committed fixture, example and test uses synthetic data (`conv-atlas fixture`).

## Quick start
```bash
cd tools/conv-atlas/export
uv run conv-atlas export          # read-only against the fs3 store; ~30 s incremental, ~60 s cold
uv run conv-atlas validate        # every table and view file against the contract

cd ../app
pnpm install
pnpm dev                          # http://localhost:5181, serves $CONV_ATLAS_HOME under /data
```
- **Long-lived page:** `pnpm build`, then `uv run --project ../export conv-atlas serve`. Leave it open: re-running `export` publishes a new snapshot, and the page offers it with a "New data" chip.
- **No fs3 store, or developing the UI:** run `pnpm fixture && pnpm dev:fixture`. That uses a deterministic synthetic corpus.
- **Simulate a re-export:** `uv run --project ../export conv-atlas fixture --home .fixture-home --conversations 260 --drift 0.3`.

### Connecting to the store
`export` connects to `CONV_ATLAS_DSN`, which defaults to the local fs3 store (`postgres://…@127.0.0.1:5433/flowspace3`).
Every session is `READ ONLY` with a `statement_timeout`. The exporter refuses to run unless the transaction is read-only.

## For agents
```bash
CA="uv run --project tools/conv-atlas/export conv-atlas"
$CA describe [table]              # data dictionary (JSON when piped)
$CA sql "select label, turns from conversations order by turns desc limit 10"
$CA insight add finding.json      # publish an insight; `$CA insight add --help` prints a valid example
```
Each snapshot ships a generated `README.md`: caveats, join keys, example queries, and how to go from a row to `flowspace3 get <address>`.

## Checks
`tools/conv-atlas/check.sh` covers Python (ruff, pytest) and the app (typecheck, vitest, build). It is separate from the cargo gate (`harness checks`).
