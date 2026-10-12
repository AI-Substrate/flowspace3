"""Pure tag and model-name normalisation. Deterministic, no I/O."""

from __future__ import annotations

import re
import unicodedata
from collections.abc import Iterable

from conv_atlas.labels.tag_map import DISPLAY, DROP, KEEP_S, SYNONYMS

MAX_TAG_LEN = 48

_QUOTES = re.compile(r"[`'\"\u2018\u2019\u201c\u201d]")
_ORDINAL = re.compile(r"#\d+\b")  # "pa sweep #391" -> "pa sweep"
_UUID = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", re.I)
_HEX_ID = re.compile(r"\b(?=[0-9a-f]*\d)(?=[0-9a-f]*[a-f])[0-9a-f]{7,}\b", re.I)  # git shas, short ids
# Everything except word chars, spaces and the in-token punctuation we keep (c++, c#, node.js, origin/main).
_PUNCT = re.compile(r"[^\w\s.+#/-]")
_SPACE = re.compile(r"\s+")
_PATHLIKE = re.compile(r"\w[./]\w")  # file names and paths keep their - and _
_WORD = re.compile(r"[a-z]+")


def _singular(word: str) -> str:
    if len(word) <= 3 or not _WORD.fullmatch(word) or word in KEEP_S:
        return word
    if word.endswith(("ss", "us", "is", "ics", "ous")):
        return word
    if word.endswith("ies"):
        return word[:-3] + "y"
    if word.endswith(("sses", "shes", "ches", "xes", "zzes")):
        return word[:-2]
    if word.endswith("s"):
        return word[:-1]
    return word


def normalize_tag(tag: str) -> str | None:
    """Canonical form of one summary tag; None means drop it.

    Steps: NFKC + casefold, strip quotes and `#123` ordinals, other punctuation to
    spaces (`-`/`_` too, unless the tag looks like a path or file name), collapse
    spaces, singularise the last word, apply SYNONYMS, then DROP. Idempotent.
    """
    if not isinstance(tag, str):
        return None
    s = unicodedata.normalize("NFKC", tag).casefold()
    if _UUID.search(s) or _HEX_ID.search(s):
        return None
    s = _ORDINAL.sub(" ", _QUOTES.sub("", s))
    s = _PUNCT.sub(" ", s)
    if not _PATHLIKE.search(s):
        s = s.replace("-", " ").replace("_", " ")
    s = _SPACE.sub(" ", s).strip().rstrip(".").rstrip()
    if len(s) < 2 or len(s) > MAX_TAG_LEN or not any(c.isalpha() for c in s):
        return None
    head, _, last = s.rpartition(" ")
    s = f"{head} {_singular(last)}" if head else _singular(last)
    s = SYNONYMS.get(s, s)
    return None if s in DROP else s


def normalize_tags(tags: Iterable[str] | None) -> list[str]:
    """normalize_tag over a list: drops None, dedupes, keeps first-seen order."""
    out: dict[str, None] = {}
    for t in tags if tags is not None else ():
        n = normalize_tag(t)
        if n is not None:
            out.setdefault(n, None)
    return list(out)


_MODEL_DATE = re.compile(r"[-@]\d{8}$")  # claude-opus-4-1-20250805
_MODEL_VERSION = re.compile(r"(?<=[a-z])-(\d{1,2})-(\d{1,2})$")  # claude-opus-5-5 -> claude-opus-5.5


def canonical_model(name: str) -> str | None:
    """Canonical assistant model name; None means drop it (`<synthetic>`, blank).

    Lower-cases, drops any `provider/` routing prefix and a trailing date stamp,
    and writes a dashed `-major-minor` version as `-major.minor`.
    """
    if not isinstance(name, str):
        return None
    s = name.strip().casefold()
    if not s or s.startswith("<"):
        return None
    s = s.rsplit("/", 1)[-1]
    s = _MODEL_DATE.sub("", s)
    s = _MODEL_VERSION.sub(r"-\1.\2", s)
    return s or None


def display_tag(tag: str) -> str:
    """Tag as it should read inside a label: known acronyms and names restored."""
    return " ".join(DISPLAY.get(w, w) for w in tag.split(" "))
