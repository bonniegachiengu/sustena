"""
Markdown -> HTML for Sustena Lore.

★★★ **Why this is hand-written rather than a library.** The input is not
arbitrary markdown from the internet; it is a closed set of authored essays
using a small, known subset. What matters most about rendering them is not
breadth of syntax support but that **nothing is ever silently dropped**. A
permissive library's failure mode is a paragraph that quietly does not appear;
this renderer's failure mode is a build that stops and names the line. For a
publication surface, loud is correct.

So every line must be consumed by a named rule. Anything unrecognised raises
``LoreSyntaxError`` with the file, the line number and the text. That is the
whole design.

The subset, surveyed from the 18 companions rather than guessed:

===========================  ==================================================
``---``                      a horizontal rule
``#### text``                a section heading (only level 4 is used)
``- item``                   a bullet, consecutive lines forming one list
``> text``                   a blockquote
``text``                     a paragraph
``**bold**  *italic*``       inline, nestable one level
```` `code` ````             inline code
``[text](https://...)``      a link; **http(s) only**, by design
===========================  ==================================================

No eval, no exec, no HTML passthrough: every character of the source is escaped
before any markup is inserted, so a draft cannot inject markup into the page
even by accident.
"""

from __future__ import annotations

import html
import re
from dataclasses import dataclass


class LoreSyntaxError(ValueError):
    """A line no rule claimed. Raised rather than skipped, on purpose."""

    def __init__(self, where: str, lineno: int, line: str) -> None:
        super().__init__(f"{where}:{lineno}: no rule for this line: {line!r}")
        self.where = where
        self.lineno = lineno
        self.line = line


@dataclass(frozen=True)
class Rendered:
    """A rendered essay: its HTML body, and the headings for a contents list."""

    html: str
    headings: list[tuple[str, str]]  # (anchor id, text)


# -- inline --------------------------------------------------------------------

# ★ Order matters and is deliberate: code first, so ``**`` inside backticks is
#   left alone; then bold before italic, so ``**x**`` is not read as two
#   italics.
_CODE = re.compile(r"`([^`]+)`")
_LINK = re.compile(r"\[([^\]]+)\]\((https?://[^)\s]+)\)")
_BOLD = re.compile(r"\*\*(.+?)\*\*", re.DOTALL)
_ITALIC = re.compile(r"(?<!\*)\*(?!\*)(.+?)(?<!\*)\*(?!\*)", re.DOTALL)


def inline(text: str) -> str:
    """Escape first, then add markup. Never the other way round."""
    out = html.escape(text, quote=False)
    # Backticks survive escaping, so code spans are still findable.
    out = _CODE.sub(lambda m: f"<code>{m.group(1)}</code>", out)
    # ★ The URL is re-escaped for an attribute, and the pattern itself only
    #   admits http(s) -- a `javascript:` href cannot be written here.
    out = _LINK.sub(
        lambda m: f'<a href="{html.escape(m.group(2), quote=True)}" '
        f'rel="noopener">{m.group(1)}</a>',
        out,
    )
    out = _BOLD.sub(lambda m: f"<strong>{m.group(1)}</strong>", out)
    out = _ITALIC.sub(lambda m: f"<em>{m.group(1)}</em>", out)
    return out


# -- block ---------------------------------------------------------------------

_HEADING = re.compile(r"^#{1,6}\s+(.*\S)\s*$")
_BULLET = re.compile(r"^[-*]\s+(.*\S)\s*$")
_QUOTE = re.compile(r"^>\s*(.*)$")
_RULE = re.compile(r"^-{3,}$")


def slug(text: str) -> str:
    """A stable anchor from a heading. Deterministic: same text, same id."""
    plain = re.sub(r"[*`_]", "", text).lower()
    plain = re.sub(r"[^a-z0-9]+", "-", plain).strip("-")
    return plain or "section"


def render(source: str, where: str = "<lore>") -> Rendered:
    """Render one essay. Raises :class:`LoreSyntaxError` on anything unknown."""
    lines = source.replace("\r\n", "\n").split("\n")
    out: list[str] = []
    headings: list[tuple[str, str]] = []
    i = 0
    seen: dict[str, int] = {}

    while i < len(lines):
        raw = lines[i]
        line = raw.rstrip()

        if not line.strip():
            i += 1
            continue

        if _RULE.match(line.strip()):
            out.append("<hr>")
            i += 1
            continue

        m = _HEADING.match(line)
        if m:
            text = m.group(1)
            anchor = slug(text)
            # ★ Two sections could share a title; ids must still be unique or
            #   a contents link would send a reader to the wrong one.
            seen[anchor] = seen.get(anchor, 0) + 1
            if seen[anchor] > 1:
                anchor = f"{anchor}-{seen[anchor]}"
            headings.append((anchor, re.sub(r"[*`]", "", text)))
            out.append(f'<h2 id="{anchor}">{inline(text)}</h2>')
            i += 1
            continue

        if _BULLET.match(line):
            items: list[str] = []
            while i < len(lines) and _BULLET.match(lines[i].rstrip()):
                items.append(inline(_BULLET.match(lines[i].rstrip()).group(1)))
                i += 1
            body = "".join(f"<li>{it}</li>" for it in items)
            out.append(f"<ul>{body}</ul>")
            continue

        if _QUOTE.match(line):
            parts: list[str] = []
            while i < len(lines) and _QUOTE.match(lines[i].rstrip()):
                parts.append(_QUOTE.match(lines[i].rstrip()).group(1))
                i += 1
            out.append(f"<blockquote><p>{inline(' '.join(parts).strip())}</p></blockquote>")
            continue

        if line.lstrip().startswith(("#", ">", "|", "```", "    ")):
            # Looks like markup, matched no rule. Refuse rather than render it
            # as prose -- this is exactly the silent-mangling case.
            raise LoreSyntaxError(where, i + 1, line)

        # A paragraph: this line and any that follow until a blank or a block.
        para: list[str] = []
        while i < len(lines):
            nxt = lines[i].rstrip()
            if not nxt.strip() or _RULE.match(nxt.strip()) or _HEADING.match(nxt):
                break
            if _BULLET.match(nxt) or _QUOTE.match(nxt):
                break
            para.append(nxt.strip())
            i += 1
        out.append(f"<p>{inline(' '.join(para))}</p>")

    return Rendered(html="\n".join(out), headings=headings)
