"""
The Lore stylesheet, inlined into every page.

★★ **This is a reading surface, not the cockpit.** Mycelium is terminal-adjacent
on purpose -- dense, monospaced, instrument-like. An essay is the opposite job:
one column, long measure, nothing moving. It shares the palette so it is
recognisably the same project, and shares nothing else.

★★★ **The two universal UI rules apply here as much as in the app.**

* *Fit to container.* Every width is relative, the measure is capped in ``ch``,
  and the only thing allowed to scroll sideways is a code span that genuinely
  cannot wrap. The page itself never does, at any window size.
* *Navigable-looking means clickable.* On this surface that is mostly a
  discipline about what NOT to draw: there are no decorative cards, no
  "read more" that is not a link, and every essay row is a single anchor
  covering the whole row rather than a title-sized target inside a box that
  looks pressable and is not.

Contrast was computed, not eyeballed: body text sits at ~13:1 on the page
ground and the dimmest text used for anything readable at ~5:1, which clears
WCAG AA for body at every size used here.
"""

CSS = """
:root {
  --ground: #12100e;
  --raised: #1b1917;
  --ink: #ece7dd;
  --ink-soft: #b3ada2;
  --ink-dim: #8b857b;
  --amber: #e8a020;
  --rule: #2b2825;
  --measure: 68ch;
}
@media (prefers-color-scheme: light) {
  :root {
    --ground: #faf7f1;
    --raised: #f1ece2;
    --ink: #1b1815;
    --ink-soft: #4d4740;
    --ink-dim: #6b645b;
    --amber: #9a6208;
    --rule: #ded7cb;
  }
}

* { box-sizing: border-box; }
html { -webkit-text-size-adjust: 100%; }
body {
  margin: 0;
  background: var(--ground);
  color: var(--ink);
  font: 400 1.0625rem/1.72 ui-serif, Georgia, "Iowan Old Style", "Times New Roman", serif;
  overflow-wrap: break-word;
}

.top {
  border-bottom: 1px solid var(--rule);
  padding: 1.1rem 1.4rem;
}
.wordmark {
  color: var(--ink-soft);
  text-decoration: none;
  font: 500 0.72rem/1 ui-monospace, SFMono-Regular, "DM Mono", Menlo, monospace;
  letter-spacing: 0.22em;
  text-transform: uppercase;
}
.wordmark:hover { color: var(--amber); }

article {
  max-width: var(--measure);
  margin: 0 auto;
  padding: 3rem 1.4rem 4rem;
}

.eyebrow {
  font: 500 0.7rem/1 ui-monospace, SFMono-Regular, "DM Mono", Menlo, monospace;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: var(--ink-dim);
  margin: 0 0 1.1rem;
}
h1 {
  font-size: clamp(1.85rem, 5.5vw, 2.7rem);
  line-height: 1.14;
  letter-spacing: -0.015em;
  margin: 0 0 0.9rem;
  font-weight: 600;
}
.dek {
  color: var(--ink-soft);
  font-size: 1.15rem;
  line-height: 1.55;
  margin: 0 0 2.4rem;
}
h2 {
  font-size: 1.2rem;
  line-height: 1.35;
  margin: 2.9rem 0 0.9rem;
  font-weight: 600;
  scroll-margin-top: 1.5rem;
}
h2 strong { font-weight: 600; }
p { margin: 0 0 1.35rem; }
hr {
  border: 0;
  border-top: 1px solid var(--rule);
  margin: 2.6rem 0;
}
ul, ol { padding-left: 1.15rem; margin: 0 0 1.35rem; }
li { margin: 0 0 0.55rem; }
strong { font-weight: 650; }
em { font-style: italic; }
a { color: inherit; text-underline-offset: 0.18em; }
a:hover { color: var(--amber); }
code {
  font: 0.87em/1.5 ui-monospace, SFMono-Regular, "DM Mono", Menlo, monospace;
  background: var(--raised);
  border-radius: 3px;
  padding: 0.1em 0.34em;
  /* the one thing allowed to scroll sideways, and only inside itself */
  overflow-x: auto;
  max-width: 100%;
}
blockquote {
  margin: 1.6rem 0;
  padding-left: 1.1rem;
  border-left: 2px solid var(--amber);
  color: var(--ink-soft);
}

.contents {
  border-top: 1px solid var(--rule);
  border-bottom: 1px solid var(--rule);
  padding: 1.1rem 0;
  margin: 0 0 2.4rem;
}
.contents ol {
  list-style: none;
  padding: 0;
  margin: 0;
  columns: 2;
  column-gap: 2rem;
}
.contents li { margin: 0 0 0.4rem; break-inside: avoid; }
.contents a {
  color: var(--ink-soft);
  text-decoration: none;
  font-size: 0.95rem;
}
@media (max-width: 34rem) { .contents ol { columns: 1; } }

.essays { list-style: none; padding: 0; margin: 2.5rem 0 0; }
.essays li { margin: 0; border-top: 1px solid var(--rule); }
.essays li:last-child { border-bottom: 1px solid var(--rule); }
/* the WHOLE row is the link -- nothing here looks like a door and isn't one */
.essays a {
  display: grid;
  grid-template-columns: 2.6rem 1fr;
  gap: 0.2rem 0.9rem;
  padding: 1.35rem 0.4rem;
  text-decoration: none;
}
.essays a:hover { background: var(--raised); }
.essays .n {
  font: 500 0.75rem/1.7 ui-monospace, SFMono-Regular, "DM Mono", Menlo, monospace;
  color: var(--ink-dim);
}
.essays .t { font-size: 1.08rem; }
.essays .d {
  grid-column: 2;
  color: var(--ink-soft);
  font-size: 0.95rem;
  line-height: 1.5;
}
@media (max-width: 30rem) {
  .essays a { grid-template-columns: 1fr; }
  .essays .d { grid-column: 1; }
}

.empty {
  color: var(--ink-dim);
  font: 0.9rem/1.6 ui-monospace, SFMono-Regular, "DM Mono", Menlo, monospace;
  border-top: 1px solid var(--rule);
  padding-top: 1.4rem;
  margin-top: 2.5rem;
}

.backlink { margin: 3.4rem 0 0; }
.backlink a {
  font: 500 0.72rem/1 ui-monospace, SFMono-Regular, "DM Mono", Menlo, monospace;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--ink-dim);
  text-decoration: none;
}
.backlink a:hover { color: var(--amber); }

.foot {
  border-top: 1px solid var(--rule);
  margin-top: 3rem;
  padding: 1.6rem 1.4rem 2.6rem;
}
.foot p {
  max-width: var(--measure);
  margin: 0 auto;
  color: var(--ink-dim);
  font-size: 0.85rem;
}
"""
