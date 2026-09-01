"""
Tests for the canon reader -- the local reading surface for the 36 articles.

Two properties matter most here and are tested directly rather than implied:

1. Nothing is silently dropped. The renderer's whole design is that an
   unrecognised line raises instead of vanishing, so that is asserted, and the
   real articles are put through it end to end when they are on disk.
2. Nothing is published. The articles are staged for release and not yet
   cleared, so the build must write outside the repository.

The article files are gitignored (they are the specification), so the tests
that need them skip cleanly rather than fail when they are absent -- which is
the normal case in CI.
"""

from __future__ import annotations

import re

import pytest

from sustena.lore_site import canon
from sustena.lore_site.canon_render import CanonSyntaxError, prettify_math, render


# -- the no-silent-loss contract -----------------------------------------------


class TestNothingIsSilentlyDropped:
    def test_an_unclaimed_line_raises_and_names_the_place(self):
        # A table row with no separator under it cannot be rendered as a table
        # and is not prose either. That must stop the build loudly.
        with pytest.raises(CanonSyntaxError) as e:
            render("ok paragraph\n\n| a | b |\n", where="probe.md")
        assert e.value.where == "probe.md"
        assert e.value.lineno == 3

    def test_an_odd_looking_line_is_still_shown_as_prose(self):
        # The paragraph rule is total on purpose: an unusual line is rendered,
        # never skipped, so text cannot disappear between source and page.
        out = render("ok\n\n\x01\x02 odd but real", where="probe.md").html
        assert "odd but real" in out

    def test_every_block_kind_survives_a_round_trip(self):
        src = (
            "# Heading\n\nA paragraph.\n\n- one\n- two\n\n1. first\n2. second\n\n"
            "> quoted\n\n```python\nx = 1\n```\n\n"
            "| a | b |\n|---|---|\n| 1 | 2 |\n\n$$E = m c^2$$\n\n**A Section**\n\nTail.\n"
        )
        out = render(src, where="probe.md").html
        for fragment in ("<h2", "<ul>", "<ol>", "<blockquote>", "<pre", "<table", "math display", "<h3"):
            assert fragment in out, fragment
        assert "Tail." in out


# -- security ------------------------------------------------------------------


class TestNoMarkupInjection:
    def test_html_in_a_draft_is_shown_not_executed(self):
        out = render("<script>alert(1)</script> and <b>x</b>", where="probe.md").html
        assert "<script>" not in out
        assert "&lt;script&gt;" in out

    def test_only_http_links_become_anchors(self):
        out = render("[bad](javascript:alert(1)) [good](https://example.com)", where="p.md").html
        # The unsafe one stays inert text rather than becoming a link at all.
        assert "href=" in out and 'href="javascript:' not in out
        assert out.count("<a ") == 1
        assert 'href="https://example.com"' in out

    def test_maths_cannot_smuggle_markup(self):
        out = render("$<img onerror=x>$", where="p.md").html
        assert "<img" not in out


# -- maths presentation --------------------------------------------------------


class TestPrettifyMath:
    @pytest.mark.parametrize(
        "src,want",
        [
            (r"\Sigma", "Σ"),
            (r"V \subseteq S", "V ⊆ S"),
            (r"a \oplus b", "a ⊕ b"),
            (r"\forall x \in S", "∀ x ∈ S"),
        ],
    )
    def test_unambiguous_symbols_become_unicode(self, src, want):
        assert prettify_math(src) == want

    def test_font_wrappers_are_unwrapped(self):
        assert prettify_math(r"\mathrm{Viab}") == "Viab"
        assert prettify_math(r"\mathcal{L}") == "L"

    def test_subscripts_and_superscripts_become_tags(self):
        assert prettify_math("x_{t+1}") == "x<sub>t+1</sub>"
        assert prettify_math("x^2") == "x<sup>2</sup>"

    def test_structural_commands_are_left_as_written(self):
        # The failure mode is deliberate: show the source rather than guess a
        # layout that might read as a different claim.
        assert r"\frac{a}{b}" in prettify_math(r"\frac{a}{b}")

    def test_it_escapes_before_substituting(self):
        assert "<b>" not in prettify_math("<b>")


# -- the real canon on disk ----------------------------------------------------


def _articles_or_skip():
    try:
        return canon.articles_dir()
    except canon.CanonError:
        pytest.skip("the article files are gitignored and not present in this tree")


class TestTheRealArticles:
    def test_every_article_renders_without_a_single_unclaimed_line(self):
        root = _articles_or_skip()
        failures = []
        for sub in ("technical", "inspirations"):
            for path in sorted((root / sub).glob("*.md")):
                try:
                    render(path.read_text(encoding="utf-8"), where=path.name)
                except CanonSyntaxError as exc:
                    failures.append(str(exc))
        assert not failures, failures

    def test_no_article_loses_its_words(self):
        # Rendering must not quietly swallow prose. Compared as word counts
        # because markup adds tokens but must never remove them.
        root = _articles_or_skip()
        thin = []
        for sub in ("technical", "inspirations"):
            for path in sorted((root / sub).glob("*.md")):
                src = path.read_text(encoding="utf-8")
                out = render(src, where=path.name).html
                shown = len(re.sub(r"<[^>]+>", " ", out).split())
                written = len(re.sub(r"`{3}[a-z]*", "", src).split())
                if written and shown / written < 0.97:
                    thin.append((path.name, written, shown))
        assert not thin, thin

    def test_the_index_pairs_every_module_with_both_halves(self):
        pairs = canon.load_pairs(_articles_or_skip())
        assert len(pairs) == 18
        assert [p.number for p in pairs] == list(range(1, 19))
        for p in pairs:
            assert p.technical.kind == "technical"
            assert p.inspiration.kind == "inspiration"
            assert p.technical.words > 0 and p.inspiration.words > 0

    def test_a_stale_index_stops_the_build(self, tmp_path):
        # An index naming a file that is not on disk must fail loudly rather
        # than quietly building a canon with a hole in it.
        (tmp_path / "technical").mkdir()
        (tmp_path / "inspirations").mkdir()
        (tmp_path / "INDEX.md").write_text(
            "| # | Module | T | I |\n|---|---|---|---|\n"
            "| 1 | Sustain | `gone.md` | `also-gone.md` |\n",
            encoding="utf-8",
        )
        with pytest.raises(canon.CanonError, match="not on disk"):
            canon.load_pairs(tmp_path)


class TestItDoesNotPublish:
    def test_the_default_output_is_outside_the_repository(self):
        # The articles are staged for release, not cleared. A build that landed
        # inside the working tree could be committed by accident.
        repo = canon.Path(canon.__file__).resolve().parents[4]
        assert canon.DEFAULT_OUT.resolve() != repo
        assert repo not in canon.DEFAULT_OUT.resolve().parents
