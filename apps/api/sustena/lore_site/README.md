# Two reading surfaces, and why they are separate

There are two, and the difference between them is not cosmetic — it is what is
cleared for publication and what is not.

## 1. Sustena Lore — public

`https://lore.vyybandasky.online`

Built by `build.py` from `apps/api/lore_content/` (18 edited companion essays,
cleared for release). Served by `serve.py` on **127.0.0.1:9100**, which is what
the cloudflared ingress for the lore hostnames points at.

```bash
python -m sustena.lore_site.build     # rebuild after editing a draft
python -m sustena.lore_site.serve     # serve it (the watchdog does this for you)
```

Kept alive by `scripts/lore-keepalive.ps1`, registered as the scheduled task
`SustenaLore` by `scripts/register-lore-task.ps1`. **If lore returns 502, it is
almost always this process being dead** — the tunnel is shared with
`sustena.*`, so if `sustena.vyybandasky.online` answers and `lore.*` does not,
the tunnel is fine and the blog process is not.

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\register-lore-task.ps1
```

## 2. Sustena Canon — local only, never published

All **36** article files (18 formal papers + their 18 companions), paired.

`.gitignore` records these as *staged* publication: intended for Apache-2.0
release, but held back "until each has been cleaned of private, business, and
economy-layer material". So they are readable, **not publishable**, and this
build is deliberately awkward to expose:

- it writes to `C:\Users\DELL\dev\_canon-reader` — **outside the repository**,
  so a build cannot be committed by accident;
- `canon_serve.py` binds loopback and listens on **9101**, a port the tunnel
  does not map (it maps 9100).

```bash
python -m sustena.lore_site.canon          # build all 36
python -m sustena.lore_site.canon_serve    # read at http://127.0.0.1:9101
```

Or just open `C:\Users\DELL\dev\_canon-reader\index.html` — the links are
relative, so it works as plain files with no process running at all.

When an article is cleared for release, it moves into `lore_content/` and goes
out through `build.py` like everything else.

### Rendering notes

`canon_render.py` is a second renderer, not a widening of `render.py`.
`render.py` refuses code fences, tables and maths so that a draft containing
them cannot be published half-rendered — and those three are exactly what the
formal papers are made of. Widening it would relax the guarantee protecting the
*published* surface in order to serve a private one.

Two things worth knowing about the output:

- **Section headings.** The papers use standalone `**Bold**` lines, not `#` —
  most have zero ATX headings and twenty-odd bold ones. Those are promoted to
  headings so a 70,000-character paper has a contents list.
- **Maths.** There is no typesetter (no JavaScript, no external requests), so
  only unambiguous LaTeX is converted to Unicode — symbols, font wrappers,
  sub/superscripts. Structural commands (`\frac`, `\underbrace`, matrices) are
  left as written rather than approximated into something that might read as a
  different claim. The untouched original is in each expression's `title`, so
  hovering shows exactly what was authored.
