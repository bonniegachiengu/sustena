"""
Build the Sustena Lore site from the edited drafts in ``apps/api/lore_content``.

★★ **Why a build step and not a CMS.** These are authored documents, versioned
as files, reviewed as diffs. The existing ``lore_entries`` CMS is for
user-written entries with a draft/published lifecycle; putting eighteen essays
through it would mean converting prose into content blocks and losing the shape
of the thing on the way in. A file is already the right storage for an essay.

The output is plain, self-contained HTML: one page per essay, one index. No
JavaScript, no external requests, no fonts pulled from anywhere. An essay
should render on a bad connection in a browser that has been switched off for
five years.

Determinism: the same drafts produce byte-identical output, so a rebuild that
changes something says so in the diff.
"""

from __future__ import annotations

import html
import shutil
from dataclasses import dataclass
from pathlib import Path

from .render import Rendered, render
from .theme import CSS

CONTENT_DIR = Path(__file__).resolve().parent.parent.parent / "lore_content"
DIST_DIR = Path(__file__).resolve().parent / "dist"

SITE_NAME = "Sustena Lore"
SITE_BLURB = (
    "Eighteen essays on the ideas Sustena is built from — one per module, each "
    "starting somewhere that is not software."
)


class DraftError(ValueError):
    """A draft that cannot be published as written."""


@dataclass(frozen=True)
class Essay:
    number: int
    kicker: str
    title: str
    module: str
    dek: str
    date: str
    slug: str
    body: Rendered

    @property
    def path(self) -> str:
        return f"{self.number:03d}-{self.slug}.html"


REQUIRED = ("number", "kicker", "title", "module", "dek", "date")


def parse_draft(path: Path) -> Essay:
    """Split frontmatter from prose, then render the prose."""
    text = path.read_text(encoding="utf-8").replace("\r\n", "\n")
    if not text.startswith("---\n"):
        raise DraftError(f"{path.name}: no frontmatter block")
    end = text.index("\n---\n", 4)
    meta: dict[str, str] = {}
    for line in text[4:end].split("\n"):
        if not line.strip():
            continue
        if ":" not in line:
            raise DraftError(f"{path.name}: frontmatter line is not key: value -> {line!r}")
        k, v = line.split(":", 1)
        meta[k.strip()] = v.strip()

    missing = [k for k in REQUIRED if k not in meta]
    if missing:
        raise DraftError(f"{path.name}: frontmatter missing {', '.join(missing)}")

    body = render(text[end + 5 :], where=path.name)
    return Essay(
        number=int(meta["number"]),
        kicker=meta["kicker"],
        title=meta["title"],
        module=meta["module"],
        dek=meta["dek"],
        date=meta["date"],
        slug=path.stem.split("-", 1)[1],
        body=body,
    )


def load_all(content_dir: Path = CONTENT_DIR) -> list[Essay]:
    essays = [parse_draft(p) for p in sorted(content_dir.glob("*.md"))]
    numbers = [e.number for e in essays]
    if len(set(numbers)) != len(numbers):
        raise DraftError(f"two drafts claim the same number: {sorted(numbers)}")
    return sorted(essays, key=lambda e: e.number)


# -- pages ---------------------------------------------------------------------


def _shell(title: str, description: str, main: str) -> str:
    e = lambda s: html.escape(s, quote=True)  # noqa: E731
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{e(title)}</title>
<meta name="description" content="{e(description)}">
<meta property="og:title" content="{e(title)}">
<meta property="og:description" content="{e(description)}">
<meta property="og:site_name" content="{e(SITE_NAME)}">
<meta name="color-scheme" content="dark light">
<style>{CSS}</style>
</head>
<body>
<header class="top"><a class="wordmark" href="/">Sustena&nbsp;Lore</a></header>
{main}
<footer class="foot"><p>Sustena — the constant work of holding yourself
together, done out loud.</p></footer>
</body>
</html>
"""


def essay_page(essay: Essay) -> str:
    e = lambda s: html.escape(s, quote=False)  # noqa: E731
    contents = ""
    if len(essay.body.headings) > 2:
        items = "".join(
            f'<li><a href="#{a}">{e(t)}</a></li>' for a, t in essay.body.headings
        )
        contents = f'<nav class="contents"><ol>{items}</ol></nav>'
    return _shell(
        f"{essay.kicker} — {essay.title} · {SITE_NAME}",
        essay.dek,
        f"""<article>
<p class="eyebrow">{essay.number:02d} · {e(essay.module)} · {e(essay.kicker)}</p>
<h1>{e(essay.title)}</h1>
<p class="dek">{e(essay.dek)}</p>
{contents}
{essay.body.html}
<p class="backlink"><a href="/">← every essay</a></p>
</article>""",
    )


def index_page(essays: list[Essay]) -> str:
    e = lambda s: html.escape(s, quote=False)  # noqa: E731
    if essays:
        rows = "".join(
            f"""<li><a href="/{es.path}">
<span class="n">{es.number:02d}</span>
<span class="t"><strong>{e(es.kicker)}</strong> — {e(es.title)}</span>
<span class="d">{e(es.dek)}</span></a></li>"""
            for es in essays
        )
        listing = f'<ul class="essays">{rows}</ul>'
    else:
        # ★ The house empty state: the frame stays, one dim line, no CTA.
        listing = '<p class="empty">nothing published yet · the first essay is still being edited</p>'
    return _shell(
        SITE_NAME,
        SITE_BLURB,
        f"""<article class="home">
<h1>Sustena Lore</h1>
<p class="dek">{SITE_BLURB}</p>
{listing}
</article>""",
    )


def build(content_dir: Path = CONTENT_DIR, dist: Path = DIST_DIR) -> list[Essay]:
    essays = load_all(content_dir)
    if dist.exists():
        shutil.rmtree(dist)
    dist.mkdir(parents=True)
    for essay in essays:
        (dist / essay.path).write_text(essay_page(essay), encoding="utf-8", newline="\n")
    (dist / "index.html").write_text(index_page(essays), encoding="utf-8", newline="\n")
    return essays


if __name__ == "__main__":  # pragma: no cover
    built = build()
    print(f"built {len(built)} essay(s) into {DIST_DIR}")
    for essay in built:
        print(f"  {essay.number:02d}  {essay.kicker} — {essay.title}  ->  /{essay.path}")
