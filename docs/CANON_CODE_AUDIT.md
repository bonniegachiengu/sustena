# Canon ↔ Code — the bidirectional completeness and integrity audit

*Every feature the code implements, every requirement the articles specify, and
what each one says about the other. Plus the two new universal UI rules, their
scientific grounding, and a screen-by-screen run of both rules across Mycelium
and Orchie.*

Written 30 Aug 2026, against `dev` + `feat/ui-universal-rules`.
Nothing in the articles or the code was changed to produce §§1–4 or §6.

---

# 1. The article set — confirmed, counted, listed

**You said 32. Earlier sessions said 18, and one tracker implies 15 pairs. All
three are wrong or partial. The real number is 36 files in 18 matched pairs.**

I walked `Articles (Serious)/` rather than trusting any of the counts:

| | count |
|---|---|
| `technical/` — the `3B1B — … Formal Execution.md` papers | **18** |
| `inspirations/` — the prose companion to each | **18** |
| **Total `.md` files** | **36** |
| **Matched pairs** | **18** |

`.gitignore` line 8 independently corroborates it: *"The 36 article files are the
project's specification."*

**The 18 pairs, matched:**

| # | Technical paper | Inspiration companion |
|---|---|---|
| 1 | Sustain | CELL — The Constant Work of Holding Yourself Together |
| 2 | Operator | ENZYME — Nothing Happens Until Something Says Yes |
| 3 | Constraint | LAW — The Violation That Never Gets to Happen |
| 4 | Events and Time | RECORD — Everything That Ever Happened Is Still Arriving |
| 5 | DSL | GENOME — The Meaning Was Decided Before Anything Read It |
| 6 | Editing | SLOPE — Every Step Up Has to Be Somewhere You Can Stand |
| 7 | Ingest | RECEPTOR — The World Never Actually Gets In |
| 8 | Immune | TOLERANCE — The Defence That Spends Most of Its Life Not Attacking |
| 9 | Monitor | The Art of Watching Everything Without Looking |
| 10 | Controller | The Feedback Loop That Keeps Everything Alive |
| 11 | Tenet | Temporal Decision Architecture |
| 12 | Operative | SCOUT — The Ones Who Go Looking, and Never Get to Decide |
| 13 | Curated UI | PERCEPT — Nothing Reaches You That Did Not Outbid Something Else |
| 14 | Multiparty | SLIME — The One That Was Always Many |
| 15 | Mycelium | MYCELIUM — The Forest That Trades |
| 16 | Arena | ARENA — The Oldest Market on Earth… |
| 17 | Pawa | The Price of a Thought |
| 18 | Capstone | GAIA — The Same Thing, All the Way Up |

**Where the other numbers come from, so they stop recurring:**

- **18** — correct, but only for the technical half. It is the number people mean
  when they say "the canon papers".
- **36** — correct for files. Both halves.
- **15** — the `SUSTENA_UPGRADE_SPEC.md` "article pair N" scheme is its **own**
  numbering, which counted 14 pairs and later added a 15th (the Pawa/Juul
  economy). It never matched the shipped file set and was never meant to.
- **32** — matches nothing on disk. I searched outside the repo too; there is no
  second article collection. My best guess is 36 minus the 4 papers that have no
  `M-` prefix in the WBD, or a half-remembered 16×2. **Worth you confirming
  whether you were thinking of a different set**, because if 32 is real then
  four articles exist somewhere I cannot see.

---

# 2. What the code actually implements

## 2.1 The portable core — `sustena-core`

**154 modules** (143 top-level + 11 in `operator/` and `predicate/`), **2,091
unit tests** plus the conformance binaries (3,049 total).

The seven primitives and everything built on them: typed state and the fold,
the admission gate, predicates and the DSL, events and time (event-time vs
ingest-time, watermarks, windowing, periods, checkpoints), composition ⊕ and
roll-up ρ, the council and consensus, operatives (utility vectors, strategy
DAGs, memes and learning, world models, attention, the Pareto frontier, Goodhart
guards, Cynefin routing, mixture-of-experts gating), the monitor stack (EWMA,
CUSUM, criticality, harmonics, damping, observability), the controller (Lyapunov
descent, Sheridan levels, OODA), the simulator and viability kernel, ingest (the
transducer, parse rules, correlation, intake keys, effect capture), the immune
layer (self/non-self, secret shape, capability, provenance), the economy (the
pawa meter, the juul ledger, royalties, treasury, issuance, pricing, reputation,
publishing, the market, orders, the commons, abuse economics, resolution,
settlement, the peg), and the network (membership, health, CRDTs, vector clocks,
quorum, Paxos, the Physarum router).

## 2.2 The host — `apps/mycelium/src-tauri`

**23 modules, 256 tests.** The engine wrapper, the local store (append-only
logs), identity (ed25519 sealed under PBKDF2), ingest wiring, the Orchie feed,
the world/summary push channel, peer transport, quorum-gated writes, the arena,
templates, definitions, the driver (OODA over real sustains), and the Tauri
command surface with generated TS bindings.

## 2.3 The faces

- **Mycelium** — 14 desktop screens (§6 lists them).
- **Orchie** — one file, ~20 card components, phone-first, `compose(r)`-driven.

## 2.4 The Python reference — `apps/api`

**30 core modules, 2,347 tests.** The engine that has run your household daily
for months. Feature-complete for the S0–S9 build; behind the Rust core on
everything R2 added.

---

# 3. Bidirectional correspondence

## 3.1 Article → code: this map already exists, and it is closed

`docs/SUSTENA_UPGRADE_SPEC.md` is a row-per-requirement extraction of all 18
technical papers — **268 rows**, every one traced to an article and section:

| module | rows | | module | rows |
|---|---:|---|---|---:|
| OPV Operative | 31 | | MON Monitor | 13 |
| CAP Capstone | 20 | | PAWA Pawa | 13 |
| SUS Sustain | 17 | | TEN Tenet | 12 |
| MUL Multiparty | 16 | | CTL Controller | 12 |
| EDIT Editing | 16 | | ARE Arena | 12 |
| EVT Events | 15 | | OP Operator | 11 |
| DSL DSL | 15 | | CON Constraint | 10 |
| UI Curated UI | 15 | | MYC Mycelium | 9 |
| IMM Immune | 13 | | ADD Additions | 5 |
| ING Ingest | 13 | | | |

**Status: 264 done · 0 open · 0 parked · 4 declined.** So in the
article→code direction there is **no MISSING**, with four declared exceptions:

| declined row | why |
|---|---|
| **MON-2** Kalman filter | The Monitor Additions' own fit caveat: the apparatus assumes a continuous linear ODE that discrete event-sourced state is not. Replaced by `W = d(s,V)` → EWMA → CUSUM. |
| **MON-1** observability matrix | Same caveat, same paragraph. `CAᵏ` needs a single `A` to take powers of and the dynamics here is a *choice*, not a fixed linear map. |
| **OPV-14** tail shape | CSN's goodness-of-fit needs a random source ADR-0001 forbids in core. The constraint a future slice must satisfy is recorded. |
| **CTL-6** | Relocated out of core by ADR-0001. |
| **UI-12** bounded generation | Needs an LLM to propose; the core has no model and no I/O. The schema-disposes half is built (`proposal.rs`). |

Those are positions, argued and marked `🚫` rather than `⬜`, so they do not read
as unfinished.

## 3.2 Code → article: the direction nobody had run

This is the new analysis. I took all 154 core modules and asked whether each is
named anywhere in the WBD. **135 are. 19 are not** — and most of those are
article-backed under a different name (`state.rs` ← Sustain, `event.rs` ←
Events and Time, `council.rs` ← Multiparty §V, `predicate/*` ← Constraint/DSL,
`holon.rs` ← Capstone ⊕, `path.rs`/`error.rs`/`mutation.rs` are plumbing that
needs no paper).

**Five are genuinely undocumented, and they are one coherent family.**

---

# 4. The findings

## FINDING 1 — an entire feature family has no article behind it

`tab.rs`, `ledger.rs`, `operator/vendor.rs`, `operator/inventory.rs`, and
`parse_rule_learn.rs` carry **no article citation in their headers**, unlike
every other module in the crate. They trace instead to
`Orchie_Money_Model.md` §6 — a design document that is **not one of the 18
pairs**.

What they implement is not small:

| module | what it does |
|---|---|
| `ledger.rs` | **Double-entry.** Every transaction is postings summing to zero. This changed the money model from single-sided (a spend lowers a pocket) to two-sided. |
| `tab.rs` | **Person-pockets and the running tab** — a pocket tied to a person, netting both directions, derived from the log rather than stored. |
| `operator/vendor.rs` | Vendor identity, remembered categorisation, suggestion. |
| `operator/inventory.rs` | Spends become **assets** — supplies itemised into things the household holds, feeding net worth. |
| `parse_rule_learn.rs` | A confirmed human correction becomes a candidate ParseRule. |

**Repercussions of leaving it undocumented.** The canon currently describes a
system whose money model is single-sided. Anyone implementing from the papers
builds something that cannot represent a tab, a vendor, an asset, or a
liability — and the Python reference is exactly that system, which is *why* it
cannot express an overdraft (already recorded as a standing conformance
divergence). The gap is not cosmetic: **the two engines disagree about what money
is, and only one of them has a paper.**

**Repercussions of documenting it.** Double-entry has a real formal spine —
conservation as `Σ postings = 0` is the same shape as `V_net`'s conservation
conjunct and the royalty split's exactness theorem. It belongs in the
**Constraint** paper (conservation as a `D` shape) with the account taxonomy in
**Sustain** (`S` gains asset/liability dimensions). It is a paper's worth of
material and it would make the canon match the build.

**Plan.** A 19th pair — *LEDGER / The Two Sides of Every Shilling* — or, cheaper
and probably better, three sections folded into existing papers: conservation
into Constraint §III, the account/asset taxonomy into Sustain §S, and the
person-tab into Multiparty (a tab is a two-party relationship, which is §VII's
subject). **My recommendation: fold, don't add a pair** — the material is
already three papers' business and a 19th pair would duplicate their machinery.

## FINDING 2 — `inverse.rs` documents a row the Operator paper declares unbuilt

`inverse.rs` cites *"ENZYME / the Operator paper, §V"* and implements the
inverse. The WBD's own OP rows and the Operator article's Sustena Note list
`inverse` among *declared-not-built*. **The note is stale** — the same class of
staleness the royalty parentheticals were. Low cost, one sentence.

## FINDING 3 — the Sustena Notes are dated snapshots and several are stale

Confirmed for Pawa/Mycelium/Arena (the royalty claims — corrected this week) and
now for Operator. **I have not audited all 18.** This is a real, bounded job:
each paper's Sustena Note describes the code as it stood when written, and the
build has moved a long way. **Plan:** one pass, 18 notes, each either re-dated
or corrected. **Repercussion of not doing it:** the papers become untrustworthy
as a status source, which is exactly what caused the royalty drift — a stale
claim in a document was read as canon and implemented in good faith.

## FINDING 4 — the four declined rows are sound, and one deserves revisiting

MON-1/MON-2 and CTL-6 are well argued. **UI-12** is worth revisiting *now*
rather than later: the reason it was declined (no model, no I/O in core) is
still true, but the host now has an LLM policy layer (`llm_policy.rs`) and a
proposal verifier (`proposal.rs`). The missing half is a host-side generator.
**Repercussion of having it:** generated views constrained by the widget schema —
the "LLM proposes, schema disposes" property, which is the only safe way to let
a model touch a UI. **Of not having it:** views stay hand-declared, which is
fine today and a ceiling later.

---

# 5. The two new universal UI rules — for the Curated UI paper

These are written to slot into **Curated UI (PERCEPT)** as a new section, in the
paper's own idiom. Both are *laws about the surface*, which is that paper's
subject: it already owns `w = ⟨inputs, render, emits⟩`, the binding table β,
`compose(r)`, salience, and the attention budget `K ≈ 4`.

---

## §X. Two laws of the rendered surface

The paper so far governs **what** reaches a person: β decides what is eligible,
salience ranks it, and the knapsack bounds it at about four things. Two things
it does not yet govern are properties of what happens **after** the selection —
how the chosen content occupies the space it was given, and whether what looks
actionable is.

Both are stated as laws rather than guidance because both have the same failure
signature: **the surface makes a promise the system does not keep**, and the
person pays for it in a currency the budget was supposed to protect.

---

### §X.1 — Fit: selected content occupies its container and no more

$$\forall w \in \mathrm{selected}(r):\quad \mathrm{bounds}(\mathrm{render}(w)) \subseteq \mathrm{bounds}(\mathrm{container}(w))$$

**Why it is a law and not a preference.** The knapsack's guarantee is that
*everything on this screen is necessary* — a computed fact rather than a design
intention (§II). Overflow silently voids it. A widget that paints past its
container has not been de-selected; it has been **selected and then hidden**,
which is strictly worse than exclusion, because exclusion is reported with its
score (§V) and clipping is reported to nobody.

**Fitts (1954)** gives the sharpest form. Movement time to a target of width $W$
at distance $D$ is $MT = a + b\log_2(2D/W)$. A clipped target has $W \to 0$ in
the visible field, so $MT \to \infty$: **the affordance is not merely hard to
reach, it is unreachable**, and the law is the boundary condition that keeps
$W$ finite.

**Change blindness** supplies the second half. Rensink, O'Regan & Clark (1997)
and Simons & Levin (1997) established that observers reliably fail to notice
substantial changes they are not attending to. Clipped content is not perceived
as *missing*; the surface simply looks complete and is not. A person cannot
forage for what gives no sign of existing — which connects it directly to the
next law.

**The mechanism the law needs.** Fit cannot be delegated to layout alone.
Content carrying **intrinsic** dimensions — a drawing with an aspect ratio, an
image, a canvas — sizes itself from its own geometry and will exceed a container
that is correctly constrained in every other respect. So the law binds the
*content*, not the frame, and the frame must be **derived from what is drawn**
rather than declared alongside it. A declared frame and a drawn extent are two
independent claims about the same thing, and they agree only by coincidence.

**Corollary (the derived frame).** For a rendering with intrinsic geometry, the
frame is
$$\mathrm{frame} = \mathrm{pad}\big(\bigcup_{e \in \mathrm{painted}} \mathrm{extent}(e)\big)$$
over *everything that paints*, labels included — not over the primitives alone.

---

### §X.2 — Navigability: what signifies a destination is a destination

$$\forall e \in \mathrm{rendered}(r):\quad \mathrm{signifies\text{-}navigable}(e) \iff \mathrm{navigable}(e)$$

**The distinction the law rests on.** Gibson (1979) defined an **affordance** as
what an environment *makes possible* for an actor. Norman (1988) imported it to
design and then, in *Affordance, Conventions and Design* (1999), corrected the
import: what a designer places is a **signifier** — a perceptible signal that an
affordance exists. The two can come apart, and a **false signifier** is precisely
the defect this law forbids: an element that signals a destination where none
exists.

**Why it costs more than the click.** Pirolli & Card's **information foraging
theory** (1995, 1999) models a person navigating an interface as a forager
following **information scent** — proximal cues about the value of distal
content. A false signifier is scent with no prey. Its cost is not the wasted
click; it is the **recalibration of the scent model**. After a small number of
dead ends the forager stops treating that cue class as informative, and the
working affordances of the same class stop being tried. **One lying row degrades
every honest row that looks like it.**

The empirical analogue is well documented: Benway (1998) showed users become
blind to a whole visual class once it stops paying — *banner blindness*. The
mechanism is ordinary extinction, and the same logic makes a dead affordance a
systemic cost rather than a local one.

**Norman's gulfs** (Norman & Draper, 1986) name the two failure directions, and
this law addresses both. A navigable-looking element that does nothing widens the
**gulf of execution** — the person forms a correct intention and finds no
mechanism. An element that *is* navigable but does not signify it widens the
**gulf of evaluation** — the mechanism exists and the surface never says so, so
reach is lost silently. **The biconditional is deliberate**: the law is violated
in both directions, and only the second is usually noticed.

**Discoverability** (Norman, 2013) is what the second direction buys. Sustena's
surfaces are deliberately dense with real structure — Sustains, holons, pockets,
members, rules, gate decisions. That structure is the product. If it is not
reachable it is not nuance, it is a wall.

**The mechanism the law needs.** The biconditional cannot be maintained by
convention, because two independent decisions (does it look navigable, is it
navigable) will drift. It has to be **one decision expressed once**: a single
value that determines both the handler and the appearance, so an element cannot
be rendered as a door and wired as a wall. In Sustena's implementation
`openRow(id)` returns a handler when the target genuinely exists and `undefined`
when it does not, and the row primitive paints from that same value.

**Corollary (honest absence).** Where there is genuinely nowhere to go, the
element must signify a **fact**, not a door. This is not a weaker form of the
law — it is the law's other half, and it is what makes the affordances that *do*
work worth trying.

---

### §X.3 — Why these belong to this paper

Both laws are about the **budget the paper already defends**. §V spends a scarce
slot on a widget on the argument that it earned it. Overflow spends the slot and
delivers nothing. A false signifier spends the person's *next* forage and
delivers nothing, and then taxes every subsequent one. Neither is a rendering
detail; both are ways the surface can take the budget's payment without
providing the good.

**References.**
Benway, J.P. (1998). Banner blindness: the irony of attention grabbing on the
World Wide Web. *Proc. Human Factors and Ergonomics Society*, 42(5).
Fitts, P.M. (1954). The information capacity of the human motor system in
controlling the amplitude of movement. *J. Experimental Psychology*, 47(6).
Gibson, J.J. (1979). *The Ecological Approach to Visual Perception.* Houghton
Mifflin.
Hick, W.E. (1952); Hyman, R. (1953). — already cited in §V.
Norman, D.A. & Draper, S.W. (1986). *User Centered System Design.* Erlbaum.
Norman, D.A. (1999). Affordance, conventions and design. *Interactions*, 6(3).
Norman, D.A. (2013). *The Design of Everyday Things*, rev. ed. Basic Books.
Pirolli, P. & Card, S.K. (1999). Information foraging. *Psychological Review*,
106(4).
Rensink, R.A., O'Regan, J.K. & Clark, J.J. (1997). To see or not to see: the
need for attention to perceive changes in scenes. *Psychological Science*, 8(5).
Simons, D.J. & Levin, D.T. (1997). Change blindness. *Trends in Cognitive
Sciences*, 1(7).

> **Not yet inserted into the article.** The section above is drafted and ready.
> I have not written it into `3B1B — Curated UI - Formal Execution.md`, because
> the royalty episode established that a change entering the canon without your
> explicit word is exactly the failure mode to avoid. Say the word and it goes in
> as §X.

---

# 6. Both rules, run against every screen

**Method.** Every screen was checked for (R1) content with intrinsic dimensions
or hardcoded boxes that could exceed a pane, and (R2) elements that read as
"leads somewhere". R1's geometry was verified by running the shipped math in a
real browser at 5 pane sizes × 5 household shapes and measuring painted bounds.

## 6.1 Mycelium — 14 screens

| screen | R1 fit | R2 navigable | violations found | fix |
|---|---|---|---|---|
| **Constellation** | ✗ → ✅ | ✗ → ✅ | `height:auto` on a fixed viewBox clipped the bottom node; fixed radius crowded 8 members; **every gate-stream line** was a dead end; household attention rows were dead ends | frame derived from painted bounds; ring sized from the widest label; `StreamRow` + `NoteRow` navigable |
| **Composition** | ✅ | ✗ → ✅ | **linked children and the parent** were dead ends — the plainest "leads somewhere" rows in the app | `openRow(k.id)`, `openRow(p().id)` |
| **Monitor** | ✅ | ✗ → ✅ | attention rows named a problem and could take you nowhere | → Console, where a person can act |
| **Network** | ✅ | ✗ → ✅ | the Sustain named on a sync report | `NavMeta`, plain when the store lacks it |
| **Console** | ✅ | n/a | — | it is an input surface; its rows are output |
| **Define** | ✅ | ✅ (by design) | none | a definition is a *template*, not a Sustain; its real action is already an `instantiate` chip |
| **Economy** | ✅ (one 88px input) | ✅ (by design) | none | parameters are readings; there is no per-parameter view |
| **Ingest** | ✅ | ✅ (by design) | none | rule-library entries have no detail view |
| **Library** | ✅ | ✅ | none | rows carry install/order actions directly |
| **Simulate** | ✅ | ✅ | none | branch rows are already interactive |
| **Lock** | ✅ | ✅ | none | verified on the real phone at 1080×2400 — no overflow |
| **Panels** | ✅ (one 68px control) | ✅ | none | |
| **Profile** | ✅ | ✅ | none | |
| **Orchie** (in Mycelium) | ✅ | see 6.2 | | |

## 6.2 Orchie — ~20 card surfaces

Orchie is one file rendering cards chosen by `compose(r)`. **R2 is largely
satisfied by construction**: Orchie has no drill-down, because the card *is* the
detail — a classify card carries classify, an itemize card carries itemize.

| card | R1 | R2 | note |
|---|---|---|---|
| SmsCard / Classify / Itemize | ✅ | ✅ | actions are on the card |
| AccountsCard | ✅ | ✅ | 5 handlers |
| InventoryCard | ✅ | ✅ | 2 handlers |
| OwnNumbersCard | ✅ | ✅ | 3 handlers |
| LinkNumberControl | ✅ | ✅ | 3 handlers |
| CaptureFacts | ✅ | ✅ | 1 handler |
| **PersonTabCard** | ✅ | ⚠ plain | renders a person and a running tab with no action and no target. **There is no per-person view in Orchie**, so this is honest absence — but it is the one place a person is most likely to expect one. |
| **NetWorthCard** | ✅ | ⚠ plain | same shape: a figure that invites a breakdown that does not exist. |
| FiledCard / QueueCard / DeviceCard / Summary / Why | ✅ | ✅ | readings, correctly plain |

**Two soft findings, not violations.** `PersonTabCard` and `NetWorthCard` obey
the law as written (they do not falsely signify), but they are where R2's
*second* direction bites: real structure exists behind both — the tab is derived
from the log and net worth from the ledger — and the surface offers no way in.
**Plan:** a person view and a net-worth breakdown are each a real card, not a
wiring fix, and belong to an Orchie slice rather than this pass.

## 6.3 What R1 is now guaranteed by

A sweep found the constellation is the **only** intrinsically-sized content in
the entire app. Combined with the global floor in `layout.css.ts`, R1 holds by
construction rather than by vigilance — a new screen inherits it.

---

# 7. What still needs your device, and what needs your word

**Needs a real M-Pesa/KCB SMS on your phone** — the last link of the real-time
chain. Everything either side of it is verified on-device (§ the SMS report).

**Needs your word:**
1. Whether "32 articles" points at something I cannot see.
2. Whether the money-model family gets folded into three existing papers (my
   recommendation) or becomes a 19th pair.
3. Whether §X goes into the Curated UI paper as drafted.
4. Whether the 18 Sustena Notes get the staleness pass.
