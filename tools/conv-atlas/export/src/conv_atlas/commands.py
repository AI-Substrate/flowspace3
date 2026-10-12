"""Cartographer's CLI verbs, registered into `conv_atlas.cli` on import."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from conv_atlas import bundle, insights
from conv_atlas.cli import COMMANDS, arg, command


@command("export", "read-only incremental export from the flowspace3 store to a new snapshot")
@arg("--home", type=Path)
@arg("--full", action="store_true", help="ignore the incremental cache and re-pull everything")
@arg("--turn-vectors", action="store_true", help="also write float16 work-turn vectors (~340 MB)")
@arg("--sessions", type=int, default=40, help="long sessions to detail (novelty, shifts, layout)")
@arg("--force", action="store_true", help="publish a snapshot even when nothing changed")
def _export(a: argparse.Namespace):
    from conv_atlas import export

    out = export.run(a.home, a.full, a.turn_vectors, a.sessions, a.force)
    if out["unchanged"]:
        return out, "nothing changed since the latest snapshot; `--force` publishes anyway"
    return out, f"`conv-atlas validate --snapshot {out['snapshot']}`, then `conv-atlas describe`"


def _snapshot(snapshot: str | None, home: Path | None) -> tuple[Path, dict]:
    root = bundle.snapshot_dir(snapshot, home)
    return root, json.loads((root / "datapackage.json").read_text())


def describe(table: str | None = None, snapshot: str | None = None, home: Path | None = None) -> dict:
    """The data dictionary from datapackage.json, with row counts from the Parquet footers."""
    import pyarrow.parquet as pq

    root, dp = _snapshot(snapshot, home)
    resources = {r["name"]: r for r in dp["resources"]}
    rows = {n: pq.read_metadata(root / r["path"]).num_rows for n, r in resources.items()}
    if table is not None:
        if table not in resources:
            raise ValueError(f"unknown table {table!r}; tables: {', '.join(resources)}")
        return {"snapshot": root.name, "rows": rows[table], **resources[table]}
    return {
        "snapshot": root.name,
        "schema_version": dp["conv_atlas_schema_version"],
        "tables": [
            {
                "name": n,
                "description": r["description"],
                "rows": rows[n],
                "primary_key": r["schema"]["primaryKey"],
                "fields": [f["name"] for f in r["schema"]["fields"]],
            }
            for n, r in resources.items()
        ],
        "vector_views": [v for v in VECTOR_VIEWS if (root / "vectors" / VECTOR_VIEWS[v]).exists()],
        "vectors": dp.get("vectors", {}),
        "provenance": dp.get("provenance", {}),
    }


# row -> key index tables beside the .npy arrays, exposed as views too
VECTOR_VIEWS = {
    "conversation_centroids_index": "conversation_centroids.index.parquet",
    "turn_vectors_index": "turn_vectors.index.parquet",
}


def sql(query: str, snapshot: str | None = None, home: Path | None = None, limit: int = 200) -> dict:
    """Run DuckDB SQL with every table registered as a view; rows as JSON-ready dicts."""
    import duckdb

    root, dp = _snapshot(snapshot, home)
    con = duckdb.connect()
    con.execute("SET TimeZone = 'UTC'")
    views = {r["name"]: root / r["path"] for r in dp["resources"]}
    views |= {v: root / "vectors" / f for v, f in VECTOR_VIEWS.items() if (root / "vectors" / f).exists()}
    for name, path in views.items():
        con.execute(f"create view {name} as select * from read_parquet('{path.as_posix()}')")
    try:
        rel = con.sql(query)
    except duckdb.Error as e:
        raise ValueError(f"{type(e).__name__}: {e}") from None
    if rel is None:  # a statement with no result set
        return {"snapshot": root.name, "columns": [], "rows": [], "row_count": 0, "truncated": False}
    df = (rel.limit(limit + 1) if limit > 0 else rel).df()
    truncated = limit > 0 and len(df) > limit
    df = df.iloc[:limit] if truncated else df
    rows = json.loads(df.to_json(orient="records", date_format="iso", date_unit="ms"))
    return {
        "snapshot": root.name,
        "columns": list(df.columns),
        "rows": rows,
        "row_count": len(rows),
        "truncated": truncated,
    }


@command("describe", "the snapshot's data dictionary (tables, fields, keys, provenance) as JSON")
@arg("table", nargs="?", help="one table (default: all tables, summarised)")
@arg("--home", type=Path)
@arg("--snapshot", help="snapshot id (default: latest)")
def _describe(a: argparse.Namespace):
    hint = 'conv-atlas sql "select * from <table> limit 5"' if a.table is None else None
    return describe(a.table, a.snapshot, a.home), hint


@command("sql", "run DuckDB SQL over a snapshot; every table is a pre-registered view")
@arg("query", help='e.g. "select label, turns from conversations order by turns desc"')
@arg("--home", type=Path)
@arg("--snapshot", help="snapshot id (default: latest)")
@arg("--limit", type=int, default=200, help="max rows returned (0 = no limit)")
def _sql(a: argparse.Namespace):
    out = sql(a.query, a.snapshot, a.home, a.limit)
    hint = f"truncated at {a.limit} rows: aggregate, add LIMIT, or pass --limit 0" if out["truncated"] else None
    return out, hint


@command("insight", "add an agent finding (validated, append-only) or list them")
@arg("action", choices=["add", "list"])
@arg("file", nargs="?", help="insight JSON file for `add` ('-' = stdin)")
@arg("--home", type=Path)
def _insight(a: argparse.Namespace):
    if a.action == "list":
        items = insights.list_insights(a.home)
        return {"insights": items, "count": len(items)}, None
    if not a.file:
        raise ValueError("insight add needs a file: conv-atlas insight add finding.json (see --help for an example)")
    source = "stdin" if a.file == "-" else a.file
    text = sys.stdin.read() if a.file == "-" else Path(a.file).read_text()
    try:
        doc = insights.complete(json.loads(text), a.home)
        path = insights.add(doc, a.home)
    except json.JSONDecodeError as e:
        raise ValueError(f"{source}: not JSON: {e}") from None
    except ValueError as e:
        raise ValueError(f"{source}: {e}") from None
    warnings = insights.unresolved_refs(doc, a.home)
    hint = "the open app picks it up from insights/index.json"
    if warnings:
        hint = f"published, but {len(warnings)} ref group(s) do not resolve in {doc['snapshot_id']}: fix with a new id"
    return {"id": doc["id"], "path": str(path), "warnings": warnings}, hint


_EXAMPLE_HELP = f"""minimal insight (kind, created_ms and snapshot_id default to finding, now and the latest snapshot;
refs needs at least one of guids, addresses, seats, repos, tags, time_range; kinds: finding question anomaly story):

{json.dumps(insights.EXAMPLE, indent=2)}

  conv-atlas insight add finding.json     every schema problem is reported at once;
                                          refs missing from the snapshot are warned about"""


def _insight_parser(p: argparse.ArgumentParser, configure=COMMANDS["insight"][0]) -> None:
    configure(p)
    p.formatter_class = argparse.RawDescriptionHelpFormatter
    p.epilog = _EXAMPLE_HELP


COMMANDS["insight"] = (_insight_parser, *COMMANDS["insight"][1:])
