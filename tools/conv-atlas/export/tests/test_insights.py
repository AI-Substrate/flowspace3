"""Insights are validated before touching disk, append-only, and the index never loses an entry."""

import json
from concurrent.futures import ProcessPoolExecutor

import pytest

from conv_atlas import insights, paths
from conv_atlas.cli import main

GUID = "0b9e3f8e-3c55-4c1e-9a49-6a3f0f2a7d11"


def _doc(i: str, created_ms: int = 1_790_000_000_000, **extra) -> dict:
    return {
        "id": i,
        "title": f"finding {i}",
        "body_md": "Synthetic body.",
        "author": "pij-test-seat",
        "created_ms": created_ms,
        "snapshot_id": "20260801T000000Z",
        "refs": {"guids": [GUID], "addresses": [f"conv:{GUID}#t3"]},
        **extra,
    }


def _index(home):
    return json.loads((paths.insights_dir(home) / "index.json").read_text())


def test_add_publishes_document_and_index_newest_first(tmp_path):
    insights.add(_doc("older-one", 1000), tmp_path)
    insights.add(_doc("newer-one", 2000, kind="anomaly"), tmp_path)
    idx = _index(tmp_path)
    assert [e["id"] for e in idx["insights"]] == ["newer-one", "older-one"]
    assert idx["insights"][0] == {
        "id": "newer-one",
        "title": "finding newer-one",
        "kind": "anomaly",
        "created_ms": 2000,
    }
    assert insights.list_insights(tmp_path) == idx["insights"]
    stored = json.loads((paths.insights_dir(tmp_path) / "older-one.json").read_text())
    assert stored["kind"] == "finding", "kind defaults to finding"


@pytest.mark.parametrize(
    "bad",
    [
        {"id": "Bad Id"},
        {"title": "no"},
        {"refs": {"addresses": ["conv:not-a-guid"]}},
        {"unexpected": 1},
    ],
)
def test_invalid_documents_write_nothing(tmp_path, bad):
    with pytest.raises(ValueError):
        insights.add({**_doc("valid-id"), **bad}, tmp_path)
    d = paths.insights_dir(tmp_path)
    assert not d.exists() or not [p for p in d.iterdir() if not p.name.startswith(".")]


def test_duplicate_id_is_refused_and_keeps_the_original(tmp_path):
    insights.add(_doc("same-id", title="first"), tmp_path)
    with pytest.raises(ValueError, match="already exists"):
        insights.add(_doc("same-id", title="second"), tmp_path)
    assert json.loads((paths.insights_dir(tmp_path) / "same-id.json").read_text())["title"] == "first"
    assert [e["title"] for e in _index(tmp_path)["insights"]] == ["first"]


def test_ensure_index_creates_an_empty_index_and_never_clobbers_entries(tmp_path):
    insights.ensure_index(tmp_path)
    assert _index(tmp_path) == {"schema_version": 1, "insights": []}
    insights.add(_doc("kept-entry"), tmp_path)
    insights.ensure_index(tmp_path)
    assert [e["id"] for e in _index(tmp_path)["insights"]] == ["kept-entry"]


def _add(args):
    home, i = args
    insights.add(_doc(f"concurrent-{i:03d}", 1000 + i), home)


def test_concurrent_writers_never_lose_an_index_entry(tmp_path):
    with ProcessPoolExecutor(max_workers=4) as pool:
        list(pool.map(_add, [(tmp_path, i) for i in range(24)]))
    ids = [e["id"] for e in _index(tmp_path)["insights"]]
    assert ids == [f"concurrent-{i:03d}" for i in reversed(range(24))]
    assert not list(paths.insights_dir(tmp_path).glob(".concurrent-*")), "no temp files left behind"


def test_cli_add_and_list(tmp_path, capsys):
    f = tmp_path / "f.json"
    f.write_text(json.dumps(_doc("from-cli")))
    assert main(["--json", "insight", "add", str(f), "--home", str(tmp_path)]) == 0
    assert json.loads(capsys.readouterr().out)["data"]["id"] == "from-cli"
    assert main(["--json", "insight", "list", "--home", str(tmp_path)]) == 0
    assert json.loads(capsys.readouterr().out)["data"]["count"] == 1
    f.write_text("{not json")
    assert main(["--json", "insight", "add", str(f), "--home", str(tmp_path)]) == 1
    assert "not JSON" in json.loads(capsys.readouterr().out)["error"]


def test_every_schema_problem_is_reported_at_once_and_names_the_file(tmp_path, capsys):
    f = tmp_path / "draft.json"
    f.write_text(json.dumps({"title": "x", "body_md": "y", "snapshot_id": "s", "refs": {}}))
    assert main(["--json", "insight", "add", str(f), "--home", str(tmp_path)]) == 1
    error = json.loads(capsys.readouterr().out)["error"]
    assert error.startswith(f"{f}: invalid insight, 4 problem(s)")
    for problem in ("'id' is a required property", "'author' is a required property", "title: ", "refs: "):
        assert problem in error
    assert "insights/.json" not in error


def test_the_help_example_is_a_valid_insight(tmp_path, capsys):
    with pytest.raises(SystemExit):
        main(["insight", "--help"])
    shown = capsys.readouterr().out
    assert json.dumps(insights.EXAMPLE, indent=2) in shown
    insights.add({**insights.EXAMPLE, "snapshot_id": "20260801T000000Z"}, tmp_path)


def test_refs_missing_from_the_snapshot_are_warned_about_but_published(tmp_path, capsys):
    from conv_atlas import bundle, fixture

    data = fixture.generate(n_conversations=20, seed=3)
    snap = bundle.write_snapshot(data, tmp_path).name
    real = data.tables["conversations"]["guid"].iloc[0]
    doc = _doc("mixed-refs", snapshot_id=snap)
    doc["refs"] = {"guids": [real, GUID], "addresses": [f"conv:{real}", f"conv:{GUID}#t3"], "seats": ["pij-nope-seat"]}
    f = tmp_path / "mixed.json"
    f.write_text(json.dumps(doc))
    assert main(["--json", "insight", "add", str(f), "--home", str(tmp_path)]) == 0
    out = json.loads(capsys.readouterr().out)
    assert out["data"]["warnings"] == [
        f"refs.guids not in snapshot {snap}: {GUID}",
        f"refs.seats not in snapshot {snap}: pij-nope-seat",
        f"refs.addresses not in snapshot {snap}: conv:{GUID}#t3",
    ]
    assert [e["id"] for e in insights.list_insights(tmp_path)] == ["mixed-refs"]
