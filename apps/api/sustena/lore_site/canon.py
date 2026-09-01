"""
Build the canon reader: all 36 article files, paired, as a local reading site.

*** Why this is separate from the published site, and stays local.
``build.py`` publishes ``lore_content/`` -- eighteen edited companions that have
been cleared for release. The 36 files under ``Articles (Serious)/`` are a
different thing: ``.gitignore`` records them as STAGED publication, held back
"until each has been cleaned of private, business, and economy-layer material
and released in phases". They are therefore readable, not publishable, and this
builder writes OUTSIDE the repository so that a build can never be committed by
accident, and serves on a port the tunnel does not map so it can never be
reached from the internet.

If and when an article is cleared, it moves into ``lore_content/`` and goes out
through ``build.py`` like everything else. This reader is for reading.

The pairing is parsed from ``Articles (Serious)/INDEX.md`` rather than
hardcoded, because that file calls itself the authoritative map. If a pair is
renamed there and not here, the build fails loudly rather than quietly reading
a stale table.

Run:  python -m sustena.lore_site.canon
"""

from __future__ import annotations

import html
import re
import shutil
from dataclasses import dataclass
from pathlib import Path

from .canon_render import Rendered, render
from .theme import CSS

SITE_NAME = "Sustena Canon"
SITE_BLURB = (
    "The eighteen pairs, in full: one formal paper and one companion essay per "
    "module. Local reading copy — not published."
)

# The articles are gitignored, so they live in whichever working tree has them.
_ARTICLE_CANDIDATES = (
    Path(__file__).resolve().parents[3] / "Articles (Serious)",
    Path("C:/Users/DELL/dev/sustena/Articles (Serious)"),
)

DEFAULT_OUT = Path(__file__).resolve().parents[4].parent / "_canon-reader"

_ROW = re.compile(
    r"^\|\s*(\d+)\s*\|\s*([^|]+?)\s*\|\s*`([^`]+)`\s*\|\s*`([^`]+)`\s*\|\s*$"
)
_EMPH_LINE = re.compile(r"^\s*(?:\*\*(.+?)\*\*|\*(.+?)\*)\s*$")


class CanonError(ValueError):
    """Something about the canon on disk does not line up."""


def articles_dir() -> Path:
    for c in _ARTICLE_CANDIDATES:
        if (c / "technical").is_dir() and (c / "inspirations").is_dir():
            return c
    raise CanonError(
        "cannot find 'Articles (Serious)' with technical/ and inspirations/ in: "
        + ", ".join(str(c) for c in _ARTICLE_CANDIDATES)
    )


@dataclass(frozen=True)
class Half:
    kind: str  # "technical" | "inspiration"
    number: int
    module: str
    kicker: str
    title: str
    dek: str
    body: Rendered
    words: int

    @property
    def slug(self) -> str:
        return f"{self.number:02d}-{self.kind[:4]}"

    @property
    def path(self) -> str:
        return f"{self.slug}.html"


@dataclass(frozen=True)
class Pair:
    number: int
    module: str
    technical: Half
    inspiration: Half


def _norm(s: str) -> str:
    return re.sub(r"[^a-z0-9]+", "", s.lower())


def _peel(text: str) -> tuple[list[str], str]:
    """Lift the leading emphasis lines (title/subtitle) off the top of a file."""
    lines = text.replace("\r\n", "\n").split("\n")
    peeled: list[str] = []
    i = 0
    while i < len(lines) and len(peeled) < 2:
        line = lines[i].strip()
        if not line:
            i += 1
            continue
        m = _EMPH_LINE.match(line)
        if not m:
            break
        peeled.append((m.group(1) or m.group(2)).strip())
        i += 1
    # A rule immediately after the title block is decoration, not content.
    while i < len(lines) and (not lines[i].strip() or re.fullmatch(r"-{3,}", lines[i].strip())):
        if lines[i].strip() and not re.fullmatch(r"-{3,}", lines[i].strip()):
            break
        i += 1
    return peeled, "\n".join(lines[i:])


def _split_title(stem: str, kind: str) -> tuple[str, str]:
    """(kicker, title) from a filename stem."""
    if kind == "technical":
        m = re.match(r"^3B1B\s*[-\u2014]+\s*(.+?)\s*-\s*Formal Execution$", stem)
        module = m.group(1).strip() if m else stem
        return module, "Formal Execution"
    if "\u2014" in stem:
        left, right = stem.split("\u2014", 1)
        return left.strip(), right.strip()
    return "", stem.strip()


def load_half(path: Path, kind: str, number: int, module: str) -> Half:
    raw = path.read_text(encoding="utf-8")
    peeled, body_src = _peel(raw)
    kicker, title = _split_title(path.stem, kind)
    # Drop a peeled line that merely repeats the title we already show.
    deks = [p for p in peeled if _norm(p) not in (_norm(title), _norm(path.stem))]
    body = render(body_src, where=path.name)
    return Half(
        kind=kind,
        number=number,
        module=module,
        kicker=kicker,
        title=title,
        dek=" \u00b7 ".join(deks),
        body=body,
        words=len(re.sub(r"<[^>]+>", " ", body.html).split()),
    )


def load_pairs(root: Path | None = None) -> list[Pair]:
    root = root or articles_dir()
    index = root / "INDEX.md"
    if not index.is_file():
        raise CanonError(f"no INDEX.md in {root}")

    pairs: list[Pair] = []
    for line in index.read_text(encoding="utf-8").split("\n"):
        m = _ROW.match(line.strip())
        if not m:
            continue
        number, module, tech_name, insp_name = (
            int(m.group(1)),
            m.group(2).strip(),
            m.group(3).strip(),
            m.group(4).strip(),
        )
        tech = root / "technical" / tech_name
        insp = root / "inspirations" / insp_name
        # Loud, not quiet: a stale index must stop the build.
        for p in (tech, insp):
            if not p.is_file():
                raise CanonError(f"INDEX.md row {number} names a file that is not on disk: {p}")
        pairs.append(
            Pair(
                number=number,
                module=module,
                technical=load_half(tech, "technical", number, module),
                inspiration=load_half(insp, "inspiration", number, module),
            )
        )
    if not pairs:
        raise CanonError("INDEX.md has no pair rows this build understands")
    return pairs


# -- pages ---------------------------------------------------------------------

EXTRA_CSS = """
:root { --measure: 74ch; }
h3 { font-size: 1.06rem; margin: 2.2rem 0 0.7rem; font-weight: 650; }
h4 { font-size: 0.97rem; margin: 1.9rem 0 0.6rem; font-weight: 650; color: var(--ink); }
pre {
  background: var(--raised);
  border: 1px solid var(--rule);
  border-radius: 5px;
  padding: 0.85rem 1rem;
  overflow-x: auto;
  margin: 0 0 1.35rem;
  font-size: 0.83rem;
  line-height: 1.6;
}
pre code { background: none; padding: 0; border-radius: 0; font-size: 1em; }
.math {
  font: 0.93em/1.5 ui-monospace, SFMono-Regular, "DM Mono", Menlo, monospace;
  color: var(--amber);
  overflow-wrap: break-word;
}
.math.display {
  display: block;
  white-space: pre-wrap;
  background: var(--raised);
  border-left: 2px solid var(--amber);
  padding: 0.8rem 1rem;
  margin: 0 0 1.35rem;
  overflow-x: auto;
  font-size: 0.86rem;
}
.tablewrap { overflow-x: auto; margin: 0 0 1.35rem; }
table { border-collapse: collapse; width: 100%; font-size: 0.92rem; }
th, td {
  border: 1px solid var(--rule);
  padding: 0.5rem 0.7rem;
  text-align: left;
  vertical-align: top;
}
th { color: var(--ink-soft); font-weight: 650; background: var(--raised); }

.pairs { list-style: none; padding: 0; margin: 2.5rem 0 0; }
.pairs > li { border-top: 1px solid var(--rule); padding: 1.25rem 0.2rem; }
.pairs > li:last-child { border-bottom: 1px solid var(--rule); }
.pairhead {
  font: 500 0.72rem/1 ui-monospace, SFMono-Regular, "DM Mono", Menlo, monospace;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--ink-dim);
  margin: 0 0 0.75rem;
}
.halves { display: grid; grid-template-columns: 1fr 1fr; gap: 0.6rem; }
@media (max-width: 40rem) { .halves { grid-template-columns: 1fr; } }
.halves a {
  display: block;
  text-decoration: none;
  border: 1px solid var(--rule);
  border-radius: 5px;
  padding: 0.8rem 0.9rem;
}
.halves a:hover { background: var(--raised); border-color: var(--amber); }
.halves .lbl {
  display: block;
  font: 500 0.66rem/1 ui-monospace, SFMono-Regular, "DM Mono", Menlo, monospace;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--ink-dim);
  margin-bottom: 0.4rem;
}
.halves .ttl { display: block; font-size: 1rem; line-height: 1.4; }
.halves .meta { display: block; color: var(--ink-dim); font-size: 0.8rem; margin-top: 0.35rem; }
.companion {
  border: 1px solid var(--rule);
  border-radius: 5px;
  padding: 0.85rem 1rem;
  margin: 0 0 2.2rem;
}
.companion a { color: var(--amber); }
.companion .lbl {
  display: block;
  font: 500 0.66rem/1 ui-monospace, SFMono-Regular, "DM Mono", Menlo, monospace;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--ink-dim);
  margin-bottom: 0.35rem;
}
.notice {
  border-left: 2px solid var(--amber);
  padding: 0.7rem 1rem;
  margin: 1.6rem 0 0;
  color: var(--ink-soft);
  font-size: 0.9rem;
}
/* No max-height here on purpose: a nested scrollbox eats the page's own
   scroll whenever the pointer happens to be over it, which is most of the
   time on a contents list this tall. A long list is the lesser evil. */
.contents ol { columns: 1; }
.contents li.lvl-2 { font-weight: 650; margin-top: 0.5rem; }
.contents li.lvl-4 { padding-left: 1.1rem; font-size: 0.88rem; }
"""


def _shell(title: str, description: str, main: str) -> str:
    e = lambda s: html.escape(s, quote=True)  # noqa: E731
    return (
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n"
        '<meta name="viewport" content="width=device-width, initial-scale=1">\n'
        f"<title>{e(title)}</title>\n"
        f'<meta name="description" content="{e(description)}">\n'
        '<meta name="robots" content="noindex, nofollow">\n'
        '<meta name="color-scheme" content="dark light">\n'
        f"<style>{CSS}{EXTRA_CSS}</style>\n</head>\n<body>\n"
        # Relative, not root-absolute: the same build then works both from the
        # local server and from a plain double-click on index.html, which needs
        # no process running at all.
        '<header class="top"><a class="wordmark" href="index.html">Sustena&nbsp;Canon</a></header>\n'
        f"{main}\n"
        '<footer class="foot"><p>Local reading copy of the canon. Not published '
        "\u2014 these files are staged for release and not yet cleared.</p></footer>\n"
        "</body>\n</html>\n"
    )


def half_page(half: Half, other: Half) -> str:
    e = lambda s: html.escape(s, quote=False)  # noqa: E731
    label = "Formal paper" if half.kind == "technical" else "Companion essay"
    other_label = "Formal paper" if other.kind == "technical" else "Companion essay"

    contents = ""
    if len(half.body.headings) > 2:
        items = "".join(
            f'<li class="lvl-{lvl}"><a href="#{a}">{e(t)}</a></li>'
            for lvl, a, t in half.body.headings
        )
        contents = f'<nav class="contents"><ol>{items}</ol></nav>'

    name = f"{half.kicker} \u2014 {half.title}" if half.kicker else half.title
    other_name = f"{other.kicker} \u2014 {other.title}" if other.kicker else other.title
    dek = f'<p class="dek">{e(half.dek)}</p>' if half.dek else ""

    return _shell(
        f"{name} \u00b7 {SITE_NAME}",
        half.dek or name,
        f"""<article>
<p class="eyebrow">{half.number:02d} \u00b7 {e(half.module)} \u00b7 {label}</p>
<h1>{e(name)}</h1>
{dek}
<div class="companion"><span class="lbl">The other half of this pair</span>
<a href="{other.path}">{other_label}: {e(other_name)}</a> \u00b7 {other.words:,} words</div>
{contents}
{half.body.html}
<p class="backlink"><a href="index.html">\u2190 all eighteen pairs</a></p>
</article>""",
    )


def index_page(pairs: list[Pair]) -> str:
    e = lambda s: html.escape(s, quote=False)  # noqa: E731
    rows = []
    for p in pairs:
        t, i = p.technical, p.inspiration
        i_name = f"{i.kicker} \u2014 {i.title}" if i.kicker else i.title
        rows.append(
            f"""<li>
<p class="pairhead">{p.number:02d} \u00b7 {e(p.module)}</p>
<div class="halves">
<a href="{i.path}"><span class="lbl">Companion essay</span>
<span class="ttl">{e(i_name)}</span><span class="meta">{i.words:,} words</span></a>
<a href="{t.path}"><span class="lbl">Formal paper</span>
<span class="ttl">{e(t.module)} \u2014 Formal Execution</span>
<span class="meta">{t.words:,} words</span></a>
</div></li>"""
        )
    total = sum(p.technical.words + p.inspiration.words for p in pairs)
    return _shell(
        SITE_NAME,
        SITE_BLURB,
        f"""<article class="home">
<h1>Sustena Canon</h1>
<p class="dek">{SITE_BLURB}</p>
<p class="notice">Start with a companion essay on the left, then read its formal
paper on the right \u2014 each pair is the same idea told twice, once from
somewhere that is not software and once as mathematics. {len(pairs)} pairs,
{len(pairs) * 2} files, {total:,} words.</p>
<ul class="pairs">{"".join(rows)}</ul>
</article>""",
    )


def build(root: Path | None = None, out: Path | None = None) -> list[Pair]:
    pairs = load_pairs(root)
    out = out or DEFAULT_OUT
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    for p in pairs:
        (out / p.technical.path).write_text(
            half_page(p.technical, p.inspiration), encoding="utf-8", newline="\n"
        )
        (out / p.inspiration.path).write_text(
            half_page(p.inspiration, p.technical), encoding="utf-8", newline="\n"
        )
    (out / "index.html").write_text(index_page(pairs), encoding="utf-8", newline="\n")
    return pairs


if __name__ == "__main__":  # pragma: no cover
    built = build()
    where = DEFAULT_OUT
    words = sum(p.technical.words + p.inspiration.words for p in built)
    print(f"built {len(built)} pairs ({len(built) * 2} files, {words:,} words) into {where}")
    for p in built:
        print(f"  {p.number:02d}  {p.module:<12} {p.inspiration.path}  |  {p.technical.path}")
