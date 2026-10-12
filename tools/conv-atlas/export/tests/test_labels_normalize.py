"""Tag and model-name normalisation (synthetic, real-shaped strings only: the repo is public)."""

from __future__ import annotations

import pytest

from conv_atlas.labels import canonical_model, display_tag, normalize_tag, normalize_tags
from conv_atlas.labels.tag_map import DROP, SYNONYMS


@pytest.mark.parametrize(
    ("raw", "want"),
    [
        # case, spacing, punctuation and hyphen/underscore forms meet
        ("Rust", "rust"),
        ("  Git   Status ", "git status"),
        ("git-status", "git status"),
        ("plan_versions", "plan version"),
        ("`Vitest`", "vitest"),
        ("Unreal Engine.", "unreal engine"),
        # plurals: last word only, with the irregular endings
        ("integration tests", "integration test"),
        ("dependencies", "dependency"),
        ("processes", "process"),
        ("fixes", "fix"),
        ("status", None),  # ends in -us: kept as is, then dropped as a status word
        ("analysis", "analysis"),
        ("diagnostics", "diagnostics"),
        ("kubernetes", "kubernetes"),
        ("apis", "api"),
        ("aws", "aws"),
        # synonyms
        ("tests", "testing"),
        ("Testing", "testing"),
        ("regression testing", "regression test"),
        ("Docs", "documentation"),
        ("Postgres", "postgresql"),
        ("rust analyzer", "rust-analyzer"),
        ("rust-analyzer", "rust-analyzer"),
        ("GitHub Actions", "github actions"),
        ("PR #118", "pull request"),
        # in-token punctuation that carries meaning survives
        ("C++", "c++"),
        ("C#", "c#"),
        ("Node.js", "node.js"),
        ("Cargo.toml", "cargo.toml"),
        ("import_ground.py", "import_ground.py"),
        ("origin/main", "origin/main"),
        # ordinals go, the topic stays
        ("PA Sweep #391", "pa sweep"),
        # noise
        ("Completed", None),
        ("file write", None),
        ("Bash", None),
        ("", None),
        ("#42", None),
        ("x", None),
        ("123", None),
        ("session 3f2a9c1e-8b7d-4e6f-9a0b-1c2d3e4f5a6b", None),
        ("commit 9fceb02d0ae598e95dc970b74767f19372d61af8", None),
        ("a1b2c3d", None),
        ("w" * 60, None),
    ],
)
def test_normalize_tag(raw: str, want: str | None) -> None:
    assert normalize_tag(raw) == want


def test_non_strings_are_dropped() -> None:
    assert normalize_tag(None) is None  # type: ignore[arg-type]
    assert normalize_tag(float("nan")) is None  # type: ignore[arg-type]


def test_every_synonym_value_is_a_fixed_point() -> None:
    for alias, canon in SYNONYMS.items():
        assert normalize_tag(canon) == canon, f"{alias} -> {canon} is not stable"


def test_every_synonym_key_is_reachable() -> None:
    # A key not written in mechanical-output form silently never matches.
    for alias, canon in SYNONYMS.items():
        want = None if canon in DROP else canon
        assert normalize_tag(alias) == want, alias


def test_every_drop_entry_is_reachable() -> None:
    for word in DROP:
        assert normalize_tag(word) is None, word
        assert normalize_tag(word.upper()) is None, word


SAMPLE = [
    "validation", "Validation", "Bash", "bash", "tests", "Tests", "testing", "test",
    "Rust", "rust", "git", "Git", "git status", "Git status", "git-status",
    "integration tests", "integration testing", "Integration Test", "PR #12", "pull requests",
    "GitHub PR", "Postgres", "PostgreSQL", "completed", "file write", "watchdog", "Watchdog",
    "PA Sweep #391", "PA Sweep #392", "worktrees", "worktree", "handoff", "handover",
]  # fmt: skip


def test_map_effect_on_a_real_shaped_sample() -> None:
    out = [normalize_tag(t) for t in SAMPLE]
    kept = {t for t in out if t is not None}
    assert sum(t is None for t in out) == 4  # Bash, bash, completed, file write
    assert kept == {
        "validation", "testing", "rust", "git", "git status", "integration test", "pull request",
        "postgresql", "watchdog", "pa sweep", "worktree", "handover",
    }  # fmt: skip
    for t in SAMPLE:
        n = normalize_tag(t)
        assert n is None or normalize_tag(n) == n, t


def test_normalize_tags_dedupes_in_first_seen_order() -> None:
    assert normalize_tags(["Tests", "rust", "completed", "testing", "Rust", "docs"]) == [
        "testing",
        "rust",
        "documentation",
    ]
    assert normalize_tags(None) == []


@pytest.mark.parametrize(
    ("raw", "want"),
    [
        ("claude-opus-5-5", "claude-opus-5.5"),
        ("claude-opus-5.5", "claude-opus-5.5"),
        ("Claude-Sonnet-5-5", "claude-sonnet-5.5"),
        ("claude-opus-4-1-20250805", "claude-opus-4.1"),
        ("vendor/kimi-k3", "kimi-k3"),
        ("gpt-5.6-sol", "gpt-5.6-sol"),
        ("gpt-6-astra", "gpt-6-astra"),
        ("<synthetic>", None),
        ("  ", None),
        (None, None),
    ],
)
def test_canonical_model(raw: str, want: str | None) -> None:
    assert canonical_model(raw) == want


def test_display_tag_restores_acronyms_and_names() -> None:
    assert display_tag("rust daemon") == "Rust daemon"
    assert display_tag("ci gate") == "CI gate"
    assert display_tag("watchdog") == "watchdog"
