# Where the code and the articles disagree — and which disagreements are the articles' fault

*A plain-language walk through every recorded divergence in `conformance/README.md`,
written to answer one question: **which of these mean an article should be annotated?***

Written 30 Aug 2026, against `main` at v1.0.0.

---

## What a "divergence" is here, in one paragraph

Sustena has two engines. The **Python reference** (`apps/api`) is the one that has
been running Bonnie's household daily for months. The **Rust core**
(`sustena-core`) is the portable rewrite, built slice by slice from the 18 canon
articles. Conformance vectors run the same inputs through both and compare the
answers.

When the two answers differ on purpose, the vector carries a `divergence` block
saying so. The rule the repo holds itself to is: **a divergence is documented or
it is a bug.** Nothing silently differs.

There are **three closed divergences** and **just over eighty standing ones**
(43 summary entries plus 39 detailed per-vector sections). That number sounds
alarming and is not. Read on.

---

## The short answer

**Almost every divergence is the same shape, and it is not interesting.**

The Rust core implements what an article specifies. The Python reference, which
predates most of the articles, does not have that machinery at all. The two
engines differ because one of them was built later against a written spec and the
other grew organically to run a real household. That is not a disagreement about
what is *right*; it is a difference in *how far along* each engine is.

The conformance log calls this **"rust-ahead-of-python"**, and it accounts for
roughly **95%** of the entries. **None of it needs an article change**, because
in every one of those cases the article is correct and Rust follows it.

What is worth your time is the small set where **the code is right and the
article is not** — **four of them**, in Part 1. A fifth entry sits there too,
the royalty split, and it turned out to be the opposite case: the **code** was
wrong, not the article. It is kept in place, corrected, because how it got there
is the most useful thing in this document.

Everything else is summarised briefly afterwards so you can see that it was
looked at.

There is also a set of **six article corrections that have already been made** —
mostly in the ratified Additions blocks of 5 Aug 2026. Part 2 lists them so you
do not annotate the same thing twice.

---

# PART 1 — The five that needed a verdict

Four of these are cases where the build **levelled up beyond the article**: the
code holds a better design than the paper does, and the paper has not caught up.
Per the repo's own rule — *"where an article and the code disagree, level UP to
whichever is the better design"* — those four are the real annotation
candidates.

**The first is not one of them.** It is here because the same question was asked
of it and the answer came back the other way round.

---

## (b1) The royalty split — the CODE was wrong, not the article

> **Corrected 30 Aug 2026.** The first version of this section had this
> backwards. It treated the word *"ratified"* in the Pawa Additions as settled,
> called the articles correct, and recommended striking three parentheticals
> that described the code as stale. **That was wrong on the substance**, and it
> is worth naming why: I trusted a claim in a document instead of tracing where
> the claim came from. The trace is below.

**ORIGINAL — and this is the part that took work to establish.** Before 4 Aug
2026 the canon was a **four-way `70 / 20 / 5 / 5`**: contributor 70, treasury
20, *referrer 5 (else treasury)*, *validator 5 (treasury, Phase 1)*. Mycelium
§VI stated it as its formula; Arena §IX carried the same; Pawa §4 was abstract
(φ_p, φ_r) with no numbers and **no Additions block at all**.

**WHAT CHANGED, and when.** A **five-way `70 / 15 / 5 / 5 / 5`** — treasury cut
to 15 to fund a new **proposer** share — plus a separate licence-sale
`80 / 10 / 3 / 2 / 5`, entered the corpus on **4 Aug** (in `GLOSSARY.md` and
`SUSTENA_WBD.md`, both created that day and both stamped *"ratified"*), and was
promoted into the article formulas on **5 Aug**. The 7 Aug article↔code
alignment pass then found the articles already carrying it, made **zero article
edits**, and concluded *"the articles are the corrected spec; the CODE is what
lags"* — which is how the five-way became load-bearing.

**WHY THE AUTHORSHIP COULD NOT BE SETTLED.** `Articles (Serious)/` is
**gitignored**, so there is no history, no blame and no commit message for any
article. Four things pushed against reading *"ratified"* as *"Bonnie decided"*:
GLOSSARY names Bonnie explicitly for a different 4 Aug item and not for this
one; its own 4 Aug change-log entry does not mention the royalty split at all;
no document Bonnie authored himself contains either schedule; and *validator +
proposer* is the Ethereum proof-of-stake role pair, arriving in the same pass
that listed **web3** among its queued "conceptual-enrichment" threads.

**THE DECISION.** Bonnie's canon is the **four-way 70 / 20 / 5 / 5**. The
five-way was not his. **Both engines were reverted on 30 Aug** — `royalty.rs`
and `pawa.py`, with the vectors and conservation tests — and `proposer` was
**removed rather than deprecated**: a role with no share is not a role, and
leaving the variant in place would let it drift back.

**CLASS. Not (b) at all.** This was never a case of the code levelling up past
the article. It was a policy change that entered the documents marked as
ratified, propagated outward, and was then implemented in good faith by a build
that read the documents as canon. **The articles are now the thing that is out
of step**, and correcting them is a separate open question — see the note at
the end of this document.

**What survives the revert, and is genuinely better.** Two things the migration
added are *not* percentages and were kept: the reference's signature could not
express any five-way split at all, and it had **no licence/usage distinction**,
so paying to *run* something and paying to *have* it settled identically. The
`RevenueType` distinction stays for that reason. **Its access figures are
provisional and mirror usage**, because no licence-sale split has been decided —
`access_is_provisional()` says so executably in both engines, with a test that
breaks if somebody quietly fills it in.

---

## (b2) Tenet §V — the trigger fires on day one

**ORIGINAL.** Tenet §V gives the signal-monitoring loop as:

```
"future_C_warning": lambda history: (
    no_customers_by(history, week=6) OR
    weekly_revenue_below(history, amount=5000, by_week=3)
)

FUNCTION Monitor(history, signals):
    FOR each signal σ in signals:
        IF σ(history) == 1:
            TRIGGER Inversion(σ.linked_action)
```

**WHAT EACH ENGINE DOES.** Python has none of this layer (`signal_function`,
`replan`, `pincer` are all absent). Rust's `pincer.rs` implements it
**three-valued**: a signal answers *true*, *false*, or *not yet determined*.

**WHY.** The article's version is two-valued, and that makes it fire immediately.
"No customers by week 6" is a claim about week 6. Evaluated at week 0 with a
two-valued predicate, *no customers have arrived yet* is **true** — so the
warning triggers on the first day of the plan, every time, for every venture.
The strategy inversion the whole section exists to protect would fire before the
business had opened.

Three values fix it exactly: at week 0 the answer is *not yet determined*,
which triggers nothing. At week 6 with no customers it becomes *true*. At week 3
with customers it becomes *false* and stays false.

**CLASS. (b) — annotate.** This is a genuine bug in the article's logic, not a
matter of taste. Anyone implementing §V literally would ship a monitor that
alarms on day one and would then blame their own code.

---

## (b3) Controller §V — the holarchy invariant uses an undefined symbol

**ORIGINAL.** Controller §V states the holarchy invariant as:

> ∀s ∈ Sustain, C(s) = True ∧ s.**s** ⊆ parent(s).**s**

with the accompanying pseudocode calling `ASSERT is_consistent(child.state,
parent.state)`.

**WHAT EACH ENGINE DOES.** Python has no escalation layer. Rust's `holarchy.rs`
implements the invariant on a specific reading: **a child must also satisfy its
parent's viable region, on the dimensions the parent actually bounds** — and the
conformance log records that as *an interpretation rather than as the article's
own*.

**WHY.** `⊆` is set inclusion, and a state is not a set. Two states are two
points in a typed space; there is no standard meaning for one being "a subset of"
the other. `is_consistent` is never defined either, so the pseudocode does not
resolve it.

The reading the build chose is the one that makes the section's own argument
work — a soil sensor breaching its moisture threshold escalates because it has
left a region its parent declared — but it is a choice, and a different
implementer could reasonably read `⊆` as "the child's dimensions are a subset of
the parent's", which is a statement about *schemas* rather than about *values*
and would make the invariant say nothing about viability at all.

**CLASS. (b) — annotate.** The article is ambiguous rather than wrong. The
annotation is to define the symbol: say which of the two readings is meant.

---

## (b4) Multiparty §IX — "a final merged value" is exact for the wrong half

**ORIGINAL.** Multiparty §IX, on dissolution:

> "On dispersal, shared CRDT state settles to a final merged value and each node
> carries its portion home."

**WHAT EACH ENGINE DOES.** Python has §IX's *dissolution semantics* already, and
cites the article by name — `holon.dissolve_child` is *"⊕⁻¹: return the child's
funds, then unlink"*, and `unlink_child` removes the edge while the child keeps
its state. Rust implements the same, and records that for a **contested**
dimension the settle keeps *what the merge preserves*, rather than a single
value.

**WHY.** The singular noun is exact for a snapshot dimension — one authoritative
reading, one value. For a dimension that is genuinely a CRDT (a shared counter,
a set several members added to concurrently), there is no single value to settle
to; there is a merge, and the merge's result is the least upper bound, which is
a structure rather than a number. Saying "a final merged value" quietly implies
the contested case has an answer of the same shape as the uncontested one.

**CLASS. (b) — annotate, mildly.** This is the weakest of the five: the sentence
is imprecise rather than wrong, and the conformance log deliberately files it as
*a reading of a singular noun rather than a correction to the article*. Annotate
it if you want the section to be safe to implement from directly; skip it if you
would rather not add words to a sentence that reads well.

---

## (b5) Multiparty §VII — `rb > c` assumes something the architecture removes

**ORIGINAL.** Multiparty §VII, on why an autonomous node accepts a costly role:

> "**Kin selection** (Hamilton, 1964): the costly role is favored when *rb > c* —
> a coefficient of shared interest, times the benefit to the others, exceeds the
> cost to the one who pays."

**WHAT EACH ENGINE DOES.** Python assigns no roles and has no relatedness
notion — though it computes both halves of `r` and never forms the ratio, because
it has nothing to spend it on. Rust's `division.rs` implements the rule and
records a limit alongside it.

**WHY.** `rb > c` is the ESS condition **provided the defector is pivotal** —
provided their walking away actually costs the group the benefit. Sustena's own
architecture removes that condition: redundancy `ρ > 1` means somebody else covers
the role, so the defector is structurally *not* pivotal, and the inequality stops
predicting the behaviour it was cited to predict. That is the free-rider problem,
and it arrives here by design rather than by accident.

Resolving it needs the group's **response** to defection — sanction, exclusion,
reduced share — and §VII does not specify one. The build did not invent one,
which is the right call: an unspecified mechanism invented to make an inequality
come out is a fabrication with a citation attached.

**CLASS. (b) — annotate.** The annotation is a caveat, not a correction: state
that `rb > c` holds where the contributor is pivotal, note that `ρ > 1` makes them
not, and either name the group's response or name it as open. Arena §V already
has the vocabulary — Ostrom's graduated sanctions — so the two sections may want
to point at each other.

---

# PART 2 — Six corrections already made (do not redo these)

The Additions blocks ratified **5 Aug 2026** on Controller, Monitor, Tenet,
Multiparty and Pawa already carry most of the article-level corrections the build
found. Listed so you can recognise them and move on.

| Article | What was wrong | Where it is fixed |
|---|---|---|
| **Monitor §VI** | The CUSUM pseudocode zeroed `S_pos` *before* `classify()` read it, so every upward shift would report INFO and never escalate. | Fixed **inline** — the code now classifies first, with the comment *"classify BEFORE the reset — else it reads a zeroed accumulator."* |
| **Tenet §V** | The Bellman value function was called `V`, colliding with `V` = the viable region. | Tenet Additions, *"A notational correction"* — renamed to `J`, with per-state utility `u`. |
| **Controller §II** | The Lyapunov function was written as distance to a **point** (`‖s_desired − s_actual‖²`); the desired thing is a **region**. | Controller Additions, *"The potential and the action"* — `W(s) = d(s, V)`, a potential well with the region as its floor. |
| **Controller §II** | Lyapunov descent alone does not forbid a controller that reduces `V` while oscillating. | Controller Additions, *"Stability has a frequency-domain face"* — damping named as the separate guarantee. |
| **Monitor §I–II, §IX** | The observability matrix and Kalman filter assume a continuous linear ODE that discrete, event-sourced state is not. | Monitor Additions, *"A fit caveat"* — the native form (`W = d(s,V)` → EWMA → CUSUM) replaces it. Both the Kalman filter **and** the rank-based observability matrix are declined on this basis, together, because the caveat names them together. |
| **Pawa, Sustena Note** | *"None of pawa/Juul exists yet"* predated the meter shipping. | Pawa Additions, *"Correction"* — the meter is built and wired into `execute_operator`. |

Constraint's §4E.2 audit note also already carries its own *"A correction to the
audit"* block.

---

# PART 3 — The rust-ahead cases, briefly

These are class **(a)**: Rust follows the article, Python simply has not got
there. **No article change is warranted for any of them.** Grouped by why the two
engines differ, with representative examples rather than an exhaustive list —
`conformance/README.md` has the full term-by-term for each.

### The article's machinery is simply absent in Python

The largest family by far. `approval`, `kernel`, `tenet`, `ensemble`, `signal`,
`pincer`, `ooda`, `detect`, `controller`, `region`, `migrate`, `version`,
`compose`, `population`, `consensus`, `router`, `division`, `criticality`,
`cynefin`, `boundary`, `clocks`, `watermark`, `period`, `checkpoint`,
`dimension`, `belief`, `harmonics`, `windowing`, `damping`, and more.

In each case the log names the greppable near-misses so nobody re-derives them —
and some are genuinely funny. `pulse` returns five hits in Python and every one
is the substring in *"imPULSE spending"*. `ESS` returns 1058 hits and zero real
ones. The only hits for `ewma` and `cusum` are a comment recording their absence.
`band` matches the deployment hostname `vyyBANDasky.online`.

### Python is architecturally unable, not merely behind

Worth separating, because these will not close by writing more Python.

- **`crdt`** — `get_shared_engine()` is a single-process singleton over one SQLite
  file. There have never been two replicas to reconcile, so a lattice merge
  answers a question this deployment cannot ask.
- **`monitor`** — Python **watches by recomputing**; the core **watches by
  accumulating**. A CUSUM is definitionally an accumulator, so in a
  recompute-per-request design it does not lack an implementation, it has
  nowhere to live.
- **`constraints`** — both Python evaluators are single-state *by signature*, so
  conservation is not unimplemented there, it is **inexpressible**.
- **`vclock`** — Python's `seq` counter is a correct total order *precisely
  because* there is one writer. It stops being sufficient at exactly two.

### Python has the shape and not the algebra (the strong counterweights)

These are the ones where calling the module "rust-ahead" would overstate the gap,
and the log says so term by term.

- **`editing`** — `Safe(e, μ)` is recorded **AT PARITY**: Python's
  `check_definition_edit_safety` really does implement §III, witness set included.
- **`holarchy`** — Python has a genuine shipped holon system. Downward authority
  and upward information both exist; only upward *escalation* does not.
- **`disaggregation`** — §IX's dissolution semantics are already in Python,
  citing the article by name. Only the automatic *trigger* is absent, because
  Python's dissolution is human-initiated — which is a coherent design.
- **`crdt`** — the G-counter is at parity **in shape**; what is absent is the
  merge.
- **`obligation`** — `post_constraints` really are evaluated before commit, and
  §III itself says the dynamic position is the honest one for Sustena today.
- **`flow`** — the conservation **law** is at parity; only the crossing notion `F`
  is not.
- **`cynefin` / `mixture`** — the abstain **seam** is at parity
  (`abstain_reason: "no_domain_overlap"`); what is absent is the axis to abstain on.

### A settled decision Python cannot express

- **`transducer`** — nine instrument shapes (Fuliza, M-Shwari, cash withdrawal,
  Pochi) are *mapped* in Rust and *unmapped* in Python. This implements
  `Orchie_Money_Model.md` §6, a separately settled document, and Python **cannot**
  express an overdraft: nothing in it writes `finances.liabilities.*`, so borrowed
  money can only be filed as income, which overstates the household by the whole
  of what it owes. Also mapped in Rust and not Python: the message's own `date`
  and `time`, which the parsers already match and Python discards one line later.

---

# PART 4 — Neither class: things the log records that are not article business

Included so the count adds up and nothing looks swept aside.

- **A Rust bug, found on your real data.** All 45 curated cases were green on both
  sides while Rust returned `-0.0` where Python returns `0.0` — Rust's `Sum for
  f64` folds from `-0.0`, and `clamp` kept it because it is in bounds. Found by a
  real feed printing `urgency -0.000` on your household, not by a test. Pinned now
  by asserting the **sign bit**, since `-0.0 == 0.0` is `true` and an
  equality-comparing vector would have passed. The same hazard bit `rollup.rs`,
  where an empty pocket rendered `-0.00` and read as a debt.
- **A vector that was wrong.** The `authorization` privilege case used a negative
  amount, so `budget.record_income`'s own guard fired first — it was proving the
  guard, not the gate. Corrected to use a ceiling the operator cannot pre-empt,
  and recorded rather than quietly fixed.
- **An expectation that was wrong.** `PANEL_POLICY` is grep-0 on **both** sides;
  the panel *list* exists as frontend navigation, the *policy* only in the article.
- **Areas with no vectors, and why** — peer transport, transport encryption,
  quorum-gated writes, packages over the wire, effect-first capture. In each case
  Python has no counterpart at all, so a comparison would compare something to
  nothing. Each is proven by host-side tests instead, and the log says so.

### Three divergences that closed

- **Intake keying (ING-5)**, closed 30 Aug — the two engines disagreed about what
  *"the same intake"* means, which is a disagreement about **identity** rather
  than about a value, the sharpest class there is. Python now runs the same rule;
  127 saved captures were re-keyed, none lost, none merged.
- **`edit.state_patch` `remove`**, closed 29 Aug — Python wrote `None` and
  reported success. It refuses now. A missing capability should refuse, not
  improvise.
- **`edit.operator_spec` price editing**, closed 29 Aug — Python let `pawa_cost`
  be set at runtime, free, unchecked, so changing an Enzyme's price cost less than
  using it. Removed rather than gated, because there is no authority model on that
  path to gate with, and a permission parameter nobody checks looks like a fence
  and is a comment.

---

# If you annotate: a suggested shape

Each of the five is a small, self-contained edit. The format the articles already
use for this is the dated Additions block, which keeps the original text intact
and readable:

> **Original.** *(quote the sentence or formula as it stands)*
> **Change.** *(what it should say)*
> **Reason.** *(what goes wrong if somebody implements the original literally)*

My suggested order, by value per minute spent:

1. **(b2)** Tenet's two-valued signal — a real bug that would bite any
   implementer.
2. **(b3)** Controller's undefined `⊆` — one sentence of definition.
3. **(b5)** Multiparty's `rb > c` — a caveat, and a cross-reference to Arena §V.
4. **(b4)** Multiparty's "final merged value" — optional; the log deliberately
   files it as a reading rather than a correction.

**(b1) is not on this list any more.** It is not an annotation — it is a
restoration, and it is blocked. See below.

---

## The open one: restoring the four-way in the articles

The code is back on `70 / 20 / 5 / 5`. **The articles are not**, and this could
not be done as the pure restore it was expected to be.

**Two of the three are clean.** Arena §IX and the Pawa Additions block both
gained the five-way *on 5 Aug* and had no trace of it before, so the pre-5-Aug
text is a faithful source.

**Mycelium §VI is not.** Its pre-5-Aug text **already described the five-way as
"the ratified target schedule"** in prose, alongside the four-way as its
displayed formula. So restoring that file verbatim restores a sentence saying
the five-way is the target — the opposite of the decision. Removing it is an
**edit**, not a restore.

**And it does not stop at the articles.** `GLOSSARY.md` §Royalty roles and the
retired `SUSTENA_WBD.md` §1.6 both carry the five-way as ratified, both dated
4 Aug — earlier than any snapshot, so there is no pre-five-way version of them
to recover at all.

Reverting the corpus is therefore a small authoring job, not a file copy, and it
needs Bonnie's word on exactly how far it reaches. Until then the conformance log
records the disagreement, which is what it is for.

---

## What this document is not

I read every divergence entry in `conformance/README.md` to write this, and
verified each of the five (b) claims against the article text itself rather than
trusting the log's summary of it. What I did **not** do is audit all 18 **Sustena
Notes** against the current tree — (b1) shows that class of staleness is real, and
finding the rest is a separate pass.

Nothing in the code or the articles was changed to produce this document.
