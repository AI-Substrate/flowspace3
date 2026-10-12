"""`conv-atlas export`: one read-only, incremental run from the prod store to a published snapshot.

Incremental state lives in `paths.cache_dir()/v1` (never a deliverable):
- `state.json`: per-conversation `(max(turn_no), count)`, the shared-hash stop-set, a digest of the
  conversation metadata, the run parameters and the database time of the last pull;
- `turns.parquet`: extracted turn rows (no bodies), `centroids.npz`, `workvec/<guid>.npz`.
A conversation is re-pulled when its head changes or when one of its turns gained a summary or
vector since the last pull; centroids are also recomputed when the shared-hash stop-set moved under
it. With nothing changed the run publishes nothing (`{"unchanged": true}`) unless `force`.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import time
from collections.abc import Callable, Iterable, Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import asdict
from pathlib import Path

import numpy as np
import pandas as pd

from conv_atlas import bundle, extract, insights, labels, model, paths

CACHE_VERSION = 1
# any change to the extraction SQL invalidates cached rows (they were shaped by the old queries)
EXTRACT_FINGERPRINT = hashlib.sha256(Path(extract.__file__).read_bytes()).hexdigest()[:16]
WORKERS = 2
TURN_BATCH = 60_000  # turns per extraction query
CENTROID_BATCH = 150  # conversations per centroid query
VECTOR_BATCH = 20_000  # work turns per vector query

KNOWN_DEFECTS = [
    "DL-029: Write/Edit tool_call text stores whole file bodies (intake elision bug). tool_call and "
    "tool_result turns are excluded from every vector, centroid and layout; do not trust their text.",
    "Summaries lag ingestion: unsummarised work turns carry `head` (first 280 chars) instead of `summary`; "
    "see stats.coverage_summary.",
    "omp transcripts from 2026-09-07 store human/peer turns as role=agent (fs3 intake bug; reported to "
    "o-prime). See stats.omp_sessions_zero_human out of stats.omp_sessions_ge50_since_0907.",
]


def _git_sha() -> str | None:
    try:
        sha = subprocess.run(
            ["git", "-C", str(paths.TOOL_ROOT), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
        ).stdout.strip()
        dirty = subprocess.run(
            ["git", "-C", str(paths.TOOL_ROOT), "status", "--porcelain", "--", "."],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None
    return f"{sha}-dirty" if dirty else sha


def _batches(items: Sequence[str], weight: Callable[[str], int], cap: int) -> list[list[str]]:
    out: list[list[str]] = []
    cur: list[str] = []
    total = 0
    for g in items:
        w = max(int(weight(g)), 1)
        if cur and total + w > cap:
            out.append(cur)
            cur, total = [], 0
        cur.append(g)
        total += w
    if cur:
        out.append(cur)
    return out


def _parallel(url: str, fn: Callable, batches: Iterable[list[str]], *args) -> list:
    def run(batch: list[str]):
        with extract.connect(url) as conn:
            return fn(conn, batch, *args)

    with ThreadPoolExecutor(max_workers=WORKERS) as pool:
        return list(pool.map(run, list(batches)))


def _digest(df: pd.DataFrame) -> str:
    return hashlib.sha256(df.sort_values("guid").to_json(orient="records", date_format="iso").encode()).hexdigest()


class Cache:
    def __init__(self, root: Path):
        self.root = root
        self.state: dict = {}
        self.turns: pd.DataFrame | None = None
        self.centroids: dict[str, tuple[int, np.ndarray]] = {}

    @classmethod
    def load(cls, root: Path) -> Cache:
        c = cls(root)
        state = root / "state.json"
        if not state.exists():
            return c
        s = json.loads(state.read_text())
        if s.get("version") != CACHE_VERSION or s.get("extract") != EXTRACT_FINGERPRINT:
            return c
        c.state = s
        c.turns = pd.read_parquet(root / "turns.parquet")
        z = np.load(root / "centroids.npz")
        c.centroids = {g: (int(n), v) for g, n, v in zip(z["guids"], z["counts"], z["vecs"], strict=True)}
        return c

    def workvec(self, guid: str, head: tuple[int, int]) -> tuple[np.ndarray, np.ndarray] | None:
        p = self.root / "workvec" / f"{guid}.npz"
        if not p.exists():
            return None
        z = np.load(p)
        if tuple(int(x) for x in z["head"]) != tuple(head):
            return None
        return z["turn_nos"], z["vecs"]

    def save(
        self,
        state: dict,
        turns: pd.DataFrame,
        centroids: dict[str, tuple[int, np.ndarray]],
        workvecs: dict[str, tuple[np.ndarray, np.ndarray]],
        heads: dict[str, tuple[int, int]],
    ) -> None:
        """Write everything to a sibling dir, then swap it in (a crash leaves the old cache intact)."""
        staging = self.root.with_name(self.root.name + ".new")
        shutil.rmtree(staging, ignore_errors=True)
        (staging / "workvec").mkdir(parents=True)
        turns.to_parquet(staging / "turns.parquet", index=False, compression="zstd")
        guids = sorted(centroids)
        dim = next(iter(centroids.values()))[1].shape[0] if centroids else 0
        np.savez(
            staging / "centroids.npz",
            guids=np.array(guids, dtype=str),
            counts=np.array([centroids[g][0] for g in guids], dtype=np.int64),
            vecs=np.stack([centroids[g][1] for g in guids]) if guids else np.zeros((0, dim), np.float32),
        )
        old = self.root / "workvec"
        for g, (nos, vecs) in workvecs.items():
            src = old / f"{g}.npz"
            dst = staging / "workvec" / f"{g}.npz"
            if src.exists() and self.workvec(g, heads[g]) is not None:
                os.link(src, dst)
            else:
                np.savez(dst, turn_nos=nos, vecs=vecs, head=np.array(heads[g]))
        (staging / "state.json").write_text(json.dumps(state, indent=1))
        retired = self.root.with_name(self.root.name + ".old")
        shutil.rmtree(retired, ignore_errors=True)
        if self.root.exists():
            os.rename(self.root, retired)
        os.rename(staging, self.root)
        shutil.rmtree(retired, ignore_errors=True)


def previous_snapshot(home: Path | None) -> tuple[str | None, model.Previous | None]:
    """The newest prod snapshot's layout, raw centroids and clusters, so a re-export keeps places, ids and labels."""
    manifest = paths.bundles_dir(home) / "manifest.json"
    if not manifest.exists():
        return None, None
    prod = [s for s in json.loads(manifest.read_text())["snapshots"] if s["source"] == "prod"]
    if not prod:
        return None, None
    sid = prod[-1]["id"]
    root = paths.bundles_dir(home) / sid
    conv = pd.read_parquet(root / "tables" / "conversations.parquet", columns=["guid", "x", "y", "community_id"])
    placed = conv.dropna(subset=["x", "y"])
    clustered = conv.dropna(subset=["community_id"])
    index = pd.read_parquet(root / "vectors" / "conversation_centroids.index.parquet")
    vecs = np.load(root / "vectors" / "conversation_centroids.npy")
    return sid, model.Previous(
        xy={g: (float(x), float(y)) for g, x, y in zip(placed["guid"], placed["x"], placed["y"], strict=True)},
        centroids={g: vecs[r] for r, g in zip(index["row"], index["guid"], strict=True)},
        communities={g: int(c) for g, c in zip(clustered["guid"], clustered["community_id"], strict=True)},
        community_rows=pd.read_parquet(root / "tables" / "communities.parquet"),
    )


def run(
    home: Path | None = None,
    full: bool = False,
    turn_vectors: bool = False,
    sessions: int = model.Params.sessions,
    force: bool = False,
    url: str | None = None,
) -> dict:
    url = url or extract.dsn()
    p = model.Params(sessions=sessions)
    timings: dict[str, float] = {}
    t0 = tick = time.monotonic()

    def lap(name: str) -> None:
        nonlocal tick
        now = time.monotonic()
        timings[name] = round(now - tick, 2)
        tick = now

    root = paths.cache_dir(home) / f"v{CACHE_VERSION}"
    cache = Cache(root) if full else Cache.load(root)
    state = cache.state
    run_params = {**asdict(p), "turn_vectors": turn_vectors}

    with extract.connect(url) as conn:
        db_now = conn.execute("select now()").fetchone()[0]
        heads_df = extract.turn_heads(conn)
        conv_meta = extract.conversations(conn)
        shared = extract.shared_work_hashes(conn)
        heads = {g: (int(m), int(n)) for g, m, n in heads_df.itertuples(index=False)}
        prev_heads = {g: tuple(v) for g, v in state.get("heads", {}).items()}
        dirty = {g for g, h in heads.items() if prev_heads.get(g) != h}
        late: set[str] = set()
        if state:
            late = extract.enriched_since(conn, pd.Timestamp(state["pulled_at"])) & (set(heads) - dirty)
            dirty |= late
    removed = set(prev_heads) - set(heads)
    lap("probe")

    conv_digest = _digest(conv_meta)
    shared_delta = shared ^ set(state.get("shared", []))
    latest_sid, previous = previous_snapshot(home)
    unchanged = (
        bool(state)
        and not dirty
        and not removed
        and not shared_delta
        and state.get("conv_digest") == conv_digest
        and state.get("params") == run_params
        and latest_sid is not None
        and state.get("snapshot_id") == latest_sid
    )
    if unchanged and not force:
        return {"unchanged": True, "latest": latest_sid, "conversations": len(heads), "timings_s": timings}

    # turns: keep cached rows of clean conversations, pull the rest
    keep = cache.turns[~cache.turns["guid"].isin(dirty | removed)] if cache.turns is not None else None
    order = sorted(dirty, key=lambda g: g)
    pulled = _parallel(url, extract.turns, _batches(order, lambda g: heads[g][1], TURN_BATCH))
    frames = [f for f in [keep, *pulled] if f is not None and len(f)]
    raw = pd.concat(frames, ignore_index=True) if frames else pd.DataFrame()
    lap("turns")

    # centroids: recompute for pulled conversations and those whose work hashes entered/left the stop-set
    restale = (
        set(raw.loc[raw["class"].eq("work") & raw["raw_hash"].isin(shared_delta), "guid"]) if shared_delta else set()
    )
    recentroid = sorted((dirty | restale) & set(heads))
    centroids = {g: v for g, v in cache.centroids.items() if g in heads and g not in recentroid}
    for part in _parallel(url, extract.centroids, _batches(recentroid, lambda g: 1, CENTROID_BATCH), sorted(shared)):
        centroids.update(part)
    lap("centroids")

    # per-turn work vectors: the detailed sessions, or every conversation with --turn-vectors
    work_n = raw.loc[raw["class"].eq("work")].groupby("guid").size()
    want = sorted(heads) if turn_vectors else model.detail_guids(raw, p.sessions)
    workvecs: dict[str, tuple[np.ndarray, np.ndarray]] = {}
    fetch = []
    for g in want:
        hit = None if g in dirty else cache.workvec(g, heads[g])
        if hit is None:
            fetch.append(g)
        else:
            workvecs[g] = hit
    for part in _parallel(url, extract.work_vectors, _batches(fetch, lambda g: int(work_n.get(g, 1)), VECTOR_BATCH)):
        workvecs.update(part)
    lap("work_vectors")

    built = model.build(model.Extracted(conv_meta, raw, shared, centroids, workvecs), p, labels, previous, turn_vectors)
    lap("model")

    provenance = {
        "database": extract.db_identity(url),
        "git_sha": _git_sha(),
        "embedder": extract.EMBEDDER,
        "stopset": model.stopset_stats(raw, shared, extract.SHARED_HASH_MIN_CONVERSATIONS),
        "known_defects": KNOWN_DEFECTS,
        "labels": labels.provenance(),
        "incremental": {
            "previous_snapshot": latest_sid,
            "pulled_conversations": len(dirty),
            "late_enriched_conversations": len(late),
            "recentroided_conversations": len(recentroid),
            "fetched_vector_conversations": len(fetch),
            "full": full or not state,
        },
        **built.provenance,
    }
    data = bundle.SnapshotData(
        source="prod", tables=built.tables, vectors=built.vectors, provenance=provenance, knn_k=p.knn_k
    )
    provenance["timings_s"] = dict(timings)  # extraction + model; write/cache times are in the command output
    out = bundle.write_snapshot(data, home)
    insights.ensure_index(home)
    lap("write")

    cache.save(
        {
            "version": CACHE_VERSION,
            "extract": EXTRACT_FINGERPRINT,
            "pulled_at": db_now.isoformat(),
            "heads": {g: list(h) for g, h in heads.items()},
            "shared": sorted(shared),
            "conv_digest": conv_digest,
            "params": run_params,
            "snapshot_id": out.name,
        },
        raw,
        centroids,
        {g: workvecs[g] for g in want if g in workvecs},
        heads,
    )
    lap("cache")
    timings["total"] = round(time.monotonic() - t0, 2)
    return {
        "unchanged": False,
        "snapshot": out.name,
        "path": str(out),
        "conversations": len(built.tables["conversations"]),
        "embedded": len(built.vectors.centroid_guids),
        "pulled_conversations": len(dirty),
        "timings_s": timings,
    }
