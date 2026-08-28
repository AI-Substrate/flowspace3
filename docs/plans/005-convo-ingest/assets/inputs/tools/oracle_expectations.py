#!/usr/bin/env python3
"""Regenerate the committed expectations for the plan-005 conversation fixtures.

Usage:
    python3 oracle_expectations.py          # rewrite every expectations.json
    python3 oracle_expectations.py --check  # regenerate in memory, fail on drift

WHY THIS EXISTS (plan 005, tk-c105). ``reconvo.py`` is the pinned reference
oracle, but it cannot be pointed at a fixture directory as shipped: its store
roots are module-level ``$HOME`` constants, and its ``main()`` windows,
regex-selects and truncates to 400 characters. This driver imports the oracle
WITHOUT EDITING IT, rebinds those three constants at the fixture tree, and
calls the store readers directly — no window, no selectors, no truncation — so
what lands in an expectation file is the oracle's full-fidelity output.

Editing the pinned oracle is forbidden (prime ruling, 2026-08-28); the sha in
``../SHA256SUMS`` is verified on every run and a mismatch is a hard stop.

EVERY FILE CARRIES TWO KINDS OF CLAIM. Read ``claims`` before believing one.

``structural`` — every store, PM-derived, read off the committed bytes
    Record count, the histogram of the store's OWN record-type vocabulary, and
    an ordered per-record identity: the id a reader will report as
    ``RawRecord::ordinal`` (claude ``uuid``, omp record ``id``, ledger ``seq``,
    metrics-db ``rowid``), its parent, and a hash of its bytes. Nothing here
    interprets what a turn MEANS, so nothing here can be wrong in the way an
    interpretation can.

    The claim it supports is a SUBSEQUENCE claim, not equality: not every store
    record becomes a turn (claude's record-type allowlist drops `attachment`,
    `mode`, `file-history-delta` and friends; claude's per-block merge folds
    several records sharing a `message.id` into one turn), so a reader emits
    FEWER ordinals than are listed — but every ordinal it emits must be one of
    these, in this relative order, without repeats. That catches an invented
    record, a lost record, a reordering and a duplicate, which is a mechanical
    done-bar independent of any oracle.

``subset`` — omp, pij and metrics_db only, oracle-derived
    Every turn listed MUST appear in the Rust reader's output for the same
    fixture, in this order, with the same text. The reader is EXPECTED to emit
    more: the oracle drops record types fs3 must keep (``read_omp`` handles
    only ``type == "message"``, so omp's first-class ``compaction`` record,
    which plan-005 ac-0005 says is never dropped, is absent by construction).
    Extra records are not a failure; a missing one, a reordering, or different
    text is.

    The claude fixtures have NO ``subset`` section, because the pinned oracle
    has no claude-native reader at all — its ``READERS`` map covers metrics,
    omp and pij-ledger, and its ``dialect: claude`` is claude-via-git-ai-
    metrics-db, a different store. The obvious strengthening was checked and
    does not apply: metrics.sqlite3 mirrors the SAME claude session id, but
    over a disjoint window (mirror 2026-08-27T08:03:36..08:10:16, claude-native
    harvest 22:54:13..23:00:11), so a reconciliation would assert nothing. The
    independent semantic check for the claude reader is the tk-c305 first-light
    transcript against a live session.

Every file pins the sha256 of the fixture bytes it was derived from, so editing
a fixture without regenerating its expectations fails in testkit immediately
rather than in phase 2 as a mystery reader bug.
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
import sqlite3
import sys
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
INPUTS_DIR = TOOLS_DIR.parent
ORACLE = INPUTS_DIR / "reconvo.py"
SHA256SUMS = INPUTS_DIR / "SHA256SUMS"

REPO_ROOT = next(p for p in TOOLS_DIR.parents if (p / "crates").is_dir())
FIXTURES = REPO_ROOT / "crates" / "testkit" / "fixtures" / "conversations"

# Written by this driver, so never an input to its own fixture digest.
EXPECTATIONS_NAME = "expectations.json"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(65536), b""):
            digest.update(block)
    return digest.hexdigest()


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def load_oracle():
    """Import the pinned oracle after proving it is the pinned oracle."""
    expected = None
    for line in SHA256SUMS.read_text().splitlines():
        if line.strip().endswith("reconvo.py"):
            expected = line.split()[0]
    if expected is None:
        raise SystemExit(f"no reconvo.py row in {SHA256SUMS}")
    actual = sha256_file(ORACLE)
    if actual != expected:
        raise SystemExit(
            "the pinned oracle changed — expectations must not be regenerated "
            f"from a drifted oracle.\n  expected {expected}\n  actual   {actual}\n"
            "Restore reconvo.py, or get the new sha ruled by the plan's PM."
        )

    spec = importlib.util.spec_from_file_location("reconvo_pinned", ORACLE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    # Rebind the three $HOME store roots at the fixture tree. PIJ_LEDGER keeps
    # no ``{seat}`` placeholder on purpose: ``.format(seat=...)`` is then a
    # no-op and the fixture is read whatever seat name is passed.
    module.METRICS_DB = str(FIXTURES / "metrics_db" / "metrics.sqlite3")
    module.OMP_GLOB = str(FIXTURES / "omp" / "*_{sid}.jsonl")
    module.PIJ_LEDGER = str(FIXTURES / "pij" / "events.ndjson")
    return module, actual


# ------------------------------------------------------------------ structural
#
# One record shape for all four stores, so the testkit loader is one type:
#   n, type, ts, id (the reader's future RawRecord::ordinal), parent, sha256.


def record_row(n: int, record_type: str, ts, record_id, parent, sha: str) -> dict:
    return {
        "n": n,
        "type": record_type,
        "ts": None if ts is None else str(ts),
        "id": None if record_id is None else str(record_id),
        "parent": None if parent is None else str(parent),
        "record_sha256": sha,
    }


def jsonl_structural(path: Path, id_key: str, parent_key: str | None) -> dict:
    """Per-record identity for a jsonl store, in the store's own vocabulary."""
    by_type: dict[str, int] = {}
    records = []
    for ordinal, raw in enumerate(path.read_text().splitlines(), start=1):
        if not raw.strip():
            continue
        record = json.loads(raw)
        record_type = record.get("type") or "<untyped>"
        by_type[record_type] = by_type.get(record_type, 0) + 1
        records.append(
            record_row(
                ordinal,
                record_type,
                record.get("timestamp"),
                record.get(id_key),
                record.get(parent_key) if parent_key else None,
                sha256_text(raw),
            )
        )
    return {
        "file": str(path.relative_to(FIXTURES)),
        "record_count": len(records),
        "by_type": dict(sorted(by_type.items())),
        "records": records,
    }


def fixture_digest(root: Path) -> dict[str, str]:
    """sha256 of every committed fixture byte under ``root``, path-sorted."""
    files = sorted(
        p for p in root.rglob("*") if p.is_file() and p.name != EXPECTATIONS_NAME
    )
    return {str(p.relative_to(FIXTURES)): sha256_file(p) for p in files}


# ---------------------------------------------------------------------- oracle


def turn_rows(turns) -> tuple[list[dict], dict[str, int]]:
    """The oracle's output as a deterministic, diffable, reviewable list."""
    rows: list[dict] = []
    by_kind: dict[str, int] = {}
    for ordinal, turn in enumerate(turns, start=1):
        text = turn["text"]
        rows.append(
            {
                "n": ordinal,
                "kind": turn["kind"],
                "ts": turn["ts"],
                "text_len": len(text),
                "text_sha256": sha256_text(text),
                "head": " ".join(text.split())[:80],
            }
        )
        by_kind[turn["kind"]] = by_kind.get(turn["kind"], 0) + 1
    return rows, dict(sorted(by_kind.items()))


def session_row(
    key: str,
    files: list[str],
    structural: dict,
    turns: list[dict],
    by_kind: dict[str, int],
    extras: dict | None = None,
) -> dict:
    return {
        "key": key,
        "files": files,
        "record_count": structural["record_count"],
        "by_type": structural["by_type"],
        "oracle_turns": len(turns),
        "oracle_by_kind": by_kind,
        "extras": extras or {},
    }


def header(store: str, claims: list[str], oracle_sha: str, entrypoint: str | None) -> dict:
    return {
        "store": store,
        "claims": claims,
        # WHICH ORACLE TURNS CAN BE COMPARED BY TEXT AT ALL. The oracle yields
        # verbatim store text for these kinds and a RENDERING for the rest —
        # `compact_args(name, args)` for tool_call and report_card, a
        # "→ to: delivery state" line for a pij receipt. A Rust reader will
        # never reproduce a rendering, and demanding it would be demanding the
        # reader imitate a python helper. So: prose kinds are matched by text
        # hash (on the TRIMMED body, because the oracle strips), and every
        # other kind is held to its COUNT in `oracle_by_kind` and nothing more.
        "prose_kinds": ["assistant", "human", "pij_in"],
        "generated_by": "docs/plans/005-convo-ingest/assets/inputs/tools/oracle_expectations.py",
        "regenerate": "python3 docs/plans/005-convo-ingest/assets/inputs/tools/oracle_expectations.py",
        "oracle": {
            "script": "docs/plans/005-convo-ingest/assets/inputs/reconvo.py",
            "sha256": oracle_sha,
            "entrypoint": entrypoint,
        },
    }


# ------------------------------------------------------------------- per store


def build_claude(oracle_sha: str) -> dict:
    root = FIXTURES / "claude"
    doc = header("claude", ["structural"], oracle_sha, None)
    doc["grade_of_proof"] = (
        "STRUCTURAL ONLY, PM-derived, NOT oracle-derived — the pinned oracle has "
        "no claude-native reader. The mirror strengthening was checked and does "
        "not apply (same session id in metrics.sqlite3, disjoint window). This "
        "pins framing, resolution, ordering and block-merge arithmetic; the "
        "reader's SEMANTICS are checked by tk-c305 first light."
    )

    sessions = []
    structural = {}
    for main_jsonl in sorted(root.glob("*.jsonl")):
        session_id = main_jsonl.stem
        sidecar_dir = root / session_id / "subagents"
        spill_dir = root / session_id / "tool-results"

        main = jsonl_structural(main_jsonl, "uuid", "parentUuid")
        assistant_ids = []
        for raw in main_jsonl.read_text().splitlines():
            if not raw.strip():
                continue
            record = json.loads(raw)
            if record.get("type") == "assistant":
                message_id = (record.get("message") or {}).get("id")
                if message_id:
                    assistant_ids.append(message_id)

        sidecars = sorted(sidecar_dir.glob("*.jsonl")) if sidecar_dir.is_dir() else []
        sidecar_structural = [jsonl_structural(p, "uuid", "parentUuid") for p in sidecars]

        sessions.append(
            session_row(
                session_id,
                [main["file"]] + [s["file"] for s in sidecar_structural],
                main,
                [],
                {},
                {
                    # Recipe gotcha 1: claude writes ONE LINE PER CONTENT BLOCK,
                    # so several assistant records share a message id and the
                    # reader must merge them into one turn. The gap between
                    # these two numbers is exactly how much merging is owed.
                    "assistant_records": len(assistant_ids),
                    "distinct_assistant_message_ids": len(set(assistant_ids)),
                    # Recipe gotcha 6: a sidecar is a CHILD conversation, and
                    # one can appear mid-session, so resolve re-globs every poll.
                    "sidecar_record_counts": {
                        s["file"]: s["record_count"] for s in sidecar_structural
                    },
                    "sidecar_meta": sorted(
                        str(p.relative_to(FIXTURES))
                        for p in sidecar_dir.glob("*.meta.json")
                    )
                    if sidecar_dir.is_dir()
                    else [],
                    # Recipe gotcha 9: an oversized tool result is spilled to a
                    # sidecar FILE and the record only references it.
                    "spilled_tool_results": sorted(
                        str(p.relative_to(FIXTURES)) for p in spill_dir.glob("*")
                    )
                    if spill_dir.is_dir()
                    else [],
                },
            )
        )
        structural[session_id] = {"main": main, "sidecars": sidecar_structural}

    doc["sessions"] = sessions
    doc["structural"] = structural
    doc["turns"] = {}
    doc["fixture_sha256"] = fixture_digest(root)
    return doc


def build_omp(oracle, oracle_sha: str) -> dict:
    session_id = "01a03d08-7c56-7000-ac9b-95c4b3ef34d7"
    root = FIXTURES / "omp"
    main_jsonl = next(root.glob(f"*_{session_id}.jsonl"))
    main = jsonl_structural(main_jsonl, "id", "parentId")
    turns, by_kind = turn_rows(oracle.read_omp(session_id))

    doc = header("omp", ["structural", "subset"], oracle_sha, "read_omp")
    doc["grade_of_proof"] = (
        "read_omp handles only type=='message', so the title slot, the session "
        "header, model_change, thinking_level_change and the first-class "
        "compaction record are absent from the subset section BY CONSTRUCTION. "
        "The Rust reader must still emit the compaction record (ac-0005); the "
        "structural section is what holds it to that."
    )
    doc["sessions"] = [
        session_row(
            session_id,
            [main["file"]],
            main,
            turns,
            by_kind,
            {
                "spilled_tool_results": sorted(
                    str(p.relative_to(FIXTURES))
                    for p in (root / main_jsonl.stem).glob("*")
                )
                if (root / main_jsonl.stem).is_dir()
                else [],
            },
        )
    ]
    doc["structural"] = {session_id: {"main": main, "sidecars": []}}
    doc["turns"] = {session_id: turns}
    doc["fixture_sha256"] = fixture_digest(root)
    return doc


def build_pij(oracle, oracle_sha: str) -> dict:
    seat = "pij-linguistic-narwhal"
    root = FIXTURES / "pij"
    main = jsonl_structural(root / "events.ndjson", "seq", None)
    turns, by_kind = turn_rows(oracle.read_pij_ledger(seat))

    doc = header("pij", ["structural", "subset"], oracle_sha, "read_pij_ledger")
    doc["grade_of_proof"] = (
        "THE SUBSET SECTION IS WEAK HERE AND THAT IS MEASURED, NOT ASSUMED: "
        f"the oracle yields only {len(turns)} turns from {main['record_count']} "
        "records, because read_pij_ledger emits solely 'receipt' and 'message' "
        "events, keeps only role user/assistant, and requires a 'text' content "
        "block — and this harvested window is tool-heavy: of its 14 text blocks "
        "13 sit on role 'toolResult', and every assistant record is thinking "
        "plus toolCall with no prose. The structural section, not the subset "
        "section, is this store's real done-bar; the seq cursor and the two "
        "receipt records are what the fixture was harvested to cover."
    )
    doc["sessions"] = [session_row(seat, [main["file"]], main, turns, by_kind)]
    doc["structural"] = {seat: {"main": main, "sidecars": []}}
    doc["turns"] = {seat: turns}
    doc["fixture_sha256"] = fixture_digest(root)
    return doc


def build_metrics_db(oracle, oracle_sha: str) -> dict:
    root = FIXTURES / "metrics_db"
    db_path = root / "metrics.sqlite3"
    db = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    dialects = {
        session_id: ("copilot" if tool == "github-copilot-cli" else "claude", tool)
        for session_id, tool in db.execute(
            "select distinct external_session_id, tool from metrics where event_kind=5"
        )
    }
    rows = list(
        db.execute(
            "select id, event_ts, external_session_id, external_event_id, "
            "external_parent_event_id, event_json from metrics "
            "where event_kind=5 order by external_session_id, event_ts, id"
        )
    )
    db.close()

    # The rowid cursor's ordering is `rowid`, so the structural rows are keyed
    # and ordered by it — the same key a reader reports as RawRecord::ordinal.
    per_session: dict[str, list] = {}
    for row_id, event_ts, session_id, event_id, parent_id, event_json in rows:
        try:
            record = json.loads(event_json)["v"]["0"]
        except Exception:
            record = {}
        # The event's own name: claude mirrors carry `type`, copilot `name`.
        record_type = record.get("type") or record.get("name") or "<unnamed>"
        per_session.setdefault(session_id, []).append(
            (row_id, event_ts, record_type, event_id, parent_id, event_json)
        )

    doc = header("metrics_db", ["structural", "subset"], oracle_sha, "read_metrics")
    doc["grade_of_proof"] = (
        "The oracle's own de-duplication is part of the subset expectation — "
        "claude-dialect rows are deduped on the first 80 chars of user/assistant "
        "text and on tool_use id, which is exactly the dedup behaviour tk-c105 "
        "pins — and its HUMAN_NOISE prefix filter means a Rust reader emitting "
        "those turns is ahead of it, not wrong. The rowid cursor is pinned by "
        "the structural section. Repo scoping is NOT expressed here: it is "
        "proven by API shape in u1d, over the foreign-repo negative rows this "
        "fixture deliberately carries."
    )

    doc["sessions"] = []
    doc["structural"] = {}
    doc["turns"] = {}
    for session_id in sorted(per_session):
        dialect, tool = dialects[session_id]
        by_type: dict[str, int] = {}
        records = []
        for n, (row_id, event_ts, record_type, event_id, parent_id, event_json) in enumerate(
            sorted(per_session[session_id]), start=1
        ):
            by_type[record_type] = by_type.get(record_type, 0) + 1
            records.append(
                record_row(
                    n,
                    record_type,
                    event_ts,
                    row_id,
                    parent_id,
                    sha256_text(event_json),
                )
            )
        main = {
            "file": str(db_path.relative_to(FIXTURES)),
            "record_count": len(records),
            "by_type": dict(sorted(by_type.items())),
            "records": records,
        }
        turns, by_kind = turn_rows(oracle.read_metrics(session_id, dialect))
        doc["sessions"].append(
            session_row(
                session_id,
                [main["file"]],
                main,
                turns,
                by_kind,
                {"tool": tool, "dialect": dialect},
            )
        )
        doc["structural"][session_id] = {"main": main, "sidecars": []}
        doc["turns"][session_id] = turns

    doc["fixture_sha256"] = fixture_digest(root)
    return doc


# ------------------------------------------------------------------------ main


def main(argv: list[str]) -> int:
    check_only = "--check" in argv[1:]
    oracle, oracle_sha = load_oracle()

    built = {
        FIXTURES / "claude" / EXPECTATIONS_NAME: build_claude(oracle_sha),
        FIXTURES / "omp" / EXPECTATIONS_NAME: build_omp(oracle, oracle_sha),
        FIXTURES / "pij" / EXPECTATIONS_NAME: build_pij(oracle, oracle_sha),
        FIXTURES / "metrics_db" / EXPECTATIONS_NAME: build_metrics_db(oracle, oracle_sha),
    }

    drifted = []
    for path, doc in built.items():
        rendered = json.dumps(doc, indent=1, ensure_ascii=False) + "\n"
        if check_only:
            current = path.read_text() if path.exists() else ""
            if current != rendered:
                drifted.append(str(path.relative_to(REPO_ROOT)))
            continue
        path.write_text(rendered)
        sessions = doc["sessions"]
        records = sum(s["record_count"] for s in sessions)
        turns = sum(s["oracle_turns"] for s in sessions)
        print(
            f"{path.relative_to(REPO_ROOT)}: {len(sessions)} session(s), "
            f"{records} records structural, {turns} turns from the oracle"
        )

    if check_only:
        if drifted:
            print("expectations are stale — re-run without --check:")
            for path in drifted:
                print(f"  {path}")
            return 1
        print("expectations match the committed fixtures")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
