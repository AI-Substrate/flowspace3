"""`conv-atlas` command line. JSON envelope when stdout is not a TTY (or --json),
human text otherwise: the same audience rule as flowspace3.

Subcommands register themselves in COMMANDS; each returns (data, next_action)
or raises. Owners: fixture/serve/validate = core; export/describe/sql/insight = Cartographer.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from collections.abc import Callable
from pathlib import Path

from conv_atlas import bundle, paths

Handler = Callable[[argparse.Namespace], tuple[object, str | None]]
COMMANDS: dict[str, tuple[Callable[[argparse.ArgumentParser], None], Handler, str]] = {}


def command(name: str, help_text: str):
    def wrap(fn):
        def configure(p: argparse.ArgumentParser) -> None:
            for args, kwargs in getattr(fn, "_args", []):
                p.add_argument(*args, **kwargs)

        COMMANDS[name] = (configure, fn, help_text)
        return fn

    return wrap


def arg(*args, **kwargs):
    def wrap(fn):
        fn._args = [(args, kwargs), *getattr(fn, "_args", [])]
        return fn

    return wrap


@command("fixture", "write a deterministic synthetic snapshot (no real content)")
@arg("--home", type=Path, help="data home (default: $CONV_ATLAS_HOME or ~/.local/share/flowspace3/conv-atlas)")
@arg("--conversations", type=int, default=240, help="a larger n with the same seed keeps every earlier guid")
@arg("--seed", type=int, default=7)
@arg("--drift", type=float, default=0.0, help="layout jitter for matched points (simulates a re-export)")
@arg("--id", dest="snapshot_id", help="snapshot id YYYYMMDDTHHMMSSZ (default: now)")
@arg("--no-insights", action="store_true", help="skip seeding the synthetic demo insights")
def _fixture(a: argparse.Namespace):
    from conv_atlas import fixture, insights

    data = fixture.generate(n_conversations=a.conversations, seed=a.seed, drift=a.drift)
    data.snapshot_id = a.snapshot_id
    out = bundle.write_snapshot(data, a.home)
    seeded = []
    if not a.no_insights:
        existing = {i["id"] for i in insights.list_insights(a.home)}
        for doc in fixture.demo_insights(data):
            if doc["id"] not in existing:
                insights.add({**doc, "snapshot_id": out.name}, a.home)
                seeded.append(doc["id"])
    result = {"snapshot": out.name, "path": str(out), "insights_seeded": seeded}
    return result, "`conv-atlas serve` (or `pnpm dev` in app/) to view it"


@command("validate", "check a snapshot's tables and view files against the contract")
@arg("--home", type=Path)
@arg("--snapshot", help="snapshot id (default: latest)")
def _validate(a: argparse.Namespace):
    import pandas as pd

    from conv_atlas import contract, view

    root = bundle.snapshot_dir(a.snapshot, a.home)
    tables = {n: pd.read_parquet(root / "tables" / f"{n}.parquet") for n in contract.TABLES}
    bundle.validate_tables(tables)
    checked = 0
    for f in sorted((root / "view").rglob("*.json")):
        view.validate(str(f.relative_to(root / "view")), json.loads(f.read_text()))
        checked += 1
    return {"snapshot": root.name, "tables": len(tables), "view_files": checked}, None


@command("serve", "serve the built app and the data home for a long-lived page")
@arg("--home", type=Path)
@arg("--port", type=int, default=5180)
@arg("--host", default="127.0.0.1")
def _serve(a: argparse.Namespace):
    from conv_atlas import serve

    serve.run(paths.home(a.home), a.host, a.port)
    return {"stopped": True}, None


def _emit(ok: bool, name: str, data: object, next_action: str | None, as_json: bool, error: str | None = None) -> None:
    if as_json:
        env = {"ok": ok, "command": name, "v": 1}
        env.update({"data": data} if ok else {"error": error})
        if next_action:
            env["next_action"] = next_action
        print(json.dumps(env, indent=2, default=str))
    elif ok:
        print(json.dumps(data, indent=2, default=str) if not isinstance(data, str) else data)
        if next_action:
            print(f"\nnext: {next_action}")
    else:
        print(f"conv-atlas {name}: {error}", file=sys.stderr)


def main(argv: list[str] | None = None) -> int:
    # importing registers Cartographer's commands without core editing this file
    try:
        import conv_atlas.commands  # noqa: F401
    except ModuleNotFoundError as e:
        if e.name != "conv_atlas.commands":
            raise

    parser = argparse.ArgumentParser(prog="conv-atlas")
    parser.add_argument("--json", action="store_true", help="force the JSON envelope")
    sub = parser.add_subparsers(dest="cmd", required=True)
    for name, (configure, _, help_text) in COMMANDS.items():
        configure(sub.add_parser(name, help=help_text))
    a = parser.parse_args(argv)
    as_json = a.json or os.environ.get("CONV_ATLAS_OUTPUT") == "json" or not sys.stdout.isatty()
    _, handler, _ = COMMANDS[a.cmd]
    try:
        data, next_action = handler(a)
    except (bundle.ContractError, FileNotFoundError, ValueError) as e:
        _emit(False, a.cmd, None, None, as_json, str(e))
        return 1
    _emit(True, a.cmd, data, next_action, as_json)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
