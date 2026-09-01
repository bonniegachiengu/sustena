"""
Serve the built canon reader, on loopback only.

*** Why this refuses to be reachable from the internet.
The 36 articles are staged for release but not yet cleared (see ``canon.py``
and the ``.gitignore`` entry). So this server is deliberately hard to expose:
it defaults to 127.0.0.1, it serves a directory that lives outside the
repository, and it listens on a port the cloudflared ingress does not map --
the tunnel routes the lore hostnames to 9100, and this is 9101.

Like ``serve.py`` it is a file reader and nothing else: no database handle, no
router, no application import. It cannot disturb the app or the published blog
even by accident, because it cannot reach them.

Run:  python -m sustena.lore_site.canon_serve [--port 9101]
"""

from __future__ import annotations

import argparse
from pathlib import Path

from starlette.applications import Starlette
from starlette.requests import Request
from starlette.responses import FileResponse, PlainTextResponse, Response
from starlette.routing import Route

from .canon import DEFAULT_OUT

DIST = DEFAULT_OUT


def _resolve(url_path: str) -> Path | None:
    """Map a URL path to a file inside dist, or None if it escapes or is absent."""
    rel = url_path.lstrip("/") or "index.html"
    if rel.endswith("/"):
        rel += "index.html"
    try:
        candidate = (DIST / rel).resolve()
    except (OSError, ValueError):
        return None
    # Containment first, existence second. A traversal must be refused even
    # when it happens to name a real file.
    if not candidate.is_relative_to(DIST.resolve()) or not candidate.is_file():
        return None
    return candidate


async def handler(request: Request) -> Response:
    # The reader is read-only. Anything that is not a read is refused before a
    # path is even looked at.
    if request.method not in ("GET", "HEAD"):
        return PlainTextResponse("method not allowed", status_code=405)

    if not DIST.is_dir():
        return PlainTextResponse(
            "the canon reader has not been built -- run "
            "`python -m sustena.lore_site.canon` first",
            status_code=503,
        )

    found = _resolve(request.url.path)
    if found is None:
        # The index doubles as the 404 page: a reader who mistypes still lands
        # somewhere with every article on it.
        return FileResponse(DIST / "index.html", status_code=404)
    return FileResponse(found)


app = Starlette(
    routes=[Route("/{path:path}", handler, methods=["GET", "HEAD", "POST", "PUT", "DELETE"])]
)


def main() -> None:  # pragma: no cover
    import uvicorn

    ap = argparse.ArgumentParser(description="Serve the built Sustena canon reader.")
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=9101)
    args = ap.parse_args()

    if not DIST.is_dir():
        raise SystemExit("dist is missing -- run `python -m sustena.lore_site.canon` first.")
    uvicorn.run(app, host=args.host, port=args.port, log_level="warning")


if __name__ == "__main__":  # pragma: no cover
    main()
