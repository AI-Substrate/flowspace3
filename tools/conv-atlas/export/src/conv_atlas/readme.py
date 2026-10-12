"""Render a snapshot's README.md: the data dictionary an agent reads first.

Owned by Scribe (labels + docs). Input is the datapackage descriptor and the
tables; output is Markdown. Deterministic: same inputs, same bytes.

`EXAMPLE_QUERIES` and `register_sql` are rendered into the README verbatim and
executed against the fixture snapshot by tests/test_readme.py, so every query an
agent copies out of a README is known to run.
"""

from __future__ import annotations

import json
import re

import pandas as pd

# (title, why you would run it, DuckDB SQL over the table views). Keep 6-12.
EXAMPLE_QUERIES: list[tuple[str, str, str]] = [
    (
        "Biggest conversations",
        "Orient: what the corpus is mostly made of.",
        """select label, kind, harness, repo_key, turns, n_work, started_at::date as started
from conversations
order by turns desc
limit 10""",
    ),
    (
        "Work per week by harness",
        "The growth curve; split by harness because omp and claude transcripts differ in texture.",
        """select date_trunc('week', t.turn_at) as week, c.harness, count(*) as work_turns
from turns t join conversations c using (guid)
where t.class = 'work'
group by all
order by week, c.harness""",
    ),
    (
        "Communities and their members",
        "What each cluster is about, and which repos and harnesses dominate it.",
        """select m.community_id, m.label, m.size,
       mode(c.repo_key) as top_repo,
       round(avg((c.harness = 'omp')::int), 2) as omp_share
from communities m join conversations c using (community_id)
group by all
order by m.size desc""",
    ),
    (
        "Cross-repo bridges",
        "Project pairs that share work, with their strongest kNN bridge. knn_edges orders a/b by guid, "
        "not repo, so fold the repo pair with least()/greatest() or each pair is counted twice. Raw counts "
        "scale with repo size: two big repos bridge often by chance alone, so rank by the next query's lift.",
        """select least(a.repo_key, b.repo_key) as repo_1, greatest(a.repo_key, b.repo_key) as repo_2,
       count(*) as bridges,
       arg_max(a.label || '  <->  ' || b.label, e.cos_centred) as strongest,
       round(max(e.cos_centred), 3) as cos_centred
from knn_edges e
join conversations a on a.guid = e.a_guid
join conversations b on b.guid = e.b_guid
where e.cross_repo
group by all
order by bridges desc
limit 10""",
    ),
    (
        "Cross-repo bridges, normalised for repo size",
        "Bridges against what random wiring would give two repos of that size: `lift` 1 = chance, above 1 = "
        "the projects share more work than their sizes explain. Sizes count embedded conversations (only "
        "those have edges); the floor of 10 per repo keeps small repos from topping the list on a handful of "
        "edges. Raise it for steadier ranks.",
        """with size as (
  select repo_key, count(*) as convs from conversations where embedded and repo_key is not null group by all
), total as (
  select (select count(*) from knn_edges) as edges, (select count(*) from conversations where embedded) as convs
), pair as (
  select least(a.repo_key, b.repo_key) as repo_1, greatest(a.repo_key, b.repo_key) as repo_2, count(*) as bridges
  from knn_edges e
  join conversations a on a.guid = e.a_guid
  join conversations b on b.guid = e.b_guid
  where e.cross_repo
  group by all
)
select p.repo_1, p.repo_2, p.bridges, s1.convs as convs_1, s2.convs as convs_2,
       round(p.bridges / (2.0 * t.edges * s1.convs * s2.convs / (t.convs * t.convs)), 2) as lift
from pair p
join size s1 on s1.repo_key = p.repo_1
join size s2 on s2.repo_key = p.repo_2
cross join total t
where s1.convs >= 10 and s2.convs >= 10
order by lift desc
limit 10""",
    ),
    (
        "Subagent fan-out",
        "Which sessions delegate most, and to what.",
        """select p.label as parent, count(*) as subagents, list(c.label order by c.started_at)[1:3] as first_children
from subagent_edges s
join conversations p on p.guid = s.parent_guid
join conversations c on c.guid = s.child_guid
group by p.guid, p.label
order by subagents desc
limit 10""",
    ),
    (
        "Autonomy index",
        "Human turns per 100 agent turns: low means the agent ran mostly unattended.",
        """select label, harness, n_agent, n_human, n_peer,
       round(100.0 * n_human / n_agent, 1) as human_per_100_agent
from conversations
where n_agent >= 50
order by human_per_100_agent
limit 10""",
    ),
    (
        "Who messages whom",
        "The pij seat network: peer messages by sender and receiver seat. Unknown seats are grouped as "
        "`(unknown)`, not dropped; a `to_seat is not null` filter would silently lose that traffic (the "
        "caveats give this snapshot's seat coverage). `to_guid` is always set, if you need the receiving "
        "conversation instead.",
        """select coalesce(m.from_seat, '(unknown)') as from_seat, coalesce(m.to_seat, '(unknown)') as to_seat,
       count(*) as messages, min(m.sent_at) as first, max(m.sent_at) as last
from messages m
group by all
order by messages desc
limit 10""",
    ),
    (
        "Tags that travel together",
        "Co-occurring topics across conversations, with each tag's own reach.",
        """select p.tag_a, p.tag_b, p.n as together, a.conversations as a_alone, b.conversations as b_alone
from tag_cooc p
join tags a on a.tag = p.tag_a
join tags b on b.tag = p.tag_b
order by p.n desc
limit 10""",
    ),
    (
        "Sharpest topic shifts in long sessions, from what to what",
        "Long sessions are the top `params.sessions` by work turns (At a glance gives the cut-off). A shift row "
        "names only the topic it starts; the topic it left is the previous shift's, or the session's opening "
        "stretch for the first shift (`from_topic` null: read turns `from_turn` to `turn_no`).",
        """select c.label as session, s.from_turn, s.turn_no, round(s.score, 2) as score,
       s.from_topic, s.label as to_topic, 'conv:' || s.guid || '#t' || s.turn_no as address
from (
  select *, lag(label) over w as from_topic, coalesce(lag(turn_no) over w, 0) as from_turn
  from session_shifts
  window w as (partition by guid order by turn_no)
) s join conversations c using (guid)
order by s.score desc
limit 10""",
    ),
    (
        "From a tag to turns you can read",
        "Find work turns on a topic, then `flowspace3 get <address>` for the raw text.",
        """select t.address, t.turn_at, t.summary
from turns t
where t.class = 'work' and list_contains(t.tags, (select tag from tags order by n desc limit 1))
order by t.turn_at desc
limit 5""",
    ),
]

VECTOR_SNIPPET = """import numpy as np, pandas as pd
cent = np.load("vectors/conversation_centroids.npy")          # [N, D] float32, NOT centred
guids = pd.read_parquet("vectors/conversation_centroids.index.parquet")["guid"]
mean = np.load("vectors/embedding_mean.npy")
x = cent - mean
x /= np.linalg.norm(x, axis=1, keepdims=True)                 # centred, unit length
sims = x @ x.T                                                # == knn_edges.cos_centred"""

# Caveats every reader needs, whatever the provenance says. Data-driven ones are added in render().
STATIC_CAVEATS = [
    "**Mean-centre before comparing vectors.** Raw cosines between conversation centroids are all high "
    "(embedding anisotropy). Use `knn_edges.cos_centred`, or subtract `vectors/embedding_mean.npy` and "
    "re-normalise. `cos_raw` is kept only for comparison.",
    "**Harness confounds similarity.** omp and claude transcripts have different textures, so neighbours "
    "share a harness far more often than chance. Compare within one harness, or control for it, before "
    "claiming two conversations are about the same thing.",
    "**Only work turns describe content.** Centroids, tags, labels and novelty use `class = 'work'` turns "
    "minus the boilerplate stop-set (`turns.stopset`). Tool calls and results are counted, not embedded.",
    "**Not every work turn is summarised.** Unsummarised turns have `summary` null, empty `tags` and a "
    "`head` (first 280 raw characters) instead. Coverage figures are under Provenance.",
    "**Labels and blurbs are generated**, from tags and summaries (`provenance.labels`). Stored titles "
    "are uuids. Tags are normalised (case, plurals, synonyms); the map version is recorded.",
    "**Identity is partial.** `repo_key` is null for conversations with no repository anchor. `seat` is "
    "recovered from metadata or transcript text; `seats.identity_source` says how.",
    "**Raw bodies are not here.** Every turn row carries its fs3 address; `flowspace3 get` reads the text.",
]

# Joins that are not declared foreign keys but are how the tables are meant to meet.
EXTRA_JOINS = [
    ("`turns.tags[]`, `conversations.tags[]`", "`tags.tag`", "unnest or `list_contains`"),
    ("`messages.to_seat`", "`seats.seat`", "receiver seat, when known"),
    ("`messages.from_guid`", "`conversations.guid`", "sender's conversation, when resolvable"),
    ("`conversation_centroids.index.parquet` row", "`conversations.guid`", "row i of the .npy is that guid"),
    ("`turn_vectors.index.parquet` row", "`turns.address`", "row i of the .npy is that turn"),
    ("`session_shifts (guid, turn_no)`", "`turns (guid, turn_no)`", "the turn where the topic changed"),
]


def register_sql(descriptor: dict) -> str:
    """DuckDB statements that expose every table under its own name (run from the snapshot dir)."""
    return "\n".join(
        f"create or replace view {r['name']} as select * from '{r['path']}';" for r in descriptor["resources"]
    )


_SHARE_KEY = re.compile(r"coverage|share|baseline|fraction|_rel$")


def _fmt(value: object, key: str = "") -> str:
    """Numbers for people: fractions as percentages only when the key says it is a share."""
    if isinstance(value, float):
        return f"{value:.1%}" if _SHARE_KEY.search(key) else f"{value:,.4g}"
    if isinstance(value, int) and not isinstance(value, bool):
        return f"{value:,}"
    if isinstance(value, dict | list):
        return "`" + json.dumps(value, sort_keys=True, separators=(", ", ": "), default=str) + "`"
    return str(value)


def _provenance(prov: dict) -> list[str]:
    lines = ["## Provenance", ""]
    for key in sorted(prov):
        value = prov[key]
        if key in ("known_defects", "stats", "stopset"):
            continue
        if isinstance(value, dict) and value and all(not isinstance(v, dict | list) for v in value.values()):
            inner = ", ".join(f"{k}={_fmt(v, k)}" for k, v in sorted(value.items()))
            lines.append(f"- **{key}**: {inner}")
        else:
            lines.append(f"- **{key}**: {_fmt(value)}")
    stats = prov.get("stats") or {}
    if stats:
        lines += ["", "### Coverage and corpus stats", "", "| stat | value |", "|---|---|"]
        lines += [f"| `{k}` | {_fmt(v, k)} |" for k, v in sorted(stats.items())]
    stopset = prov.get("stopset") or {}
    if stopset:
        lines += ["", "### Boilerplate stop-set", ""]
        rules = stopset.get("rules") or {}
        if isinstance(rules, dict) and rules:
            lines += ["| rule | turns excluded |", "|---|---|"]
            lines += [f"| `{k}` | {_fmt(v)} |" for k, v in sorted(rules.items())]
            lines.append("")
        lines += [f"- **{k}**: {_fmt(v)}" for k, v in sorted(stopset.items()) if k != "rules"]
    return lines


def _caveats(prov: dict, tables: dict[str, pd.DataFrame]) -> list[str]:
    lines = ["## Caveats (read before drawing conclusions)", ""]
    lines += [f"- {c}" for c in STATIC_CAVEATS]
    m = tables.get("messages")
    if m is not None and len(m):
        n = len(m)
        lines.append(
            f"- **Seats on messages are partial.** Of {n:,} peer messages, {_seat_share(m['to_seat'])} name the "
            f"receiver seat and {_seat_share(m['from_seat'])} the sender seat. `to_guid` (the receiving "
            "conversation) is always set: attribute messages through it, and count unknown seats rather than "
            "filtering them out."
        )
    stats = prov.get("stats") or {}
    share = stats.get("harness_confound_same_harness_edge_share")
    if isinstance(share, int | float):
        base = stats.get("harness_confound_random_baseline")
        tail = f" against a random baseline of {_fmt(base, 'baseline')}" if isinstance(base, int | float) else ""
        lines.append(f"- In this snapshot {_fmt(share, 'share')} of kNN edges join the same harness{tail}.")
    layout = prov.get("layout") or {}
    if isinstance(layout, dict) and str(layout.get("method", "")).startswith("kept"):
        lines.append(
            f"- **The map layout was carried over**, not recomputed: {_fmt(layout.get('kept_points', 0))} points keep "
            f"their earlier coordinates and {_fmt(layout.get('placed_points', 0))} new or changed ones were placed "
            "among their nearest kept neighbours. Positions compare across snapshots; distances on the map are "
            "approximate, so measure similarity with `knn_edges` or the vectors."
        )
    models = prov.get("summariser_models")
    if isinstance(models, dict) and len(models) > 1:
        shown = ", ".join(f"`{k}` ({_fmt(v)})" for k, v in sorted(models.items()))
        lines.append(f"- Summaries come from {len(models)} summariser generations: {shown}. They share one embedder.")
    defects = prov.get("known_defects") or []
    if defects:
        lines += ["", "### Known defects", ""] + [f"- {d}" for d in defects]
        dl029 = stats.get("dl029_tool_call_turns")
        if isinstance(dl029, int):
            lines.append(f"- DL-029 affects {_fmt(dl029)} tool_call turns in this snapshot.")
    return lines


def _seat_share(col: pd.Series) -> str:
    known = int(col.notna().sum())
    return f"{known:,} ({known / len(col):.0%})"


def _at_a_glance(tables: dict[str, pd.DataFrame], prov: dict) -> list[str]:
    c = tables.get("conversations")
    params = prov.get("params") or {}
    lines = ["## At a glance", ""]
    if c is not None and len(c):
        lines.append(
            f"- {len(c):,} conversations ({int((c['kind'] == 'subagent').sum()):,} subagents), "
            f"{int(c['turns'].sum()):,} turns, {int(c['n_work'].sum()):,} work turns"
        )
        floor = params.get("min_work_vectors")
        why = f"at least {floor} embedded work turns" if isinstance(floor, int) else "enough embedded work turns"
        lines.append(
            f"- {pd.Timestamp(c['started_at'].min()):%Y-%m-%d} to {pd.Timestamp(c['last_turn_at'].max()):%Y-%m-%d}"
            f" (UTC); {int(c['embedded'].sum()):,} conversations embedded, meaning they have {why} for a "
            "centroid (only these have map positions, kNN edges and communities). The `coverage_*` stats under "
            "Provenance are shares of turns, not conversations."
        )
        counts = c["harness"].value_counts(dropna=False)
        shown = ", ".join(f"{'no harness' if pd.isna(k) else k} {v:,}" for k, v in counts.items())
        lines.append(f"- harnesses: {shown}")
        detailed = set()
        for name in ("session_novelty", "session_shifts"):
            if name in tables:
                detailed |= set(tables[name]["guid"])
        if detailed:
            cut = int(c.loc[c["guid"].isin(detailed), "n_work"].min())
            lines.append(
                f"- {len(detailed):,} long sessions get topic shifts and novelty: the top "
                f"{params.get('sessions', len(detailed))} conversations by work turns (here, {cut:,} or more)"
            )
    if "communities" in tables:
        lines.append(f"- {len(tables['communities']):,} communities, {len(tables.get('knn_edges', [])):,} kNN edges")
    return lines


def _snapshot_id(descriptor: dict) -> str:
    return descriptor.get("provenance", {}).get("snapshot_id") or descriptor["name"].removeprefix("conv-atlas-").upper()


def insight_example(descriptor: dict, tables: dict[str, pd.DataFrame]) -> dict:
    """A minimal insight that `conv-atlas insight add` accepts for this snapshot, anchored on a real conversation."""
    snapshot_id = _snapshot_id(descriptor)
    c = tables.get("conversations")
    if c is not None and len(c):
        refs: dict = {"guids": [str(c.sort_values(["turns", "guid"], ascending=[False, True])["guid"].iloc[0])]}
    else:
        refs = {"tags": ["testing"]}
    return {
        "id": "example-biggest-conversation",
        "title": "What the biggest conversation was about",
        "body_md": "What you found, in Markdown. Name the query that shows it so a reader can rerun it.",
        "author": "your pij seat, harness/session, or name",
        "snapshot_id": snapshot_id,
        "refs": refs,
    }


def _insight_section(descriptor: dict, tables: dict[str, pd.DataFrame]) -> list[str]:
    example = json.dumps(insight_example(descriptor, tables), indent=2, ensure_ascii=False)
    return [
        "## Writing an insight",
        "",
        "An insight is a finding you file back for people and agents; the open app lists it and jumps to "
        "its refs. Save a JSON file like this one, then run `conv-atlas insight add finding.json` "
        "(or pipe it to `conv-atlas insight add -`). It is validated, and append-only: an id is used once.",
        "",
        "```json",
        example,
        "```",
        "",
        "- Required: `id` (lowercase letters, digits and `-`, 3-80 chars), `title` (3-140 chars), `body_md`, "
        "`author`, `refs`.",
        "- Filled in if you leave them out: `kind` (`finding`; or `question`, `anomaly`, `story`), "
        "`created_ms` (now), `snapshot_id` (the latest snapshot; set it to the one you queried).",
        "- `refs` needs at least one anchor, and every list in it at least one item: `guids` "
        "(`conversations.guid`), `addresses` (`conv:<guid>` or `conv:<guid>#t<turn_no>`), `seats`, `repos` "
        '(`repo_key`), `tags`, `time_range` (`{"start_ms": …, "end_ms": …}`, epoch milliseconds). Use keys '
        "from this snapshot: they are stable across re-exports, so the app can still place the insight later.",
        "",
    ]


def render(descriptor: dict, tables: dict[str, pd.DataFrame]) -> str:
    prov = descriptor.get("provenance", {})
    snapshot_id = _snapshot_id(descriptor)
    q = EXAMPLE_QUERIES
    lines = [
        f"# {descriptor['title']}",
        "",
        "Analysis layer of a conv-atlas snapshot: flowspace3's conversation corpus as Parquet tables and "
        "embedding vectors, for agents and data science. Nothing here contains raw conversation bodies.",
        "",
        "## Start here",
        "",
        "`conv-atlas` is not on PATH. Run it from a flowspace3 checkout (`<repo>` below) with uv:",
        "",
        "```bash",
        "uv run --project <repo>/tools/conv-atlas/export conv-atlas <command> [--json]",
        "```",
        "",
        f"`describe`, `sql` and `validate` read the latest snapshot; add `--snapshot {snapshot_id}` to read this one.",
        "",
        "1. `conv-atlas describe` prints this dictionary as JSON; `conv-atlas describe <table>` one table.",
        '2. `conv-atlas sql "<duckdb sql>"` runs a query with every table registered as a view. '
        "Plain DuckDB works too: run the block below from this directory, then any example query.",
        "3. Rows carry fs3 addresses. `flowspace3 get <address>` reads the source (see *From a row to the source*).",
        "4. Found something? File it with `conv-atlas insight add <file.json>` (see *Writing an insight*); "
        "the open app shows it.",
        "",
        "```sql",
        register_sql(descriptor),
        "```",
        "",
    ]
    lines += _at_a_glance(tables, prov)
    lines += [""] + _caveats(prov, tables) + [""] + _provenance(prov)
    lines += [
        "",
        "## Join keys",
        "",
        "Every table is keyed by stable ids, never array positions: `guid` (a conversation), `address` "
        "(`conv:<guid>` or `conv:<guid>#t<turn_no>`), `community_id`, `seat`, `tag`.",
        "",
        "| from | to | note |",
        "|---|---|---|",
    ]
    for res in descriptor["resources"]:
        for fk in res["schema"].get("foreignKeys", []):
            src = ", ".join(f"{res['name']}.{f}" for f in fk["fields"])
            ref = fk["reference"]
            dst = ", ".join(f"{ref['resource'] or res['name']}.{f}" for f in ref["fields"])
            lines.append(f"| `{src}` | `{dst}` | declared foreign key |")
    lines += [f"| {a} | {b} | {n} |" for a, b, n in EXTRA_JOINS]
    lines += [
        "",
        "## From a row to the source",
        "",
        "```bash",
        "flowspace3 tree conv:<guid>                               # outline every turn of a conversation",
        "flowspace3 get conv:<guid>                                # header plus the opening turns",
        "flowspace3 get 'conv:<guid>#t<turn_no>' --before 0 --after 0   # exactly one turn",
        "flowspace3 get 'conv:<guid>#t<turn_no>'                  # that turn with 10 before, 20 after",
        "```",
        "",
        "`turns.address` is already in that form; for conversations use `conversations.address` with `tree`. "
        "Quote addresses in a shell: `#` starts a comment. Add `--json` for the machine envelope.",
        "",
        f"## Example queries ({len(q)})",
        "",
        "Each one runs against this snapshot as-is (they are executed in conv-atlas's tests).",
        "",
    ]
    for i, (title, why, sql) in enumerate(q, 1):
        lines += [f"### {i}. {title}", "", why, "", "```sql", sql + ";", "```", ""]
    lines += _insight_section(descriptor, tables)
    lines += ["## Vectors", "", "```python", VECTOR_SNIPPET, "```", ""]
    if descriptor.get("vectors"):
        lines += [f"- `vectors/{k}`: {v}" for k, v in descriptor["vectors"].items()]
        lines.append("")
    lines += ["## Tables", ""]
    for res in descriptor["resources"]:
        rows = len(tables[res["name"]])
        lines += [f"### `{res['name']}` ({rows:,} rows)", "", res["description"], ""]
        lines += ["| field | type | description |", "|---|---|---|"]
        for f in res["schema"]["fields"]:
            req = "" if f.get("constraints", {}).get("required") else " (nullable)"
            lines.append(f"| `{f['name']}` | {f['type']}{req} | {f['description']} |")
        lines += ["", f"Primary key: `{', '.join(res['schema']['primaryKey'])}`", ""]
    return "\n".join(lines).rstrip("\n") + "\n"
