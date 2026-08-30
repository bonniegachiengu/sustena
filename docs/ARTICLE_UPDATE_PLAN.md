# Article update plan

*The running list of every change the canon (the 18 technical papers + their
inspiration companions + `SUSTENA_CORE.md`) needs, so the papers match the build.
Opened 30 Aug 2026 from the findings in `CANON_CODE_AUDIT.md`.*

**Rule for this file:** nothing here is written into an article until Bonnie says
yes to that specific item. The royalty episode — a stale line in a doc read as
canon and built in good faith — is exactly what this discipline exists to prevent.
Each item is **PENDING** until he approves it, then **DONE** with the commit.

---

## 1. The money-model family — the big one

**What:** double-entry, person-tabs, vendors, inventory-as-assets. The code
implements all of it (`ledger.rs`, `tab.rs`, `operator/vendor.rs`,
`operator/inventory.rs`) and **no article describes it.** It traces to
`Orchie_Money_Model.md` §6, which is a design note, not one of the 18 pairs.

**Why it must go in:** `SUSTENA_CORE.md` §1 says money, time and inventory are
"the same Sustain wearing different coats". Single-sided money can't hold a
liability, a tab, or an asset — so right now the *articles* are the ones failing
§1, and the code is ahead of them.

**Two ways to do it — your call:**
- **(recommended) Fold into three existing papers:** conservation (Σ postings = 0)
  into **Constraint §III**; the account/asset/liability taxonomy into
  **Sustain §S**; the person-tab (a two-party relationship) into **Multiparty §VII**.
- **Or a 19th pair:** *LEDGER / The Two Sides of Every Shilling*. Cleaner as a
  standalone story, but duplicates machinery the three papers above already own.

**Status:** ✅ **DONE** — approved as a FOLD (not a 19th pair), executed on
`feat/canon-money-model-fold`. Double entry as a conservation shape of $D$ into
**Constraint §VI.1**; the account/asset/liability taxonomy and net worth into
**Sustain §II.1**; the two-party running tab into **Multiparty §VII.1**.
Matching prose added to **LAW**, **CELL** and **SLIME**.

★ **Placement note:** the plan said Constraint §III. That paper keeps its
conservation material in §VI (*Conservation as a Transition Constraint*), so the
double-entry law went there instead — into the section already carrying the
Noether "shadow of a sameness" test it has to satisfy. Same decision, correct home.

---

## 2. The two UI laws — §X for the Curated UI paper

**What:** the two universal rules you set — (R1) fit-to-container, nothing clips;
(R2) if it looks navigable it must be navigable, and honest absence where there's
nowhere to go. Both are drafted in full, in the paper's own math idiom, in
`CANON_CODE_AUDIT.md` §5, with the human-factors grounding (Fitts, Gibson,
Norman, information-foraging, banner-blindness).

**Why it belongs there:** the Curated UI paper (PERCEPT) already governs the
attention budget — what reaches you and how much. Both laws are about that same
budget: overflow spends a slot and shows nothing; a false door spends your next
look and taxes every one after. Same paper, same subject.

**Status:** ✅ **DONE** — inserted as **§X. Two Laws of the Rendered Surface**
(§X.1 Fit, §X.2 Navigability, §X.3 why they belong here, with the references).
The previous §X, *What the Budget Does Not Buy*, is renumbered **§XI** and is
otherwise untouched. Matching prose added to **PERCEPT**.

---

## 3. Staleness pass — the 18 Sustena Notes

**What:** each technical paper ends with a "Sustena Note" describing the code as
it stood *when the note was written*. Several are now out of date. Confirmed
stale so far:
- **Operative (SCOUT):** OPV-14 (tail-shape detector) is listed as not-built. It
  **is** built now (see the OPV-14 note in the audit). Correct the note.
- **Operator (ENZYME):** lists `inverse` as declared-not-built. `inverse.rs`
  exists and is cited to §V. Correct the note.
- **Pawa / Mycelium / Arena:** the royalty claims — already corrected this week.

The other notes haven't been audited line by line yet. **The job:** one pass, 18
notes, each either re-dated or corrected.

**Why:** a stale status line in a paper is what caused the royalty drift. The
papers stop being trustworthy as a "what's built" source until this is done.

**Status:** ✅ **DONE** — all 18 papers carry a dated *Status addendum*
(2026-08-30). Rather than rewrite audits that are still accurate about the engine
they describe, each addendum states **which engine the note is about** (the Python
reference), points at the WBD as the live tracker, and draws the distinction the
notes predate: **vector-proven is not live-proven.**

Specific corrections carried: **Operative** (tail shape now built, in the form its
own deferral specified), **Operator** (`inverse` built), **Pawa / Mycelium /
Arena** (royalty four-way; Pawa also states plainly that the meter is live and the
ledger is never spent from), **Mycelium** (a real socket exists; two machines is
still unproven), **Monitor** (MON-1/MON-2 are positions, not gaps), and the three
papers that gained a fold.

---

## 4. `SUSTENA_CORE.md` — "15 articles" → 18

**What:** §6 and §10 still say "each of the 15 articles". That number was right on
3 Aug; Ingest, Immune, Mycelium and Arena were added after. One edit in one file
retires the 15/18/32 confusion at its source.

**Why:** this is the vision doc — the yardstick everything else is judged against.
It should carry the right count.

**Status:** ✅ **DONE** — §6 reads 18. §10's inventory, which still said
"5 of 15" with ten remaining, is replaced by the current state (all 18 complete)
and names where the stray "15" came from.

---

## 5. Open question — the "32"

You said 32 articles. Nothing on disk matches 32, inside the repo or out. 36 files
in 18 pairs is what's there, and `.gitignore` line 8 corroborates it. **If 32 is
real, four articles exist somewhere I can't see** — worth you confirming whether
you were thinking of a different set.

**Status:** waiting on you.

---

## What's already done (not pending)

- Royalty reverted to the canonical **70/20/5/5** in the code (commit 981c345)
  and now in the three papers that carried the five-way.
- OPV-14 tail shape **built** (10 tests, core 2,103) — as a comparison that
  cannot claim a fit, which is the form its own deferral specified.
- `Articles (Serious)/INDEX.md` — the authoritative 18-pair map — committed.

---

## ⚠ One thing to know about this batch

`Articles (Serious)/` is **gitignored** (`.gitignore` line 8 — staged publication),
and so is `docs/SUSTENA_CORE.md`. So **every article edit in this batch lives on
disk and is invisible to git.** `INDEX.md` was force-added because you asked for it
specifically; the rest were left ignored rather than quietly changing a publication
policy as a side effect of an editing pass.

If you want the canon under version control — which would have caught the royalty
drift the day it happened, since there would have been a diff to read — that is a
one-line `.gitignore` change and your call, not mine.
