"""
Markdown -> HTML for the canon reader (all 36 article files).

*** Why this is a second renderer and not a widening of render.py.
``render.py`` serves the *public* site, and its strictness is a feature there:
it refuses code fences, tables and maths so that a draft containing them cannot
be published half-rendered. Those three constructs are exactly what the 18
technical papers are made of. Widening render.py would relax the guarantee that
protects the published surface in order to serve a private one -- so the two
surfaces get two renderers, and the published one keeps its narrow mouth.

What is kept from render.py, deliberately, is the part that matters most:
nothing is ever silently dropped. Every line is claimed by a named rule, and
the paragraph rule is total -- a line that looks like nothing in particular is
still shown as prose rather than skipped, so text cannot vanish. The shapes
that cannot be rendered as anything at all (a table row with no separator row
under it) raise ``CanonSyntaxError`` naming the file and the line. A reader
must never wonder whether a paragraph was in the source.

The subset, surveyed from the 36 files rather than guessed:

  ``# .. ######``       headings (levels 1, 2 and 4 occur)
  ``$$ .. $$``          display maths, held as source
  ``$x$``               inline maths, held as source
  ``` fence            a code block, with optional language
  four-space indent     an indented code block
  ``| a | b |``         a table, with its separator row
  ``1. item``           an ordered list
  ``- item``            a bullet list
  ``> text``            a blockquote
  ``---``               a horizontal rule
  ``text``              a paragraph

Maths is preserved, not evaluated. The site is self-contained -- no JavaScript
and no external requests -- so there is no typesetter to hand LaTeX to.
Rendering it as styled source is the honest option: the notation is shown
exactly as written rather than approximated into something that might read as a
different claim. Nothing here executes, fetches, or interprets the maths.

No eval, no exec, no HTML passthrough: every character of the source is escaped
before any markup is inserted.
"""

from __future__ import annotations

import html
import re
from dataclasses import dataclass, field


class CanonSyntaxError(ValueError):
    """A line no rule claimed. Raised rather than skipped, on purpose."""

    def __init__(self, where: str, lineno: int, line: str) -> None:
        super().__init__(f"{where}:{lineno}: no rule for this line: {line!r}")
        self.where = where
        self.lineno = lineno
        self.line = line


@dataclass(frozen=True)
class Rendered:
    html: str
    headings: list[tuple[int, str, str]] = field(default_factory=list)


# -- inline --------------------------------------------------------------------

_CODE = re.compile(r"`([^`\n]+)`")
_IMATH = re.compile(r"(?<!\\)\$([^$\n]+?)(?<!\\)\$")
_LINK = re.compile(r"\[([^\]]+)\]\((https?://[^)\s]+)\)")
_BOLD = re.compile(r"\*\*(.+?)\*\*", re.DOTALL)
_ITALIC = re.compile(r"(?<!\*)\*(?!\*)(.+?)(?<!\*)\*(?!\*)", re.DOTALL)
_SENTINEL = re.compile("\x00(\\d+)\x00")

# -- maths presentation --------------------------------------------------------
#
# There is no typesetter here (no JavaScript, no external requests), so the
# choice is between showing LaTeX source and showing Unicode. Source is honest
# but `\Sigma \in \mathrm{State}` reads badly 3,827 times across 150,000 words.
#
# The compromise: substitute only what is unambiguous -- a symbol command with
# exactly one Unicode counterpart, a font wrapper that adds nothing in plain
# text, a sub/superscript. Anything structural (\frac, \underbrace, matrices)
# is left as written rather than approximated into something that could read as
# a different claim. The untouched original is kept in the title attribute, so
# nothing is destroyed: hovering any expression shows exactly what was authored.

_SYMBOLS = {
    "Sigma": "\u03a3", "sigma": "\u03c3", "Gamma": "\u0393", "gamma": "\u03b3",
    "Delta": "\u0394", "delta": "\u03b4", "Theta": "\u0398", "theta": "\u03b8",
    "Lambda": "\u039b", "lambda": "\u03bb", "Pi": "\u03a0", "pi": "\u03c0",
    "Phi": "\u03a6", "phi": "\u03c6", "varphi": "\u03c6", "Psi": "\u03a8",
    "psi": "\u03c8", "Omega": "\u03a9", "omega": "\u03c9", "alpha": "\u03b1",
    "beta": "\u03b2", "epsilon": "\u03b5", "varepsilon": "\u03b5",
    "zeta": "\u03b6", "eta": "\u03b7", "iota": "\u03b9", "kappa": "\u03ba",
    "mu": "\u03bc", "nu": "\u03bd", "xi": "\u03be", "rho": "\u03c1",
    "tau": "\u03c4", "upsilon": "\u03c5", "chi": "\u03c7",
    "langle": "\u27e8", "rangle": "\u27e9", "oplus": "\u2295",
    "otimes": "\u2297", "wedge": "\u2227", "land": "\u2227", "vee": "\u2228",
    "lor": "\u2228", "lnot": "\u00ac", "neg": "\u00ac", "vdash": "\u22a2",
    "models": "\u22a8", "in": "\u2208", "notin": "\u2209", "ni": "\u220b",
    "subseteq": "\u2286", "subset": "\u2282", "supseteq": "\u2287",
    "supset": "\u2283", "cup": "\u222a", "cap": "\u2229",
    "setminus": "\u2216", "emptyset": "\u2205", "varnothing": "\u2205",
    "forall": "\u2200", "exists": "\u2203", "nexists": "\u2204",
    "rightarrow": "\u2192", "to": "\u2192", "leftarrow": "\u2190",
    "Rightarrow": "\u21d2", "Leftarrow": "\u21d0", "leftrightarrow": "\u2194",
    "Leftrightarrow": "\u21d4", "iff": "\u27fa", "implies": "\u27f9",
    "mapsto": "\u21a6", "uparrow": "\u2191", "downarrow": "\u2193",
    "le": "\u2264", "leq": "\u2264", "ge": "\u2265", "geq": "\u2265",
    "neq": "\u2260", "ne": "\u2260", "equiv": "\u2261", "approx": "\u2248",
    "sim": "\u223c", "simeq": "\u2243", "cong": "\u2245", "propto": "\u221d",
    "times": "\u00d7", "div": "\u00f7", "pm": "\u00b1", "mp": "\u2213",
    "cdot": "\u00b7", "cdots": "\u22ef", "ldots": "\u2026", "dots": "\u2026",
    "vdots": "\u22ee", "circ": "\u2218", "bullet": "\u2022", "star": "\u22c6",
    "sum": "\u2211", "prod": "\u220f", "int": "\u222b", "sqrt": "\u221a",
    "infty": "\u221e", "partial": "\u2202", "nabla": "\u2207",
    "mid": "|", "parallel": "\u2225", "perp": "\u22a5", "angle": "\u2220",
    "top": "\u22a4", "bot": "\u22a5", "aleph": "\u2135", "hbar": "\u210f",
    "ell": "\u2113", "Re": "\u211c", "Im": "\u2111", "prime": "\u2032",
    "degree": "\u00b0", "therefore": "\u2234", "because": "\u2235",
    "max": "max", "min": "min", "log": "log", "exp": "exp", "lim": "lim",
    # Found by grepping the built pages for commands that survived, rather than
    # guessed: these are the ones the articles actually reach for.
    "llbracket": "⟦", "rrbracket": "⟧",
    "longrightarrow": "⟶", "Longrightarrow": "⟹",
    "longleftarrow": "⟵", "longmapsto": "⟼",
    "hookrightarrow": "↪", "leadsto": "⇝", "nrightarrow": "↛",
    "sqsubseteq": "⊑", "sqsupseteq": "⊒", "sqcup": "⊔",
    "sqcap": "⊓", "preceq": "⪯", "succeq": "⪰",
    "prec": "≺", "succ": "≻", "ll": "≪", "gg": "≫",
    "subsetneq": "⊊", "supsetneq": "⊋", "vDash": "⊨",
    "dashv": "⊣", "triangleq": "≜", "coloneqq": "≔",
    "doteq": "≐", "asymp": "≍", "bigcup": "⋃",
    "bigcap": "⋂", "bigoplus": "⨁", "bigotimes": "⨂",
    "lceil": "⌈", "rceil": "⌉", "lfloor": "⌊", "rfloor": "⌋",
    "odot": "⊙", "oslash": "⊘", "uplus": "⊎", "amalg": "⨿",
    "wp": "℘", "bigwedge": "⋀", "bigvee": "⋁", "square": "□",
    "ast": "∗", "rightharpoonup": "⇀", "leftharpoonup": "↼",
    # Operator names: the backslash is markup, the word is the thing.
    "Pr": "Pr", "det": "det", "arg": "arg", "sup": "sup", "inf": "inf",
    "deg": "deg", "dim": "dim", "ker": "ker", "gcd": "gcd", "mod": "mod",
    "sin": "sin", "cos": "cos", "tan": "tan", "ln": "ln",
}

# Font/semantic wrappers that contribute nothing once the maths is plain text.
_WRAPPER = re.compile(
    r"\\(?:mathrm|mathbf|mathcal|mathsf|mathit|mathbb|mathtt|textrm|textbf|textit"
    r"|texttt|textsf|textsc|emph|operatorname|text|bm|boldsymbol)\s*\{([^{}]*)\}"
)
_SIZERS = re.compile(
    r"\\(?:left|right|bigl|bigr|Bigl|Bigr|biggl|biggr|big|Big|bigg|Bigg"
    r"|textstyle|displaystyle|limits|nolimits)\b"
)
_SPACERS = re.compile(r"\\(?:qquad|quad|,|;|:|!|\s)")
_CMD = re.compile(r"\\([a-zA-Z]+)")
_SUB = re.compile(r"_\{([^{}]*)\}|_(\w)")
_SUP = re.compile(r"\^\{([^{}]*)\}|\^(\w)")


def prettify_math(src: str, block: bool = False) -> str:
    """LaTeX -> readable HTML for the subset that maps unambiguously.

    Returns escaped HTML. Structural commands survive as visible source, which
    is the intended failure mode: shown as written rather than guessed at.
    """
    out = html.escape(src, quote=False)

    for _ in range(4):  # unwrap nested wrappers a level at a time
        new = _WRAPPER.sub(lambda m: m.group(1), out)
        if new == out:
            break
        out = new

    out = _SIZERS.sub("", out)
    out = _SPACERS.sub(" ", out)
    out = _CMD.sub(lambda m: _SYMBOLS.get(m.group(1), m.group(0)), out)
    out = _SUB.sub(lambda m: "<sub>" + (m.group(1) or m.group(2)) + "</sub>", out)
    out = _SUP.sub(lambda m: "<sup>" + (m.group(1) or m.group(2)) + "</sup>", out)

    if block:
        out = out.replace("\\\\", "\n").replace("&amp;", " ")
    else:
        out = re.sub(r"[ \t]+", " ", out).strip()
    return out


def inline(text: str) -> str:
    """Escape first, then add markup. Never the other way round.

    Code spans and maths are lifted out *before* emphasis runs, so a ``*``
    inside them is never mistaken for markup. They come back last.
    """
    held: list[str] = []

    def hold(fragment: str) -> str:
        held.append(fragment)
        return "\x00" + str(len(held) - 1) + "\x00"

    # Code first: a dollar inside backticks is code, not maths.
    text = _CODE.sub(lambda m: hold("<code>" + html.escape(m.group(1), quote=False) + "</code>"), text)
    text = _IMATH.sub(
        lambda m: hold(
            '<span class="math" title="'
            + html.escape(m.group(1), quote=True)
            + '">'
            + prettify_math(m.group(1))
            + "</span>"
        ),
        text,
    )

    out = html.escape(text, quote=False)
    out = _LINK.sub(
        lambda m: '<a href="' + html.escape(m.group(2), quote=True) + '" rel="noopener">' + m.group(1) + "</a>",
        out,
    )
    out = _BOLD.sub(lambda m: "<strong>" + m.group(1) + "</strong>", out)
    out = _ITALIC.sub(lambda m: "<em>" + m.group(1) + "</em>", out)
    return _SENTINEL.sub(lambda m: held[int(m.group(1))], out)


# -- block ---------------------------------------------------------------------

_HEADING = re.compile(r"^(#{1,6})\s+(.*\S)\s*$")
_BULLET = re.compile(r"^\s{0,3}[-*]\s+(.*\S)\s*$")
_ORDERED = re.compile(r"^\s{0,3}(\d+)[.)]\s+(.*\S)\s*$")
_QUOTE = re.compile(r"^>\s?(.*)$")
_RULE = re.compile(r"^\s{0,3}(?:-{3,}|\*{3,}|_{3,})\s*$")
_FENCE = re.compile(r"^\s{0,3}```+\s*([A-Za-z0-9_+-]*)\s*$")
_TABLE_SEP = re.compile(r"^\s*\|?[\s:|-]+\|[\s:|-]*$")
# A line that is nothing but bold text. The papers use this, not ATX, as their
# section marker -- most have zero `#` headings and twenty-odd of these. Read
# literally they are paragraphs, and a 70,000-character paper then has no
# structure at all; so they are promoted to headings, which is what they are.
_BOLD_LINE = re.compile(r"^\s{0,3}\*\*([^*].*?)\*\*\s*$")

_HLEVEL = {1: 2, 2: 3, 3: 4, 4: 4, 5: 5, 6: 6}


def slug(text: str) -> str:
    plain = re.sub(r"[*`_$\\]", "", text).lower()
    plain = re.sub(r"[^a-z0-9]+", "-", plain).strip("-")
    return plain or "section"


def _cells(row: str) -> list[str]:
    row = row.strip()
    if row.startswith("|"):
        row = row[1:]
    if row.endswith("|"):
        row = row[:-1]
    return [c.strip() for c in row.split("|")]


def render(source: str, where: str = "<canon>") -> Rendered:
    """Render one article. Raises :class:`CanonSyntaxError` on anything unknown."""
    lines = source.replace("\r\n", "\n").split("\n")
    out: list[str] = []
    headings: list[tuple[int, str, str]] = []
    seen: dict[str, int] = {}
    i = 0

    while i < len(lines):
        line = lines[i].rstrip()

        if not line.strip():
            i += 1
            continue

        # -- fenced code -------------------------------------------------------
        m = _FENCE.match(line)
        if m:
            lang = m.group(1)
            i += 1
            body: list[str] = []
            while i < len(lines) and not _FENCE.match(lines[i].rstrip()):
                body.append(lines[i])
                i += 1
            # An unterminated fence runs to end of file rather than raising: the
            # content is still shown, which beats refusing the whole article.
            if i < len(lines):
                i += 1
            cls = ' class="lang-' + html.escape(lang, quote=True) + '"' if lang else ""
            code = html.escape("\n".join(body), quote=False)
            out.append("<pre" + cls + "><code>" + code + "</code></pre>")
            continue

        # -- display maths -----------------------------------------------------
        if line.strip().startswith("$$"):
            stripped = line.strip()
            inner = stripped[2:]
            if inner.endswith("$$") and len(stripped) > 4:
                body_text = inner[:-2]
                i += 1
            else:
                buf: list[str] = [inner] if inner else []
                i += 1
                while i < len(lines) and "$$" not in lines[i]:
                    buf.append(lines[i])
                    i += 1
                if i < len(lines):
                    tail = lines[i].rstrip()
                    cut = tail.find("$$")
                    if cut > 0:
                        buf.append(tail[:cut])
                    i += 1
                body_text = "\n".join(buf)
            out.append(
                '<div class="math display" title="'
                + html.escape(body_text.strip(), quote=True)
                + '">'
                + prettify_math(body_text.strip(), block=True)
                + "</div>"
            )
            continue

        # -- table -------------------------------------------------------------
        if line.lstrip().startswith("|") and i + 1 < len(lines) and _TABLE_SEP.match(lines[i + 1]):
            head = _cells(line)
            i += 2
            rows: list[list[str]] = []
            while i < len(lines) and lines[i].lstrip().startswith("|"):
                rows.append(_cells(lines[i]))
                i += 1
            th = "".join("<th>" + inline(c) + "</th>" for c in head)
            body_html = "".join(
                "<tr>" + "".join("<td>" + inline(c) + "</td>" for c in r) + "</tr>" for r in rows
            )
            out.append(
                '<div class="tablewrap"><table><thead><tr>'
                + th
                + "</tr></thead><tbody>"
                + body_html
                + "</tbody></table></div>"
            )
            continue

        if _RULE.match(line):
            out.append("<hr>")
            i += 1
            continue

        # -- heading -----------------------------------------------------------
        m = _HEADING.match(line)
        if m:
            level = len(m.group(1))
            text = m.group(2)
            anchor = slug(text)
            seen[anchor] = seen.get(anchor, 0) + 1
            if seen[anchor] > 1:
                anchor = anchor + "-" + str(seen[anchor])
            headings.append((level, anchor, re.sub(r"[*`$\\]", "", text)))
            tag = "h" + str(_HLEVEL[level])
            out.append("<" + tag + ' id="' + anchor + '">' + inline(text) + "</" + tag + ">")
            i += 1
            continue

        # -- bold-line section marker ------------------------------------------
        mb = _BOLD_LINE.match(line)
        if mb:
            text = mb.group(1).strip()
            anchor = slug(text)
            seen[anchor] = seen.get(anchor, 0) + 1
            if seen[anchor] > 1:
                anchor = anchor + "-" + str(seen[anchor])
            # A short marker is a section; a long one is an emphatic sentence.
            # Both render as a heading, but only the short one earns a contents
            # entry, so the contents list stays a map rather than a transcript.
            if len(text) <= 100:
                headings.append((3, anchor, re.sub(r"[*`$\\]", "", text)))
            out.append('<h3 id="' + anchor + '">' + inline(text) + "</h3>")
            i += 1
            continue

        # -- lists -------------------------------------------------------------
        mo = _ORDERED.match(line)
        if mo:
            items: list[str] = []
            start = int(mo.group(1))
            while i < len(lines):
                mm = _ORDERED.match(lines[i].rstrip())
                if not mm:
                    break
                items.append(inline(mm.group(2)))
                i += 1
            body_html = "".join("<li>" + it + "</li>" for it in items)
            attr = ' start="' + str(start) + '"' if start != 1 else ""
            out.append("<ol" + attr + ">" + body_html + "</ol>")
            continue

        if _BULLET.match(line):
            items = []
            while i < len(lines):
                mm = _BULLET.match(lines[i].rstrip())
                if not mm:
                    break
                items.append(inline(mm.group(1)))
                i += 1
            body_html = "".join("<li>" + it + "</li>" for it in items)
            out.append("<ul>" + body_html + "</ul>")
            continue

        if _QUOTE.match(line):
            parts: list[str] = []
            while i < len(lines):
                mm = _QUOTE.match(lines[i].rstrip())
                if not mm:
                    break
                parts.append(mm.group(1))
                i += 1
            out.append("<blockquote><p>" + inline(" ".join(parts).strip()) + "</p></blockquote>")
            continue

        # -- indented code -----------------------------------------------------
        if lines[i].startswith("    ") and lines[i].strip():
            buf = []
            while i < len(lines):
                cur = lines[i]
                if cur.startswith("    "):
                    buf.append(cur[4:])
                    i += 1
                    continue
                if not cur.strip() and i + 1 < len(lines) and lines[i + 1].startswith("    "):
                    buf.append("")
                    i += 1
                    continue
                break
            out.append("<pre><code>" + html.escape("\n".join(buf), quote=False) + "</code></pre>")
            continue

        # -- paragraph ---------------------------------------------------------
        para: list[str] = []
        while i < len(lines):
            nxt = lines[i].rstrip()
            if not nxt.strip():
                break
            if (
                _RULE.match(nxt)
                or _HEADING.match(nxt)
                or _BULLET.match(nxt)
                or _ORDERED.match(nxt)
                or _QUOTE.match(nxt)
                or _FENCE.match(nxt)
                or _BOLD_LINE.match(nxt)
                or nxt.strip().startswith("$$")
                or nxt.lstrip().startswith("|")
            ):
                break
            para.append(nxt.strip())
            i += 1
        if not para:
            raise CanonSyntaxError(where, i + 1, lines[i])
        out.append("<p>" + inline(" ".join(para)) + "</p>")

    return Rendered(html="\n".join(out), headings=headings)
