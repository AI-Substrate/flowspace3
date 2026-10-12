"""Read-only extraction from the flowspace3 Postgres store.

Every session is `READ ONLY` with a `statement_timeout`; every query is bounded
to a batch of conversations. Raw bodies never leave Postgres: byte counts, the
280-char head of unsummarised work turns, boilerplate (stop-set) rules and the
pij seat / message envelopes are all computed server-side. Peer messages and
send calls also ship their first 1,200 chars, reduced here to a 40-char match
key (`message_key`) and dropped before anything is cached.
"""

from __future__ import annotations

import os
import re
from collections.abc import Iterator, Sequence
from contextlib import contextmanager
from urllib.parse import urlsplit

import numpy as np
import pandas as pd
import psycopg

DEFAULT_DSN = "postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3"
EMBEDDER = "text-embedding-3-small-no-rate@1024"
# newest first: when a turn has summaries from both generations, the first wins
SUMMARISERS = ("gpt-6-luna@1", "gpt-5.6-luna@1")
STATEMENT_TIMEOUT_MS = 120_000
SHARED_HASH_MIN_CONVERSATIONS = 20
STOP_RULES = ("empty", "local_command_caveat", "compact", "task_notification", "shared_hash")

# Server-side SQL fragments. `el.raw_text` is the turn body as indexed.
_STOP_RULE = """
  case when el.turn_class <> 'work' then null
       when btrim(el.raw_text) = '' then 'empty'
       when ltrim(el.raw_text) like '<local-command-caveat>%%' then 'local_command_caveat'
       when btrim(el.raw_text) = '/compact' or el.raw_text like '%%<command-name>/compact</command-name>%%'
         then 'compact'
       when ltrim(el.raw_text) like '<task-notification>%%' then 'task_notification'
  end"""
_PREAMBLE_SEAT = r"""case when t.turn_no <= 5
  then substring(el.raw_text from 'You are pij session (pij-[a-z]+-[a-z]+)') end"""
_PEER_FROM = r"""case when t.source = 'peer'
  then substring(el.raw_text from '\[pij(?:-rs)? from (pij-[a-z0-9-]+(?:@[A-Za-z0-9.-]+)?)\]') end"""
_SEND_TO = r"""case when el.turn_class = 'tool_call' and el.raw_text like '%%pij%%send%%' then coalesce(
    case when el.raw_text like '[tool-call pij_send]%%'
         then substring(el.raw_text from '\\?"to\\?"\s*:\s*\\?"(pij-[a-z0-9-]+)') end,
    substring(el.raw_text from 'pij(?:-rs)? send(?:\s+--?[a-z-]+)*\s+[''"]?(pij-[a-z0-9-]+)')) end"""
_MESSAGE_TEXT = """case when t.source = 'peer' or (el.turn_class = 'tool_call' and el.raw_text like '%%pij%%send%%')
  then left(el.raw_text, 1200) end"""

# A peer turn and the send call that produced it share their message body; these
# find where that body starts on each side (peer envelope, pij_send JSON, `pij send <seat>`).
_ENVELOPE = re.compile(r"\[pij(?:-rs)? from [^\]]+\]\s*")
_SEND_JSON = re.compile(r'\\?"message\\?"\s*:\s*\\?"')
_SEND_CLI = re.compile(r"""pij(?:-rs)? send(?:\s+--?[a-z-]+)*\s+\\?['"]?pij-[a-z0-9-]+\\?['"]?""")
# quoting, flags (`--body-file -`) and heredoc openers between the target seat and the body
_BODY_OPENER = re.compile(
    r"""^(?:\s|\\[ntr]|\\?["']|--?[a-z-]+(?=\s)|-(?=\s)|\$\(\s*cat|<<-?\s*\\?['"]?\w+\\?['"]?)+"""
)
MESSAGE_KEY_CHARS = 40
MESSAGE_KEY_MIN_CHARS = 12  # shorter bodies ("ack", "/compact") collide across unrelated messages


def message_key(text: str | None, send: bool) -> str | None:
    """The first 40 letters/digits of a message body, lower-cased, or None when there is no usable body.

    `send=False` reads a delivered peer turn, `send=True` the tool call that sent it; both sides of
    one message produce the same key whatever the quoting or JSON escaping."""
    if not text:
        return None
    body = None
    if send:
        m = _SEND_JSON.search(text) if text.startswith("[tool-call pij_send]") else None
        if m:
            body = text[m.end() :]
        elif m := _SEND_CLI.search(text):
            body = _BODY_OPENER.sub("", text[m.end() :])
    elif m := _ENVELOPE.search(text):
        body = text[m.end() :]
    if body is None:
        return None
    key = re.sub(r"[^a-z0-9]+", "", re.sub(r"\\[ntr]", " ", body.lower()))[:MESSAGE_KEY_CHARS]
    return key if len(key) >= MESSAGE_KEY_MIN_CHARS else None


_WORK_VEC = "coalesce(es.vector, er.vector)"
_VEC_JOINS = f"""
  left join embeddings_1024 es on es.source_hash = s.text_hash and es.source_kind = 'smart'
                              and es.chunk_no = 0 and es.model_key = '{EMBEDDER}'
  left join embeddings_1024 er on er.source_hash = el.raw_hash and er.source_kind = 'raw'
                              and er.chunk_no = 0 and er.model_key = '{EMBEDDER}'"""
_TURN_JOIN = """
  from turns t
  join elements el on el.kind = 'turn' and el.blob_sha = t.blob_sha
                  and el.address = 'conv:' || t.conversation_id || '#t' || t.turn_no
  left join lateral (
    select sc.text, sc.tags, sc.model_key, sc.text_hash
    from smart_content sc
    where sc.raw_hash = el.raw_hash and sc.model_key = any(%(summarisers)s)
    order by array_position(%(summarisers)s, sc.model_key)
    limit 1
  ) s on true"""


def dsn() -> str:
    return os.environ.get("CONV_ATLAS_DSN", DEFAULT_DSN)


def db_identity(url: str) -> dict:
    """Host and database name only: credentials never reach provenance."""
    u = urlsplit(url)
    return {"host": f"{u.hostname}:{u.port}" if u.port else u.hostname, "name": u.path.lstrip("/")}


@contextmanager
def connect(url: str | None = None) -> Iterator[psycopg.Connection]:
    with psycopg.connect(
        url or dsn(),
        options=f"-c default_transaction_read_only=on -c statement_timeout={STATEMENT_TIMEOUT_MS}",
        application_name="conv-atlas export",
    ) as conn:
        conn.execute("SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY")
        if conn.execute("show transaction_read_only").fetchone()[0] != "on":
            raise RuntimeError("refusing to run: session is not read-only")
        yield conn


def _frame(cur: psycopg.Cursor) -> pd.DataFrame:
    rows = cur.fetchall()
    return pd.DataFrame(rows, columns=[d.name for d in cur.description])


def turn_heads(conn: psycopg.Connection) -> pd.DataFrame:
    """guid, max_turn_no, n_turns per conversation: the cheap change detector."""
    cur = conn.execute(
        "select conversation_id::text as guid, max(turn_no) as max_turn_no, count(*) as n_turns from turns group by 1"
    )
    return _frame(cur)


def conversations(conn: psycopg.Connection) -> pd.DataFrame:
    cur = conn.execute(
        """select guid::text as guid, repo_identity as repo_key, worktree, agent_harness as harness,
                  agent_models as models, pij_seat, parent_conversation_id::text as parent_guid, started_at
           from conversations"""
    )
    return _frame(cur)


def enriched_since(conn: psycopg.Connection, since: pd.Timestamp) -> set[str]:
    """Conversations whose turns gained a summary or vector after `since` (late enrichment)."""
    cur = conn.execute(
        """with fresh as (
             select raw_hash from smart_content where created_at > %(since)s and model_key = any(%(summarisers)s)
             union
             select source_hash from embeddings_1024
               where created_at > %(since)s and source_kind = 'raw' and model_key = %(embedder)s
             union
             select sc.raw_hash from embeddings_1024 e join smart_content sc on sc.text_hash = e.source_hash
               where e.created_at > %(since)s and e.source_kind = 'smart' and e.model_key = %(embedder)s
           )
           select distinct split_part(substr(el.address, 6), '#', 1) as guid
           from fresh f join elements el on el.raw_hash = f.raw_hash and el.kind = 'turn'""",
        {"since": since.to_pydatetime(), "summarisers": list(SUMMARISERS), "embedder": EMBEDDER},
    )
    return {r[0] for r in cur.fetchall()}


def shared_work_hashes(conn: psycopg.Connection, min_conversations: int = SHARED_HASH_MIN_CONVERSATIONS) -> set[str]:
    """Work-turn hashes repeated across many conversations: boilerplate by construction."""
    cur = conn.execute(
        """select el.raw_hash
           from turns t
           join elements el on el.kind = 'turn' and el.blob_sha = t.blob_sha
                           and el.address = 'conv:' || t.conversation_id || '#t' || t.turn_no
           where el.turn_class = 'work'
           group by el.raw_hash having count(distinct t.conversation_id) >= %(n)s""",
        {"n": min_conversations},
    )
    return {r[0] for r in cur.fetchall()}


def turns(conn: psycopg.Connection, guids: Sequence[str]) -> pd.DataFrame:
    """Turn rows (no bodies) for a batch of conversations, with server-side derived fields and `msg_key`."""
    cur = conn.execute(
        f"""select t.conversation_id::text as guid, t.turn_no, t.at, t.role, t.source,
                   el.turn_class as class, el.raw_hash, octet_length(el.raw_text) as bytes,
                   s.text as summary, s.tags, s.model_key as summariser,
                   case when el.turn_class = 'work' and s.text is null then left(el.raw_text, 280) end as head,
                   {_STOP_RULE} as stop_rule,
                   {_PREAMBLE_SEAT} as preamble_seat,
                   {_PEER_FROM} as peer_from,
                   {_SEND_TO} as send_to,
                   {_MESSAGE_TEXT} as msg_text,
                   el.turn_class = 'work' and es.source_hash is not null as smart_vec,
                   el.turn_class = 'work' and er.source_hash is not null as raw_vec
            {_TURN_JOIN}
            {_VEC_JOINS}
            where t.conversation_id = any(%(guids)s::uuid[])
            order by 1, 2""",
        {"guids": list(guids), "summarisers": list(SUMMARISERS)},
    )
    df = _frame(cur)
    # NULLs arrive as None or NaN depending on the column's other values: test for str
    df["msg_key"] = [
        message_key(text, send=isinstance(send, str))
        if isinstance(text, str) and (isinstance(peer, str) or isinstance(send, str))
        else None
        for text, peer, send in zip(df["msg_text"], df["peer_from"], df["send_to"], strict=True)
    ]
    return df.drop(columns="msg_text")


def _vec(b: bytes | None) -> np.ndarray | None:
    """pgvector binary send format: int16 dim, int16 unused, float4[dim] big-endian."""
    if b is None:
        return None
    dim = int.from_bytes(b[:2], "big")
    return np.frombuffer(b, dtype=">f4", count=dim, offset=4).astype(np.float32)


def centroids(
    conn: psycopg.Connection, guids: Sequence[str], excluded_hashes: Sequence[str]
) -> dict[str, tuple[int, np.ndarray]]:
    """Mean of work-turn vectors (smart, else raw) per conversation, computed in SQL.

    Excludes pattern stop-set turns and the given shared hashes. Returns guid -> (n_vectors, centroid).
    """
    cur = conn.execute(
        f"""select t.conversation_id::text, count({_WORK_VEC}), vector_send(avg({_WORK_VEC}))
            {_TURN_JOIN}
            {_VEC_JOINS}
            where t.conversation_id = any(%(guids)s::uuid[])
              and el.turn_class = 'work' and {_STOP_RULE} is null
              and not (el.raw_hash = any(%(excluded)s))
            group by 1""",
        {"guids": list(guids), "excluded": list(excluded_hashes), "summarisers": list(SUMMARISERS)},
    )
    return {g: (int(n), _vec(b)) for g, n, b in cur.fetchall() if n}


def work_vectors(conn: psycopg.Connection, guids: Sequence[str]) -> dict[str, tuple[np.ndarray, np.ndarray]]:
    """Per-turn work vectors (smart, else raw) for a batch: guid -> (turn_nos int32[M], vectors float32[M, D])."""
    cur = conn.execute(
        f"""select t.conversation_id::text, t.turn_no, vector_send({_WORK_VEC})
            {_TURN_JOIN}
            {_VEC_JOINS}
            where t.conversation_id = any(%(guids)s::uuid[])
              and el.turn_class = 'work' and {_WORK_VEC} is not null
            order by 1, 2""",
        {"guids": list(guids), "summarisers": list(SUMMARISERS)},
    )
    out: dict[str, tuple[list[int], list[np.ndarray]]] = {}
    for g, n, b in cur:
        nos, vecs = out.setdefault(g, ([], []))
        nos.append(n)
        vecs.append(_vec(b))
    return {g: (np.asarray(n, dtype=np.int32), np.stack(v)) for g, (n, v) in out.items()}
