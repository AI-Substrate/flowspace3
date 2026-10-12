"""Write one immutable snapshot: analysis tables, vectors, view JSON, descriptor, manifest.

`bundle.write_snapshot` is the only writer of bundle files. Producers (the prod
extractor and the synthetic fixture) hand it a `SnapshotData`; it validates the
tables against `contract.TABLES`, derives the view layer, validates that against
`contract/bundle.schema.json`, and only then publishes the snapshot by renaming
its directory into place and rewriting `manifest.json` atomically. A reader
polling the manifest therefore never sees a half-written snapshot.
"""

from __future__ import annotations

import json
import os
import shutil
import tempfile
import time
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path

import numpy as np
import pandas as pd

from conv_atlas import contract, paths, readme, view

DAY_MS = 86_400_000


class ContractError(ValueError):
    """A table or view file does not match the frozen contract."""


@dataclass
class Vectors:
    centroids: np.ndarray  # float32 [N, D], not centred
    centroid_guids: list[str]
    mean: np.ndarray  # float32 [D]
    turn_vectors: np.ndarray | None = None  # float16 [M, D]
    turn_addresses: list[str] | None = None


@dataclass
class SnapshotData:
    source: str  # "prod" | "synthetic"
    tables: dict[str, pd.DataFrame]
    vectors: Vectors
    provenance: dict = field(default_factory=dict)
    knn_k: int = 10
    snapshot_id: str | None = None  # default: now, UTC


def new_snapshot_id(now: datetime | None = None) -> str:
    return (now or datetime.now(UTC)).strftime("%Y%m%dT%H%M%SZ")


def validate_tables(tables: dict[str, pd.DataFrame]) -> None:
    missing = set(contract.TABLES) - set(tables)
    if missing:
        raise ContractError(f"missing tables: {sorted(missing)}")
    for name, df in tables.items():
        spec = contract.TABLES.get(name)
        if spec is None:
            raise ContractError(f"unknown table {name!r}")
        if list(df.columns) != spec.columns:
            raise ContractError(f"{name}: columns {list(df.columns)} != contract {spec.columns}")
        for f in spec.fields:
            col = df[f.name]
            if f.required and col.isna().any():
                raise ContractError(f"{name}.{f.name} is required but has {int(col.isna().sum())} nulls")
            if f.type == "datetime" and len(col) and not isinstance(col.dtype, pd.DatetimeTZDtype):
                raise ContractError(f"{name}.{f.name} must be tz-aware datetime, got {col.dtype}")
        if len(df) and df.duplicated(list(spec.primary_key)).any():
            raise ContractError(f"{name}: duplicate primary key {spec.primary_key}")
    guids = set(tables["conversations"]["guid"])
    for name in ("turns", "activity_daily", "session_shifts", "session_novelty"):
        stray = set(tables[name]["guid"]) - guids
        if stray:
            raise ContractError(f"{name}.guid references {len(stray)} unknown conversations")


def _write_tables(root: Path, tables: dict[str, pd.DataFrame]) -> None:
    out = root / "tables"
    out.mkdir(parents=True)
    for name, df in tables.items():
        df.to_parquet(out / f"{name}.parquet", index=False, compression="zstd")


def _write_vectors(root: Path, v: Vectors) -> set[str]:
    out = root / "vectors"
    out.mkdir(parents=True)
    if v.centroids.shape[0] != len(v.centroid_guids):
        raise ContractError("centroids rows != centroid_guids")
    np.save(out / "conversation_centroids.npy", v.centroids.astype(np.float32))
    pd.DataFrame({"row": range(len(v.centroid_guids)), "guid": v.centroid_guids}).to_parquet(
        out / "conversation_centroids.index.parquet", index=False
    )
    np.save(out / "embedding_mean.npy", v.mean.astype(np.float32))
    written = {"conversation_centroids.npy", "conversation_centroids.index.parquet", "embedding_mean.npy"}
    if v.turn_vectors is not None:
        if v.turn_addresses is None or v.turn_vectors.shape[0] != len(v.turn_addresses):
            raise ContractError("turn_vectors rows != turn_addresses")
        np.save(out / "turn_vectors.f16.npy", v.turn_vectors.astype(np.float16))
        pd.DataFrame({"row": range(len(v.turn_addresses)), "address": v.turn_addresses}).to_parquet(
            out / "turn_vectors.index.parquet", index=False
        )
        written |= {"turn_vectors.f16.npy", "turn_vectors.index.parquet"}
    return written


def _write_json(path: Path, obj: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, separators=(",", ":"), ensure_ascii=False))


def write_snapshot(data: SnapshotData, home: Path | None = None) -> Path:
    """Validate, write and publish a snapshot. Returns its directory."""
    validate_tables(data.tables)
    bundles = paths.bundles_dir(home)
    bundles.mkdir(parents=True, exist_ok=True)
    snap_id = data.snapshot_id or new_snapshot_id()
    final = bundles / snap_id
    if final.exists():
        raise ContractError(f"snapshot {snap_id} already exists")

    created_ms = int(time.time() * 1000)
    staging = Path(tempfile.mkdtemp(prefix=f".{snap_id}-", dir=bundles))
    try:
        _write_tables(staging, data.tables)
        present = set(contract.TABLES) | _write_vectors(staging, data.vectors)
        provenance = {
            "snapshot_id": snap_id,
            "created_at": datetime.fromtimestamp(created_ms / 1000, UTC).isoformat(),
            "source": data.source,
            **data.provenance,
        }
        descriptor = contract.datapackage(snap_id, provenance, present)
        _write_json(staging / "datapackage.json", descriptor)
        (staging / "README.md").write_text(readme.render(descriptor, data.tables))

        files = view.derive(snap_id, created_ms, data.source, data.tables, data.provenance, data.knn_k)
        for rel, obj in files.items():
            view.validate(rel, obj, sample=True)
            _write_json(staging / "view" / rel, obj)

        os.rename(staging, final)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise

    _publish(bundles, snap_id, created_ms, data)
    return final


def _publish(bundles: Path, snap_id: str, created_ms: int, data: SnapshotData) -> None:
    manifest_path = bundles / "manifest.json"
    manifest = (
        json.loads(manifest_path.read_text())
        if manifest_path.exists()
        else {"schema_version": contract.SCHEMA_VERSION, "latest": snap_id, "snapshots": []}
    )
    conv = data.tables["conversations"]
    entry = {
        "id": snap_id,
        "created_ms": created_ms,
        "source": data.source,
        "counts": {
            "conversations": len(conv),
            "turns": int(conv["turns"].sum()),
            "seats": len(data.tables["seats"]),
            "communities": len(data.tables["communities"]),
        },
        "time_range": {
            "start_ms": view.to_ms(conv["started_at"].min()),
            "end_ms": view.to_ms(conv["last_turn_at"].max()),
        },
    }
    manifest["snapshots"] = [s for s in manifest["snapshots"] if s["id"] != snap_id] + [entry]
    manifest["snapshots"].sort(key=lambda s: s["id"])
    manifest["latest"] = manifest["snapshots"][-1]["id"]
    view.validate("manifest.json", manifest)
    tmp = manifest_path.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(manifest, indent=1))
    os.replace(tmp, manifest_path)


def snapshot_dir(snapshot_id: str | None = None, home: Path | None = None) -> Path:
    """Resolve a snapshot directory; None means the manifest's latest."""
    bundles = paths.bundles_dir(home)
    if snapshot_id is None:
        manifest_path = bundles / "manifest.json"
        if not manifest_path.exists():
            raise FileNotFoundError(f"no manifest at {manifest_path}: run `conv-atlas export` first")
        snapshot_id = json.loads(manifest_path.read_text())["latest"]
    path = bundles / snapshot_id
    if not path.is_dir():
        raise FileNotFoundError(f"snapshot {snapshot_id} not found under {bundles}")
    return path
