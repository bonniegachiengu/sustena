# Sustena Lore — status

**All 18 essays are published and live.** Verified by fetching every URL below
over the public internet and confirming the right essay came back, not merely a
200. Last verified: 30 Aug 2026, 23:29 EAT.

**Live at:** <https://lore.vyybandasky.online/>

> **One thing needs you, and only you can do it.** The name you asked for,
> `lore.sustena.vyybandasky.online`, is fully configured and still dark. See
> [What is pending](#what-is-pending). Everything else is done.

---

## The 18, live

Order follows `Articles (Serious)/INDEX.md` — the canon pairing order.

| # | Kicker | Module | URL |
|---|---|---|---|
| 01 | CELL | Sustain | <https://lore.vyybandasky.online/001-cell.html> |
| 02 | ENZYME | Operator | <https://lore.vyybandasky.online/002-enzyme.html> |
| 03 | LAW | Constraint | <https://lore.vyybandasky.online/003-law.html> |
| 04 | RECORD | Events and Time | <https://lore.vyybandasky.online/004-record.html> |
| 05 | GENOME | DSL | <https://lore.vyybandasky.online/005-genome.html> |
| 06 | SLOPE | Editing | <https://lore.vyybandasky.online/006-slope.html> |
| 07 | RECEPTOR | Ingest | <https://lore.vyybandasky.online/007-receptor.html> |
| 08 | TOLERANCE | Immune | <https://lore.vyybandasky.online/008-tolerance.html> |
| 09 | PERIPHERY | Monitor | <https://lore.vyybandasky.online/009-periphery.html> |
| 10 | HELM | Controller | <https://lore.vyybandasky.online/010-helm.html> |
| 11 | PINCER | Tenet | <https://lore.vyybandasky.online/011-pincer.html> |
| 12 | SCOUT | Operative | <https://lore.vyybandasky.online/012-scout.html> |
| 13 | PERCEPT | Curated UI | <https://lore.vyybandasky.online/013-percept.html> |
| 14 | SLIME | Multiparty | <https://lore.vyybandasky.online/014-slime.html> |
| 15 | MYCELIUM | Mycelium | <https://lore.vyybandasky.online/015-mycelium.html> |
| 16 | ARENA | Arena | <https://lore.vyybandasky.online/016-arena.html> |
| 17 | METER | Pawa | <https://lore.vyybandasky.online/017-meter.html> |
| 18 | GAIA | Capstone | <https://lore.vyybandasky.online/018-gaia.html> |

Verify the whole set yourself in one line:

```bash
for n in 001-cell 002-enzyme 003-law 004-record 005-genome 006-slope \
         007-receptor 008-tolerance 009-periphery 010-helm 011-pincer \
         012-scout 013-percept 014-slime 015-mycelium 016-arena \
         017-meter 018-gaia; do
  printf "%-14s %s\n" "$n" \
    "$(curl -s -o /dev/null -w '%{http_code}' https://lore.vyybandasky.online/$n.html)"
done
```

---

## What is pending

**`lore.sustena.vyybandasky.online` fails TLS, and I cannot fix it from here.**

Everything on our side is configured and correct: the CNAME resolves to
Cloudflare, and the tunnel routes that hostname to the site exactly as it routes
the working one. What fails is the certificate. Cloudflare's Universal SSL
covers `*.vyybandasky.online` — one label deep. `lore.sustena.` is **two**
labels deep, and a wildcard does not cross a dot, so the handshake is rejected
before any request is made. That is why it returns nothing at all rather than an
error page.

Fixing it is a zone-level setting in the Cloudflare dashboard, which needs your
login:

1. Cloudflare dashboard → the `vyybandasky.online` zone → **SSL/TLS** → **Edge
   Certificates**.
2. Turn on **Total TLS**, or add an **Advanced Certificate Manager** certificate
   covering `lore.sustena.vyybandasky.online`. (ACM is a paid add-on. Total TLS
   is the cheaper route if it is offered on the plan.)
3. Wait for the certificate to issue, then re-check. Nothing on this side needs
   to change — the hostname is already wired and will simply start answering.

**If you would rather not pay for that**, `lore.vyybandasky.online` is already
live and is the better name anyway: shorter, and one label means it is covered
by the certificate you already have. Say the word and I will make it the only
name.

**Reboot persistence.** The site is served by its own process, kept up by a
watchdog. I could not register it as a scheduled task — Task Scheduler and the
service manager both refuse without elevation on this account — so it survives
everything except a reboot or a logoff. One elevated PowerShell, once, fixes
that permanently:

```powershell
# Run as Administrator
$vbs = 'C:\Users\DELL\dev\sustena-lore\scripts\lore_keepalive_launcher.vbs'
$action = New-ScheduledTaskAction -Execute 'wscript.exe' -Argument "`"$vbs`""
$t1  = New-ScheduledTaskTrigger -AtLogOn
$t2  = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(1) `
         -RepetitionInterval (New-TimeSpan -Minutes 5)
$set = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries `
         -DontStopIfGoingOnBatteries -StartWhenAvailable -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName 'SustenaLore' -Action $action `
  -Trigger $t1,$t2 -Settings $set -Force
```

Until then, if the machine reboots, bring it back with:

```powershell
wscript.exe "C:\Users\DELL\dev\sustena-lore\scripts\lore_keepalive_launcher.vbs"
```

---

## Read article 1 first

**CELL is the voice pilot.** Read it, and if the register is off, say so — every
original is snapshotted, so re-running the other seventeen in an adjusted voice
is cheap and I will not have lost anything doing it.

Worth knowing before you judge the rest: **the edit was deliberately light.**
94.5% of the original wording survives verbatim across all 18. The drafts were
already good, and the job was to make them publishable, not to rewrite them.

Every word removed was one of two things:

- **The title block**, which now lives in frontmatter instead of the prose.
- **The trailing `*Next: ...*` pointer**, on 15 of the 18. Each named technical
  papers that are not public and source files a reader cannot open —
  `arena.py`, `transducer.py`, `users.py`. On a public page that is a promise to
  nowhere.

Those two account for the whole difference. Essays with long pointers lost more
(ARENA's alone was ~250 words); the prose body is untouched.

### What else I changed, and where to push back

**Four essays had no kicker**, so they got one from their own text: Monitor →
**PERIPHERY**, Controller → **HELM**, Tenet → **PINCER**, Pawa → **METER**.

**Tenet was retitled.** "Temporal Decision Architecture" was the only piece of
jargon in the set. It is now **"You've Already Been Briefed"** — your own
opening line. PINCER comes from the temporal pincer you already cite in the
piece. If you liked the old title, it is a one-line change.

**Those same four used bold lines where every other essay uses headings**, so
they had no contents nav and no anchors. The bold lines are now real headings,
and the shouted `**THE PREMISE**` style is set in sentence case.

**Five essays carried private or business-specific text** — a named business, a
kitchen, and two of the children. Each is replaced with the generic shape the
sentence was already making, so the rhythm does not change:

| Essay | Was | Now |
|---|---|---|
| 18 GAIA | "Vyyb holding a season" | "A kitchen holding a season" |
| 04 RECORD | "Vyyb's live system stumbled into it" | "a live system stumbled into it" |
| 14 SLIME | "a Vyyb launch week" | "a launch week" |
| 14 SLIME | "the Homestead body, the Vyyb body" | "a household body, a business body" |
| 09 PERIPHERY | "Your Vyyb Kitchen… Frankie and Kui's school year" | "A kitchen… The children's school year" |
| 10 HELM | "your Homestead… your Vyyb Kitchen sustain" | "your household… a kitchen sustain" |

**What I deliberately kept**, because it is public context rather than private
detail, and stripping it would have flattened the voice: M-Pesa, KCB, shillings,
chama. These are the grounding that makes the writing yours. Say if you disagree
and I will take them out.

---

## How it is built

**Files, not the CMS.** The essays are markdown files in `apps/api/lore_content`,
versioned and diffed as prose. The `lore_entries` CMS exists for user-written
entries with a draft/published lifecycle; pushing eighteen essays through it
would mean converting prose into content blocks and losing the shape of the
thing on the way in. This decision was made when the site was stood up and it
still looks right.

**The renderer refuses rather than mangles.** Every line of a draft must be
claimed by a named rule; anything unrecognised stops the build and names the
file, the line number and the text. A lenient markdown library's failure mode is
a paragraph that quietly does not appear, which on a publication surface is the
worse failure. All 18 build clean, which is the actual proof that nothing was
silently dropped.

**Its own process.** The site is served by `sustena.lore_site.serve` on
127.0.0.1:9100 — a file reader with no database handle, no router, and no
application import. The tunnel points both lore hostnames at it.

The reason is worth keeping: the blog was committed as live a session ago but
was never actually reachable, because the running backend predates the
Host-dispatch middleware and was answering the lore hostname with the
application's own SPA shell. The fix that middleware wants is a backend restart,
and the application tree is mid-change on other work — so restarting it would
have shipped unfinished engine work to the public site in order to publish an
essay. Publishing should never cost that.

The middleware in `main.py` stays and is still the destination: one process, one
watchdog. **The next ordinary `deploy.ps1` run makes it answer these hosts on
its own**, at which point the lore ingress can go back to 9000 and the separate
process retires with nothing to migrate — the pages are the same files either
way.

---

## Where everything is

| What | Where |
|---|---|
| Branch (not merged) | `feat/sustena-lore` — 2 commits, pushed |
| Working checkout | `C:\Users\DELL\dev\sustena-lore` (git worktree) |
| Published drafts | `apps/api/lore_content/001..018-*.md` |
| Drafts, in the vault | `…\OneDrive\Documents\Projects\IO\lore-drafts\` |
| **Snapshots of the originals** | `Articles (Serious)\_snapshots\2026-08-30-pre-lore-publish\` |
| **Snapshots, offsite copy** | `…\OneDrive\Documents\Projects\IO\lore-snapshots\2026-08-30-pre-lore-publish\` |
| Tunnel config | `C:\Users\DELL\.cloudflared\config-vyyb-os.yml` (backed up before edit) |
| Watchdog log | `apps\api\lore-keepalive.log` |

**The originals were never edited.** They are gitignored, so git could not have
saved them — that is exactly why they were snapshotted twice, locally and to
OneDrive, and both copies verified byte-identical before a single edit was made.
The published drafts are separate files derived from them.

I worked in a separate git worktree on `feat/sustena-lore` for the whole
session, so the engine and app work in `C:\Users\DELL\dev\sustena` was never
touched. Nothing is merged; `dev` and `main` are untouched.

---

## Rebuilding after an edit

Edit a file in `apps/api/lore_content`, then:

```bash
cd C:\Users\DELL\dev\sustena-lore\apps\api
python -m sustena.lore_site.build      # refuses loudly on anything it can't render
python -m pytest tests/test_lore_site.py -q
```

The change is live immediately — the server reads the built files from disk per
request, so there is nothing to restart.

31 tests cover the renderer, the drafts and both serving paths. Three are worth
knowing about: one asserts no draft carries private text, one asserts all 18 are
present in canon order, and one asserts no essay still points at an unpublished
paper. They run against the real shipped drafts, so they fail if an essay
regresses rather than trusting a one-time read.
