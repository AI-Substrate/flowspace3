"""The bundle writer's guarantees: what it publishes validates, and the two layers agree."""

import json

import pandas as pd
import pytest

from conv_atlas import bundle, contract, fixture, view


@pytest.fixture(scope="module")
def snap(tmp_path_factory):
    home = tmp_path_factory.mktemp("home")
    data = fixture.generate(n_conversations=60, seed=3, detailed=2)
    data.snapshot_id = "20260801T000000Z"
    return bundle.write_snapshot(data, home), data


def test_view_layer_agrees_with_the_tables(snap):
    root, data = snap
    convs = json.loads((root / "view" / "conversations.json").read_text())
    table = pd.read_parquet(root / "tables" / "conversations.parquet")
    assert sorted(c["guid"] for c in convs) == sorted(table["guid"])
    assert sum(c["turns"] for c in convs) == int(table["turns"].sum())
    assert [c["started_ms"] for c in convs] == sorted(c["started_ms"] for c in convs), "sorted by start"
    sessions = {p.stem for p in (root / "view" / "sessions").glob("*.json")}
    assert sessions == {c["guid"] for c in convs if c["session_detail"]}


def test_every_view_element_validates_not_just_the_write_path_sample(snap):
    root, _ = snap
    for f in sorted((root / "view").rglob("*.json")):
        view.validate(str(f.relative_to(root / "view")), json.loads(f.read_text()))


def test_sampled_validation_still_rejects_a_malformed_item_in_the_sample():
    convs = [{"guid": "x"}] * 1000  # every item invalid: the sample must see one
    with pytest.raises(ValueError, match="conversations.json"):
        view.validate("conversations.json", convs, sample=True)


def test_manifest_points_at_the_newest_published_snapshot(snap, tmp_path):
    data = fixture.generate(n_conversations=30, seed=4, detailed=1)
    for sid in ("20260802T000000Z", "20260801T000000Z"):
        data.snapshot_id = sid
        bundle.write_snapshot(data, tmp_path)
    manifest = json.loads((tmp_path / "bundles" / "manifest.json").read_text())
    assert manifest["latest"] == "20260802T000000Z"
    assert [s["id"] for s in manifest["snapshots"]] == ["20260801T000000Z", "20260802T000000Z"]


def test_a_contract_violation_publishes_nothing(tmp_path):
    data = fixture.generate(n_conversations=20, seed=5, detailed=1)
    data.tables["conversations"] = data.tables["conversations"].drop(columns=["label"])
    with pytest.raises(bundle.ContractError, match="conversations: columns"):
        bundle.write_snapshot(data, tmp_path)
    bundles = tmp_path / "bundles"
    assert not bundles.exists() or list(bundles.iterdir()) == []


def test_datapackage_describes_every_table_with_keys(snap):
    root, _ = snap
    dp = json.loads((root / "datapackage.json").read_text())
    names = {r["name"] for r in dp["resources"]}
    assert names == set(contract.TABLES)
    turns = next(r for r in dp["resources"] if r["name"] == "turns")
    assert turns["schema"]["primaryKey"] == ["address"]
    assert dp["provenance"]["source"] == "synthetic"
