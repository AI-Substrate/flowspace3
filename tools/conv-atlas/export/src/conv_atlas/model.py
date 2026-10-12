"""Shape extracted rows into the contract tables: centring, kNN, communities, layout, seats, messages.

Pure: no database, no filesystem. `build()` takes what `extract` pulled (as frames
and numpy arrays) plus the previous snapshot's layout, and returns the tables,
vectors and the provenance pieces it measured. Labels come from the Scribe's
`conv_atlas.labels` module, passed in so tests can substitute a stub.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from itertools import combinations
from types import ModuleType

import networkx as nx
import numpy as np
import pandas as pd

from conv_atlas import contract
from conv_atlas import sessions as detail
from conv_atlas.bundle import Vectors


@dataclass(frozen=True)
class Params:
    knn_k: int = 10
    min_work_vectors: int = 5
    sessions: int = 40
    novelty_window: int = detail.WINDOW
    max_shifts: int = detail.MAX_SHIFTS
    louvain_resolution: float = 1.0
    stable_cos: float = 0.999  # a conversation whose raw centroid moved less than this keeps its layout position
    community_match_jaccard: float = 0.3  # a cluster keeps the previous snapshot's id above this member overlap
    # a fresh Louvain partition replaces the warm-started one only when its modularity is this much higher
    recluster_min_gain: float = 0.02
    umap_neighbors: int = 15
    umap_min_dist: float = 0.1
    seed: int = 42
    top_tags: int = 8
    message_match_s: int = 600


@dataclass
class Extracted:
    """Everything `extract` pulled, in memory. Frames have extract's column names."""

    conversations: pd.DataFrame  # extract.conversations()
    turns: pd.DataFrame  # extract.turns() for every conversation with turns
    shared_hashes: set[str]
    centroids: dict[str, tuple[int, np.ndarray]]  # guid -> (n_vectors, raw centroid)
    work_vectors: dict[str, tuple[np.ndarray, np.ndarray]] = field(default_factory=dict)  # guid -> (turn_nos, vecs)


@dataclass
class Previous:
    """The newest earlier snapshot's layout, raw centroids and clusters: what keeps a re-export steady."""

    xy: dict[str, tuple[float, float]]
    centroids: dict[str, np.ndarray]
    communities: dict[str, int] = field(default_factory=dict)  # guid -> community_id
    community_rows: pd.DataFrame | None = None  # the previous communities table


@dataclass
class Built:
    tables: dict[str, pd.DataFrame]
    vectors: Vectors
    provenance: dict


def _unit(v: np.ndarray) -> np.ndarray:
    n = np.linalg.norm(v, axis=-1, keepdims=True)
    return v / np.where(n == 0, 1, n)


def _utc(s: pd.Series) -> pd.Series:
    return pd.to_datetime(s, utc=True)


# ---------------------------------------------------------------- turns


def normalise_turn_tags(tags: pd.Series, labels: ModuleType) -> pd.Series:
    """Normalise every turn's tag list once per distinct tag (726k turns, far fewer tags)."""
    distinct = {t for ts in tags if ts is not None for t in ts}
    mapping = {t: labels.normalize_tag(t) for t in distinct}
    out = []
    for ts in tags:
        seen: dict[str, None] = {}
        for t in ts if ts is not None else ():
            n = mapping[t]
            if n is not None:
                seen.setdefault(n)
        out.append(list(seen))
    return pd.Series(out, index=tags.index, dtype=object)


def turn_table(raw: pd.DataFrame, shared_hashes: set[str], labels: ModuleType) -> pd.DataFrame:
    work = raw["class"].eq("work")
    t = pd.DataFrame(
        {
            "address": "conv:" + raw["guid"] + "#t" + raw["turn_no"].astype(str),
            "guid": raw["guid"],
            "turn_no": raw["turn_no"].astype("int64"),
            "turn_at": _utc(raw["at"]),
            "role": raw["role"],
            "source": raw["source"],
            "class": raw["class"],
            "summary": raw["summary"],
            "head": raw["head"],
            "tags": normalise_turn_tags(raw["tags"], labels),
            "raw_hash": raw["raw_hash"],
            "bytes": raw["bytes"].astype("int64"),
            "summarized": raw["summary"].notna(),
            "stopset": raw["stop_rule"].notna() | (work & raw["raw_hash"].isin(shared_hashes)),
            "session_x": np.nan,
            "session_y": np.nan,
        }
    )
    return t.reset_index(drop=True)


def stopset_stats(raw: pd.DataFrame, shared_hashes: set[str], min_conversations: int) -> dict:
    work = raw[raw["class"].eq("work")]
    rules = work["stop_rule"].value_counts().to_dict()
    shared = int((work["stop_rule"].isna() & work["raw_hash"].isin(shared_hashes)).sum())
    hits = {r: int(rules.get(r, 0)) for r in ("empty", "local_command_caveat", "compact", "task_notification")}
    hits["shared_hash"] = shared
    return {
        "rules": hits,
        "shared_hash_min_conversations": min_conversations,
        "shared_hashes": len(shared_hashes),
        "excluded_turns": int(sum(hits.values())),
        "applies_to": "work turns; excluded from centroids, session novelty and layouts",
    }


def coverage(raw: pd.DataFrame) -> dict:
    work = raw[raw["class"].eq("work")]
    n = max(len(work), 1)
    return {
        "coverage_summary": round(float(work["summary"].notna().sum()) / n, 4),
        "coverage_smart_vector": round(float(work["smart_vec"].sum()) / n, 4),
        "coverage_raw_vector": round(float(work["raw_vec"].sum()) / n, 4),
        "coverage_any_vector": round(float((work["smart_vec"] | work["raw_vec"]).sum()) / n, 4),
    }


# ---------------------------------------------------------------- embedding space


def knn(centred: np.ndarray, raw_unit: np.ndarray, k: int) -> list[tuple[int, int, float, float]]:
    """Undirected kNN edges (i < j) on centred unit vectors: (i, j, cos_centred, cos_raw)."""
    n = len(centred)
    if n < 2:
        return []
    sims = centred @ centred.T
    np.fill_diagonal(sims, -np.inf)
    kk = min(k, n - 1)
    nbrs = np.argsort(-sims, axis=1, kind="stable")[:, :kk]
    pairs = {(min(i, int(j)), max(i, int(j))) for i in range(n) for j in nbrs[i]}
    return [(i, j, float(sims[i, j]), float(raw_unit[i] @ raw_unit[j])) for i, j in sorted(pairs)]


def _graph(n: int, edges: list[tuple[int, int, float, float]]) -> nx.Graph:
    """The kNN graph Louvain runs on: weights = centred cosine, floored at 1e-3."""
    g = nx.Graph()
    g.add_nodes_from(range(n))
    g.add_weighted_edges_from((i, j, max(c, 1e-3)) for i, j, c, _ in edges)
    return g


def _dense(parts) -> np.ndarray:
    """Node sets -> labels with dense ids, largest set first (ties: smallest member)."""
    parts = sorted((p for p in parts if p), key=lambda p: (-len(p), min(p)))
    out = np.empty(sum(len(p) for p in parts), dtype=np.int64)
    for cid, members in enumerate(parts):
        out[list(members)] = cid
    return out


def communities(n: int, edges: list[tuple[int, int, float, float]], resolution: float, seed: int) -> np.ndarray:
    """Louvain on the kNN graph from scratch. Ids dense, largest first."""
    return _dense(nx.community.louvain_communities(_graph(n, edges), weight="weight", resolution=resolution, seed=seed))


def warm_communities(
    n: int, edges: list[tuple[int, int, float, float]], start: np.ndarray, resolution: float, max_passes: int = 50
) -> np.ndarray:
    """Continue the previous partition instead of starting over: `start` holds each node's previous community
    (-1 for nodes the previous snapshot did not cluster). New nodes join the community they are most strongly
    linked to, then every node takes Louvain's single-node move while it raises modularity. Moving one node at a
    time cannot merge or split a community in one step, so a small re-export keeps the cluster structure.
    Ids dense, largest first."""
    g = _graph(n, edges)
    nbrs = [[(j, d["weight"]) for j, d in g[i].items() if j != i] for i in range(n)]
    k = np.array([sum(w for _, w in nb) for nb in nbrs])
    two_m = float(k.sum()) or 1.0
    lab = np.asarray(start, dtype=np.int64).copy()
    pending = [int(i) for i in np.flatnonzero(lab < 0)]
    while pending:  # chains of new nodes resolve over passes; a new node with no clustered neighbour starts its own
        rest = []
        for i in pending:
            pull: dict[int, float] = {}
            for j, w in nbrs[i]:
                if lab[j] >= 0:
                    pull[lab[j]] = pull.get(lab[j], 0.0) + w
            if pull:
                lab[i] = max(pull, key=lambda c: (pull[c], -c))
            else:
                rest.append(i)
        if len(rest) == len(pending):
            for i in rest:
                lab[i] = lab.max() + 1
            break
        pending = rest
    tot: dict[int, float] = {}
    for i in range(n):
        tot[lab[i]] = tot.get(lab[i], 0.0) + k[i]
    for _ in range(max_passes):
        moved = 0
        for i in range(n):
            here = int(lab[i])
            links: dict[int, float] = {}
            for j, w in nbrs[i]:
                links[lab[j]] = links.get(lab[j], 0.0) + w
            tot[here] -= k[i]
            best, best_gain = here, links.get(here, 0.0) - resolution * tot[here] * k[i] / two_m
            for c, w in sorted(links.items()):
                gain = w - resolution * tot[c] * k[i] / two_m
                if gain > best_gain + 1e-12:
                    best, best_gain = c, gain
            tot[best] += k[i]
            lab[i] = best
            moved += best != here
        if not moved:
            break
    return _dense([set(np.flatnonzero(lab == c).tolist()) for c in np.unique(lab)])


def modularity(n: int, edges: list[tuple[int, int, float, float]], labels: np.ndarray, resolution: float) -> float:
    parts = [set(np.flatnonzero(labels == c).tolist()) for c in np.unique(labels)]
    return float(nx.community.modularity(_graph(n, edges), parts, weight="weight", resolution=resolution))


def cluster(
    emb: list[str], edges: list[tuple[int, int, float, float]], previous: Previous | None, p: Params
) -> tuple[np.ndarray, dict, dict]:
    """Community labels for the embedded conversations, ids matched to the previous snapshot's.

    With a previous partition, the warm start (`warm_communities`) wins unless a fresh Louvain run beats its
    modularity by `p.recluster_min_gain`: on a diffuse corpus fresh Louvain's k moves with the random trajectory
    alone, so a re-export must not re-roll the clusters without that evidence. Returns (labels, id_matching,
    start) where `start` records which partition was taken and both modularities.
    """
    n = len(emb)
    if not n:
        return np.zeros(0, np.int64), {"matched": 0, "unmatched": 0}, {"chosen": "fresh"}
    fresh = communities(n, edges, p.louvain_resolution, p.seed)
    q_fresh = modularity(n, edges, fresh, p.louvain_resolution)
    if previous is None or not previous.communities:
        return (
            fresh,
            {"matched": 0, "unmatched": int(fresh.max()) + 1},
            {"chosen": "fresh", "q_fresh": round(q_fresh, 4)},
        )
    seeded = np.array([previous.communities.get(g, -1) for g in emb], dtype=np.int64)
    warm = warm_communities(n, edges, seeded, p.louvain_resolution)
    q_warm = modularity(n, edges, warm, p.louvain_resolution)
    chosen = "fresh" if q_fresh - q_warm > p.recluster_min_gain else "warm"
    start = {
        "chosen": chosen,
        "q_warm": round(q_warm, 4),
        "q_fresh": round(q_fresh, 4),
        "min_gain": p.recluster_min_gain,
        "seeded": int((seeded >= 0).sum()),
        "k_warm": int(warm.max()) + 1,
        "k_fresh": int(fresh.max()) + 1,
    }
    ids, id_matching = stable_ids(
        fresh if chosen == "fresh" else warm, emb, previous.communities, p.community_match_jaccard
    )
    return ids, id_matching, start


def stable_ids(
    labels: np.ndarray, guids: list[str], previous: dict[str, int], threshold: float
) -> tuple[np.ndarray, dict]:
    """Re-key fresh cluster ids (dense, largest first) so clusters keep the previous snapshot's ids.

    New and old clusters are matched one-to-one by member Jaccard (Hungarian; pairs below `threshold` don't
    count). A matched cluster keeps its old id when that id fits the dense range 0..n-1; when the count shrank
    it can't, so it stays matched (its `previous_id` carries its label forward) but is renumbered into a free
    id. Unmatched clusters take the remaining free ids in ascending order, largest cluster first. Returns the
    new labels and the match table.
    """
    from scipy.optimize import linear_sum_assignment

    n = int(labels.max()) + 1 if len(labels) else 0
    new_sets = [set(np.asarray(guids)[labels == c]) for c in range(n)]
    old_ids = sorted(set(previous.values()))
    old_sets = [{g for g, c in previous.items() if c == o} for o in old_ids]
    match: dict[int, tuple[int, float]] = {}  # new cluster -> (old id, jaccard)
    if n and old_sets:
        jac = np.array([[len(a & b) / len(a | b) for b in old_sets] for a in new_sets])
        rows, cols = linear_sum_assignment(-jac)
        for r, c in zip(rows, cols, strict=True):
            if jac[r, c] >= threshold:
                match[int(r)] = (old_ids[c], float(jac[r, c]))
    keep = {r: o for r, (o, _) in match.items() if o < n}
    free = iter(sorted(set(range(n)) - set(keep.values())))
    final = {r: keep[r] if r in keep else next(free) for r in range(n)}  # r ascends = largest first
    out = np.array([final[int(c)] for c in labels], dtype=np.int64)
    js = [j for _, j in match.values()]
    table = [
        {
            "id": final[r],
            "previous_id": match[r][0] if r in match else None,
            "jaccard": round(match[r][1], 4) if r in match else None,
            "size": len(new_sets[r]),
        }
        for r in range(n)
    ]
    info = {
        "matched": len(match),
        "renumbered": len(match) - len(keep),
        "unmatched": n - len(match),
        "threshold": threshold,
        "min_jaccard": round(min(js), 4) if js else None,
        "median_jaccard": round(float(np.median(js)), 4) if js else None,
        "matches": sorted(table, key=lambda t: t["id"]),
    }
    return out, info


def place(centred: np.ndarray, known: np.ndarray, anchors: np.ndarray, k: int) -> np.ndarray:
    """Positions for the unknown rows: the centred-cosine-weighted mean of their k nearest known rows' positions.

    The same initialisation UMAP's own `transform` uses for new points, without moving anyone else.
    """
    free = np.flatnonzero(~known)
    if not len(free):
        return np.zeros((0, 2))
    sims = centred[free] @ centred[known].T
    kk = min(k, int(known.sum()))
    top = np.argsort(-sims, axis=1, kind="stable")[:, :kk]
    w = np.maximum(np.take_along_axis(sims, top, axis=1), 1e-3)
    return (w[..., None] * anchors[top]).sum(axis=1) / w.sum(axis=1, keepdims=True)


def layout(
    guids: list[str], centred: np.ndarray, kept: dict[str, tuple[float, float]], p: Params
) -> tuple[np.ndarray, dict]:
    """2-D conversation layout that stays put across snapshots.

    When at least half the points are `kept` (previous position, centroid unchanged) they keep their exact
    coordinates and only new or changed conversations are placed among their nearest kept neighbours.
    Otherwise (first export, or the corpus changed wholesale) a fresh UMAP layout is computed.
    """
    n = len(guids)
    known = np.array([g in kept for g in guids], dtype=bool)
    if known.sum() >= max(10, n // 2):
        xy = np.zeros((n, 2))
        anchors = np.array([kept[g] for g in np.asarray(guids)[known]], dtype=np.float64)
        xy[known] = anchors
        xy[~known] = place(centred, known, anchors, p.knn_k)
        return xy, {
            "method": "kept + knn placement",
            "kept_points": int(known.sum()),
            "placed_points": int((~known).sum()),
        }

    import umap  # slow import (numba): only when a fresh layout is computed

    reducer = umap.UMAP(
        n_neighbors=min(p.umap_neighbors, max(2, n - 1)),
        min_dist=p.umap_min_dist,
        metric="cosine",
        random_state=p.seed,
        n_jobs=1,
    )
    xy = reducer.fit_transform(centred).astype(np.float64)
    return xy, {"method": "umap", "kept_points": 0, "placed_points": n}


# ---------------------------------------------------------------- seats and messages


def conversation_seats(conv: pd.DataFrame, raw: pd.DataFrame) -> pd.DataFrame:
    """guid, seat, identity: metadata (conversations.pij_seat) wins over the preamble in turns <= 5."""
    pre = raw.loc[raw["preamble_seat"].notna(), ["guid", "turn_no", "preamble_seat"]].sort_values(["guid", "turn_no"])
    pre = pre.drop_duplicates("guid").set_index("guid")["preamble_seat"]
    meta = conv.set_index("guid")["pij_seat"]
    seat = meta.where(meta.notna(), pre.reindex(meta.index))
    identity = np.where(meta.notna(), "metadata", np.where(seat.notna(), "preamble", None))
    return pd.DataFrame({"guid": meta.index, "seat": seat.values, "identity": identity})


def _send_matches(msgs: pd.DataFrame, sends: pd.DataFrame, tolerance_s: float) -> tuple[pd.DataFrame, int]:
    """Pair delivered messages with the send call that produced them: the same body key, sent from another
    conversation at most `tolerance_s` before delivery. Returns (send_to, send_guid indexed like `msgs`,
    number of ambiguous messages).

    Several targets for one body (a broadcast) resolve only when one of them is the receiving conversation's
    own seat; otherwise the message is ambiguous and stays unmatched. The latest qualifying send wins."""
    keyed = msgs.loc[msgs["key"].notna(), ["key", "sent_at", "to_guid", "conv_seat"]].reset_index()
    cand = keyed.merge(sends, on="key")
    lag = (cand["sent_at"] - cand["at"]).dt.total_seconds()
    cand = cand[(lag >= 0) & (lag <= tolerance_s) & cand["send_guid"].ne(cand["to_guid"])]
    targets = cand.groupby("index")["send_to"].nunique()
    broadcast = cand["index"].map(targets) > 1
    cand = cand[~broadcast | cand["send_to"].eq(cand["conv_seat"])]
    best = cand.sort_values("at").groupby("index").tail(1).set_index("index")
    out = pd.DataFrame(index=msgs.index, columns=["send_to", "send_guid"], dtype=object)
    out.loc[best.index, ["send_to", "send_guid"]] = best[["send_to", "send_guid"]].values
    ambiguous = int(((targets > 1) & ~targets.index.isin(best.index)).sum())
    return out, ambiguous


def message_table(raw: pd.DataFrame, conv: pd.DataFrame, p: Params) -> tuple[pd.DataFrame, dict]:
    """One row per peer turn.

    to_seat, first evidence wins:
      (1) send call: the sender's tool call whose body matches this message (`msg_key`) names the seat;
      (2) bracketed: the earlier and later send-call-matched messages into the same conversation agree;
      (3) the receiving conversation's own seat (metadata, else its pij preamble).
    (1) and (2) beat (3) because long-lived conversations are re-seated: their seat is the latest one.
    from_guid: the matched send call's conversation, else a send call to the resolved seat in the time
    window, else the single conversation of the sender seat that was live at delivery time."""
    seat_of = conv.set_index("guid")["seat"]
    peer = raw.loc[raw["source"].eq("peer"), ["guid", "turn_no", "at", "peer_from", "msg_key"]].copy()
    peer["at"] = _utc(peer["at"])
    msgs = pd.DataFrame(
        {
            "address": "conv:" + peer["guid"] + "#t" + peer["turn_no"].astype(str),
            "sent_at": peer["at"],
            "from_seat": peer["peer_from"],
            "to_seat": None,
            "to_guid": peer["guid"],
            "from_guid": None,
            "key": peer["msg_key"],
            "conv_seat": peer["guid"].map(seat_of),
        }
    ).reset_index(drop=True)
    # object dtype on every join key: an all-None column and a string column must still merge
    msgs[["to_seat", "from_guid", "key"]] = msgs[["to_seat", "from_guid", "key"]].astype(object)

    sends = raw.loc[raw["send_to"].notna(), ["guid", "at", "send_to", "msg_key"]].rename(
        columns={"guid": "send_guid", "msg_key": "key"}
    )
    sends = sends.astype({"send_to": object, "key": object})
    sends["at"] = _utc(sends["at"])
    sends["send_seat"] = sends["send_guid"].map(seat_of)

    # (1) send call that produced the message
    matched, ambiguous = _send_matches(
        msgs, sends.loc[sends["key"].notna(), ["key", "at", "send_to", "send_guid"]], p.message_match_s
    )
    direct = matched["send_to"].notna()
    msgs.loc[direct, "to_seat"] = matched.loc[direct, "send_to"]
    msgs.loc[direct, "from_guid"] = matched.loc[direct, "send_guid"]
    # (2) bracketed by two send-call matches into the same conversation that name the same seat
    order = msgs.sort_values(["to_guid", "sent_at"]).index
    seq = msgs.loc[order, ["to_guid", "to_seat"]]
    before = seq.groupby("to_guid")["to_seat"].ffill()
    after = seq.groupby("to_guid")["to_seat"].bfill()
    bracketed = seq["to_seat"].isna() & before.notna() & before.eq(after)
    msgs.loc[bracketed[bracketed].index, "to_seat"] = before[bracketed]
    # (3) the receiving conversation's seat
    by_conv = msgs["to_seat"].isna() & msgs["conv_seat"].notna()
    msgs.loc[by_conv, "to_seat"] = msgs.loc[by_conv, "conv_seat"]
    # seat identity is time-varying: a re-seated conversation received earlier messages under an earlier seat
    differs = msgs["conv_seat"].notna() & msgs["to_seat"].notna() & msgs["conv_seat"].ne(msgs["to_seat"])
    reseated = direct & differs

    # from_guid for the rest: a send call to the resolved seat shortly before delivery
    by_send = 0
    want = msgs[msgs["from_guid"].isna() & msgs["to_seat"].notna()]
    if len(sends) and len(want):
        want = want.drop(columns="key").reset_index().rename(columns={"sent_at": "at"}).sort_values("at")
        hit = pd.merge_asof(
            want,
            sends.drop(columns="key").sort_values("at"),
            on="at",
            left_by="to_seat",
            right_by="send_to",
            direction="backward",
            tolerance=pd.Timedelta(seconds=p.message_match_s),
        )
        ok = hit["send_guid"].notna() & hit["send_guid"].ne(hit["to_guid"])
        ok &= hit["from_seat"].isna() | hit["send_seat"].isna() | hit["from_seat"].eq(hit["send_seat"])
        msgs.loc[hit.loc[ok, "index"], "from_guid"] = hit.loc[ok, "send_guid"].values
        by_send = int(ok.sum())

    live = conv.loc[conv["seat"].notna(), ["guid", "seat", "started_at", "last_turn_at"]]
    by_live = 0
    todo = msgs.index[msgs["from_guid"].isna() & msgs["from_seat"].isin(set(live["seat"]))]
    if len(todo):
        cand = (
            msgs.loc[todo, ["sent_at", "from_seat", "to_guid"]]
            .reset_index()
            .merge(live, left_on="from_seat", right_on="seat")
        )
        cand = cand[(cand["started_at"] <= cand["sent_at"]) & (cand["sent_at"] <= cand["last_turn_at"])]
        cand = cand[cand["guid"].ne(cand["to_guid"])]
        unique = cand.groupby("index")["guid"].agg(["first", "size"])
        unique = unique[unique["size"] == 1]
        msgs.loc[unique.index, "from_guid"] = unique["first"].values
        by_live = len(unique)

    n = len(msgs)
    stats = {
        "peer_turns": n,
        "sender_parsed": int(msgs["from_seat"].notna().sum()),
        "to_seat_by_send_call": int(direct.sum()),
        "to_seat_by_bracketing": int(bracketed.sum()),
        "to_seat_by_conversation_seat": int(by_conv.sum()),
        "to_seat_coverage": round(float(msgs["to_seat"].notna().mean()), 4) if n else None,
        "to_seat_send_call_beats_conversation_seat": int(reseated.sum()),
        "to_seat_differs_from_conversation_seat": int(differs.sum()),
        "to_seat_differs_conversations": int(msgs.loc[differs, "to_guid"].nunique()),
        "send_call_ambiguous": ambiguous,
        "from_guid_by_send_call": int(direct.sum()) + by_send,
        "from_guid_by_live_seat": by_live,
        "send_calls": len(sends),
    }
    return msgs.drop(columns=["key", "conv_seat"]), stats


def seat_table(conv: pd.DataFrame, msgs: pd.DataFrame) -> pd.DataFrame:
    rows = []
    mine_by = {s: g for s, g in conv[conv["seat"].notna()].groupby("seat")}
    sent_by = {s: g for s, g in msgs[msgs["from_seat"].notna()].groupby("from_seat")}
    recv_by = {s: g for s, g in msgs[msgs["to_seat"].notna()].groupby("to_seat")}
    empty_c, empty_m = conv.iloc[:0], msgs.iloc[:0]
    for s in sorted(set(mine_by) | set(sent_by) | set(recv_by)):
        mine, sent, recv = mine_by.get(s, empty_c), sent_by.get(s, empty_m), recv_by.get(s, empty_m)
        times = pd.concat([mine["started_at"], mine["last_turn_at"], sent["sent_at"], recv["sent_at"]])
        harness = mine["harness"].dropna()
        identity = "metadata" if mine["identity"].eq("metadata").any() else "preamble" if len(mine) else "message-only"
        rows.append(
            {
                "seat": s,
                "harness": harness.mode().sort_values().iat[0] if len(harness) else None,
                "first_seen": times.min(),
                "last_seen": times.max(),
                "n_conversations": len(mine),
                "n_sent": len(sent),
                "n_received": len(recv),
                "identity_source": identity,
            }
        )
    df = pd.DataFrame(rows, columns=contract.TABLES["seats"].columns)
    for c in ("first_seen", "last_seen"):
        df[c] = _utc(df[c])
    return df


# ---------------------------------------------------------------- tags and activity


def top_tags(turns: pd.DataFrame, n: int) -> pd.Series:
    """guid -> top n normalised tags over work turns (stop-set excluded), most frequent first, ties by name."""
    w = turns.loc[turns["class"].eq("work") & ~turns["stopset"], ["guid", "tags"]].explode("tags").dropna()
    if w.empty:
        return pd.Series(dtype=object)
    counts = w.groupby(["guid", "tags"]).size().reset_index(name="n")
    counts = counts.sort_values(["guid", "n", "tags"], ascending=[True, False, True])
    return counts.groupby("guid")["tags"].agg(lambda s: list(s[:n]))


def tag_tables(turns: pd.DataFrame, conv_tags: pd.Series) -> tuple[pd.DataFrame, pd.DataFrame]:
    work = turns.loc[turns["class"].eq("work"), "tags"].explode().dropna()
    tag_n = work.value_counts()
    in_convs: dict[str, int] = {}
    cooc: dict[tuple[str, str], int] = {}
    for ts in conv_tags:
        s = sorted(set(ts))
        for t in s:
            in_convs[t] = in_convs.get(t, 0) + 1
        for a, b in combinations(s, 2):
            cooc[(a, b)] = cooc.get((a, b), 0) + 1
    tags = pd.DataFrame(
        [{"tag": t, "n": int(tag_n.get(t, 0)), "conversations": c} for t, c in sorted(in_convs.items())],
        columns=["tag", "n", "conversations"],
    )
    tag_cooc = pd.DataFrame(
        [{"tag_a": a, "tag_b": b, "n": n} for (a, b), n in sorted(cooc.items())], columns=["tag_a", "tag_b", "n"]
    )
    return tags, tag_cooc


def activity_table(turns: pd.DataFrame) -> pd.DataFrame:
    t = turns.assign(
        day=turns["turn_at"].dt.floor("D").dt.date,
        work=turns["class"].eq("work"),
        tool=turns["class"].str.startswith("tool"),
        human=turns["source"].eq("human"),
        peer=turns["source"].eq("peer"),
    )
    return (
        t.groupby(["guid", "day"])
        .agg(
            turns=("address", "size"),
            work=("work", "sum"),
            tool=("tool", "sum"),
            human=("human", "sum"),
            peer=("peer", "sum"),
        )
        .reset_index()
        .astype({"turns": "int64", "work": "int64", "tool": "int64", "human": "int64", "peer": "int64"})
    )


# ---------------------------------------------------------------- build


def detail_guids(raw: pd.DataFrame, n: int, exclude: frozenset[str] | set[str] = frozenset()) -> list[str]:
    """The long sessions that get novelty, shifts and a per-session layout: top n by work turns, then turns."""
    raw = raw[~raw["guid"].isin(exclude)]
    if raw.empty:
        return []
    c = raw.assign(work=raw["class"].eq("work")).groupby("guid").agg(n_work=("work", "sum"), turns=("turn_no", "size"))
    c = c.reset_index().sort_values("guid")
    return c.nlargest(n, ["n_work", "turns"])["guid"].tolist()


def flagged_conversations(conv: pd.DataFrame) -> list[dict]:
    """Conversations that are not harness sessions: no harness means an imported transcript whose
    turn classes cannot be trusted (tool calls stored as work turns, n_work inflated)."""
    return [
        {
            "guid": r.guid,
            "reason": "no harness: imported transcript; tool calls are stored as work turns, so n_work is "
            "inflated. Excluded from the long-session detail (shifts, novelty, session layout); "
            "still embedded and clustered.",
            "turns": int(r.turns),
            "n_work": int(r.n_work),
        }
        for r in conv[conv["harness"].isna()].itertuples(index=False)
    ]


OMP_ROLE_DEFECT_SINCE = pd.Timestamp("2026-09-07", tz="UTC")
OMP_ROLE_DEFECT_MIN_TURNS = 50


def omp_role_defect_stats(conv: pd.DataFrame, turns: pd.DataFrame) -> dict[str, int]:
    """Sizes the omp intake bug: since 2026-09-07 omp transcripts store human and peer turns as
    role=agent, so a long omp session with zero role=human turns is a symptom, not a fact."""
    human_role = turns.loc[turns["role"].eq("human"), "guid"].value_counts()
    long_omp = conv[
        conv["harness"].eq("omp")
        & (conv["turns"] >= OMP_ROLE_DEFECT_MIN_TURNS)
        & (conv["started_at"] >= OMP_ROLE_DEFECT_SINCE)
    ]
    zero = long_omp["guid"].map(human_role).fillna(0).eq(0)
    return {"omp_sessions_ge50_since_0907": len(long_omp), "omp_sessions_zero_human": int(zero.sum())}


def _canonical_models(models, labels: ModuleType) -> list[str]:
    seen: dict[str, None] = {}
    for m in models if models is not None else ():
        c = labels.canonical_model(m)
        if c is not None:
            seen.setdefault(c)
    return list(seen)


def build(
    ex: Extracted,
    p: Params,
    labels: ModuleType,
    previous: Previous | None = None,
    turn_vectors: bool = False,
) -> Built:
    raw = ex.turns
    turns = turn_table(raw, ex.shared_hashes, labels)

    # conversations: one row per conversation that has turns
    counts = turns.assign(
        work=turns["class"].eq("work"),
        tc=turns["class"].eq("tool_call"),
        tr=turns["class"].eq("tool_result"),
        rt=turns["class"].str.startswith("retrieval"),
        hu=turns["source"].eq("human"),
        pe=turns["source"].eq("peer"),
        ag=turns["role"].eq("agent"),
    ).groupby("guid")
    agg = counts.agg(
        turns=("address", "size"),
        first_at=("turn_at", "min"),
        last_at=("turn_at", "max"),
        n_work=("work", "sum"),
        n_tool_call=("tc", "sum"),
        n_tool_result=("tr", "sum"),
        n_retrieval=("rt", "sum"),
        n_human=("hu", "sum"),
        n_peer=("pe", "sum"),
        n_agent=("ag", "sum"),
    )
    conv = ex.conversations[ex.conversations["guid"].isin(agg.index)].sort_values("guid").reset_index(drop=True)
    conv = conv.join(agg, on="guid")
    conv["started_at"] = _utc(conv["started_at"]).fillna(conv["first_at"])
    conv["started_at"] = conv[["started_at", "first_at"]].min(axis=1)
    conv["last_turn_at"] = conv["last_at"]
    seats = conversation_seats(conv, raw).set_index("guid")
    conv["seat"] = conv["guid"].map(seats["seat"])
    conv["identity"] = conv["guid"].map(seats["identity"])
    known = set(conv["guid"])
    conv["parent_guid"] = conv["parent_guid"].where(conv["parent_guid"].isin(known))
    conv["kind"] = np.where(
        ex.conversations.set_index("guid")["parent_guid"].reindex(conv["guid"]).notna(), "subagent", "session"
    )
    conv["models"] = [_canonical_models(m, labels) for m in conv["models"]]
    tags_by = top_tags(turns, p.top_tags)
    conv["tags"] = [tags_by.get(g, []) for g in conv["guid"]]

    # embedding space
    emb = [g for g in conv["guid"] if ex.centroids.get(g, (0, None))[0] >= p.min_work_vectors]
    cents = np.stack([ex.centroids[g][1] for g in emb]).astype(np.float32) if emb else np.zeros((0, 1024), np.float32)
    mean = cents.mean(axis=0) if len(emb) else np.zeros(cents.shape[1], np.float32)
    centred = _unit(cents - mean)
    raw_unit = _unit(cents)
    edges = knn(centred, raw_unit, p.knn_k)
    comm, id_matching, start = cluster(emb, edges, previous, p)
    kept: dict[str, tuple[float, float]] = {}
    if previous is not None:
        for i, g in enumerate(emb):
            old = previous.centroids.get(g)
            if g in previous.xy and old is not None and float(_unit(old) @ raw_unit[i]) >= p.stable_cos:
                kept[g] = previous.xy[g]
    xy, layout_stats = layout(emb, centred, kept, p) if len(emb) >= 3 else (np.zeros((len(emb), 2)), {})
    if previous is not None and previous.xy and len(emb):
        back = [i for i, g in enumerate(emb) if g in previous.xy]
        old_xy = np.array([previous.xy[emb[i]] for i in back])
        if back:
            moved = np.linalg.norm(xy[back] - old_xy, axis=1)
            extent = float(np.linalg.norm(old_xy.max(axis=0) - old_xy.min(axis=0))) or 1.0
            layout_stats["median_displacement"] = round(float(np.median(moved)), 4)
            layout_stats["median_displacement_rel"] = round(float(np.median(moved)) / extent, 4)
            layout_stats["moved_points"] = int((moved > 1e-9).sum())
    pos = {g: i for i, g in enumerate(emb)}
    conv["embedded"] = conv["guid"].isin(pos)
    conv["community_id"] = pd.array([int(comm[pos[g]]) if g in pos else None for g in conv["guid"]], dtype="Int64")
    conv["x"] = [float(xy[pos[g], 0]) if g in pos else np.nan for g in conv["guid"]]
    conv["y"] = [float(xy[pos[g], 1]) if g in pos else np.nan for g in conv["guid"]]

    meta = conv.set_index("guid")
    knn_edges = pd.DataFrame(
        [
            {
                "a_guid": emb[i],
                "b_guid": emb[j],
                "cos_centred": c,
                "cos_raw": r,
                "cross_repo": bool(
                    pd.notna(meta.at[emb[i], "repo_key"])
                    and pd.notna(meta.at[emb[j], "repo_key"])
                    and meta.at[emb[i], "repo_key"] != meta.at[emb[j], "repo_key"]
                ),
                "t_first": max(meta.at[emb[i], "started_at"], meta.at[emb[j], "started_at"]),
            }
            for i, j, c, r in edges
        ],
        columns=contract.TABLES["knn_edges"].columns,
    )
    # a_guid < b_guid by contract
    swap = knn_edges["a_guid"] > knn_edges["b_guid"]
    knn_edges.loc[swap, ["a_guid", "b_guid"]] = knn_edges.loc[swap, ["b_guid", "a_guid"]].values
    knn_edges = knn_edges.sort_values(["a_guid", "b_guid"]).reset_index(drop=True)
    knn_edges["t_first"] = _utc(knn_edges["t_first"])

    # community centrality: centred cos of a member to its community's mean direction
    centrality = pd.Series(dtype=float)
    if len(emb):
        cdir = _unit(np.stack([centred[comm == c].mean(axis=0) for c in range(int(comm.max()) + 1)]))
        centrality = pd.Series(np.sum(centred * cdir[comm], axis=1), index=emb)

    # long-session detail
    flagged = flagged_conversations(conv)
    detailed = detail_guids(raw, p.sessions, exclude={f["guid"] for f in flagged})
    work_rows = turns[turns["class"].eq("work")]
    stop_addr = set(turns.loc[turns["stopset"], "address"])
    nov_rows, shift_rows, turn_score = [], [], {}
    xy_rows: list[pd.DataFrame] = []
    for guid in detailed:
        if guid not in ex.work_vectors:
            continue
        nos, vecs = ex.work_vectors[guid]
        addrs = np.array([f"conv:{guid}#t{n}" for n in nos])
        keep = np.array([a not in stop_addr for a in addrs])
        nos, vecs, addrs = nos[keep], vecs[keep], addrs[keep]
        if len(nos) < 3:
            continue
        nov = detail.novelty(vecs, mean, p.novelty_window)
        nov_rows.append(pd.DataFrame({"guid": guid, "turn_no": nos.astype("int64"), "novelty": nov}))
        sxy = detail.layout(vecs)
        xy_rows.append(pd.DataFrame({"address": addrs, "sx": sxy[:, 0], "sy": sxy[:, 1]}))
        if guid in pos:
            cv = centred[pos[guid]]
            turn_score.update(zip(addrs, _unit(vecs - mean) @ cv, strict=True))
        wt = work_rows[work_rows["guid"] == guid].set_index("turn_no")
        for k in detail.shifts(nov, p.novelty_window, p.max_shifts):
            win = wt.loc[wt.index.isin(nos[k : k + p.novelty_window])]
            win = win[~win["stopset"]]
            tag_counts = win["tags"].explode().dropna().value_counts()
            tags = sorted(tag_counts.index, key=lambda t: (-tag_counts[t], t))[:6]
            summaries = [s for s in win["summary"].dropna()][:8]
            shift_rows.append(
                {
                    "guid": guid,
                    "turn_no": int(nos[k]),
                    "turn_at": wt.at[int(nos[k]), "turn_at"],
                    "score": float(nov[k]),
                    "label": labels.label_shift(tags, summaries),
                    "tags": tags,
                }
            )
    if xy_rows:
        sxy = pd.concat(xy_rows).set_index("address")
        turns["session_x"] = turns["address"].map(sxy["sx"]).astype("float64")
        turns["session_y"] = turns["address"].map(sxy["sy"]).astype("float64")
    session_novelty = (
        pd.concat(nov_rows, ignore_index=True) if nov_rows else pd.DataFrame(columns=["guid", "turn_no", "novelty"])
    )
    session_shifts = pd.DataFrame(shift_rows, columns=contract.TABLES["session_shifts"].columns)
    session_shifts["turn_at"] = _utc(session_shifts["turn_at"])

    # labels (Scribe)
    named = labels.label_conversations(
        conv[["guid", "kind", "tags", "seat", "repo_key", "started_at", "turns"]],
        turns[["guid", "turn_no", "address", "class", "summary", "tags", "stopset"]],
        pd.Series(turn_score, dtype=float) if turn_score else None,
    ).set_index("guid")
    conv["label"] = conv["guid"].map(named["label"])
    conv["blurb"] = conv["guid"].map(named["blurb"])
    comm_tbl = pd.DataFrame(columns=contract.TABLES["communities"].columns)
    if len(emb):
        # the label follows the cluster: previous rows are re-keyed from their old id to the id it matched
        renamed = {m["previous_id"]: m["id"] for m in id_matching.get("matches", []) if m["previous_id"] is not None}
        prev_rows = None
        if previous is not None and previous.community_rows is not None and renamed:
            rows = previous.community_rows
            prev_rows = rows.loc[rows["community_id"].isin(renamed), ["community_id", "label", "tags"]].assign(
                community_id=lambda d: d["community_id"].map(renamed)
            )
        cl = labels.label_communities(
            conv[["guid", "community_id", "tags", "blurb", "repo_key"]],
            turns[["guid", "class", "tags"]],
            centrality,
            previous=prev_rows,
        ).set_index("community_id")
        members = conv[conv["embedded"]].groupby("community_id")
        comm_tbl = pd.DataFrame(
            {
                "community_id": np.arange(int(comm.max()) + 1, dtype=np.int64),
            }
        )
        comm_tbl["label"] = comm_tbl["community_id"].map(cl["label"])
        comm_tbl["blurb"] = comm_tbl["community_id"].map(cl["blurb"])
        comm_tbl["tags"] = [list(cl.at[c, "tags"]) if c in cl.index else [] for c in comm_tbl["community_id"]]
        comm_tbl["size"] = comm_tbl["community_id"].map(members.size()).astype("int64")
        comm_tbl["cx"] = comm_tbl["community_id"].map(members["x"].mean()).astype("float64")
        comm_tbl["cy"] = comm_tbl["community_id"].map(members["y"].mean()).astype("float64")

    msgs, msg_stats = message_table(raw, conv, p)
    seats_df = seat_table(conv, msgs)
    tags_df, tag_cooc = tag_tables(turns, conv["tags"])
    subagent = conv.loc[conv["parent_guid"].notna(), ["parent_guid", "guid", "started_at"]].rename(
        columns={"guid": "child_guid"}
    )

    tables = {
        "conversations": conv.assign(address="conv:" + conv["guid"]),
        "turns": turns,
        "communities": comm_tbl,
        "knn_edges": knn_edges,
        "subagent_edges": subagent,
        "seats": seats_df,
        "messages": msgs,
        "activity_daily": activity_table(turns),
        "tags": tags_df,
        "tag_cooc": tag_cooc,
        "session_shifts": session_shifts,
        "session_novelty": session_novelty,
    }
    tables = {n: df[contract.TABLES[n].columns].reset_index(drop=True) for n, df in tables.items()}

    tv = ta = None
    if turn_vectors:
        tv_parts, ta = [], []
        for guid in sorted(ex.work_vectors):
            if guid not in known:
                continue
            nos, vecs = ex.work_vectors[guid]
            tv_parts.append(vecs.astype(np.float16))
            ta += [f"conv:{guid}#t{n}" for n in nos]
        tv = np.concatenate(tv_parts) if tv_parts else np.zeros((0, cents.shape[1]), np.float16)

    harness = meta["harness"]
    same_h = [
        harness[emb[i]] == harness[emb[j]]
        for i, j, _, _ in edges
        if pd.notna(harness[emb[i]]) and pd.notna(harness[emb[j]])
    ]
    shares = harness.reindex(emb).value_counts(normalize=True)
    stats = {
        "conversations": len(conv),
        "turns_total": len(turns),
        "work_turns": int(turns["class"].eq("work").sum()),
        "embedded_conversations": len(emb),
        "communities": len(comm_tbl),
        "knn_edges": len(knn_edges),
        "seats": len(seats_df),
        "peer_messages": len(msgs),
        **coverage(raw),
        "harness_confound_same_harness_edge_share": round(float(np.mean(same_h)), 4) if same_h else None,
        "harness_confound_random_baseline": round(float((shares**2).sum()), 4) if len(shares) else None,
        "cross_repo_edge_share": round(float(knn_edges["cross_repo"].mean()), 4) if len(knn_edges) else None,
        "dl029_tool_call_turns": int(turns["class"].eq("tool_call").sum()),
        **{f"messages_{k}": v for k, v in msg_stats.items()},
    }
    for k in ("median_displacement", "median_displacement_rel"):
        if k in layout_stats:
            stats[f"layout_{k}"] = layout_stats[k]
    stats["flagged_conversations"] = len(flagged)
    stats.update(omp_role_defect_stats(conv, turns))
    provenance = {
        "summariser_models": {k: int(v) for k, v in raw["summariser"].value_counts().items()},
        "stats": stats,
        "layout": {
            "method": "umap",
            "params": {
                "n_neighbors": p.umap_neighbors,
                "min_dist": p.umap_min_dist,
                "metric": "cosine on mean-centred centroids",
                "seed": p.seed,
            },
            **layout_stats,
        },
        "clustering": {
            "method": "louvain on kNN graph, warm-started from the previous snapshot's partition",
            "start": start,
            "knn_k": p.knn_k,
            "resolution": p.louvain_resolution,
            "weight": "centred cosine, floored at 1e-3",
            "seed": p.seed,
            "id_matching": id_matching,
        },
        "centring": "subtract embedding_mean (mean of embedded conversation centroids), then L2-normalise",
        "seeds": {"louvain": p.seed, "umap": p.seed},
        "params": {**asdict(p), "session_layout": "pca"},
        "flagged_conversations": flagged,
    }
    return Built(
        tables=tables,
        vectors=Vectors(
            centroids=cents,
            centroid_guids=emb,
            mean=mean.astype(np.float32),
            turn_vectors=tv,
            turn_addresses=ta,
        ),
        provenance=provenance,
    )
