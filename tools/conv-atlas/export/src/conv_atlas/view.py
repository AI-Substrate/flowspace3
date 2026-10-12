"""Derive the SPA view layer (view/*.json) from the analysis tables, and validate it.

The view layer swaps stable keys for array indices and timestamps for epoch
milliseconds, because the app renders from typed arrays. It holds no facts the
tables lack. `contract/bundle.schema.json` is its schema, and the TypeScript
types in `app/src/contract/types.ts` mirror that schema.
"""

from __future__ import annotations

import json
from functools import cache
from pathlib import Path

import pandas as pd
from jsonschema import Draft202012Validator

from conv_atlas import contract, paths

DAY_MS = 86_400_000
LABEL_MAX = 400
TAGS_MAX = 12


def to_ms(ts) -> int:
    return int(pd.Timestamp(ts).value // 1_000_000)


def _opt(v):
    """None/NaN/NaT/pd.NA -> None; numpy scalars -> python. Scalars only."""
    if v is None or pd.isna(v):
        return None
    return v.item() if hasattr(v, "item") else v


def _list(v) -> list:
    return [] if v is None else [x.item() if hasattr(x, "item") else x for x in v]


def repo_label(key: str) -> str:
    if key.startswith("git:"):
        parts = key[4:].split("/")
        return "/".join(parts[1:]) if len(parts) > 2 else key[4:]
    return Path(key).name or key


def derive(
    snapshot_id: str,
    created_ms: int,
    source: str,
    t: dict[str, pd.DataFrame],
    provenance: dict,
    knn_k: int,
) -> dict[str, object]:
    conv = t["conversations"].sort_values(["started_at", "guid"]).reset_index(drop=True)
    idx = {g: i for i, g in enumerate(conv["guid"])}

    repo_counts = conv["repo_key"].dropna().value_counts()
    repos = sorted(repo_counts.index, key=lambda k: (-repo_counts[k], k))
    repo_idx = {k: i for i, k in enumerate(repos)}
    harnesses = sorted(conv["harness"].dropna().unique())
    harness_idx = {h: i for i, h in enumerate(harnesses)}
    models = sorted({m for ms in conv["models"] for m in _list(ms)})
    model_idx = {m: i for i, m in enumerate(models)}
    seats_df = t["seats"].sort_values("seat").reset_index(drop=True)
    seat_idx = {s: i for i, s in enumerate(seats_df["seat"])}

    comm = t["communities"].sort_values("community_id").reset_index(drop=True)
    if list(comm["community_id"]) != list(range(len(comm))):
        raise ValueError("communities.community_id must be dense 0..n-1")

    start_ms = to_ms(conv["started_at"].min()) if len(conv) else 0
    end_ms = to_ms(conv["last_turn_at"].max()) if len(conv) else 0
    day0 = start_ms - start_ms % DAY_MS
    days = (end_ms - day0) // DAY_MS + 1

    def day_of(ms: int) -> int:
        return (ms - day0) // DAY_MS

    turns = t["turns"]
    detail = (
        set(t["session_novelty"]["guid"])
        | set(t["session_shifts"]["guid"])
        | set(turns.loc[turns["session_x"].notna(), "guid"])
    )

    activity: dict[str, list[list[int]]] = {}
    act = t["activity_daily"].sort_values(["guid", "day"])
    for guid, grp in act.groupby("guid", sort=False):
        activity[guid] = [
            [day_of(to_ms(pd.Timestamp(d))), int(n)] for d, n in zip(grp["day"], grp["turns"], strict=True)
        ]

    conversations = []
    xs, ys = [], []
    for r in conv.itertuples(index=False):
        x, y = _opt(r.x), _opt(r.y)
        xy = [float(x), float(y)] if x is not None and y is not None else None
        if xy:
            xs.append(xy[0])
            ys.append(xy[1])
        conversations.append(
            {
                "guid": r.guid,
                "kind": r.kind,
                "label": r.label,
                "blurb": _opt(r.blurb),
                "repo": repo_idx.get(_opt(r.repo_key)),
                "harness": harness_idx.get(_opt(r.harness)),
                "seat": seat_idx.get(_opt(r.seat)),
                "models": [model_idx[m] for m in _list(r.models)],
                "parent": idx.get(_opt(r.parent_guid)),
                "community": None if _opt(r.community_id) is None else int(r.community_id),
                "xy": xy,
                "started_ms": to_ms(r.started_at),
                "last_ms": to_ms(r.last_turn_at),
                "turns": int(r.turns),
                "counts": {
                    "work": int(r.n_work),
                    "tool_call": int(r.n_tool_call),
                    "tool_result": int(r.n_tool_result),
                    "retrieval": int(r.n_retrieval),
                    "human": int(r.n_human),
                    "peer": int(r.n_peer),
                    "agent": int(r.n_agent),
                },
                "tags": _list(r.tags)[:TAGS_MAX],
                "activity": activity.get(r.guid, []),
                "session_detail": r.guid in detail,
            }
        )

    seat_convs: dict[str, list[int]] = {}
    for i, s in enumerate(conv["seat"]):
        if isinstance(s, str):
            seat_convs.setdefault(s, []).append(i)

    meta = {
        "schema_version": contract.SCHEMA_VERSION,
        "id": snapshot_id,
        "created_ms": created_ms,
        "source": source,
        "day0_ms": day0,
        "days": int(days),
        "start_ms": start_ms,
        "end_ms": end_ms,
        "repos": [{"key": k, "label": repo_label(k)} for k in repos],
        "harnesses": harnesses,
        "models": [{"key": m, "label": m} for m in models],
        "seats": [
            {
                "id": r.seat,
                "harness": harness_idx.get(_opt(r.harness)),
                "conversations": seat_convs.get(r.seat, []),
                "first_ms": to_ms(r.first_seen),
                "last_ms": to_ms(r.last_seen),
            }
            for r in seats_df.itertuples(index=False)
        ],
        "communities": [
            {
                "label": r.label,
                "blurb": r.blurb,
                "tags": _list(r.tags)[:TAGS_MAX],
                "size": int(r.size),
                "centroid_xy": [float(r.cx), float(r.cy)],
            }
            for r in comm.itertuples(index=False)
        ],
        "layout": {
            **{"method": "unknown"},
            **{k: v for k, v in provenance.get("layout", {}).items() if k in ("method", "params")},
            "bounds": [min(xs, default=0.0), min(ys, default=0.0), max(xs, default=1.0), max(ys, default=1.0)],
        },
        "stats": {
            k: v
            for k, v in provenance.get("stats", {}).items()
            if isinstance(v, int | float) and not isinstance(v, bool)
        },
    }

    knn = []
    for r in t["knn_edges"].itertuples(index=False):
        a, b = idx[r.a_guid], idx[r.b_guid]
        knn.append([min(a, b), max(a, b), round(float(r.cos_centred), 4), to_ms(r.t_first)])
    subagent = [
        [idx[r.parent_guid], idx[r.child_guid], to_ms(r.started_at)]
        for r in t["subagent_edges"].itertuples(index=False)
        if r.parent_guid in idx and r.child_guid in idx
    ]
    agg: dict[tuple, int] = {}
    for r in t["messages"].itertuples(index=False):
        to_conv = idx.get(r.to_guid)
        if to_conv is None:
            continue  # receiving conversation outside the snapshot: cannot be placed or filtered
        key = (seat_idx.get(_opt(r.from_seat)), seat_idx.get(_opt(r.to_seat)), to_conv, day_of(to_ms(r.sent_at)))
        agg[key] = agg.get(key, 0) + 1
    order = sorted(agg.items(), key=lambda kv: (kv[0][3], -1 if kv[0][0] is None else kv[0][0], kv[0][2]))
    messages = [{"from": f, "to": to, "to_conv": tc, "day": d, "n": n} for (f, to, tc, d), n in order]
    edges = {"knn": {"k": knn_k, "edges": knn}, "subagent": subagent, "messages": messages}

    tag_df = t["tags"].sort_values(["n", "tag"], ascending=[False, True]).reset_index(drop=True)
    tag_idx = {g: i for i, g in enumerate(tag_df["tag"])}
    tags = {
        "tags": [
            {"tag": r.tag, "n": int(r.n), "conversations": int(r.conversations)} for r in tag_df.itertuples(index=False)
        ],
        "cooc": sorted(
            [sorted((tag_idx[r.tag_a], tag_idx[r.tag_b])) + [int(r.n)] for r in t["tag_cooc"].itertuples(index=False)]
        ),
    }

    files: dict[str, object] = {
        "meta.json": meta,
        "conversations.json": conversations,
        "edges.json": edges,
        "tags.json": tags,
    }

    work = turns[(turns["class"] == "work") & turns["guid"].isin(detail)].sort_values(["guid", "turn_no"])
    novelty = t["session_novelty"]
    shifts = t["session_shifts"]
    window = int(provenance.get("params", {}).get("novelty_window", 25))
    for guid, grp in work.groupby("guid", sort=False):
        pos = {int(n): i for i, n in enumerate(grp["turn_no"])}
        session_turns = []
        for r in grp.itertuples(index=False):
            label = _opt(r.summary) or _opt(r.head) or ""
            sx, sy = _opt(r.session_x), _opt(r.session_y)
            session_turns.append(
                {
                    "n": int(r.turn_no),
                    "t_ms": to_ms(r.turn_at),
                    "role": r.role,
                    "source": r.source,
                    "label": label[:LABEL_MAX],
                    "tags": _list(r.tags)[:TAGS_MAX],
                    "xy": [float(sx), float(sy)] if sx is not None and sy is not None else None,
                }
            )
        nv = novelty[novelty["guid"] == guid].sort_values("turn_no")
        sh = shifts[shifts["guid"] == guid].sort_values("turn_no")
        files[f"sessions/{guid}.json"] = {
            "guid": guid,
            "turns": session_turns,
            "novelty": {
                "window": window,
                "series": [
                    [pos[int(n)], round(float(v), 4)]
                    for n, v in zip(nv["turn_no"], nv["novelty"], strict=True)
                    if int(n) in pos
                ],
            },
            "shifts": [
                {
                    "n": int(r.turn_no),
                    "t_ms": to_ms(r.turn_at),
                    "score": round(float(r.score), 4),
                    "label": r.label,
                    "tags": _list(r.tags)[:TAGS_MAX],
                }
                for r in sh.itertuples(index=False)
            ],
        }
    return files


@cache
def _schema() -> dict:
    return json.loads(paths.SCHEMA_PATH.read_text())


def _inline(node: object, defs: dict) -> object:
    """Replace every local `$ref` with its definition. The schema is not recursive, so this
    terminates; without it jsonschema resolves references per instance node (24 s on prod)."""
    if isinstance(node, dict):
        ref = node.get("$ref")
        rest = {k: _inline(v, defs) for k, v in node.items() if k != "$ref"}
        if isinstance(ref, str) and ref.startswith("#/$defs/"):
            return {**_inline(defs[ref.removeprefix("#/$defs/")], defs), **rest}
        return rest
    if isinstance(node, list):
        return [_inline(v, defs) for v in node]
    return node


@cache
def _validator(definition: str) -> Draft202012Validator:
    defs = _schema()["$defs"]
    return Draft202012Validator(_inline(defs[definition], defs))


DEFINITION_FOR = {
    "manifest.json": "manifest",
    "meta.json": "meta",
    "conversations.json": "conversations",
    "edges.json": "edges",
    "tags.json": "tagsFile",
}


def definition_for(rel: str) -> str:
    if rel.startswith("sessions/"):
        return "session"
    if rel.startswith("insights/") and rel.endswith("index.json"):
        return "insightIndex"
    if rel.startswith("insights/"):
        return "insight"
    return DEFINITION_FOR[rel]


SAMPLE_EDGE = 100  # items kept from each end of a long list
SAMPLE_MIDDLE = 200  # evenly spaced items kept from between them


def _sampled(node: object) -> object:
    """A copy where every long list keeps its ends and an even spread of its middle.

    Every element of a view array shares one item schema, so a deterministic spread
    catches derivation bugs while skipping most of the per-node jsonschema cost."""
    if isinstance(node, dict):
        return {k: _sampled(v) for k, v in node.items()}
    if isinstance(node, list):
        if len(node) <= 2 * SAMPLE_EDGE + SAMPLE_MIDDLE:
            return [_sampled(v) for v in node]
        step = (len(node) - 2 * SAMPLE_EDGE) / SAMPLE_MIDDLE
        middle = [node[SAMPLE_EDGE + int(i * step)] for i in range(SAMPLE_MIDDLE)]
        return [_sampled(v) for v in node[:SAMPLE_EDGE] + middle + node[-SAMPLE_EDGE:]]
    return node


def validation_errors(rel: str, obj: object, *, sample: bool = False) -> list[str]:
    """Every violation as `<path>: <message>`, sorted by path; empty when valid.

    `sample=True` checks a deterministic spread of every long list (the export write path);
    `conv-atlas validate`, insight add and the tests check every element."""
    validator = _validator(definition_for(rel))
    errors = sorted(validator.iter_errors(_sampled(obj) if sample else obj), key=lambda e: list(e.absolute_path))
    return [f"{'/'.join(str(p) for p in e.absolute_path) or '<root>'}: {e.message}" for e in errors]


def validate(rel: str, obj: object, *, sample: bool = False) -> None:
    """Raise ValueError naming the first violation (see validation_errors for all of them)."""
    errors = validation_errors(rel, obj, sample=sample)
    if errors:
        suffix = " (sampled path: index is within the sample)" if sample else ""
        raise ValueError(f"{rel}: {errors[0]}{suffix}")
