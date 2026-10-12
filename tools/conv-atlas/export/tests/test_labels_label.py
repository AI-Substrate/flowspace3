"""Conversation, community and shift labels (synthetic frames only: the repo is public)."""

from __future__ import annotations

import pandas as pd
import pytest

from conv_atlas.fixture import generate
from conv_atlas.labels import is_clean, label_communities, label_conversations, label_shift

G = [f"00000000-0000-4000-8000-00000000000{i}" for i in range(6)]
T0 = pd.Timestamp("2026-06-01T09:00:00Z")


def conv(rows: list[dict]) -> pd.DataFrame:
    base = {"kind": "session", "seat": None, "repo_key": None, "tags": [], "started_at": T0, "turns": 10}
    return pd.DataFrame([{**base, **r} for r in rows])


def turns(rows: list[tuple]) -> pd.DataFrame:
    """rows: (guid, turn_no, summary, tags[, class])."""
    out = []
    for r in rows:
        guid, no, summary, tags = r[:4]
        out.append(
            {
                "guid": guid,
                "address": f"conv:{guid}#t{no}",
                "turn_no": no,
                "class": r[4] if len(r) > 4 else "work",
                "summary": summary,
                "head": None,
                "tags": tags,
                "stopset": False,
            }
        )
    return pd.DataFrame(out)


def test_label_uses_distinctive_tags_not_corpus_wide_ones() -> None:
    c = conv([{"guid": g} for g in G[:3]])
    t = turns(
        [
            (G[0], 1, "Reviews terrain shaders.", ["testing", "terrain", "shader"]),
            (G[0], 2, "Fixes terrain lod.", ["testing", "terrain", "lod"]),
            (G[1], 1, "Drafts a chapter.", ["testing", "chapter"]),
            (G[2], 1, "Tunes CI.", ["testing", "ci"]),
        ]
    )
    out = label_conversations(c, t)
    assert out["guid"].tolist() == G[:3]
    assert out.loc[0, "label"].startswith("Terrain")
    assert "testing" not in out.loc[0, "label"].lower()
    assert out.loc[2, "label"].startswith("CI")


def test_tool_turns_and_stopset_turns_do_not_label() -> None:
    c = conv([{"guid": G[0]}, {"guid": G[1]}])
    t = turns(
        [
            (G[0], 1, "Edits a file.", ["file io"], "tool_call"),
            (G[0], 2, "Plans the release train.", ["release"]),
            (G[1], 1, "Other.", ["other topic"]),
        ]
    )
    t.loc[len(t)] = {**t.iloc[1].to_dict(), "turn_no": 3, "address": f"conv:{G[0]}#t3", "tags": ["boilerplate"]}
    t.loc[len(t) - 1, "stopset"] = True
    label = label_conversations(c, t).loc[0, "label"]
    assert label == "Release"


def test_labels_never_carry_ids_or_title_words() -> None:
    c = conv(
        [
            {"guid": G[0], "repo_key": "git:example.com/acme/shipyard"},
            {"guid": G[1], "seat": "pij-amber-otter"},
            {"guid": G[2], "repo_key": f"/tmp/{G[2]}"},  # repo that is itself an id
            {"guid": G[3], "started_at": pd.NaT},
        ]
    )
    t = turns(
        [
            (G[0], 1, None, ["pij session", "peer session"]),
            (G[1], 1, f"The session {G[1]} records that subagent 9fceb02d0ae5 ran.", []),
        ]
    )
    out = label_conversations(c, t)
    for label in out["label"]:
        assert is_clean(label), label
        assert label
    assert out.loc[0, "label"].startswith("shipyard · Jun 01")
    assert out.loc[2, "label"].startswith("Untitled")
    assert G[1] not in out.loc[1, "blurb"] and "9fceb02d0ae5" not in out.loc[1, "blurb"]


def test_labels_are_distinct_within_a_seat_but_may_repeat_across_seats() -> None:
    same = ["release", "changelog", "versioning"]
    c = conv(
        [
            {"guid": G[0], "seat": "pij-a-b", "started_at": T0},
            {"guid": G[1], "seat": "pij-a-b", "started_at": T0 + pd.Timedelta(days=3)},
            {"guid": G[2], "seat": "pij-c-d", "started_at": T0},
        ]
    )
    t = turns([(g, 1, "Cuts a release.", same) for g in G[:3]])
    out = label_conversations(c, t)["label"].tolist()
    assert out[0] != out[1]
    assert out[2] == "Changelog · release"  # alone in its seat: no decoration
    assert out[0].endswith("(Jun 01)") and out[1].endswith("(Jun 04)")


def test_blurb_is_the_representative_summary_without_framing() -> None:
    c = conv([{"guid": G[0]}, {"guid": G[1]}])
    t = turns(
        [
            (G[0], 1, "Ready.", ["release"]),
            (G[0], 2, "The output lists the release checklist for the train. More detail.", ["release", "train"]),
            (G[0], 3, "Unrelated chatter about lunch plans today.", ["lunch"]),
            (G[1], 1, "Other.", ["other"]),
        ]
    )
    blurb = label_conversations(c, t).loc[0, "blurb"]
    assert blurb == "The release checklist for the train"


def test_turn_score_moves_the_blurb_to_the_central_turn() -> None:
    c = conv([{"guid": G[0]}])
    t = turns(
        [
            (G[0], 1, "Plans the first part of the migration work.", ["migration"]),
            (G[0], 2, "Plans the second part of the migration work.", ["migration"]),
        ]
    )
    plain = label_conversations(c, t).loc[0, "blurb"]
    nudged = label_conversations(c, t, turn_score=pd.Series({f"conv:{G[0]}#t2": 0.9})).loc[0, "blurb"]
    assert "first" in plain and "second" in nudged


def test_caps_hold_for_long_inputs() -> None:
    long_tag = "extraordinarily long topic name that keeps going on"
    c = conv([{"guid": G[0]}, {"guid": G[1]}])
    t = turns([(G[0], 1, "word " * 80, [long_tag, "second topic"]), (G[1], 1, "x", ["y"])])
    out = label_conversations(c, t)
    assert len(out.loc[0, "label"]) <= 48 and out.loc[0, "label"].endswith("…")
    assert len(out.loc[0, "blurb"]) <= 140


def communities_frame() -> tuple[pd.DataFrame, pd.DataFrame]:
    rows, trows = [], []
    for i in range(6):
        cid = 0 if i < 3 else 1
        guid = G[i]
        topic = ["terrain", "shader"] if cid == 0 else ["chapter", "dialogue"]
        rows.append(
            {
                "guid": guid,
                "community_id": cid,
                "tags": ["testing", *topic],
                "blurb": f"Member {i} works on {topic[0]}.",
                "repo_key": "git:example.com/acme/atlas-engine" if cid == 0 else f"git:example.com/acme/r{i}",
            }
        )
        trows += [(guid, 1, "s", ["testing", *topic]), (guid, 2, "s", ["testing", topic[0]])]
    rows.append({"guid": "unclustered", "community_id": None, "tags": ["noise"], "blurb": "x", "repo_key": None})
    c = pd.DataFrame(rows)
    c["community_id"] = c["community_id"].astype("Int64")
    return c, turns(trows)


def test_community_labels_use_tags_over_represented_in_the_cluster() -> None:
    c, t = communities_frame()
    out = label_communities(c, t)
    assert out["community_id"].tolist() == [0, 1]
    assert out.loc[0, "label"] == "Terrain · shader"
    assert out.loc[1, "label"] == "Chapter · dialogue"
    assert "testing" not in out.loc[0, "tags"]
    assert out.loc[0, "blurb"].startswith("Mostly atlas-engine (100%). ")
    assert not out.loc[1, "blurb"].startswith("Mostly")  # no repo holds half of it
    assert all(len(s) <= 32 for s in out["label"]) and all(len(s) <= 140 for s in out["blurb"])


def test_a_sharply_over_represented_tag_outranks_a_frequent_corpus_wide_one() -> None:
    # c0: "validation" is frequent but common everywhere (lift ~1.8); "shader" is rare but specific (lift ~2.4).
    c = pd.DataFrame(
        [
            {"guid": G[i], "community_id": 0 if i < 2 else 1, "tags": ["validation", "shader" if i < 2 else "chapter"]}
            for i in range(4)
        ]
    )
    c["community_id"] = c["community_id"].astype("Int64")
    rows = []
    for i in range(4):
        specific, n_specific, n_generic = ("shader", 1, 6) if i < 2 else ("chapter", 8, 2)
        rows += [(G[i], k, "s", ["validation"]) for k in range(n_generic)]
        rows += [(G[i], 100 + k, "s", [specific]) for k in range(n_specific)]
    out = label_communities(c, turns(rows))
    assert out.loc[0, "label"] == "Shader · validation"
    assert "Also" not in out.loc[0, "blurb"]  # both tags are already in the label


def test_matched_communities_keep_their_label_while_its_tags_still_hold() -> None:
    c, t = communities_frame()  # fresh labels: "Terrain · shader", "Chapter · dialogue"
    previous = pd.DataFrame(
        {
            "community_id": [0, 1, 7],
            "label": ["Shader · terrain", "Chapter · battles", "Gone · away"],
            "tags": [["shader", "terrain"], ["chapter", "battles"], ["gone"]],
        }
    )
    out = label_communities(c, t, previous=previous)
    assert out["label"].tolist() == ["Shader · terrain", "Chapter · dialogue"]  # kept; "battles" left the top 6
    fallback = previous.assign(label=["Cluster 0", "Chapter…", None])
    out = label_communities(c, t, previous=fallback)
    assert out["label"].tolist() == ["Terrain · shader", "Chapter…"]  # a clipped part matches by prefix


def test_community_blurb_comes_from_the_most_central_member() -> None:
    c, t = communities_frame()
    out = label_communities(c, t, centrality=pd.Series({G[2]: 0.9, G[0]: 0.1, G[4]: 0.8}))
    assert out.loc[0, "blurb"].endswith("Member 2 works on terrain")
    assert out.loc[1, "blurb"] == "Member 4 works on chapter"


@pytest.mark.parametrize(
    ("tags", "summaries", "want"),
    [
        (["watchdog", "governance", "handover"], [], "Watchdog · governance"),
        (["Tests", "completed"], [], "Testing"),
        ([], ["The thread records that the release train left. Then more."], "The release train left"),
        (["completed"], [""], "New topic"),
        (["pij session"], ["A session started."], "New topic"),
    ],
)
def test_label_shift(tags: list[str], summaries: list[str], want: str) -> None:
    got = label_shift(tags, summaries)
    assert got == want and len(got) <= 32


def test_fixture_corpus_labels_hold_every_invariant() -> None:
    data = generate(n_conversations=120, seed=3)
    c, t = data.tables["conversations"], data.tables["turns"]
    out = label_conversations(c, t)
    assert out["guid"].tolist() == c["guid"].tolist()
    assert out["label"].map(is_clean).all() and out["label"].str.len().le(48).all()
    assert out["blurb"].str.len().le(140).all() and out["blurb"].str.len().gt(0).all()
    seated = pd.DataFrame({"seat": c["seat"], "label": out["label"]}).dropna()
    assert not seated.duplicated().any()
    comm = label_communities(c.drop(columns="blurb").assign(blurb=out["blurb"].values), t)
    assert comm["label"].is_unique and comm["label"].map(is_clean).all()
    assert out.equals(label_conversations(c, t))  # deterministic
