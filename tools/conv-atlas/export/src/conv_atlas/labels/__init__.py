"""Human labels and tag hygiene for conv-atlas (Scribe-owned).

Everything here is pure and deterministic: no network, no database. Cartographer
calls, in order: `normalize_tags` / `canonical_model` on raw values, then the
labellers. `provenance()` is recorded at datapackage `provenance.labels`.
"""

from __future__ import annotations

from conv_atlas.labels.label import is_clean, label_communities, label_conversations, label_shift
from conv_atlas.labels.normalize import canonical_model, display_tag, normalize_tag, normalize_tags
from conv_atlas.labels.tag_map import TAG_MAP_VERSION

__all__ = [
    "TAG_MAP_VERSION",
    "canonical_model",
    "display_tag",
    "normalize_tag",
    "is_clean",
    "label_communities",
    "label_conversations",
    "label_shift",
    "normalize_tags",
    "provenance",
]


def provenance() -> dict:
    return {"tag_map_version": TAG_MAP_VERSION, "labeller": "tfidf-v1"}
