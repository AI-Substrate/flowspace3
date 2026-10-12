"""The generated snapshot README: its SQL runs, its vector recipe is right, its caveats follow provenance."""

from __future__ import annotations

import json
import re

import duckdb
import numpy as np
import pandas as pd
import pytest

from conv_atlas import bundle, contract, fixture, insights, readme

FENCE = re.compile(r"```(sql|python|json)\n(.*?)```", re.S)


@pytest.fixture(scope="module")
def snap(tmp_path_factory):
    data = fixture.generate(seed=5)
    data.snapshot_id = "20260901T000000Z"
    root = bundle.write_snapshot(data, tmp_path_factory.mktemp("home"))
    return root, (root / "README.md").read_text()


def blocks(text: str, lang: str) -> list[str]:
    return [body for kind, body in FENCE.findall(text) if kind == lang]


def test_every_sql_block_in_the_readme_runs_and_returns_rows(snap, monkeypatch):
    root, text = snap
    monkeypatch.chdir(root)  # the README's register block uses snapshot-relative paths
    register, *queries = blocks(text, "sql")
    assert len(queries) == len(readme.EXAMPLE_QUERIES) and 6 <= len(queries) <= 12
    con = duckdb.connect()
    con.execute(register)
    for sql in queries:
        rows = con.execute(sql).to_arrow_table().num_rows  # arrow: tz-aware columns need no pytz
        assert rows, f"no rows from:\n{sql}"


def test_the_vector_recipe_reproduces_knn_cos_centred(snap, monkeypatch):
    root, text = snap
    monkeypatch.chdir(root)
    (snippet,) = blocks(text, "python")
    scope: dict = {}
    exec(snippet, scope)  # noqa: S102 - the README's own recipe, run as a reader would
    index = {g: i for i, g in enumerate(scope["guids"])}
    edges = pd.read_parquet("tables/knn_edges.parquet").head(20)
    got = [scope["sims"][index[a], index[b]] for a, b in zip(edges["a_guid"], edges["b_guid"], strict=True)]
    np.testing.assert_allclose(got, edges["cos_centred"], atol=1e-4)


def test_every_table_and_declared_join_is_documented(snap):
    _, text = snap
    for name, table in contract.TABLES.items():
        assert f"### `{name}`" in text
        for fk in table.foreign_keys:
            assert f"`{name}.{fk.fields[0]}` | `{fk.table}.{fk.reference[0]}`" in text


def test_caveats_follow_the_provenance_cartographer_emits():
    descriptor = contract.datapackage(
        "20260901T000000Z",
        {
            "summariser_models": {"gen-a@1": 900, "gen-b@1": 100},
            "stats": {
                "coverage_summary": 0.82,
                "harness_confound_same_harness_edge_share": 0.85,
                "harness_confound_random_baseline": 0.5,
                "dl029_tool_call_turns": 4321,
            },
            "layout": {"method": "kept + knn placement", "kept_points": 480, "placed_points": 12},
            "stopset": {"rules": {"compact": 37, "empty": 52}, "excluded_turns": 89},
            "known_defects": ["DL-029: tool_call text polluted by the elision bug"],
            "database": {"host": "127.0.0.1", "name": "flowspace3"},
        },
        set(contract.TABLES),
    )
    tables = {name: pd.DataFrame(columns=t.columns) for name, t in contract.TABLES.items()}
    text = readme.render(descriptor, tables)
    assert "85.0% of kNN edges join the same harness against a random baseline of 50.0%" in text
    assert "2 summariser generations: `gen-a@1` (900), `gen-b@1` (100)" in text
    assert "| `coverage_summary` | 82.0% |" in text
    assert "| `empty` | 52 |" in text
    assert "- DL-029: tool_call text polluted by the elision bug" in text
    assert "- DL-029 affects 4,321 tool_call turns in this snapshot." in text
    assert "480 points keep their earlier coordinates and 12 new or changed ones were placed" in text
    assert "**database**: host=127.0.0.1, name=flowspace3" in text
    assert text == readme.render(descriptor, tables)  # same inputs, same bytes


def test_the_readme_insight_example_is_accepted_as_written_and_anchors_in_the_snapshot(snap, tmp_path):
    root, text = snap
    (doc,) = [json.loads(b) for b in blocks(text, "json")]
    guids = set(pd.read_parquet(root / "tables/conversations.parquet")["guid"])
    assert set(doc["refs"]["guids"]) <= guids and doc["snapshot_id"] == root.name
    path = insights.add(doc, tmp_path)  # what `conv-atlas insight add` does with the copied file
    assert json.loads(path.read_text())["refs"] == doc["refs"]


def test_at_a_glance_and_caveats_count_unknown_harness_and_seats_instead_of_hiding_them(snap):
    root, _ = snap
    descriptor = json.loads((root / "datapackage.json").read_text())
    tables = {r["name"]: pd.read_parquet(root / r["path"]) for r in descriptor["resources"]}
    conv, msgs = tables["conversations"], tables["messages"]
    conv.loc[conv.index[0], "harness"] = None
    msgs.loc[msgs.index[::3], "to_seat"] = None
    text = readme.render(descriptor, tables)
    assert ", no harness 1" in text
    known = int(msgs["to_seat"].notna().sum())
    assert f"Of {len(msgs):,} peer messages, {known:,} ({known / len(msgs):.0%}) name the receiver seat" in text
