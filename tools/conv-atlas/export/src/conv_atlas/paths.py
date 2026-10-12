"""Where conv-atlas keeps its data. Never inside the repository by default."""

from __future__ import annotations

import os
from pathlib import Path

DEFAULT_HOME = Path.home() / ".local" / "share" / "flowspace3" / "conv-atlas"


def home(override: Path | None = None) -> Path:
    if override is not None:
        return override
    env = os.environ.get("CONV_ATLAS_HOME")
    return Path(env).expanduser() if env else DEFAULT_HOME


def bundles_dir(override: Path | None = None) -> Path:
    return home(override) / "bundles"


def insights_dir(override: Path | None = None) -> Path:
    return home(override) / "insights"


def cache_dir(override: Path | None = None) -> Path:
    return home(override) / "cache"


TOOL_ROOT = Path(__file__).resolve().parents[3]  # tools/conv-atlas
SCHEMA_PATH = TOOL_ROOT / "contract" / "bundle.schema.json"
