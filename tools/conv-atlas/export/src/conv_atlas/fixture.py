"""Deterministic synthetic corpus: a full snapshot with no real conversation content.

Used for app development, tests and committed fixtures (the repo is public; real
bundles are never committed). Shapes and distributions imitate the prod corpus
(see the data survey): long-tailed turn counts, subagents near their parents,
a few very long sessions with topic shifts, peer messages between seats.
"""

from __future__ import annotations

import hashlib
import uuid
from datetime import UTC, datetime, timedelta
from itertools import combinations

import numpy as np
import pandas as pd

from conv_atlas import labels
from conv_atlas.bundle import SnapshotData, Vectors
from conv_atlas.view import to_ms

TOPICS = [
    ("terrain streaming", ["terrain", "streaming", "lod", "shaders", "world build"]),
    ("test gate grinding", ["cargo test", "flaky test", "ci", "lint", "regression"]),
    ("fleet supervision", ["watchdog", "governance", "status report", "anomalies", "handover"]),
    ("release trains", ["release", "changelog", "versioning", "merge", "pull request"]),
    ("flight data", ["flutter", "ios build", "airspace", "map tiles", "offline cache"]),
    ("story drafting", ["worldbuilding", "chapter", "characters", "outline", "dialogue"]),
    ("editor tooling", ["lsp", "diagnostics", "rust-analyzer", "formatting", "completion"]),
    ("data pipelines", ["ingest", "parquet", "embeddings", "backfill", "schema migration"]),
]
VERBS = ["Investigates", "Fixes", "Plans", "Reviews", "Refactors", "Verifies", "Documents", "Benchmarks"]
ADJ = ["amber", "brisk", "calm", "dapper", "eager", "fabled", "gentle", "hardy", "ivory", "jolly", "keen", "lucid"]
ANIMALS = ["otter", "heron", "lynx", "ibis", "marten", "newt", "osprey", "puffin", "quail", "raven", "stoat", "tern"]
REPOS = ["atlas-engine", "gatekeeper", "fleet-ops", "shipyard", "skyway", "inkwell", "lexicon", "conduit"]
DIM = 32
EPOCH = datetime(2026, 4, 6, tzinfo=UTC)


def _unit(v: np.ndarray) -> np.ndarray:
    return v / np.linalg.norm(v, axis=-1, keepdims=True)


def generate(
    n_conversations: int = 240, days: int = 160, seed: int = 7, detailed: int = 5, drift: float = 0.0
) -> SnapshotData:
    """Conversation i depends only on (seed, i): a larger n keeps every earlier guid and its
    content, so two fixture runs behave like successive prod exports (matched points move by
    `drift`, new conversations bloom). That is what the app's snapshot-swap tween needs."""
    rng = np.random.default_rng(seed)
    ids = np.random.default_rng(seed + 1)
    topic_vecs = _unit(rng.normal(size=(len(TOPICS), DIM)))
    centres = np.array([[np.cos(a), np.sin(a)] for a in np.linspace(0, 2 * np.pi, len(TOPICS), endpoint=False)]) * 6
    seats = sorted({f"pij-{a}-{b}" for a in ADJ for b in ANIMALS})
    seats = [seats[i] for i in rng.choice(len(seats), 28, replace=False)]

    conv_rows, turn_rows = [], []
    turn_vecs: list[np.ndarray] = []
    turn_addrs: list[str] = []
    for i in range(n_conversations):
        guid = str(uuid.UUID(bytes=ids.bytes(16), version=4))
        crng = np.random.default_rng([seed, i])
        # growth curve: activity accelerates over time, like the real corpus
        start_day = days * np.sqrt(crng.uniform(0.02, 1.0))
        topic = int(crng.integers(len(TOPICS)))
        is_sub = i > 10 and crng.random() < 0.3
        parent = None
        if is_sub:
            same = [r for r in conv_rows if r["community_id"] == topic and r["kind"] == "session"]
            if same:
                parent = same[int(crng.integers(len(same)))]
            else:
                is_sub = False
        started = EPOCH + timedelta(days=float(start_day))
        if parent is not None:
            started = max(started, parent["started_at"] + timedelta(minutes=5))
        n_turns = int(np.clip(crng.lognormal(3.6, 1.1), 4, 900))
        if i % 48 == 47:
            n_turns = int(crng.integers(900, 1600))  # a few very long sessions
        span_h = float(np.clip(crng.lognormal(1.2, 1.3), 0.2, 24 * 20))
        harness = "omp" if crng.random() < 0.45 else "claude"
        seat = seats[int(crng.integers(len(seats)))] if crng.random() < 0.6 else None
        repo = (
            None
            if crng.random() < 0.15
            else f"git:example.com/acme/{REPOS[topic if crng.random() < 0.75 else int(crng.integers(len(REPOS)))]}"
        )

        # topic walk: long sessions drift through several topics
        n_phases = 1 if n_turns < 300 else int(crng.integers(3, 6))
        phase_topics = [topic] + [int(crng.integers(len(TOPICS))) for _ in range(n_phases - 1)]
        cuts = np.sort(crng.choice(np.arange(20, n_turns - 20), n_phases - 1, replace=False)) if n_phases > 1 else []
        work_vecs = []
        for t in range(1, n_turns + 1):
            phase = int(np.searchsorted(cuts, t, side="right"))
            ptopic = phase_topics[phase]
            r = crng.random()
            cls = "work" if r < 0.24 else "tool_call" if r < 0.55 else "tool_result" if r < 0.99 else "retrieval_call"
            role = (
                "agent"
                if cls in ("tool_call", "retrieval_call") or (cls == "work" and crng.random() < 0.7)
                else "human"
            )
            source = "system"
            if role == "human" and cls == "work":
                source = "peer" if seat and crng.random() < 0.45 else "human"
            at = started + timedelta(hours=span_h * t / n_turns)
            name, tags = TOPICS[ptopic]
            summarized = cls == "work" and crng.random() < 0.85
            t_tags = list(crng.choice(tags, int(crng.integers(1, 4)), replace=False)) if summarized else []
            summary = (
                f"{VERBS[int(crng.integers(len(VERBS)))]} {t_tags[0] if t_tags else name} work in the {name} stream."
                if summarized
                else None
            )
            head = None if summarized or cls != "work" else f"short note about {name}"
            address = f"conv:{guid}#t{t}"
            turn_rows.append(
                {
                    "address": address,
                    "guid": guid,
                    "turn_no": t,
                    "turn_at": at,
                    "role": role,
                    "source": source,
                    "class": cls,
                    "summary": summary,
                    "head": head,
                    "tags": [str(x) for x in t_tags],
                    "raw_hash": hashlib.sha256(f"{seed}:{guid}:{t}".encode()).hexdigest(),
                    "bytes": int(crng.integers(40, 4000)),
                    "summarized": summarized,
                    "stopset": False,
                    "session_x": None,
                    "session_y": None,
                }
            )
            if cls == "work":
                v = _unit(topic_vecs[ptopic] + crng.normal(scale=0.55, size=DIM))
                work_vecs.append(v)
                turn_vecs.append(v)
                turn_addrs.append(address)
        last = started + timedelta(hours=span_h)
        conv_rows.append(
            {
                "guid": guid,
                "address": f"conv:{guid}",
                "kind": "subagent" if parent else "session",
                "label": f"{TOPICS[topic][0].capitalize()} #{i}",
                "blurb": f"Synthetic {TOPICS[topic][0]} conversation.",
                "repo_key": repo,
                "worktree": None,
                "harness": harness,
                "seat": seat,
                "models": ["model-a"] if harness == "claude" else ["model-b"],
                "parent_guid": parent["guid"] if parent else None,
                "started_at": started,
                "last_turn_at": last,
                "turns": n_turns,
                "community_id": topic,
                "work_vecs": work_vecs,
                "jitter": crng.normal(scale=1.1, size=2)
                + np.random.default_rng([seed, i, 1]).normal(scale=drift, size=2),
            }
        )

    turns = pd.DataFrame(turn_rows)
    turns["turn_at"] = pd.to_datetime(turns["turn_at"], utc=True)

    # centroids, centring, layout, knn
    embedded = [r for r in conv_rows if len(r["work_vecs"]) >= 3]
    cent = np.array([np.mean(r["work_vecs"], axis=0) for r in embedded], dtype=np.float32)
    mean = cent.mean(axis=0)
    centred = _unit(cent - mean)
    for r, c in zip(embedded, centred, strict=True):
        r["x"], r["y"] = (centres[r["community_id"]] + r["jitter"] + 0.3 * c[:2]).tolist()
    sims = centred @ centred.T
    np.fill_diagonal(sims, -1)
    raw = _unit(cent) @ _unit(cent).T
    knn_rows, seen = [], set()
    for a in range(len(embedded)):
        for b in np.argsort(-sims[a])[:8]:
            ga, gb = sorted((embedded[a]["guid"], embedded[int(b)]["guid"]))
            if (ga, gb) in seen:
                continue
            seen.add((ga, gb))
            ra, rb = embedded[a], embedded[int(b)]
            knn_rows.append(
                {
                    "a_guid": ga,
                    "b_guid": gb,
                    "cos_centred": float(sims[a, b]),
                    "cos_raw": float(raw[a, b]),
                    "cross_repo": bool(ra["repo_key"] and rb["repo_key"] and ra["repo_key"] != rb["repo_key"]),
                    "t_first": max(ra["started_at"], rb["started_at"]),
                }
            )

    # counts + tags per conversation
    agg = (
        turns.assign(
            work=turns["class"].eq("work"),
            tc=turns["class"].eq("tool_call"),
            tr=turns["class"].eq("tool_result"),
            rt=turns["class"].str.startswith("retrieval"),
            hu=turns["source"].eq("human"),
            pe=turns["source"].eq("peer"),
            ag=turns["role"].eq("agent"),
        )
        .groupby("guid")[["work", "tc", "tr", "rt", "hu", "pe", "ag"]]
        .sum()
    )
    work = turns[turns["class"] == "work"]
    top_tags = (
        work.explode("tags")
        .dropna(subset=["tags"])
        .groupby("guid")["tags"]
        .agg(lambda s: list(s.value_counts().index[:8]))
    )
    conversations = pd.DataFrame(
        [
            {
                "guid": r["guid"],
                "address": r["address"],
                "kind": r["kind"],
                "label": r["label"],
                "blurb": r["blurb"],
                "repo_key": r["repo_key"],
                "worktree": r["worktree"],
                "harness": r["harness"],
                "seat": r["seat"],
                "models": r["models"],
                "parent_guid": r["parent_guid"],
                "started_at": r["started_at"],
                "last_turn_at": r["last_turn_at"],
                "turns": r["turns"],
                "n_work": int(agg.at[r["guid"], "work"]),
                "n_tool_call": int(agg.at[r["guid"], "tc"]),
                "n_tool_result": int(agg.at[r["guid"], "tr"]),
                "n_retrieval": int(agg.at[r["guid"], "rt"]),
                "n_human": int(agg.at[r["guid"], "hu"]),
                "n_peer": int(agg.at[r["guid"], "pe"]),
                "n_agent": int(agg.at[r["guid"], "ag"]),
                "tags": top_tags.get(r["guid"], []),
                "community_id": r["community_id"] if "x" in r else None,
                "x": r.get("x"),
                "y": r.get("y"),
                "embedded": "x" in r,
            }
            for r in conv_rows
        ]
    )
    for col in ("started_at", "last_turn_at"):
        conversations[col] = pd.to_datetime(conversations[col], utc=True)
    conversations["community_id"] = conversations["community_id"].astype("Int64")

    # labels come from the real labellers, so fixture mode shows real-shaped labels
    named = labels.label_conversations(conversations, turns).set_index("guid")
    conversations["label"] = conversations["guid"].map(named["label"])
    conversations["blurb"] = conversations["guid"].map(named["blurb"])
    communities = labels.label_communities(conversations, turns).merge(
        pd.DataFrame(
            [
                {
                    "community_id": k,
                    "size": int((conversations["community_id"] == k).sum()),
                    "cx": float(centres[k][0]),
                    "cy": float(centres[k][1]),
                }
                for k in range(len(TOPICS))
            ]
        ),
        on="community_id",
        how="right",
    )
    communities["label"] = communities["label"].fillna("Unlabelled cluster")
    communities["blurb"] = communities["blurb"].fillna("")
    communities["tags"] = [t if isinstance(t, list) else [] for t in communities["tags"]]

    # long-session detail: novelty, shifts, per-session layout
    detail_guids = conversations.nlargest(detailed, "n_work")["guid"].tolist()
    nov_rows, shift_rows = [], []
    window = 25
    by_guid = {r["guid"]: r for r in conv_rows}
    for guid in detail_guids:
        wv = np.array(by_guid[guid]["work_vecs"])
        wt = work[work["guid"] == guid]
        nov = np.zeros(len(wv))
        for k in range(window, len(wv) - window):
            a, b = wv[k - window : k].mean(0), wv[k : k + window].mean(0)
            nov[k] = 1 - float(a @ b / np.linalg.norm(a) / np.linalg.norm(b))
        nov_rows += [
            {"guid": guid, "turn_no": int(n), "novelty": float(v)} for n, v in zip(wt["turn_no"], nov, strict=True)
        ]
        picked: list[int] = []
        for k in np.argsort(-nov):
            if nov[k] < 0.15 or len(picked) >= 5:
                break
            if all(abs(k - p) > window * 2 for p in picked):
                picked.append(int(k))
        for k in sorted(picked):
            row = wt.iloc[k]
            tags = list(pd.Series([t for ts in wt.iloc[k : k + window]["tags"] for t in ts]).value_counts().index[:4])
            shift_rows.append(
                {
                    "guid": guid,
                    "turn_no": int(row["turn_no"]),
                    "turn_at": row["turn_at"],
                    "score": float(nov[k]),
                    "label": labels.label_shift(tags, wt.iloc[k : k + window]["summary"].dropna().tolist()),
                    "tags": tags,
                }
            )
        xy = (wv - wv.mean(0)) @ np.linalg.svd(wv - wv.mean(0), full_matrices=False)[2][:2].T
        idx = turns.index[turns["address"].isin(wt["address"])]
        turns.loc[idx, "session_x"] = xy[:, 0]
        turns.loc[idx, "session_y"] = xy[:, 1]
    session_novelty = pd.DataFrame(nov_rows, columns=["guid", "turn_no", "novelty"])
    session_shifts = pd.DataFrame(shift_rows, columns=["guid", "turn_no", "turn_at", "score", "label", "tags"])
    session_shifts["turn_at"] = pd.to_datetime(session_shifts["turn_at"], utc=True)
    turns["session_x"] = turns["session_x"].astype("float64")
    turns["session_y"] = turns["session_y"].astype("float64")

    # peer messages: each peer turn came from another seat
    seat_of = conversations.set_index("guid")["seat"]
    peer = turns[turns["source"] == "peer"]
    messages = pd.DataFrame(
        {
            "address": peer["address"].values,
            "sent_at": peer["turn_at"].values,
            "from_seat": [seats[int(rng.integers(len(seats)))] for _ in range(len(peer))],
            "to_seat": [seat_of.get(g) for g in peer["guid"]],
            "to_guid": peer["guid"].values,
            "from_guid": [None] * len(peer),
        }
    )
    messages["sent_at"] = pd.to_datetime(messages["sent_at"], utc=True)

    seat_rows = []
    for s in seats:
        mine = conversations[conversations["seat"] == s]
        sent, recv = messages[messages["from_seat"] == s], messages[messages["to_seat"] == s]
        times = pd.concat([mine["started_at"], mine["last_turn_at"], sent["sent_at"], recv["sent_at"]])
        if times.empty:
            continue
        seat_rows.append(
            {
                "seat": s,
                "harness": mine["harness"].mode().iat[0] if len(mine) else None,
                "first_seen": times.min(),
                "last_seen": times.max(),
                "n_conversations": len(mine),
                "n_sent": len(sent),
                "n_received": len(recv),
                "identity_source": "preamble" if len(mine) else "message-only",
            }
        )
    seats_df = pd.DataFrame(seat_rows)
    messages = messages[messages["from_seat"].isin(seats_df["seat"])]

    t = turns.assign(day=turns["turn_at"].dt.floor("D").dt.date)
    activity = (
        t.assign(
            work=t["class"].eq("work"),
            tool=t["class"].str.startswith("tool"),
            human=t["source"].eq("human"),
            peer=t["source"].eq("peer"),
        )
        .groupby(["guid", "day"])
        .agg(
            turns=("address", "size"),
            work=("work", "sum"),
            tool=("tool", "sum"),
            human=("human", "sum"),
            peer=("peer", "sum"),
        )
        .reset_index()
    )

    tag_n = work.explode("tags").dropna(subset=["tags"])["tags"].value_counts()
    conv_tag_sets = [set(ts) for ts in conversations["tags"]]
    tags_df = pd.DataFrame(
        [{"tag": tag, "n": int(n), "conversations": sum(tag in s for s in conv_tag_sets)} for tag, n in tag_n.items()]
    )
    tags_df = tags_df[tags_df["conversations"] > 0]
    cooc: dict[tuple[str, str], int] = {}
    for s in conv_tag_sets:
        for a, b in combinations(sorted(s), 2):
            cooc[(a, b)] = cooc.get((a, b), 0) + 1
    tag_cooc = pd.DataFrame(
        [{"tag_a": a, "tag_b": b, "n": n} for (a, b), n in cooc.items()], columns=["tag_a", "tag_b", "n"]
    )

    subagent_edges = conversations.loc[
        conversations["parent_guid"].notna(), ["parent_guid", "guid", "started_at"]
    ].rename(columns={"guid": "child_guid"})
    knn_edges = pd.DataFrame(knn_rows)
    knn_edges["t_first"] = pd.to_datetime(knn_edges["t_first"], utc=True)

    tables = {
        "conversations": conversations,
        "turns": turns,
        "communities": communities,
        "knn_edges": knn_edges,
        "subagent_edges": subagent_edges.reset_index(drop=True),
        "seats": seats_df,
        "messages": messages.reset_index(drop=True),
        "activity_daily": activity,
        "tags": tags_df.reset_index(drop=True),
        "tag_cooc": tag_cooc,
        "session_shifts": session_shifts,
        "session_novelty": session_novelty,
    }
    from conv_atlas.contract import TABLES

    tables = {name: df[TABLES[name].columns] for name, df in tables.items()}
    # the stats keys Cartographer emits on prod (agreed with Chronos); measured from the
    # synthetic tables where they exist, plausible constants where the fixture has no source
    harness_of = conversations.set_index("guid")["harness"]
    same_harness = (knn_edges["a_guid"].map(harness_of) == knn_edges["b_guid"].map(harness_of)).mean()
    harness_share = conversations["harness"].value_counts(normalize=True)
    stats = {
        "seed": seed,
        "conversations": len(conversations),
        "turns_total": len(turns),
        "work_turns": int(len(work)),
        "embedded_conversations": int(conversations["embedded"].sum()),
        "communities": len(communities),
        "knn_edges": len(knn_edges),
        "seats": len(seats_df),
        "peer_messages": len(messages),
        "coverage_summary": float(work["summarized"].mean()),
        "coverage_any_vector": 0.97,
        "coverage_smart_vector": float(work["summarized"].mean()),
        "coverage_raw_vector": 0.99,
        "harness_confound_same_harness_edge_share": float(same_harness),
        "harness_confound_random_baseline": float((harness_share**2).sum()),
        "dl029_tool_call_turns": int(0.4 * (turns["class"] == "tool_call").sum()),
        "cross_repo_edge_share": float(knn_edges["cross_repo"].mean()),
        "layout_median_displacement_rel": 0.0,
    }
    return SnapshotData(
        source="synthetic",
        tables=tables,
        vectors=Vectors(
            centroids=cent,
            centroid_guids=[r["guid"] for r in embedded],
            mean=mean.astype(np.float32),
            turn_vectors=np.array(turn_vecs, dtype=np.float16),
            turn_addresses=turn_addrs,
        ),
        provenance={
            "generator": f"conv_atlas.fixture seed={seed}",
            "embedding_dim": DIM,
            "layout": {"method": "synthetic", "params": {"seed": seed}},
            "params": {"knn_k": 8, "novelty_window": window},
            "stats": stats,
            "known_defects": ["synthetic data: no real conversation content"],
        },
        knn_k=8,
    )


def demo_insights(data: SnapshotData) -> list[dict]:
    """Synthetic insights over a fixture snapshot, one per kind the app renders."""
    conv = data.tables["conversations"]
    big = conv[conv["community_id"] == 0].nlargest(5, "turns")
    edges = data.tables["knn_edges"]
    bridge = edges[edges["cross_repo"]].nlargest(1, "cos_centred")
    msgs = data.tables["messages"]
    busiest = msgs.assign(day=msgs["sent_at"].dt.floor("D")).groupby(["from_seat", "day"]).size().idxmax()
    day0 = to_ms(busiest[1])
    docs = [
        {
            "id": "demo-terrain-takes-off",
            "title": "Terrain streaming dominates late activity",
            "body_md": "Synthetic story: the five largest *terrain streaming* sessions all start in the second half.",
            "author": "conv-atlas fixture",
            "kind": "story",
            "refs": {
                "guids": big["guid"].tolist(),
                "tags": ["terrain", "streaming"],
                "time_range": {"start_ms": to_ms(big["started_at"].min()), "end_ms": to_ms(big["last_turn_at"].max())},
            },
        },
        {
            "id": "demo-peer-burst",
            "title": f"Message burst from {busiest[0]}",
            "body_md": "Synthetic anomaly: one seat sent an unusual number of peer messages in a single day.",
            "author": "conv-atlas fixture",
            "kind": "anomaly",
            "refs": {"seats": [busiest[0]], "time_range": {"start_ms": day0, "end_ms": day0 + 86_400_000}},
        },
    ]
    if len(bridge):
        a, b = bridge.iloc[0]["a_guid"], bridge.iloc[0]["b_guid"]
        docs.append(
            {
                "id": "demo-cross-repo-bridge",
                "title": "Same problem solved in two repos",
                "body_md": "Synthetic finding: the strongest cross-repo kNN edge links these two conversations.",
                "author": "conv-atlas fixture",
                "kind": "finding",
                "refs": {"guids": [a, b], "addresses": [f"conv:{a}", f"conv:{b}"]},
            }
        )
    return docs
