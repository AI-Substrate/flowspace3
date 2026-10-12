"""The analysis-layer contract: every table, field, type and key, in one place.

This module is the single source of truth for `tables/*.parquet` and for the
`datapackage.json` descriptor written beside them. The view layer
(`view/*.json`, validated by `contract/bundle.schema.json`) is derived from
these tables by `bundle.py`; it never has fields of its own.

Field types are Frictionless Table Schema types. `array` fields hold lists of
strings unless noted. Datetimes are timezone-aware UTC.
"""

from __future__ import annotations

from dataclasses import dataclass, field

SCHEMA_VERSION = 1


@dataclass(frozen=True)
class Field:
    name: str
    type: str  # string | integer | number | boolean | datetime | date | array
    description: str
    required: bool = True


@dataclass(frozen=True)
class ForeignKey:
    fields: tuple[str, ...]
    table: str
    reference: tuple[str, ...]


@dataclass(frozen=True)
class Table:
    name: str
    description: str
    fields: tuple[Field, ...]
    primary_key: tuple[str, ...]
    foreign_keys: tuple[ForeignKey, ...] = field(default_factory=tuple)

    @property
    def columns(self) -> list[str]:
        return [f.name for f in self.fields]


def _tuple(v: str | tuple[str, ...]) -> tuple[str, ...]:
    return (v,) if isinstance(v, str) else v


def _fk(fields: str | tuple[str, ...], table: str, reference: str | tuple[str, ...]) -> ForeignKey:
    return ForeignKey(_tuple(fields), table, _tuple(reference))


F = Field

TABLES: dict[str, Table] = {
    t.name: t
    for t in (
        Table(
            "conversations",
            "One row per conversation transcript indexed by flowspace3.",
            (
                F("guid", "string", "Conversation guid; `flowspace3 tree conv:<guid>` opens it"),
                F("address", "string", "fs3 address, conv:<guid>"),
                F("kind", "string", "session | subagent (subagent = has a parent conversation)"),
                F(
                    "label",
                    "string",
                    "Generated label (stored titles are uuids): the two most distinctive tags, else a summary "
                    "clause, else `<repo> · <date time>`; ≤48 chars, distinct within a seat or repo, never an id",
                ),
                F("blurb", "string", "Most representative work-turn summary, first sentence, ≤140 chars", False),
                F("repo_key", "string", "Anchor repository identity (git:host/owner/name or a path)", False),
                F("worktree", "string", "Checkout path the conversation ran in", False),
                F("harness", "string", "Agent harness that produced it (claude, omp, ...)", False),
                F(
                    "seat",
                    "string",
                    "pij seat id the conversation held most recently (metadata, else the transcript preamble). "
                    "Seat identity is time-varying: a long-lived conversation can be re-seated, so earlier turns "
                    "may belong to an earlier seat; per-message receiver identity is messages.to_seat",
                    False,
                ),
                F("models", "array", "Canonical model names seen in assistant turns, first-seen order"),
                F("parent_guid", "string", "Parent conversation guid for subagents", False),
                F("started_at", "datetime", "Conversation start (UTC)"),
                F("last_turn_at", "datetime", "Timestamp of the newest turn (UTC)"),
                F("turns", "integer", "Total stored turns"),
                F("n_work", "integer", "Turns classified work (human/agent prose)"),
                F("n_tool_call", "integer", "Turns classified tool_call"),
                F("n_tool_result", "integer", "Turns classified tool_result"),
                F("n_retrieval", "integer", "Turns classified retrieval_call or retrieval_result"),
                F("n_human", "integer", "Turns with source=human (a person typed it)"),
                F("n_peer", "integer", "Turns with source=peer (another agent seat sent it)"),
                F("n_agent", "integer", "Turns with role=agent"),
                F("tags", "array", "Top normalised summary tags across work turns, most frequent first"),
                F("community_id", "integer", "Cluster id (communities.community_id); null = unclustered", False),
                F("x", "number", "2-D layout x of the conversation centroid; null = not embedded", False),
                F("y", "number", "2-D layout y of the conversation centroid; null = not embedded", False),
                F("embedded", "boolean", "True when the conversation had enough work turns for a centroid"),
            ),
            ("guid",),
            (
                _fk("parent_guid", "conversations", "guid"),
                _fk("community_id", "communities", "community_id"),
            ),
        ),
        Table(
            "turns",
            "One row per stored turn. Raw bodies are NOT copied: `flowspace3 get <address>` fetches them.",
            (
                F("address", "string", "fs3 address, conv:<guid>#t<turn_no>"),
                F("guid", "string", "Conversation guid"),
                F("turn_no", "integer", "1-based ordinal within the conversation"),
                F("turn_at", "datetime", "Turn timestamp (UTC)"),
                F("role", "string", "human | agent"),
                F("source", "string", "human | peer | system"),
                F("class", "string", "work | tool_call | tool_result | retrieval_call | retrieval_result"),
                F("summary", "string", "LLM summary; null when unsummarised (small or pending)", False),
                F("head", "string", "First 280 chars of the raw body for unsummarised work turns", False),
                F("tags", "array", "Normalised summary tags (empty when unsummarised)"),
                F("raw_hash", "string", "Content hash; identical content shares one hash across conversations"),
                F("bytes", "integer", "Raw body length in bytes"),
                F("summarized", "boolean", "True when a summary exists"),
                F("stopset", "boolean", "True when the boilerplate filter excluded it from centroids"),
                F("session_x", "number", "Per-session 2-D layout x (work turns of detailed sessions)", False),
                F("session_y", "number", "Per-session 2-D layout y (work turns of detailed sessions)", False),
            ),
            ("address",),
            (_fk("guid", "conversations", "guid"),),
        ),
        Table(
            "communities",
            "Clusters of conversations in centred embedding space.",
            (
                F(
                    "community_id",
                    "integer",
                    "Cluster id, dense from 0. Stable across snapshots: a cluster matched to the previous "
                    "snapshot's cluster by member overlap keeps its id; first snapshot and new clusters go "
                    "largest first",
                ),
                F("label", "string", "Two most over-represented tags against the corpus, ≤32 chars, unique"),
                F("blurb", "string", "Dominant repo when it holds ≥50%, plus the most central member's blurb; ≤140"),
                F("tags", "array", "Up to 6 distinctive tags, ranked by share x log-lift against the whole corpus"),
                F("size", "integer", "Number of member conversations"),
                F("cx", "number", "Layout x of the member centroid"),
                F("cy", "number", "Layout y of the member centroid"),
            ),
            ("community_id",),
        ),
        Table(
            "knn_edges",
            "Undirected k-nearest-neighbour edges between embedded conversations (a_guid < b_guid).",
            (
                F("a_guid", "string", "Lower guid"),
                F("b_guid", "string", "Higher guid"),
                F("cos_centred", "number", "Cosine after subtracting the corpus mean (use this)"),
                F("cos_raw", "number", "Cosine of raw centroids (inflated by embedding anisotropy)"),
                F("cross_repo", "boolean", "True when both ends have different, known repos"),
                F("t_first", "datetime", "When the edge can first exist: max(started_at) of the ends"),
            ),
            ("a_guid", "b_guid"),
            (_fk("a_guid", "conversations", "guid"), _fk("b_guid", "conversations", "guid")),
        ),
        Table(
            "subagent_edges",
            "Parent conversation spawned child subagent conversation.",
            (
                F("parent_guid", "string", "Parent conversation guid"),
                F("child_guid", "string", "Subagent conversation guid"),
                F("started_at", "datetime", "Child start (UTC)"),
            ),
            ("parent_guid", "child_guid"),
            (_fk("parent_guid", "conversations", "guid"), _fk("child_guid", "conversations", "guid")),
        ),
        Table(
            "seats",
            "pij agent seats seen in the corpus.",
            (
                F("seat", "string", "pij seat id (pij-adjective-animal)"),
                F("harness", "string", "Harness of the seat's own conversations, when known", False),
                F("first_seen", "datetime", "Earliest evidence of the seat"),
                F("last_seen", "datetime", "Latest evidence of the seat"),
                F("n_conversations", "integer", "Conversations attributed to the seat"),
                F("n_sent", "integer", "Peer messages sent"),
                F("n_received", "integer", "Peer messages received"),
                F("identity_source", "string", "metadata | preamble | message-only"),
            ),
            ("seat",),
        ),
        Table(
            "messages",
            "One row per peer (agent-to-agent) message turn.",
            (
                F("address", "string", "Address of the receiving turn"),
                F("sent_at", "datetime", "Delivery timestamp (UTC)"),
                F(
                    "from_seat",
                    "string",
                    "Sender seat, parsed from the pij envelope; null for peer-sourced turns with no envelope "
                    "(system and task notifications)",
                    False,
                ),
                F(
                    "to_seat",
                    "string",
                    "Receiver seat when this message arrived: the matching send call, else agreeing matches either "
                    "side in the same conversation, else the receiving conversation's seat. Seat identity is "
                    "time-varying, so this can differ from conversations.seat for re-seated conversations "
                    "(provenance.stats.messages_to_seat_differs_from_conversation_seat); coverage in "
                    "provenance.stats.messages_to_seat_coverage",
                    False,
                ),
                F("to_guid", "string", "Receiving conversation guid"),
                F("from_guid", "string", "Sender's conversation guid when resolvable", False),
            ),
            ("address",),
            (_fk("to_guid", "conversations", "guid"), _fk("from_seat", "seats", "seat")),
        ),
        Table(
            "activity_daily",
            "Per conversation per UTC day turn counts.",
            (
                F("guid", "string", "Conversation guid"),
                F("day", "date", "UTC day"),
                F("turns", "integer", "All turns that day"),
                F("work", "integer", "Work turns that day"),
                F("tool", "integer", "Tool call + result turns that day"),
                F("human", "integer", "source=human turns that day"),
                F("peer", "integer", "source=peer turns that day"),
            ),
            ("guid", "day"),
            (_fk("guid", "conversations", "guid"),),
        ),
        Table(
            "tags",
            "Normalised summary tags over work turns (tag map version: provenance.labels.tag_map_version).",
            (
                F("tag", "string", "Normalised tag"),
                F("n", "integer", "Work turns carrying the tag"),
                F("conversations", "integer", "Conversations whose top tags include it"),
            ),
            ("tag",),
        ),
        Table(
            "tag_cooc",
            "Tag pairs co-occurring in conversations' top tags (tag_a < tag_b).",
            (
                F("tag_a", "string", "Lower tag"),
                F("tag_b", "string", "Higher tag"),
                F("n", "integer", "Conversations whose top tags contain both"),
            ),
            ("tag_a", "tag_b"),
            (_fk("tag_a", "tags", "tag"), _fk("tag_b", "tags", "tag")),
        ),
        Table(
            "session_shifts",
            "Detected topic change points inside long sessions.",
            (
                F("guid", "string", "Conversation guid"),
                F("turn_no", "integer", "First turn of the new topic"),
                F("turn_at", "datetime", "Timestamp of that turn"),
                F("score", "number", "Window novelty at the change point (0..2, higher = sharper shift)"),
                F("label", "string", "Short label (≤32 chars) for the topic that starts here, from its tags"),
                F("tags", "array", "Dominant tags of the new topic"),
            ),
            ("guid", "turn_no"),
            (_fk("guid", "conversations", "guid"),),
        ),
        Table(
            "session_novelty",
            "Window novelty series per detailed session (work turns only).",
            (
                F("guid", "string", "Conversation guid"),
                F("turn_no", "integer", "Work turn ordinal"),
                F("novelty", "number", "1 - cos(mean(prev window), mean(next window))"),
            ),
            ("guid", "turn_no"),
            (_fk("guid", "conversations", "guid"),),
        ),
    )
}

# Vector artifacts in vectors/. Row order is given by the matching index table.
VECTORS: dict[str, str] = {
    "conversation_centroids.npy": "float32 [N, D]: mean of work-turn vectors per conversation, NOT centred",
    "conversation_centroids.index.parquet": "row -> guid for conversation_centroids.npy",
    "embedding_mean.npy": "float32 [D]: corpus mean; subtract it (then re-normalise) for centred cosine",
    "turn_vectors.f16.npy": "OPTIONAL float16 [M, D]: work-turn vectors (export --turn-vectors)",
    "turn_vectors.index.parquet": "OPTIONAL row -> address for turn_vectors.f16.npy",
}


def datapackage(snapshot_id: str, provenance: dict, present: set[str]) -> dict:
    """Frictionless Data Package descriptor for one snapshot."""
    resources = []
    for name, table in TABLES.items():
        if name not in present:
            continue
        schema: dict = {
            "fields": [
                {
                    "name": f.name,
                    "type": f.type,
                    "description": f.description,
                    **({"constraints": {"required": True}} if f.required else {}),
                }
                for f in table.fields
            ],
            "primaryKey": list(table.primary_key),
        }
        if table.foreign_keys:
            schema["foreignKeys"] = [
                {
                    "fields": list(fk.fields),
                    "reference": {"resource": fk.table, "fields": list(fk.reference)},
                }
                for fk in table.foreign_keys
            ]
        resources.append(
            {
                "name": name,
                "path": f"tables/{name}.parquet",
                "format": "parquet",
                "description": table.description,
                "schema": schema,
            }
        )
    return {
        "name": f"conv-atlas-{snapshot_id.lower()}",
        "title": f"flowspace3 conversation atlas snapshot {snapshot_id}",
        "conv_atlas_schema_version": SCHEMA_VERSION,
        "resources": resources,
        "vectors": {k: v for k, v in VECTORS.items() if k in present},
        "provenance": provenance,
    }
