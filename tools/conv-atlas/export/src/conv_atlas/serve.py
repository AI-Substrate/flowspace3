"""Static server for a long-lived page: app/dist at /, the data home at /data.

Same mapping as the Vite dev middleware (app/dataServer.ts): only
<home>/bundles and <home>/insights are reachable under /data.
"""

from __future__ import annotations

import contextlib
import functools
import http.server
from pathlib import Path

from conv_atlas import paths

DIST = paths.TOOL_ROOT / "app" / "dist"


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, home: Path, **kwargs):
        self.home = home.resolve()
        super().__init__(*args, directory=str(DIST), **kwargs)

    def translate_path(self, path: str) -> str:
        clean = path.split("?", 1)[0].split("#", 1)[0]
        if clean.startswith("/data/"):
            rel = Path(clean[len("/data/") :])
            target = (self.home / rel).resolve()
            top = rel.parts[0] if rel.parts else ""
            if top in ("bundles", "insights") and self.home in target.parents:
                return str(target)
            return str(self.home / "__denied__")
        return super().translate_path(path)

    def end_headers(self) -> None:
        if self.path.startswith("/data/"):
            self.send_header("Cache-Control", "no-store")
        super().end_headers()


def run(home: Path, host: str, port: int) -> None:
    if not (DIST / "index.html").exists():
        raise FileNotFoundError(f"{DIST} has no build: run `pnpm build` in {DIST.parent}")
    server = http.server.ThreadingHTTPServer((host, port), functools.partial(Handler, home=home))
    print(f"conv-atlas: http://{host}:{port}  (data: {home})", flush=True)
    with contextlib.suppress(KeyboardInterrupt):
        server.serve_forever()
