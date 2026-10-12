"""Incremental export: what a re-run re-pulls, and when it publishes nothing. The database is faked."""

from __future__ import annotations

import json
from contextlib import contextmanager
from datetime import UTC, datetime, timedelta

import pandas as pd
import pytest
from test_model import synth

from conv_atlas import export, extract, paths


class FakeDB:
    """Serves synthetic extract-shaped data through the `extract` API and records what was asked for."""

    def __init__(self, n_conv: int = 12):
        self.ex = synth(n_conv=n_conv, turns_per=20)
        self.shared: set[str] = set()
        self.late: set[str] = set()
        self.calls: dict[str, list] = {"turns": [], "centroids": [], "work_vectors": []}

    def install(self, mp: pytest.MonkeyPatch) -> None:
        db = self

        class Conn:
            def execute(self, sql, *a):
                return type("R", (), {"fetchone": lambda _self: (datetime.now(UTC),)})()

        @contextmanager
        def connect(url=None):
            yield Conn()

        def heads(conn):
            g = db.ex.turns.groupby("guid")["turn_no"].agg(["max", "size"]).reset_index()
            return g.rename(columns={"max": "max_turn_no", "size": "n_turns"})

        def turns(conn, guids):
            db.calls["turns"] += list(guids)
            return db.ex.turns[db.ex.turns["guid"].isin(guids)].copy()

        def centroids(conn, guids, excluded):
            db.calls["centroids"] += list(guids)
            return {g: db.ex.centroids[g] for g in guids}

        def work_vectors(conn, guids):
            db.calls["work_vectors"] += list(guids)
            return {g: db.ex.work_vectors[g] for g in guids}

        mp.setattr(extract, "connect", connect)
        mp.setattr(extract, "turn_heads", heads)
        mp.setattr(extract, "conversations", lambda conn: db.ex.conversations.copy())
        mp.setattr(extract, "shared_work_hashes", lambda conn: set(db.shared))
        mp.setattr(extract, "enriched_since", lambda conn, since: set(db.late))
        mp.setattr(extract, "turns", turns)
        mp.setattr(extract, "centroids", centroids)
        mp.setattr(extract, "work_vectors", work_vectors)

    def reset_calls(self) -> None:
        for v in self.calls.values():
            v.clear()

    def append_turn(self, guid: str) -> None:
        t = self.ex.turns
        last = t[t["guid"] == guid].iloc[-1].copy()
        last["turn_no"] += 1
        last["at"] = last["at"] + timedelta(minutes=1)
        last["raw_hash"] = f"{last['raw_hash']}-next"
        self.ex.turns = pd.concat([t, last.to_frame().T], ignore_index=True).astype(t.dtypes)


@pytest.fixture
def db(monkeypatch, tmp_path):
    d = FakeDB()
    d.install(monkeypatch)
    return d


def _snapshots(home) -> list[str]:
    return [s["id"] for s in json.loads((paths.bundles_dir(home) / "manifest.json").read_text())["snapshots"]]


def _run(home, **kw):
    out = export.run(home, url="postgres://u:secret@db.example:5433/fs3", **kw)
    if not out["unchanged"]:
        # snapshot ids have one-second resolution; keep consecutive test runs distinct
        import time

        time.sleep(1.05)
    return out


def test_first_run_pulls_everything_and_a_rerun_with_no_change_publishes_nothing(db, tmp_path):
    first = _run(tmp_path)
    guids = set(db.ex.conversations["guid"])
    assert set(db.calls["turns"]) == guids and set(db.calls["centroids"]) == guids
    assert _snapshots(tmp_path) == [first["snapshot"]]
    index = json.loads((paths.insights_dir(tmp_path) / "index.json").read_text())
    assert index == {"schema_version": 1, "insights": []}, "the polling app must not 404 on a fresh home"

    db.reset_calls()
    again = _run(tmp_path)
    assert again["unchanged"] and again["latest"] == first["snapshot"]
    assert db.calls == {"turns": [], "centroids": [], "work_vectors": []}
    assert _snapshots(tmp_path) == [first["snapshot"]]


def test_force_republishes_from_cache_without_pulling(db, tmp_path):
    _run(tmp_path)
    db.reset_calls()
    out = _run(tmp_path, force=True)
    assert not out["unchanged"] and len(_snapshots(tmp_path)) == 2
    assert db.calls == {"turns": [], "centroids": [], "work_vectors": []}
    dp = json.loads((paths.bundles_dir(tmp_path) / out["snapshot"] / "datapackage.json").read_text())
    assert dp["provenance"]["layout"]["method"] == "kept + knn placement"
    assert dp["provenance"]["stats"]["layout_median_displacement"] == 0
    assert dp["provenance"]["database"] == {"host": "db.example:5433", "name": "fs3"}, "no credentials"


def test_a_grown_conversation_is_the_only_one_re_pulled(db, tmp_path):
    _run(tmp_path)
    grown = db.ex.conversations["guid"].iloc[3]
    db.append_turn(grown)
    db.reset_calls()
    out = _run(tmp_path)
    assert db.calls["turns"] == [grown] and db.calls["centroids"] == [grown]
    turns = pd.read_parquet(paths.bundles_dir(tmp_path) / out["snapshot"] / "tables" / "turns.parquet")
    assert len(turns) == len(db.ex.turns), "cached rows + the re-pulled conversation"
    assert turns.groupby("guid").size()[grown] == 21


def test_late_summaries_or_vectors_re_pull_a_conversation_whose_head_did_not_move(db, tmp_path):
    _run(tmp_path)
    late = db.ex.conversations["guid"].iloc[5]
    db.late = {late}
    db.reset_calls()
    out = _run(tmp_path)
    assert not out["unchanged"] and db.calls["turns"] == [late]


def test_a_stop_set_change_recomputes_only_the_affected_centroids(db, tmp_path):
    _run(tmp_path)
    raw = db.ex.turns
    hit = raw[raw["class"] == "work"].iloc[0]
    db.shared = {hit["raw_hash"]}
    db.reset_calls()
    out = _run(tmp_path)
    assert not out["unchanged"]
    assert db.calls["turns"] == [] and db.calls["centroids"] == [hit["guid"]]
    turns = pd.read_parquet(paths.bundles_dir(tmp_path) / out["snapshot"] / "tables" / "turns.parquet")
    assert bool(turns.set_index("raw_hash").at[hit["raw_hash"], "stopset"])


def test_changed_extraction_code_discards_the_cache(db, tmp_path, monkeypatch):
    _run(tmp_path)
    monkeypatch.setattr(export, "EXTRACT_FINGERPRINT", "changed-sql")
    db.reset_calls()
    _run(tmp_path)
    assert set(db.calls["turns"]) == set(db.ex.conversations["guid"])
