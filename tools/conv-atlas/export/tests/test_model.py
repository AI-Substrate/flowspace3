"""model.build turns extract-shaped frames into contract tables; the shaping rules that matter.

All data is synthetic (the repo is public). `synth()` imitates what `extract` returns.
"""

from __future__ import annotations

import uuid
from datetime import UTC, datetime, timedelta

import numpy as np
import pandas as pd
import pytest

from conv_atlas import bundle, contract, labels, model

DIM = 16
T0 = datetime(2026, 9, 1, tzinfo=UTC)
TAGS = [["cargo test", "rust", "ci"], ["terrain", "shaders", "lod"], ["release", "changelog", "merge"]]


def _unit(v):
    return v / np.linalg.norm(v, axis=-1, keepdims=True)


def synth(n_conv: int = 30, seed: int = 0, turns_per: int = 40, spread: float = 0.0) -> model.Extracted:
    """Extract-shaped frames: conversations(), turns(), centroids and work vectors. `spread` scatters each
    conversation away from its topic, blurring the cluster boundaries the way the real corpus does."""
    rng = np.random.default_rng(seed)
    topics = _unit(rng.normal(size=(len(TAGS), DIM)))
    convs, rows, cents, wvecs = [], [], {}, {}
    for i in range(n_conv):
        guid = str(uuid.UUID(int=int(rng.integers(2**62)) << 64 | i, version=4))
        topic = i % len(TAGS)
        centre = topics[topic] + rng.normal(scale=spread, size=DIM) if spread else topics[topic]
        started = T0 + timedelta(hours=i)
        convs.append(
            {
                "guid": guid,
                "repo_key": f"git:example.com/acme/repo{topic}" if i % 5 else None,
                "worktree": None,
                "harness": "omp" if i % 2 else "claude",
                "models": ["claude-opus-5-5", "<synthetic>"] if i % 2 == 0 else ["gpt-6"],
                "pij_seat": None,
                "parent_guid": convs[i - 1]["guid"] if i % 7 == 6 else None,
                "started_at": started,
            }
        )
        nos, vecs = [], []
        for t in range(1, turns_per + 1):
            cls = "work" if t % 3 == 1 else "tool_call" if t % 3 == 2 else "tool_result"
            summarized = cls == "work" and t % 4 != 0
            v = _unit(centre + rng.normal(scale=0.4, size=DIM)).astype(np.float32)
            rows.append(
                {
                    "guid": guid,
                    "turn_no": t,
                    "at": started + timedelta(minutes=t),
                    "role": "agent" if cls != "work" or t % 2 else "human",
                    "source": "system" if cls != "work" else "human",
                    "class": cls,
                    "raw_hash": f"h-{guid[:8]}-{t}",
                    "bytes": 100 + t,
                    "summary": f"Works on {TAGS[topic][0]}" if summarized else None,
                    "tags": [x.upper() for x in TAGS[topic][: 1 + t % 3]] if summarized else None,
                    "summariser": "gpt-5.6-luna@1" if summarized else None,
                    "head": None if summarized or cls != "work" else "short note",
                    "stop_rule": None,
                    "preamble_seat": None,
                    "peer_from": None,
                    "send_to": None,
                    "msg_key": None,
                    "smart_vec": summarized,
                    "raw_vec": cls == "work",
                }
            )
            if cls == "work":
                nos.append(t)
                vecs.append(v)
        cents[guid] = (len(vecs), np.mean(vecs, axis=0))
        wvecs[guid] = (np.array(nos, dtype=np.int32), np.stack(vecs))
    conv = pd.DataFrame(convs)
    conv["started_at"] = pd.to_datetime(conv["started_at"], utc=True)
    return model.Extracted(conv, pd.DataFrame(rows), set(), cents, wvecs)


@pytest.fixture(scope="module")
def built():
    return model.build(synth(), model.Params(sessions=3, min_work_vectors=5), labels)


def test_build_output_satisfies_the_contract_and_publishes(built, tmp_path):
    bundle.validate_tables(built.tables)
    out = bundle.write_snapshot(
        bundle.SnapshotData(source="prod", tables=built.tables, vectors=built.vectors, provenance=built.provenance),
        tmp_path,
    )
    assert (out / "view" / "conversations.json").exists()


def test_conversation_rows_agree_with_turns(built):
    conv, turns = built.tables["conversations"], built.tables["turns"]
    assert int(conv["turns"].sum()) == len(turns)
    assert (conv["n_work"] == conv["guid"].map(turns[turns["class"] == "work"].groupby("guid").size())).all()
    assert turns["address"].str.fullmatch(r"conv:[0-9a-f-]{36}#t\d+").all()
    assert set(conv.loc[conv["parent_guid"].notna(), "kind"]) == {"subagent"}


def test_models_are_canonical_and_synthetic_dropped(built):
    models = built.tables["conversations"]["models"]
    assert all("<synthetic>" not in m for m in models)
    assert ["claude-opus-5.5"] in [list(m) for m in models]


def test_tags_are_normalised_and_deduplicated(built):
    tags = built.tables["turns"]["tags"]
    flat = [t for ts in tags for t in ts]
    assert flat and all(t == t.lower() for t in flat)
    assert all(len(set(ts)) == len(ts) for ts in tags)


def test_only_conversations_with_enough_vectors_are_embedded():
    ex = synth(n_conv=12)
    thin = ex.conversations["guid"].iloc[0]
    ex.centroids[thin] = (2, ex.centroids[thin][1])
    b = model.build(ex, model.Params(sessions=0, min_work_vectors=5), labels)
    conv = b.tables["conversations"].set_index("guid")
    assert not conv.at[thin, "embedded"] and pd.isna(conv.at[thin, "x"]) and pd.isna(conv.at[thin, "community_id"])
    assert thin not in b.vectors.centroid_guids
    assert len(b.vectors.centroid_guids) == int(conv["embedded"].sum()) == 11


def test_knn_edges_are_undirected_unique_and_centred(built):
    e = built.tables["knn_edges"]
    assert (e["a_guid"] < e["b_guid"]).all()
    assert not e.duplicated(["a_guid", "b_guid"]).any()
    assert (e["cos_raw"] > e["cos_centred"]).mean() > 0.9, "centring removes the shared direction"


def test_communities_are_dense_largest_first_and_cover_members(built):
    c = built.tables["communities"]
    assert list(c["community_id"]) == list(range(len(c)))
    assert list(c["size"]) == sorted(c["size"], reverse=True)
    conv = built.tables["conversations"]
    assert int(c["size"].sum()) == int(conv["embedded"].sum())


def test_stopset_rules_and_shared_hashes_flag_work_turns_only():
    ex = synth(n_conv=6)
    raw = ex.turns
    work = raw.index[raw["class"] == "work"]
    raw.loc[work[0], "stop_rule"] = "compact"
    shared_hash = raw.at[work[1], "raw_hash"]
    tool = raw.index[raw["class"] == "tool_call"][0]
    raw.loc[tool, "raw_hash"] = shared_hash  # same hash on a tool turn must not be flagged
    turns = model.turn_table(raw, {shared_hash}, labels)
    assert turns.loc[[work[0], work[1]], "stopset"].all()
    assert not turns.at[tool, "stopset"]
    stats = model.stopset_stats(raw, {shared_hash}, 20)
    assert stats["rules"]["compact"] == 1 and stats["rules"]["shared_hash"] == 1 and stats["excluded_turns"] == 2


def test_detail_guids_rank_by_work_turns():
    raw = pd.DataFrame(
        {
            "guid": ["a"] * 3 + ["b"] * 5 + ["c"] * 4,
            "turn_no": range(12),
            "class": ["work"] * 3 + ["tool_call"] * 5 + ["work"] * 4,
        }
    )
    assert model.detail_guids(raw, 2) == ["c", "a"]


# ---------------------------------------------------------------- seats and messages


def _msg_fixture():
    def conv(guid, seat, start_h, end_h):
        return {
            "guid": guid,
            "seat": seat,
            "started_at": T0 + timedelta(hours=start_h),
            "last_turn_at": T0 + timedelta(hours=end_h),
        }

    conv_df = pd.DataFrame(
        [
            conv("a", "pij-a-sender", 0, 10),
            conv("b", "pij-b-receiver", 0, 10),
            conv("c1", "pij-c-twin", 0, 10),
            conv("c2", "pij-c-twin", 0, 10),
            conv("d", None, 0, 10),
        ]
    )

    def turn(guid, n, minutes, source="system", peer_from=None, send_to=None, msg_key=None):
        return {
            "guid": guid,
            "turn_no": n,
            "at": T0 + timedelta(minutes=minutes),
            "source": source,
            "peer_from": peer_from,
            "send_to": send_to,
            "msg_key": msg_key,
        }

    raw = pd.DataFrame(
        [
            turn("a", 1, 10, send_to="pij-b-receiver"),  # a's send call ...
            turn("b", 1, 11, "peer", "pij-a-sender"),  # ... arrives in b a minute later
            turn("d", 1, 20, "peer", "pij-a-sender"),  # receiver seat unknown: a is the only live pij-a-sender
            turn("b", 2, 30, "peer", "pij-c-twin"),  # two live pij-c-twin conversations: ambiguous
            turn("b", 3, 50, "peer", None),  # no envelope, no send call in range
        ]
    )
    return raw, conv_df


def test_message_from_guid_resolution():
    raw, conv = _msg_fixture()
    msgs, stats = model.message_table(raw, conv, model.Params())
    by = msgs.set_index("address")["from_guid"].to_dict()
    assert by == {"conv:b#t1": "a", "conv:d#t1": "a", "conv:b#t2": None, "conv:b#t3": None}
    assert stats["from_guid_by_send_call"] == 1 and stats["from_guid_by_live_seat"] == 1
    assert msgs.set_index("address").at["conv:b#t1", "to_seat"] == "pij-b-receiver"


def test_send_call_from_a_different_seat_does_not_claim_the_message():
    raw, conv = _msg_fixture()
    raw.loc[0, "guid"] = "c1"  # the send now comes from a pij-c-twin conversation, envelope says pij-a-sender
    msgs, _ = model.message_table(raw, conv, model.Params())
    assert msgs.set_index("address").at["conv:b#t1", "from_guid"] == "a", "falls back to the live sender seat"


def _routing(*rows, seats=None):
    """Conversations s (sender), r (receiver, re-seated), x (a third seat) and turns given as
    (guid, turn_no, minutes, source, send_to, msg_key)."""
    seats = seats or {"s": "pij-send-er", "r": "pij-late-seat", "x": "pij-other-seat"}
    conv = pd.DataFrame(
        [{"guid": g, "seat": s, "started_at": T0, "last_turn_at": T0 + timedelta(hours=10)} for g, s in seats.items()]
    )
    raw = pd.DataFrame(
        [
            {
                "guid": g,
                "turn_no": n,
                "at": T0 + timedelta(minutes=m),
                "source": src,
                "peer_from": "pij-send-er" if src == "peer" else None,
                "send_to": to,
                "msg_key": key,
            }
            for g, n, m, src, to, key in rows
        ]
    )
    msgs, stats = model.message_table(raw, conv, model.Params())
    return msgs.set_index("address"), stats


def test_the_send_call_names_the_receiver_even_when_the_conversation_was_re_seated():
    msgs, stats = _routing(
        ("s", 1, 10, "system", "pij-early-seat", "firstmessagebodyhere"),
        ("r", 1, 11, "peer", None, "firstmessagebodyhere"),
        ("r", 2, 30, "peer", None, None),  # no matching send call: falls back to the conversation seat
    )
    assert msgs.at["conv:r#t1", "to_seat"] == "pij-early-seat", "per-message evidence beats the latest seat"
    assert msgs.at["conv:r#t1", "from_guid"] == "s"
    assert msgs.at["conv:r#t2", "to_seat"] == "pij-late-seat"
    assert stats["to_seat_send_call_beats_conversation_seat"] == 1


def test_a_send_call_outside_the_window_or_from_the_receiver_itself_does_not_match():
    msgs, _ = _routing(
        ("s", 1, 10, "system", "pij-early-seat", "stalemessagebody"),
        ("r", 1, 25, "peer", None, "stalemessagebody"),  # 15 minutes later: beyond message_match_s
        ("r", 2, 30, "system", "pij-echo-seat", "selfsentmessagebody"),
        ("r", 3, 31, "peer", None, "selfsentmessagebody"),  # the "send" is in the receiving conversation
    )
    assert msgs["to_seat"].tolist() == ["pij-late-seat", "pij-late-seat"]


def test_unmatched_messages_between_two_matches_naming_the_same_seat_take_that_seat():
    msgs, stats = _routing(
        ("s", 1, 10, "system", "pij-early-seat", "openingmessagebody"),
        ("r", 1, 11, "peer", None, "openingmessagebody"),
        ("r", 2, 20, "peer", None, None),  # bracketed by early/early
        ("s", 2, 29, "system", "pij-early-seat", "closingmessagebody"),
        ("r", 3, 30, "peer", None, "closingmessagebody"),
        ("r", 4, 40, "peer", None, None),  # after the last match: not bracketed
        ("s", 3, 49, "system", "pij-next-seat", "renamedmessagebody"),
        ("r", 5, 50, "peer", None, "renamedmessagebody"),
    )
    assert msgs.at["conv:r#t2", "to_seat"] == "pij-early-seat"
    assert msgs.at["conv:r#t4", "to_seat"] == "pij-late-seat", "early/next disagree: conversation seat"
    assert stats["to_seat_by_bracketing"] == 1 and stats["to_seat_by_send_call"] == 3
    assert stats["to_seat_send_call_beats_conversation_seat"] == 3, "direct matches only"
    assert stats["to_seat_differs_from_conversation_seat"] == 4, "bracketed t2 counts too"
    assert stats["to_seat_differs_conversations"] == 1


def test_a_broadcast_resolves_only_through_the_receivers_own_seat():
    body = "samebodytoeveryone"
    msgs, stats = _routing(
        ("s", 1, 10, "system", "pij-late-seat", body),
        ("s", 2, 10, "system", "pij-other-seat", body),
        ("r", 1, 11, "peer", None, body),  # r's seat is one of the targets
        ("s", 3, 20, "system", "pij-one-seat", "anotherbroadcastbody"),
        ("s", 4, 20, "system", "pij-two-seat", "anotherbroadcastbody"),
        ("x", 1, 21, "peer", None, "anotherbroadcastbody"),  # neither target is x's seat
        seats={"s": "pij-send-er", "r": "pij-late-seat", "x": None},
    )
    assert msgs.at["conv:r#t1", "to_seat"] == "pij-late-seat" and msgs.at["conv:r#t1", "from_guid"] == "s"
    assert pd.isna(msgs.at["conv:x#t1", "to_seat"])
    assert stats["send_call_ambiguous"] == 1


def test_seat_identity_prefers_metadata_over_preamble():
    conv = pd.DataFrame({"guid": ["m", "p", "n"], "pij_seat": ["pij-meta-seat", None, None]})
    raw = pd.DataFrame(
        {
            "guid": ["m", "p", "p"],
            "turn_no": [1, 4, 2],
            "preamble_seat": ["pij-other-seat", "pij-late-seat", "pij-early-seat"],
        }
    )
    seats = model.conversation_seats(conv, raw).set_index("guid")
    assert seats.loc["m"].tolist() == ["pij-meta-seat", "metadata"]
    assert seats.loc["p"].tolist() == ["pij-early-seat", "preamble"]
    assert seats.loc["n"].isna().all()


def test_seat_table_covers_every_message_sender(built):
    seats = set(built.tables["seats"]["seat"])
    senders = set(built.tables["messages"]["from_seat"].dropna())
    assert senders <= seats


# ---------------------------------------------------------------- re-export stability


def _previous(b: model.Built) -> model.Previous:
    conv = b.tables["conversations"]
    placed, clustered = conv.dropna(subset=["x", "y"]), conv.dropna(subset=["community_id"])
    return model.Previous(
        xy={g: (x, y) for g, x, y in zip(placed["guid"], placed["x"], placed["y"], strict=True)},
        centroids=dict(zip(b.vectors.centroid_guids, b.vectors.centroids, strict=True)),
        communities={g: int(c) for g, c in zip(clustered["guid"], clustered["community_id"], strict=True)},
        community_rows=b.tables["communities"],
    )


def _members(prefix: str, n: int) -> list[str]:
    return [f"{prefix}{i}" for i in range(n)]


def test_matched_clusters_keep_their_ids_even_when_sizes_reorder():
    prev = {g: 0 for g in _members("a", 10)} | {g: 1 for g in _members("b", 8)} | {g: 2 for g in _members("c", 5)}
    # fresh Louvain ids are largest first: b grew past a, so b=0, a=1; c=2 is unchanged
    guids = _members("b", 14) + _members("a", 10) + _members("c", 5)
    fresh = np.array([0] * 14 + [1] * 10 + [2] * 5)
    ids, info = model.stable_ids(fresh, guids, prev, 0.3)
    by = dict(zip(guids, ids, strict=True))
    assert (by["a0"], by["b0"], by["c0"]) == (0, 1, 2)
    assert info["matched"] == 3 and info["min_jaccard"] == round(8 / 14, 4)


def test_new_clusters_fill_free_ids_densely_largest_first():
    prev = {g: 0 for g in _members("a", 10)} | {g: 3 for g in _members("gone", 6)}
    guids = _members("big", 9) + _members("a", 10) + _members("small", 4)
    fresh = np.array([1] * 9 + [0] * 10 + [2] * 4)  # a=0 (largest), big=1, small=2
    ids, info = model.stable_ids(fresh, guids, prev, 0.3)
    by = dict(zip(guids, ids, strict=True))
    assert sorted(set(ids)) == [0, 1, 2], "dense even though old id 3 vanished"
    assert (by["a0"], by["big0"], by["small0"]) == (0, 1, 2)
    assert info["unmatched"] == 2


def test_low_overlap_is_not_a_match():
    prev = {g: 0 for g in _members("a", 10)}
    guids = _members("a", 2) + _members("x", 20)
    ids, info = model.stable_ids(np.array([0] * 22), guids, prev, 0.3)
    assert info["matched"] == 0 and set(ids) == {0}


def test_omp_role_defect_counts_only_long_omp_sessions_since_the_cutover():
    since = model.OMP_ROLE_DEFECT_SINCE
    conv = pd.DataFrame(
        [
            ("zero", "omp", 50, since),  # counted, zero human
            ("has-human", "omp", 80, since + pd.Timedelta(days=3)),  # counted, one human turn
            ("short", "omp", 49, since),  # under the turn floor
            ("before", "omp", 500, since - pd.Timedelta(seconds=1)),  # before the cutover
            ("claude", "claude", 500, since),  # other harness
        ],
        columns=["guid", "harness", "turns", "started_at"],
    )
    turns = pd.DataFrame(
        {"guid": ["zero", "has-human", "has-human", "short"], "role": ["agent", "agent", "human", "agent"]}
    )
    assert model.omp_role_defect_stats(conv, turns) == {
        "omp_sessions_ge50_since_0907": 2,
        "omp_sessions_zero_human": 1,
    }


def test_a_conversation_without_a_harness_is_flagged_and_never_a_long_session():
    ex = synth(n_conv=12)
    longest = model.detail_guids(ex.turns, 1)[0]
    ex.conversations.loc[ex.conversations["guid"].eq(longest), "harness"] = None
    b = model.build(ex, model.Params(sessions=1, min_work_vectors=5), labels)
    assert [f["guid"] for f in b.provenance["flagged_conversations"]] == [longest]
    detailed = set(b.tables["session_novelty"]["guid"])
    assert detailed and longest not in detailed, "the next-longest harness session takes its place"


def test_a_three_percent_delta_keeps_the_community_count_every_id_and_its_label():
    p = model.Params(sessions=0)
    corpus = dict(seed=2, turns_per=15, spread=0.4)  # diffuse, like the real corpus: Louvain's k is fragile
    first = model.build(synth(n_conv=120, **corpus), p, labels)
    second = model.build(synth(n_conv=124, **corpus), p, labels, _previous(first))  # +4 conversations
    start = second.provenance["clustering"]["start"]
    assert start["k_fresh"] != len(first.tables["communities"]), "precondition: a fresh re-cluster changes k"
    assert start["chosen"] == "warm"
    before = first.tables["communities"].set_index("community_id")["label"]
    after = second.tables["communities"].set_index("community_id")["label"]
    assert after.index.tolist() == before.index.tolist()
    assert after.tolist() == before.tolist()
    matches = second.provenance["clustering"]["id_matching"]["matches"]
    assert all(m["previous_id"] == m["id"] for m in matches)
    a = first.tables["conversations"].set_index("guid")["community_id"].dropna()
    b = second.tables["conversations"].set_index("guid")["community_id"].reindex(a.index)
    assert (a == b).mean() >= 0.95, "single-node moves only: membership barely changes"


def test_a_cluster_that_must_take_a_new_id_keeps_its_own_label():
    p = model.Params(sessions=0)
    ex = synth(n_conv=30)
    first = model.build(ex, p, labels)
    prev = _previous(first)
    # a previous snapshot with one more cluster at id 0: a lone conversation that rejoins its topic on the
    # rerun, so every surviving cluster cannot keep its id and the highest one moves into the freed 0
    loner = next(iter(prev.communities))
    prev.communities = {g: (0 if g == loner else c + 1) for g, c in prev.communities.items()}
    rows = prev.community_rows.assign(community_id=lambda d: d["community_id"] + 1)
    ghost = rows.iloc[[0]].assign(community_id=0, label="ghost cluster", tags=[["ghost"]])
    prev.community_rows = pd.concat([ghost, rows], ignore_index=True)

    second = model.build(ex, p, labels, prev)
    matches = second.provenance["clustering"]["id_matching"]["matches"]
    renamed = [m for m in matches if m["previous_id"] is not None and m["previous_id"] != m["id"]]
    assert renamed, "precondition: a matched cluster changed id"
    label_of = second.tables["communities"].set_index("community_id")["label"]
    old_label = prev.community_rows.set_index("community_id")["label"]
    for m in renamed:
        assert label_of[m["id"]] == old_label[m["previous_id"]]
    assert "ghost cluster" not in label_of.tolist()


def test_unchanged_conversations_keep_their_exact_position_and_new_ones_land_among_their_topic():
    p = model.Params(sessions=0)
    first = model.build(synth(n_conv=30), p, labels)
    assert first.provenance["layout"]["method"] == "umap"
    prev = _previous(first)

    grown = synth(n_conv=36)  # same seed: the first 30 conversations are identical
    second = model.build(grown, p, labels, prev)
    lay = second.provenance["layout"]
    assert lay["method"] == "kept + knn placement" and lay["kept_points"] == 30 and lay["placed_points"] == 6
    assert lay["median_displacement"] == 0 and lay["moved_points"] == 0

    conv = second.tables["conversations"].set_index("guid")
    for g, (x, y) in prev.xy.items():
        assert (conv.at[g, "x"], conv.at[g, "y"]) == (x, y)
    # a new conversation sits nearer its own topic's members than the other topics' (topic = index % 3)
    guids = list(grown.conversations["guid"])
    xy = conv.loc[guids, ["x", "y"]].to_numpy()
    for i in range(30, 36):
        d = np.linalg.norm(xy[:30] - xy[i], axis=1)
        same = np.array([j % 3 == i % 3 for j in range(30)])
        assert d[same].mean() < d[~same].mean()


def test_a_conversation_whose_centroid_moved_is_replaced_near_its_new_topic():
    p = model.Params(sessions=0)
    ex = synth(n_conv=30)
    prev = _previous(model.build(ex, p, labels))
    guids = list(ex.conversations["guid"])
    mover, target = guids[0], guids[1]  # topic 0 -> topic 1
    ex.centroids[mover] = (ex.centroids[mover][0], ex.centroids[target][1].copy())
    b = model.build(ex, p, labels, prev)
    assert b.provenance["layout"]["moved_points"] == 1
    conv = b.tables["conversations"].set_index("guid")
    new = conv.loc[mover, ["x", "y"]].to_numpy(dtype=float)
    topic1 = conv.loc[[g for j, g in enumerate(guids) if j % 3 == 1 and j], ["x", "y"]].to_numpy(dtype=float)
    topic0 = conv.loc[[g for j, g in enumerate(guids) if j % 3 == 0 and j], ["x", "y"]].to_numpy(dtype=float)
    assert np.linalg.norm(topic1 - new, axis=1).mean() < np.linalg.norm(topic0 - new, axis=1).mean()


def test_a_mostly_new_corpus_gets_a_fresh_layout():
    p = model.Params(sessions=0)
    prev = _previous(model.build(synth(n_conv=12), p, labels))
    b = model.build(synth(n_conv=30), p, labels, prev)
    assert b.provenance["layout"]["method"] == "umap"


def test_contract_columns_in_order(built):
    for name, df in built.tables.items():
        assert list(df.columns) == contract.TABLES[name].columns
