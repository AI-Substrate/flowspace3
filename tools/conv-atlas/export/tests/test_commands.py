"""`describe` and `sql`: the agent entry points over a snapshot (synthetic fixture)."""

import json

import pytest

from conv_atlas import bundle, commands, contract, fixture
from conv_atlas.cli import main


@pytest.fixture(scope="module")
def home(tmp_path_factory):
    h = tmp_path_factory.mktemp("home")
    data = fixture.generate(n_conversations=40, seed=11, detailed=1)
    data.snapshot_id = "20260901T000000Z"
    bundle.write_snapshot(data, h)
    return h


def test_describe_lists_every_table_with_footer_row_counts(home):
    d = commands.describe(home=home)
    assert {t["name"] for t in d["tables"]} == set(contract.TABLES)
    conv = next(t for t in d["tables"] if t["name"] == "conversations")
    out = commands.sql("select count(*) as n from conversations", home=home)
    assert conv["rows"] == out["rows"][0]["n"]
    assert "conversation_centroids_index" in d["vector_views"]


def test_describe_one_table_and_unknown_table(home):
    t = commands.describe("knn_edges", home=home)
    assert t["schema"]["primaryKey"] == ["a_guid", "b_guid"]
    with pytest.raises(ValueError, match="unknown table 'nope'; tables: conversations"):
        commands.describe("nope", home=home)


def test_sql_joins_views_and_returns_iso_timestamps(home):
    out = commands.sql(
        "select least(a.repo_key, b.repo_key) as repo_a, greatest(a.repo_key, b.repo_key) as repo_b, count(*) as edges "
        "from knn_edges e join conversations a on a.guid = e.a_guid join conversations b on b.guid = e.b_guid "
        "where e.cross_repo group by all order by edges desc, repo_a, repo_b",
        home=home,
    )
    assert out["columns"] == ["repo_a", "repo_b", "edges"] and out["rows"]
    assert all(r["repo_a"] < r["repo_b"] for r in out["rows"])
    ts = commands.sql("select max(turn_at) as last from turns", home=home)["rows"][0]["last"]
    assert ts.endswith("Z") and "T" in ts


def test_sql_truncates_at_the_limit_and_says_so(home):
    out = commands.sql("select guid from conversations order by guid", home=home, limit=5)
    assert out["row_count"] == 5 and out["truncated"]
    assert not commands.sql("select guid from conversations", home=home, limit=0)["truncated"]


def test_sql_vector_index_view_joins_back_to_conversations(home):
    out = commands.sql(
        "select count(*) as n from conversation_centroids_index i join conversations c using (guid) where c.embedded",
        home=home,
    )
    n_embedded = commands.sql("select count(*) as n from conversations where embedded", home=home)["rows"][0]["n"]
    assert out["rows"][0]["n"] == n_embedded


def test_sql_errors_come_back_in_the_envelope(home, capsys):
    assert main(["--json", "sql", "select nope from conversations", "--home", str(home)]) == 1
    env = json.loads(capsys.readouterr().out)
    assert not env["ok"] and "nope" in env["error"]
