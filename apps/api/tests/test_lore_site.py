"""
Sustena Lore: the renderer, the build, and the host dispatch.

The renderer's whole promise is that nothing is silently dropped, so most of
what is asserted here is about **loudness** rather than output shape.
"""

from __future__ import annotations

import pytest
from fastapi.testclient import TestClient

from sustena.api.main import app, is_lore_host
from sustena.lore_site.build import DraftError, essay_page, index_page, load_all, parse_draft
from sustena.lore_site.render import LoreSyntaxError, render, slug

# -- the renderer --------------------------------------------------------------


class TestRenderer:
    def test_the_subset_the_essays_actually_use(self):
        out = render(
            "#### **A heading**\n\n"
            "A paragraph with **bold**, *italic* and `code`.\n\n"
            "- one\n- two\n\n"
            "---\n\n"
            "> a quoted line\n"
        ).html
        assert "<h2 " in out and "A heading" in out
        assert "<strong>bold</strong>" in out
        assert "<em>italic</em>" in out
        assert "<code>code</code>" in out
        assert out.count("<li>") == 2
        assert "<hr>" in out
        assert "<blockquote>" in out

    def test_a_paragraph_wraps_across_source_lines(self):
        # ★ The drafts are hard-wrapped; a wrapped sentence is still one
        #   paragraph, not three.
        out = render("one line\nsecond line\nthird line\n").html
        assert out == "<p>one line second line third line</p>"

    def test_an_unknown_construct_stops_the_build_and_names_the_line(self):
        # ★★★ The point of the whole module. A table would be rendered as
        #     nonsense prose by a lenient renderer; here it refuses.
        with pytest.raises(LoreSyntaxError) as e:
            render("fine\n\n| a | b |\n", where="draft.md")
        assert e.value.lineno == 3
        assert "draft.md" in str(e.value)

    def test_a_code_fence_is_refused_rather_than_half_rendered(self):
        with pytest.raises(LoreSyntaxError):
            render("```python\nx = 1\n```\n")

    def test_markup_in_a_draft_cannot_reach_the_page(self):
        # ★★ Escape first, then add markup -- never the other way round.
        out = render("<script>alert(1)</script> and 5 < 6").html
        assert "<script>" not in out
        assert "&lt;script&gt;" in out

    def test_only_http_links_are_possible(self):
        assert 'href="https://example.org"' in render("[x](https://example.org)").html
        # ★ A `javascript:` href cannot be written, because the pattern that
        #   makes links only admits http(s) -- it stays literal text.
        out = render("[x](javascript:alert(1))").html
        assert "href" not in out

    def test_bold_is_not_read_as_two_italics(self):
        assert render("**x**").html == "<p><strong>x</strong></p>"

    def test_headings_get_stable_unique_anchors(self):
        r = render("#### Same\n\n#### Same\n")
        ids = [a for a, _ in r.headings]
        assert ids == ["same", "same-2"]
        assert slug("**The boundary — what it owns**") == "the-boundary-what-it-owns"

    def test_rendering_is_deterministic(self):
        src = "#### **H**\n\ntext *here*\n\n- a\n"
        assert render(src).html == render(src).html


# -- the drafts ----------------------------------------------------------------


class TestDrafts:
    def test_every_shipped_draft_parses_and_renders(self):
        # ★★★ This is the real regression guard: if an essay is added with a
        #     construct the renderer does not know, or missing frontmatter, the
        #     suite fails rather than the site quietly losing a paragraph.
        essays = load_all()
        assert essays, "no drafts found"
        for e in essays:
            assert e.body.html.strip()
            assert e.title and e.dek and e.kicker

    def test_article_one_is_cell(self):
        first = load_all()[0]
        assert first.number == 1
        assert first.kicker == "CELL"
        assert first.module == "Sustain"

    def test_the_published_draft_carries_nothing_private(self, tmp_path):
        # ★★★ The editing discipline, asserted rather than trusted to a
        #     one-time read. These drafts are public the moment they build.
        import re

        banned = re.compile(
            r"vyyb|gachiengu|bonnie|mkulima|colosso|biashara|\b\d{9,}\b", re.I
        )
        for e in load_all():
            page = essay_page(e)
            hit = banned.search(page)
            assert hit is None, f"essay {e.number} carries {hit.group(0)!r}"

    def test_a_draft_without_frontmatter_is_refused(self, tmp_path):
        p = tmp_path / "001-x.md"
        p.write_text("just prose\n", encoding="utf-8")
        with pytest.raises(DraftError):
            parse_draft(p)

    def test_a_draft_missing_a_required_field_is_refused(self, tmp_path):
        p = tmp_path / "001-x.md"
        p.write_text("---\nnumber: 1\n---\n\nprose\n", encoding="utf-8")
        with pytest.raises(DraftError, match="missing"):
            parse_draft(p)

    def test_two_drafts_cannot_claim_the_same_number(self, tmp_path):
        for name in ("001-a.md", "002-b.md"):
            (tmp_path / name).write_text(
                "---\nnumber: 1\nkicker: K\ntitle: T\nmodule: M\ndek: D\ndate: 2026-08-30\n"
                "---\n\nprose\n",
                encoding="utf-8",
            )
        with pytest.raises(DraftError, match="same number"):
            load_all(tmp_path)

    def test_the_index_has_an_honest_empty_state(self, tmp_path):
        page = index_page([])
        assert "nothing published yet" in page
        assert "<ul" not in page


# -- the host dispatch ---------------------------------------------------------


class TestHostDispatch:
    def test_which_hosts_are_the_blog(self):
        assert is_lore_host("lore.sustena.vyybandasky.online")
        assert is_lore_host("lore.localhost:9000")
        assert not is_lore_host("sustena.vyybandasky.online")
        assert not is_lore_host(None)
        # ★ Not a suffix match: a host that merely ends in the blog's name is
        #   somebody else's.
        assert not is_lore_host("evil-lore.example.com")

    def test_the_blog_host_serves_the_index(self):
        c = TestClient(app)
        r = c.get("/", headers={"host": "lore.localhost"})
        assert r.status_code == 200
        assert "Sustena Lore" in r.text

    def test_the_blog_host_serves_an_essay(self):
        c = TestClient(app)
        r = c.get("/001-cell.html", headers={"host": "lore.localhost"})
        assert r.status_code == 200
        assert "Constant Work of Holding Yourself Together" in r.text

    def test_the_blog_host_never_reaches_the_application(self):
        # ★★★ A reader on the blog cannot fall through into the app: the API
        #     path is answered by the site's 404, not by the API.
        c = TestClient(app)
        r = c.get("/health", headers={"host": "lore.localhost"})
        assert r.status_code == 404
        assert "git_commit" not in r.text

    def test_the_blog_is_read_only(self):
        c = TestClient(app)
        r = c.post("/anything", headers={"host": "lore.localhost"})
        assert r.status_code == 405

    def test_a_traversal_gets_the_404_page_not_a_file(self):
        c = TestClient(app)
        r = c.get("/../../../main.py", headers={"host": "lore.localhost"})
        assert r.status_code in (400, 404)
        assert "app = FastAPI" not in r.text

    def test_the_application_host_is_untouched(self):
        # ★★ The dispatch must be invisible to everything that already worked.
        c = TestClient(app)
        r = c.get("/health", headers={"host": "sustena.vyybandasky.online"})
        assert r.status_code == 200
        assert "git_commit" in r.text
