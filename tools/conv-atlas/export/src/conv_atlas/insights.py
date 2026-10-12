"""Agent-written findings: `insights/<id>.json` plus `insights/index.json`, outside any snapshot.

Insights are append-only. A document is validated against `$defs/insight` before
anything touches disk; it is published with an exclusive hard link (never
overwrites), and the index is rewritten under a file lock with tmp + rename,
so the app polling `index.json` never reads a half-written file and two
concurrent writers never lose each other's entry.
"""

from __future__ import annotations

import fcntl
import json
import os
import tempfile
import time
from contextlib import contextmanager
from pathlib import Path

import pandas as pd

from conv_atlas import bundle, contract, paths, view

# The smallest document `add` accepts; kind, created_ms and snapshot_id are filled in.
# Synthetic on purpose: this file ships in a public repo, so no corpus names or numbers.
EXAMPLE = {
    "id": "alpha-beta-bridge",
    "title": "alpha and beta share the most cross-repo neighbours",
    "author": "pij-calm-otter",
    "body_md": "More kNN edges join these two repos than any other pair.",
    "refs": {"repos": ["git:example.com/acme/alpha", "git:example.com/acme/beta"]},
}


@contextmanager
def _locked(directory: Path):
    with open(directory / ".index.lock", "a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)


def _write_atomic(path: Path, text: str) -> None:
    fd, tmp = tempfile.mkstemp(prefix=f".{path.name}-", dir=path.parent)
    with os.fdopen(fd, "w") as f:
        f.write(text)
    os.replace(tmp, path)


def _read_index(directory: Path) -> dict:
    path = directory / "index.json"
    if not path.exists():
        return {"schema_version": contract.SCHEMA_VERSION, "insights": []}
    return json.loads(path.read_text())


def ensure_index(home: Path | None = None) -> Path:
    """Create an empty `insights/index.json` if missing, so the polling app never 404s on a fresh home."""
    directory = paths.insights_dir(home)
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / "index.json"
    with _locked(directory):
        if not path.exists():
            _write_atomic(path, json.dumps(_read_index(directory), indent=1))
    return path


def complete(doc: dict, home: Path | None = None) -> dict:
    """Fill the fields an agent should not have to compute: kind, created_ms, snapshot_id (latest)."""
    out = dict(doc)
    out.setdefault("kind", "finding")
    out.setdefault("created_ms", int(time.time() * 1000))
    if "snapshot_id" not in out:
        out["snapshot_id"] = bundle.snapshot_dir(None, home).name
    return out


def add(doc: dict, home: Path | None = None) -> Path:
    """Validate and publish one insight; returns its path.

    Raises ValueError listing every schema violation at once, or on a duplicate id."""
    doc = complete(doc, home)
    errors = view.validation_errors("insights/<insight>.json", doc)  # the path only selects $defs/insight
    if errors:
        raise ValueError(f"invalid insight, {len(errors)} problem(s):\n  " + "\n  ".join(errors))
    directory = paths.insights_dir(home)
    directory.mkdir(parents=True, exist_ok=True)
    final = directory / f"{doc['id']}.json"

    fd, tmp = tempfile.mkstemp(prefix=f".{doc['id']}-", dir=directory)
    try:
        with os.fdopen(fd, "w") as f:
            json.dump(doc, f, indent=1, ensure_ascii=False)
        with _locked(directory):
            try:
                os.link(tmp, final)  # exclusive: fails if the id is taken
            except FileExistsError:
                raise ValueError(f"insight {doc['id']!r} already exists (insights are append-only)") from None
            index = _read_index(directory)
            entry = {k: doc[k] for k in ("id", "title", "kind", "created_ms")}
            index["insights"] = sorted(
                [e for e in index["insights"] if e["id"] != doc["id"]] + [entry],
                key=lambda e: (-e["created_ms"], e["id"]),
            )
            view.validate("insights/index.json", index)
            _write_atomic(directory / "index.json", json.dumps(index, indent=1, ensure_ascii=False))
    finally:
        os.unlink(tmp)
    return final


def unresolved_refs(doc: dict, home: Path | None = None) -> list[str]:
    """Refs that do not resolve in the insight's `snapshot_id`: the app cannot highlight them.

    Warnings, not errors: an insight may outlive the rows it cites."""
    refs, sid = doc.get("refs") or {}, doc.get("snapshot_id")
    try:
        tables = bundle.snapshot_dir(sid, home) / "tables"
    except FileNotFoundError as e:
        return [f"{e}: refs were not checked"]

    def values(table: str, column: str, filters=None) -> set:
        return set(pd.read_parquet(tables / f"{table}.parquet", columns=[column], filters=filters)[column].dropna())

    out = []
    wanted = {
        "guids": lambda: values("conversations", "guid"),
        "seats": lambda: values("seats", "seat"),
        "repos": lambda: values("conversations", "repo_key"),
        "tags": lambda: values("tags", "tag"),
    }
    for key, known in wanted.items():
        if refs.get(key):
            have = known()
            missing = [v for v in refs[key] if v not in have]
            if missing:
                out.append(f"refs.{key} not in snapshot {sid}: {', '.join(missing)}")
    if refs.get("addresses"):
        convs = [a for a in refs["addresses"] if "#" not in a]
        turn_refs = [a for a in refs["addresses"] if "#" in a]
        have = {f"conv:{g}" for g in values("conversations", "guid")} if convs else set()
        if turn_refs:
            have |= values("turns", "address", filters=[("address", "in", turn_refs)])
        missing = [a for a in refs["addresses"] if a not in have]
        if missing:
            out.append(f"refs.addresses not in snapshot {sid}: {', '.join(missing)}")
    if span := refs.get("time_range"):
        conv = pd.read_parquet(tables / "conversations.parquet", columns=["started_at", "last_turn_at"])
        lo, hi = (int(t.timestamp() * 1000) for t in (conv["started_at"].min(), conv["last_turn_at"].max()))
        if span["end_ms"] < lo or span["start_ms"] > hi:
            out.append(f"refs.time_range lies outside snapshot {sid} ({lo}..{hi} ms)")
    return out


def list_insights(home: Path | None = None) -> list[dict]:
    """Index entries, newest first."""
    directory = paths.insights_dir(home)
    return _read_index(directory)["insights"] if directory.exists() else []
