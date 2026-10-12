"""Human labels for conversations, communities and topic shifts.

Pure and deterministic: frames in, frames out, no network. Tags are expected to be
normalised already (`normalize_tags`); labels show them through `display_tag`.

Every label passes `is_clean`: no uuid, no hex id, no `conv:` address and no
`session `/`subagent ` (the stored titles this replaces are `session <uuid>` and
`subagent <id>`). Tags that would break that rule are skipped for labels only.
"""

from __future__ import annotations

import math
import re
from collections import Counter
from collections.abc import Iterable

import pandas as pd

from conv_atlas.labels.normalize import display_tag, normalize_tags

CONVERSATION_LABEL_MAX = 48
COMMUNITY_LABEL_MAX = 32
SHIFT_LABEL_MAX = 32
BLURB_MAX = 140
SEP = " · "
MIN_LIFT = 2.0  # community tags this many times over-represented outrank corpus-wide ones

_UUID = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", re.I)
_HEX_ID = re.compile(r"\b(?=[0-9a-f]*\d)(?=[0-9a-f]*[a-f])[0-9a-f]{7,}\b", re.I)
_ADDRESS = re.compile(r"\bconv:\S*")
# Literal, no word boundary: the acceptance check is "no label contains `session `".
_TITLE_WORD = re.compile(r"(?:session|subagent)\s", re.I)
_ID_PHRASE = re.compile(r"\b(?:session|subagent)\s+[0-9a-f][0-9a-f-]{5,}\b", re.I)
# "The output lists ...", "The watchdog reports that ...": summariser framing, not content.
_LEAD = re.compile(
    r"^(?:the|this)\s+(?:[\w-]+\s+){1,2}?"
    r"(?:reports?|records?|lists?|shows?|says|states|notes|confirms?|explains?|describes?|indicates?|"
    r"summari[sz]es|provides?|contains?)\s+(?:that\s+)?",
    re.I,
)
_SENTENCE_END = re.compile(r"(?<=[.!?])\s")


def is_clean(label: str) -> bool:
    """True when a label carries no id-like residue of the stored titles."""
    return not (_UUID.search(label) or _HEX_ID.search(label) or _ADDRESS.search(label) or _TITLE_WORD.search(label))


def _scrub(text: str) -> str:
    """Remove ids from free text (blurbs); collapse whitespace."""
    text = _ID_PHRASE.sub("", text)
    text = _UUID.sub("", _ADDRESS.sub("", text))
    text = _HEX_ID.sub("", text)
    text = re.sub(r"`\s*`|\(\s*\)", "", text)  # quoting left empty by a removed id
    text = re.sub(r"\s+([,.;:])", r"\1", re.sub(r"\s+", " ", text))
    return text.strip(" ,;:")


def _clip(text: str, n: int) -> str:
    if len(text) <= n:
        return text
    cut = text[: n - 1]
    space = cut.rfind(" ")
    if space > n // 2:
        cut = cut[:space]
    return cut.rstrip(" ,;:-·") + "…"


def _sentence_case(text: str) -> str:
    """Capitalise a lower-case first word; leave `iOS`, `GitHub` and friends alone."""
    first = text.split(" ", 1)[0]
    return text[:1].upper() + text[1:] if first.islower() else text


def _as_list(value: object) -> list[str]:
    if value is None or (isinstance(value, float) and math.isnan(value)):
        return []
    return [str(v) for v in value]  # type: ignore[union-attr]


def _clean_summary(summary: str) -> str:
    text = _scrub(summary)
    text = _LEAD.sub("", text, count=1)
    text = _SENTENCE_END.split(text, maxsplit=1)[0].rstrip(".")
    return _sentence_case(text)


def _tag_label(tags: Iterable[str], limit: int, max_tags: int = 2) -> str | None:
    """Join up to `max_tags` label-safe tags within `limit` chars; None if none fit."""
    parts: list[str] = []
    for tag in tags:
        shown = display_tag(tag)
        if not is_clean(shown + " "):  # trailing space: "pij session" must not end a tag run either
            continue
        candidate = SEP.join([*parts, shown])
        if len(candidate) <= limit:
            parts.append(shown)
        elif not parts:
            parts.append(_clip(shown, limit))
        if len(parts) >= max_tags:
            break
    return _sentence_case(SEP.join(parts)) if parts else None


def _repo_short(repo_key: object) -> str | None:
    if not isinstance(repo_key, str) or not repo_key:
        return None
    return repo_key.rstrip("/").rsplit("/", 1)[-1] or None


def _work(turns: pd.DataFrame) -> pd.DataFrame:
    w = turns[turns["class"] == "work"]
    if "stopset" in w:
        w = w[~w["stopset"].fillna(False).astype(bool)]
    return w


def _ranked_tags(conversations: pd.DataFrame, turns: pd.DataFrame) -> dict[str, list[str]]:
    """Per conversation, its tags by TF-IDF against all conversations (work turns)."""
    w = _work(turns)[["guid", "tags"]].copy()
    w["tags"] = w["tags"].map(_as_list)
    ex = w.explode("tags").dropna(subset=["tags"])
    counts = ex.groupby(["guid", "tags"]).size().rename("n").reset_index()
    n_docs = max(len(conversations), 1)
    df = counts.groupby("tags")["guid"].nunique()
    # No "+1": a tag every conversation carries scores 0 and only labels when nothing else exists.
    idf = ((1 + n_docs) / (1 + df)).map(math.log)
    counts["score"] = counts["n"] / counts.groupby("guid")["n"].transform("sum") * counts["tags"].map(idf)
    counts = counts.sort_values(["guid", "score", "n", "tags"], ascending=[True, False, False, True])
    ranked = counts.groupby("guid")["tags"].agg(list).to_dict()
    out: dict[str, list[str]] = {}
    for guid, tags in zip(conversations["guid"], conversations.get("tags", [None] * len(conversations)), strict=True):
        out[guid] = ranked.get(guid) or _as_list(tags)
    return out


def _representatives(work: pd.DataFrame, ranked: dict[str, list[str]], turn_score: pd.Series | None) -> dict[str, str]:
    """Best summary per conversation: overlap with its top-3 tags, nudged by centrality.

    Ties go to a summary long enough to describe something, then the earliest turn.
    """
    s = work[work["summary"].notna()].reset_index(drop=True)
    if s.empty:
        return {}
    focus = pd.DataFrame(
        [(g, t, 3 - i) for g, tags in ranked.items() for i, t in enumerate(tags[:3])],
        columns=["guid", "tags", "w"],
    )
    ex = s[["guid", "tags"]].assign(tags=s["tags"].map(_as_list), _row=s.index).explode("tags")
    hits = ex.dropna(subset=["tags"]).merge(focus, on=["guid", "tags"], how="inner")
    score = hits.groupby("_row")["w"].sum().reindex(s.index, fill_value=0).astype(float)
    if turn_score is not None and "address" in s:
        score = score + 2.0 * s["address"].map(turn_score).fillna(0.0).astype(float)
    has_body = s["summary"].str.len().ge(40)  # "Ready" is a summary, but not a description
    best = (
        s.assign(_s=score, _b=has_body)
        .sort_values(["guid", "_s", "_b", "turn_no"], ascending=[True, False, False, True])
        .drop_duplicates("guid")
    )
    return {g: _clean_summary(str(x)) for g, x in zip(best["guid"], best["summary"], strict=True)}


def _first_heads(work: pd.DataFrame) -> dict[str, str]:
    if "head" not in work:
        return {}
    h = work[work["head"].notna()].sort_values(["guid", "turn_no"]).drop_duplicates("guid")
    return dict(zip(h["guid"], h["head"].astype(str), strict=True))


def label_conversations(
    conversations: pd.DataFrame,
    turns: pd.DataFrame,
    turn_score: pd.Series | None = None,
) -> pd.DataFrame:
    """[guid, label, blurb], one row per input conversation, in input order.

    conversations: guid, kind, tags, seat, repo_key, started_at, turns (optional).
    turns: guid, turn_no, class, summary, tags, stopset; plus address when turn_score
    (index=address, higher = more central) is given.
    Labels are the conversation's two most distinctive tags (≤48 chars), made
    distinct within a seat (else a repo) by a third tag, then the start date.
    """
    ranked = _ranked_tags(conversations, turns)
    work = _work(turns)
    summaries = _representatives(work, ranked, turn_score)
    heads = _first_heads(work)

    labels: list[str] = []
    blurbs: list[str] = []
    alt: list[str | None] = []
    fallback: list[str | None] = []  # the `who` of `<who> · <date time>` labels
    for row in conversations.itertuples(index=False):
        tags = ranked.get(row.guid, [])
        summary = summaries.get(row.guid) or None
        started = pd.Timestamp(row.started_at)
        who: str | None = None
        label = _tag_label(tags, CONVERSATION_LABEL_MAX)
        if label is None and summary:
            text = _clip(summary, CONVERSATION_LABEL_MAX)
            label = text if is_clean(text) else None
        if label is None:
            raw = _repo_short(getattr(row, "repo_key", None)) or getattr(row, "seat", None) or "Untitled"
            who = str(raw) if is_clean(f"{raw} ") else "Untitled"
            stamp = started.strftime("%b %d %H:%M") if not pd.isna(started) else None
            label = SEP.join(p for p in (who, stamp) if p)
        fallback.append(who)
        labels.append(label)
        alt.append(_tag_label(tags, CONVERSATION_LABEL_MAX, max_tags=3))

        if summary:
            blurb = summary
        else:
            blurb = _scrub(heads.get(row.guid, ""))
            if not blurb:
                n = getattr(row, "turns", None)
                where = _repo_short(getattr(row, "repo_key", None))
                blurb = " ".join(
                    p for p in (f"{n} turns" if n is not None else "Conversation", f"in {where}" if where else "") if p
                )
                if tags:
                    blurb += ": " + ", ".join(display_tag(t) for t in tags[:3])
        blurbs.append(_clip(blurb, BLURB_MAX))

    out = pd.DataFrame({"guid": conversations["guid"].to_numpy(), "label": labels, "blurb": blurbs})
    _disambiguate(out, conversations, alt, fallback)
    return out


def _disambiguate(
    out: pd.DataFrame, conversations: pd.DataFrame, alt: list[str | None], fallback: list[str | None]
) -> None:
    """Separate equal labels within one seat (else repo): a third tag, then the start time."""
    seat = conversations["seat"] if "seat" in conversations else pd.Series([None] * len(out))
    repo = conversations["repo_key"] if "repo_key" in conversations else pd.Series([None] * len(out))
    group = [
        f"seat:{s}" if isinstance(s, str) and s else f"repo:{r}" if isinstance(r, str) and r else "-"
        for s, r in zip(seat, repo, strict=True)
    ]
    started = pd.to_datetime(conversations["started_at"], utc=True).reset_index(drop=True)

    def dups() -> list[int]:
        keys = list(zip(group, out["label"], strict=True))
        seen = Counter(keys)
        return [i for i, k in enumerate(keys) if seen[k] > 1 and k[0] != "-"]

    base = out["label"].tolist()
    for i in dups():
        if alt[i] and alt[i] != base[i]:
            out.at[i, "label"] = alt[i]
    # A third tag did not separate them: fall back to the two-tag label plus the start time.
    # Fallback labels (`<repo> · <date time>`) already carry minutes, so they only gain seconds.
    for fmt in ("%b %d", "%b %d %H:%M", "%b %d %H:%M:%S"):
        for i in dups():
            if pd.isna(started[i]):
                continue
            stamp = started[i].strftime(fmt)
            if fallback[i] is not None:
                if fmt.endswith("%S"):
                    out.at[i, "label"] = f"{fallback[i]}{SEP}{stamp}"
                continue
            suffix = f" ({stamp})"
            out.at[i, "label"] = _clip(base[i], CONVERSATION_LABEL_MAX - len(suffix)) + suffix


def _label_still_holds(label: str, top: list[str]) -> bool:
    """True when every tag a label shows is still among `top` (clipped parts match by prefix)."""
    shown = [display_tag(t).casefold() for t in top]
    for part in label.split(SEP):
        p = part.casefold()
        if p.endswith("…"):
            if not any(s.startswith(p[:-1]) for s in shown):
                return False
        elif p not in shown:
            return False
    return True


def label_communities(
    conversations: pd.DataFrame,
    turns: pd.DataFrame,
    centrality: pd.Series | None = None,
    previous: pd.DataFrame | None = None,
) -> pd.DataFrame:
    """[community_id, label, blurb, tags], one row per community, ordered by id.

    conversations: guid, community_id (nullable), tags, blurb (from label_conversations),
    repo_key (optional). turns: guid, class, tags (work turns are used).
    centrality: optional, index=guid, higher = more central to its community.
    previous: optional, the last snapshot's [community_id, label] re-keyed to matched ids.
    A matched community keeps its previous label while every tag the label shows is
    still in its top 6, so labels do not churn when a few conversations arrive.
    Tags are ranked by how over-represented they are in the community's work turns
    against the whole corpus (share x log-lift), weighted by how many members carry them.
    Tags at least MIN_LIFT times over-represented rank first, so corpus-wide words
    (validation, testing) only label a community when nothing sharper exists.
    """
    members = conversations[conversations["community_id"].notna()].copy()
    members["community_id"] = members["community_id"].astype(int)
    members["tags"] = members["tags"].map(_as_list) if "tags" in members else [[] for _ in range(len(members))]
    member_tags = dict(zip(members["guid"], members["tags"].map(set), strict=True))
    cid_of = dict(zip(members["guid"], members["community_id"], strict=True))
    size = members.groupby("community_id").size()

    w = _work(turns)
    w = w[w["guid"].isin(cid_of)][["guid", "tags"]]
    ex = w.assign(tags=w["tags"].map(_as_list)).explode("tags").dropna(subset=["tags"])
    ex["community_id"] = ex["guid"].map(cid_of)
    corpus = ex["tags"].value_counts()
    total = max(int(corpus.sum()), 1)

    counts = ex.groupby(["community_id", "tags"]).size().rename("n").reset_index()
    counts["p_c"] = counts["n"] / counts.groupby("community_id")["n"].transform("sum")
    counts["lift"] = counts["p_c"] / (counts["tags"].map(corpus) / total)
    cover = (
        members[["guid", "community_id", "tags"]]
        .explode("tags")
        .dropna(subset=["tags"])
        .drop_duplicates(["guid", "tags"])
        .groupby(["community_id", "tags"])
        .size()
        .rename("carriers")
        .reset_index()
    )
    counts = counts.merge(cover, on=["community_id", "tags"], how="left").fillna({"carriers": 0})
    counts = counts[counts["lift"] > 1].copy()
    counts["coverage"] = counts["carriers"] / counts["community_id"].map(size)
    counts["score"] = counts["p_c"] * counts["lift"].map(math.log) * (0.25 + counts["coverage"])
    counts["sharp"] = counts["lift"] >= MIN_LIFT
    counts = counts.sort_values(["community_id", "sharp", "score", "tags"], ascending=[True, False, False, True])
    ranked = counts.groupby("community_id")["tags"].agg(lambda s: list(s)[:6]).to_dict()

    groups = dict(list(members.groupby("community_id", sort=True)))
    tops = {
        cid: ranked.get(cid) or [t for t, _ in Counter(t for ts in g["tags"] for t in ts).most_common(6)]
        for cid, g in groups.items()
    }
    labels: dict[int, str] = {}
    if previous is not None:
        for cid, old in zip(previous["community_id"], previous["label"], strict=True):
            if pd.isna(cid) or int(cid) not in tops or not isinstance(old, str) or old in labels.values():
                continue
            if _label_still_holds(old, tops[int(cid)]):
                labels[int(cid)] = old
    used_labels = set(labels.values())
    for cid, top in tops.items():
        if cid in labels:
            continue
        label = _tag_label(top, COMMUNITY_LABEL_MAX) or f"Cluster {cid}"
        if label in used_labels:
            label = _tag_label([top[0], *top[2:]], COMMUNITY_LABEL_MAX) if len(top) > 2 else None
            label = label if label and label not in used_labels else f"{_clip(label or 'Cluster', 24)} ({cid})"
        labels[cid] = label
        used_labels.add(label)

    rows = []
    for cid, group in groups.items():
        n_members = len(group)
        top, label = tops[cid], labels[cid]

        if centrality is not None:
            central = group.assign(_c=group["guid"].map(centrality).fillna(-math.inf))
        else:
            focus = set(top[:3])
            overlap = {g: len(member_tags.get(g, set()) & focus) for g in group["guid"]}
            central = group.assign(_c=group["guid"].map(overlap))
        central = central.sort_values(["_c", "guid"], ascending=[False, True])
        example = next(
            (_clean_summary(str(b)) for b in central.get("blurb", []) if isinstance(b, str) and b.strip()),
            "",
        )
        lead = ""
        if "repo_key" in group:
            repos = group["repo_key"].dropna().map(_repo_short).dropna()
            if len(repos):
                top_repo, n_repo = Counter(repos).most_common(1)[0]
                if n_repo / n_members >= 0.5:
                    lead = f"Mostly {top_repo} ({n_repo / n_members:.0%}). "
        shown = {p.casefold() for p in label.split(SEP)}
        also = [d for d in map(display_tag, top) if d.casefold() not in shown and is_clean(d + " ")][:3]
        context = _sentence_case("Also " + ", ".join(also) + ". ") if also else ""
        body = example or f"{n_members} conversations about " + ", ".join(display_tag(t) for t in top[:3])
        blurb = lead + context + body
        rows.append({"community_id": int(cid), "label": label, "blurb": _clip(blurb, BLURB_MAX), "tags": top})
    return pd.DataFrame(rows, columns=["community_id", "label", "blurb", "tags"])


def label_shift(tags: list[str], summaries: list[str]) -> str:
    """Label for the topic that starts at a change point (≤32 chars, never empty)."""
    label = _tag_label(normalize_tags(tags), SHIFT_LABEL_MAX)
    if label:
        return label
    for s in summaries:
        if isinstance(s, str) and s.strip():
            text = _clip(_clean_summary(s), SHIFT_LABEL_MAX)
            if text and is_clean(text):
                return text
    return "New topic"
