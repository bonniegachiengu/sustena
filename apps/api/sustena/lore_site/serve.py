"""
Serve the built Sustena Lore site as its own process.

★★★ **Why this exists, when main.py already dispatches the blog by Host.**
The Host-dispatch middleware is the destination and it is the better design:
one process, one watchdog, no second listener to forget about. But it only
takes effect when the application backend is next deployed, and the blog must
not have to wait for -- or force -- a deploy of an application that other
people are mid-change on. Publishing an essay should never mean shipping
somebody's half-finished engine work.

So this is a deliberately small second door: the same built pages, served by
nothing but a file reader, on its own port, pointed at by the lore hostnames.
It holds no database handle, imports no router, and cannot reach application
state even by accident -- the strongest form of "this cannot disturb the app"
is not being able to.

When the application backend is next deployed normally, the middleware in
main.py starts answering these hosts on its own. At that point the lore
ingress can be pointed back at the application port and this process retired,
with nothing to migrate: the pages are identical because they are the same
files.

Run:  python -m sustena.lore_site.serve [--port 9100]
"""

from __future__ import annotations

import argparse
from pathlib import Path

from starlette.applications import Starlette
from starlette.requests import Request
from starlette.responses import FileResponse, PlainTextResponse, Response
from starlette.routing import Route

DIST = Path(__file__).resolve().parent / "dist"


def _resolve(url_path: str) -> Path | None:
    """Map a URL path to a file inside dist, or None if it escapes or is absent."""
    rel = url_path.lstrip("/") or "index.html"
    if rel.endswith("/"):
        rel += "index.html"
    try:
        candidate = (DIST / rel).resolve()
    except (OSError, ValueError):
        return None
    # ★ Containment first, existence second. A traversal must be refused even
    #   when it happens to name a real file.
    if not candidate.is_relative_to(DIST.resolve()) or not candidate.is_file():
        return None
    return candidate


async def handler(request: Request) -> Response:
    # ★ The blog is read-only. Anything that is not a read is refused before a
    #   path is even looked at.
    if request.method not in ("GET", "HEAD"):
        return PlainTextResponse("method not allowed", status_code=405)

    if not DIST.is_dir():
        return PlainTextResponse("the lore site has not been built", status_code=503)

    found = _resolve(request.url.path)
    if found is None:
        # The index doubles as the 404 page: a reader who mistypes still lands
        # somewhere with every essay on it.
        return FileResponse(DIST / "index.html", status_code=404)
    return FileResponse(found)


app = Starlette(
    routes=[Route("/{path:path}", handler, methods=["GET", "HEAD", "POST", "PUT", "DELETE"])]
)


def main() -> None:  # pragma: no cover
    import uvicorn

    ap = argparse.ArgumentParser(description="Serve the built Sustena Lore site.")
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=9100)
    args = ap.parse_args()

    if not DIST.is_dir():
        raise SystemExit(
            "dist/ is missing -- run `python -m sustena.lore_site.build` first."
        )
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")


if __name__ == "__main__":  # pragma: no cover
    main()
