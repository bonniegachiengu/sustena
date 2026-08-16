# R2 — Article-vs-Code Backlog

*The source of truth for Phase R2: implementing what the articles specify but the code does not yet do.*

**Status: R1 (parity) complete. R2 in progress — 10 of 14 core items resolved.**

> **Read this before opening an R2 slice.** Every item traces to an article. The
> articles are the spec; the code catches up to them, never the reverse. Where
> an article and the code disagree, level UP to whichever is the better design —
> do not quietly make the doc match the code.

---

## Where things stand

| | |
|---|---|
| Reference engine (Python) | live, in daily use, **2,319 tests** |
| Portable core (Rust, `sustena-core`) | **R1 parity complete** — all 5 slices; **R2 in progress** |
| Conformance vectors | R1 parity + R2 spec, incl. **22 approval**, **26 editing**, **29 constraint**, **31 compose**, **16 version**, **15 migrate**, **20 region**, **17 detect**, **20 controller**, **18 kernel**, **16 tenet**, **18 ensemble**, **17 signal**, **16 pincer**, **17 ooda** cases (divergences recorded) |
| Rust tests | **412 unit · 161 conformance tests** across 16 binaries — all green |

**R1 slices at parity:** state · event fold · rules · operators + gate · council.

---

## Legend

- **Status** — `done` · `open` · `deferred` (travels with the economy layer)
- **Tier** — `CORE` (engine behaviour; land before it calcifies) · `follow`
- **Where** — which engine still needs it

---

## ✅ Closed since the last doc pass (8)

Five from the correctness pass, three found by the Rust port itself.

| # | Gap | Article | How |
|---|---|---|---|
| 1 | Fold fidelity — 3 operators changed state without recording it | CELL | Structural reconciler in Python; **unrepresentable in Rust** |
| 2 | Swallowed event-write failure | RECORD | Dead second writer removed; one transaction, fails loudly |
| 3 | Two rule evaluators disagreed | LAW / GENOME | Python unified onto one; **Rust has only one** |
| 4 | "Cannot parse" reported as "rule violated" | LAW | Raises a typed error instead |
| 5 | Cross-source double-count (KCB + M-Pesa) | RECEPTOR | Correlation on the transaction reference |
| 6 | **State did not own its data** — replaying a log mutated the state it rebuilt from | CELL | Found by building the vectors; deep-copy on write |
| 7 | **Number typing** — whole floats collapsed to integers | — | Found by the port; Python's promotion rule preserved |
| 8 | Gate failed open on an unparseable rule | TOLERANCE | **Rust refuses**; Python still skips → see #9 |

### Three gaps are now *impossible* in the Rust core

Not fixed — structurally unable to occur:

- **The live-reference write** (#1) — `get()` cannot hand out a mutable alias.
- **Two evaluators** (#3) — one parser, one evaluator, guards and invariants alike.
- **Gate failing open** (#8) — an unreadable rule refuses rather than being skipped.

---

## 🔴 CORE — before the core calcifies (14)

Ordered by dependency, not importance.

### Safety and admission

| # | Gap | Article | Where |
|---|---|---|---|
| 9 | Gate still fails open in Python (Rust already refuses) | TOLERANCE | Python |
| ~~10~~ | ~~`min_privilege` never read~~ — **DONE in Rust** (edge-based privilege, monotone across the holon path, enforced at the gate). Python unchanged. | ENZYME · TOLERANCE | Python |
| ~~11~~ | ~~Only `refuse` is built~~ — **DONE in Rust** (refuse / clamp / defer, with the paper's three clamp safety rules enforced at authoring time). Python unchanged. | LAW | Python |
| 12 | `simulate.run_path` advances a fork with **no gate** | TOLERANCE | Python — *structurally absent in Rust: there is one execution path, so a fork cannot skip it* |
| 13 | `api.get` / `api.post` make **real outbound calls** outside the egress boundary | TOLERANCE | Python — *structurally absent in Rust: the core has no network* |

### The primitives

| # | Gap | Article | Where |
|---|---|---|---|
| ~~14~~ | ~~State is untyped~~ — **DONE in Rust**: declared record type with bounds, load-time predicate binding, and organisational closure enforced at the gate. Opt-in per sustain. Python unchanged. | CELL | Python |
| ~~15~~ | ~~No `inverse`~~ — **DONE in Rust**: patch-level inverse derived from the mutation record. Honest about the three shapes that are not invertible from a record alone. Python unchanged. | ENZYME | Python |
| ~~16~~ | ~~No **checked composition** of operator pathways~~ — **DONE in Rust** (`compose.rs`, 2026-08-12): Hoare sequencing with a three-valued ⊨, `wp` pullback, and a `Pathway` that cannot be constructed around a refuted link. Python unchanged. | ENZYME | Python |
| ~~17~~ | ~~Ordering is `seq`~~ — **DONE in Rust**: event-time ordering with Lamport `(t_event, id)` tie-break, causal stamps, and a converging LWW join. Python unchanged. | RECORD · GAIA | Python |
| ~~18~~ | ~~No substrate dedupe~~ — **DONE in Rust**: uniform dedupe on the stable id, so at-least-once delivery gives exactly-once effect. Python unchanged. | RECORD | Python |

### Consistency

| # | Gap | Article | Where |
|---|---|---|---|
| 19 | Council keeps **two stores** (state-based and SQL) that can diverge | SLIME | Python |
| ~~20~~ | ~~Rollback registry in-memory~~ — **DISSOLVED in Rust**: the inverse is *derived* from the event record, so there is no registry to lose. Python unchanged. | Controller | Python |
| 21 | **Two database layers** — routes on SQLAlchemy, engine on sqlite3 | ADR-0001 D2 | Python |
| 22 | `seed.py` **bypasses the engine** — half-made sustains, no genesis event | — | Python |

---

## 🟡 FOLLOW (13)

### Observation

| # | Gap | Article |
|---|---|---|
| ~~23~~ | ~~No Kalman / EWMA / CUSUM — urgency is a single `pct`~~ — **DONE in Rust** across two slices (2026-08-12): `region.rs` made `urgency(s) = d(s,V)` real (replacing the `pct` proxy *and* its `allocated <= 0 → 0.0` blind spot), and `detect.rs` added **EWMA + CUSUM** over that series plus the severity classification that is the **formal Monitor→Controller boundary**. **Kalman is a declined import, not a gap** — MON-11's own fit caveat says it assumes a continuous ODE that discrete event-sourced state is not, and the Sustena-native form now ships end to end. Python unchanged. | Monitor · PERCEPT |
| 24 | No MonitorEngine; scheduling external, no heartbeats | Monitor |
| 25 | Widget type-checker exists with **no application call site** | PERCEPT |

### Editing

| # | Gap | Article |
|---|---|---|
| ~~26~~ | ~~**No definition version history** — edits in place, cannot roll back~~ — **DONE in Rust** (`version.rs`, 2026-08-12): append-only DAG, head is a pointer, nothing overwritten. Python unchanged. | SLOPE |
| ~~27~~ | ~~3-valued rollback (Total / Partial / Unavailable) not built~~ — **DONE in Rust**: and `Total` has no Δ field, so reporting Partial as Total is unspellable. Python unchanged. | SLOPE |

### Grammar

| # | Gap | Article |
|---|---|---|
| 28 | No arithmetic in the DSL (blocks `ratio * balance`) | GENOME |
| 29 | No dynamic bracket indexing (`members[loan.member_id]`) | GENOME |

### Operatives and network

| # | Gap | Article |
|---|---|---|
| 30 | 4 dormant operatives call the LLM directly, propose operators that don't exist | SCOUT |
| 31 | Streaming protocol is a no-op | SCOUT |
| 32 | CRDT merge, Lamport ordering, quorum consensus — deferred | SLIME |

### Naming

| # | Gap | Article |
|---|---|---|
| 39 | **The physical code-rename has never been executed.** Pawa (line 11) states it as a live debt: *"In code the ratified names **Enzyme**, **Symbiont**, and **Embroidery** are still `operator`, `operative`, and `dsl` — aliases until the build-phase rename."* Retire Operator→**Enzyme**, Operative→**Symbiont**, DSL→**Embroidery**, and `operator-DAG`→**`Enzyme-DAG`** across ~500+ occurrences. The articles, the GLOSSARY and this WBD already use the ratified names; **only the code does not**, so every reader currently translates between two vocabularies. **Policy (standing, ratified):** aliases and re-export shims first, **gradual module-by-module**, **never a big-bang**, and **nothing merges without a green build** — locally *and* in CI. **Sequence it against the Rust port deliberately:** `sustena-core` is new code and can be born with the ratified names, which makes the Rust boundary the natural place to stop paying the translation cost. | Pawa (l. 11) · GLOSSARY |

### Housekeeping

| # | Gap |
|---|---|
| 33 | Legacy quarantine approved but **never executed** |
| 34 | `/events` queries a non-existent column → silently returns empty |
| 35 | Logout does not revoke — `sustains.py` skips `token_version` |
| 36 | 4 overlapping widget-render systems |
| 37 | Migrations hand-rolled; `alembic/` empty |
| 38 | Firestore sync is a mock — no real cross-device sync |

---

## ⏸️ DEFERRED — travels with the economy layer (6)

Held back with the economy layer per ADR-0001 Decision 5, pending legal review.

| # | Gap | Article |
|---|---|---|
| E1 | Two-type royalty split — `charge()` still runs the old 70/20/5/5 | Mycelium |
| E2 | `charge()` has **zero callers** — the ledger is never used | Mycelium |
| E3 | Pawa meter not wired to the ledger | Pawa |
| E4 | κ constants uncalibrated | Pawa |
| E5 | Arena `trust_score` and `downloads` are dead constants | Arena |
| E6 | Arena orders do not settle | Arena |

---

## What still needs a decision

**Nothing on the core list.** #17 was resolved by re-reading the articles: they
already specify event-time convergence, so it was settled spec rather than an
open fork.

**Standing rule, from that exchange:** if the articles already answer a
question, it is not an open decision — implement it and note it. Surface only
genuinely open forks: ones the articles are silent on, or where reality clashes
with the spec.

**#35 and #13 are live security items** sitting in FOLLOW: logout does not
revoke a session, and two operators make real outbound HTTP outside the egress
boundary. Both are real today on the live host. Pull forward on request.

---

## Working rules for R2

1. **Ground every slice in its article.** Cite it in the commit.
2. **Prove it with vectors.** A slice is not done until both engines agree, or
   until the vector records a deliberate, documented divergence.
3. **Python stays live and untouched** unless the item is a correctness fix to
   the reference itself.
4. **Build clean.** Several R1 gaps closed simply by not reproducing the old
   shape. Prefer making a defect unrepresentable over guarding against it.

---

---

## ⚠️ Whole-system reconciliation (2026-08-12) — checked against SUSTENA_UPGRADE_SPEC.md

This backlog covers the **foundation + walking skeleton** well. But the master spec (`SUSTENA_UPGRADE_SPEC.md`, §9.1) defines a three-module **operational spine — Monitor / Tenet / Controller** — and **two of the three are not yet tracked here.** Added below as pending so the backlog is comprehensive.

### ✅ SPINE — **all three modules now shipped in Rust** (2026-08-16)

**Monitor ✅ · Tenet ✅ · Controller ✅.** The spine the master spec puts at the centre is no longer a gap: `region.rs` + `detect.rs` (Monitor), `tenet.rs` + `ensemble.rs` (Tenet), `controller.rs` (Controller), with `kernel.rs` underneath all three. **And as of 2026-08-16 they are wired into one cycle** — `ooda.rs` (CTL-3) sequences them, with the human structurally at DECIDE. What remains is the surrounding assembly: the `MonitorEngine` that owns the detection chain per sustain, holarchy escalation, panels. Not the mathematics, and no longer the loop.

| # | Gap | Article | Where |
|---|---|---|---|
| ✅ T1 | ~~**Tenet optimisation engine (§4)**~~ — **MATHEMATICS COMPLETE in Rust across three slices** (2026-08-16). `tenet.rs`: `T:S×A→Δ(S)`, the inversion point, **backward induction** → `J` + a complete policy, and the **CTL-12 join** (the `H=1` sweep IS the Controller's greedy Lyapunov step, checked by a vector). `ensemble.rs`: **scenario ensembles, invariant actions, decision nodes, dead drops** — the sweep run unmodified once per declared future, read for its disagreements. `pincer.rs`: **signal functions over the trajectory + the temporal pincer** — forward on a sampled `T`, backward on a declared cadence or a fired signal, consuming the Signal primitive so a persistent warning is debounced rather than re-triggering forever. **Only TEN-11 (pawa-efficiency) remains, and it is parked on the economy rather than on Tenet.** Python unchanged. | Tenet / Temporal Decision Architecture | Python |
| 🔨 G1 | ~~**Controller decision-math (§3)**~~ — **the math AND the loop are DONE in Rust** (`controller.rs` 2026-08-12; `ooda.rs` 2026-08-16): Lyapunov distance-to-V urgency, `should_surface` (the SNR filter), `is_stable_intervention`, the Sheridan `AUTOMATION_LEVEL` dispatch — and now **CTL-3, the OODA state machine**, which assembles OBSERVE→ORIENT→DECIDE→ACT out of the modules that own each phase. §III's separation is structural: the machine **cannot** fabricate an observation or a ranking, and **has no method that mints an approval token**, so the loop closing through a human is a property of what it cannot do. A vector walks a full cycle with the gate's commit asserted. **Still open: holarchy escalation (CTL-5), the IoT bridge (CTL-6), persistent panels (CTL-8), damping (CTL-11)** — none of which changes the loop. Python unchanged. | Controller / The Expanse | Python |

### 🟡 Confirm coverage (built in Python — verify the Rust plan carries them, core vs app-layer)

- **Curated UI `compose(r)` / attention budget (§4H)** — built in Python (ORCHIE).
- **Ingest transducer / parse-rules (§4K)** — built in Python.

**Caveat (honest):** T1 and G1 were never in the walking skeleton either — not built in Python, not "dropped" by Rust; they are the spec's *next depth*. The risk is only that treating this backlog as "the whole system" would silently omit the simulate-optimise (Tenet) and govern-decide (Controller) depth the master spec puts at the centre.

### ✅ N1 — the approval token — SHIPPED IN RUST (2026-08-12)

`sustena-core/src/approval.rs`, grounded in **Operative §XVI** and **Capstone §VI.1 duty 3**.

The gate clause is now the article's, in full:

```text
admit(o,s) ⟺ g_o(s) ∧ o(s)∈A ∧ D(s,o(s))
             ∧ (effect_class = sandbox ∨ valid_token(approve(o, principal)))
```

Three things are **unrepresentable** rather than guarded against:

- **A live effect with no approval.** `EffectClass::Live` *carries* the token —
  the variant that would spell "live, unapproved" does not exist.
- **An approval that skipped simulation or the vote.** `ApprovalToken` has
  private fields and no public constructor; the only route is
  `Simulated → Voted → approve()`, each stage consuming the previous **by
  value**. That is OPV-31's trace invariant enforced by move semantics rather
  than by a runtime scan something could forget to run.
- **A token matching the wrong act.** The binding is kept structurally
  (operator + the real parameters, compared by equality), not as a digest, so
  there is no collision surface to find.

**Divergence, recorded not silent:** Python has no approval token
(`approval_token` / `valid_token` / `effect_class` are grep-0, re-confirmed).
`conformance/vectors/approval.json` carries an explicit `rust-ahead-of-python`
block, and a test asserts it still exists. These become parity vectors unchanged
if the reference catches up.

**Left open, deliberately, and named:** `execute_admitted` now takes nine
arguments. The fix is a context struct carrying registry/allowed/enforcement —
a real API refactor touching every call site, and not something to do as a side
effect of this slice.

### ✅ EDIT-12 — μ + Expand–Migrate–Contract — SHIPPED IN RUST (2026-08-12)

`sustena-core/src/migrate.rs`, grounded in **Editing §VIII**. **This completes
the engine half of M-EDIT.**

**μ finally has a representation.** Until now the classification was binary —
*compatible*, or *refused* — because "migratable" needs a migration function to
represent it with, and there was none. `Migration::Apply(Mu)` makes `Safe(e, μ)`
judge **μ(sᵢ)** rather than `sᵢ`, which is the whole difference between refusing
an edit and carrying the instances into it. A vector shows the same edit against
the same instances: refused under `μ = id`, admitted under a declared μ.

**The ordering is the safety property, so it is one call.** `Emc::run` executes
expand → migrate → contract; there is **no public way to run the contract
alone**. `D†` is committed as a real version node with both shapes present, so
the transition period is inspectable rather than a moment nobody can see, and the
one destructive step is sequenced last.

- **`e₊` is auto-safe with no scan** — and that required sharpening EDIT-6:
  `can_strand()` now names exactly `AddInv`, `ModifyInv`, `RetypeDim`, because
  `Safe` evaluates the candidate's invariants against live states, so an edit
  strands only if it adds or tightens an invariant.
- **`Safe(e₋, id)` is verified, not trusted.** The article says it holds by
  construction; if μ was wrong, saying so at the contract beats discovering it
  later.

**Compatibility is told at author time.** `Emc::plan` computes both directions
before anything runs — *forward* compatibility is required **because rollback
exists**, and an edit that is only backward-compatible is one-way. *"This commits
you"* is a fact the author is entitled to, not a discovery made during a
rollback.

**Knock-on to EDIT-7:** a μ that journals its pre-image collapses the `Partial`
rollback region to `Total`. `Emc::run` does it automatically — a bare `RetireDim`
of a genesis dimension rolls back `Partial`; the same reshape through EMC rolls
back `Total`.

**A correction found by testing:** `rollback` only consulted the *parent's*
journal, so a pre-image written on the node that did the erasing — where an
author would naturally put it, and where `commit_with_pre_image` puts it — was
not read. Fixed to check the erasing node's own journal first; the EDIT-7 unit
test that documented the old limitation was corrected rather than worked around.

**Divergence — rust-ahead, with the counterweight.** The reference has no μ, no
transition period, and names neither direction of compatibility. But its
refuse-if-unsafe **is** a real safety property and is at parity in shape with
`Safe(e, id)`. What it lacks is the escape: it can say no, and cannot offer a way
through.

### ✅ EDIT-7 / #26 / #27 / N3 — version history + the algebra of rollback — SHIPPED IN RUST (2026-08-12)

`sustena-core/src/version.rs`, grounded in **Editing §V**. **This closes N3**,
recorded as the sharpest remaining gap.

**The fix.** The reference engine runs `UPDATE sustain_templates SET spec_json =
?, version = ?` — an in-place overwrite that destroys the predecessor. The
counter increments and nothing is versioned, so there is no `D_{n-1}` and
therefore no `e⁻¹` to apply. Here `VersionDag::commit` **appends**, `head` is a
**pointer rather than a counter**, and each node records the full
`⟨D, parent, e, μ, author, t⟩`. Two edits can branch from one parent — which an
integer cannot express, and which is the structural reason the article says the
version field becomes a head pointer.

**Two inverses, and only one is easy.**

- `e⁻¹(e(D)) = D` — the document. Derived over the EDIT-8 taxonomy against the
  **parent** node (undoing a `DropInv` needs the expression that was dropped, and
  only the pre-state has it). Round-trips are applied and compared, not asserted.
- `μ⁻¹(μ(s)) = s` — the instances. **Not generally available**, because μ is
  frequently not injective (Fagin 2007).

**Three-valued rollback, and the lie made unspellable.** `Rollback::Total` has
**no field** for unrestorable dimensions, and `Rollback::classify` is the only
constructor — it returns `Partial` whenever Δ is non-empty. *Reporting Partial as
Total is the same class of lie as a clamp reported as an admit*, and here it is
not a mistake to guard against but a thing that cannot be written down.

**Both pre-image escapes work.** Free: **fold the log** — the `AddDim` that
introduced a dimension is already in the history, so `μ⁻¹` becomes a lookup at no
storage cost. Paid: **journal it explicitly**, which upgrades an otherwise-lossy
edit from `Partial` to `Total`. A dimension present since genesis has neither, and
that is an honest Δ rather than a contrived one.

**A groupoid, not a group.** `path_between` returns `None` for two versions on
branches that never meet, rather than inventing a route; each definition is its
own identity; and a composed path is **no more restorable than its worst link**.

**Divergence — rust-ahead, with the breadcrumb noted.** Python has no history
table, no parent pointer and no rollback of definitions at all. It *does* bump a
`version` integer and stamp `updated_at`, which tells you *that* something changed
— an audit breadcrumb, not a history. The μ⁻¹ entry also records that **neither**
engine can restore what was genuinely forgotten; the difference is that this one
says so, with Δ naming the dimensions. Tests assert both notes stay.

### ✅ #16 — checked composition of Enzyme pathways — SHIPPED IN RUST (2026-08-12)

`sustena-core/src/compose.rs`, grounded in **Operator §IV** (Hoare sequencing).

```text
post(A) ⊨ guard(B)                       when two Enzymes may chain at all
g_{A;B} = g_A ∧ wp(e_A, g_B)             guard pulled back through A's effect
e_{A;B} = e_B ∘ e_A                      function composition (associative)
ε_{A;B} = ε_A · ε_B                      concatenation in the free monoid E*
```

**The point: illegal-chain discovery moves from RUN time to WRITE time.** Python
chains dynamically, so a bad chain is found by running it and watching step three
refuse. And the property is **structural**, not a validation pass:
`compose(A,B) -> Result<Composed, Rejected>` means a refuted chain yields no
composite value, and `Pathway::try_chain` has **no constructor that takes steps on
trust** — a broken recipe cannot be *held*, not merely rejected on save.

**Three-valued ⊨, stated as such.** `Refuted` is the strictly stronger claim that
`post ∧ guard` is unsatisfiable — not "we failed to prove it", which is
`Undecided`. That distinction is what licenses refusing to build a chain rather
than warning about it, and a boundary pair of vectors pins it: `x <= 0` then
`x >= 0` touches at a point and is **undecided**; `x < 0` then `x >= 0` is
**refuted**.

**Decidable fragment:** conjunctions of path-vs-literal interval constraints.
Everything else — OR, NOT, quantifiers, `!=`, params — returns `Undecided` with
`runtime_guard_required` and a reason. **No SMT solver** was pulled in; a
dependency-free core is what lets this crate ship to a phone.

**`wp`, and the honest limit.** Effects are Rust functions, so `wp` is computable
only where an operator declares an `EffectSummary`: unchanged path (guard passes
through), `SetTo` (constant-folds), `ShiftBy` (moves the literal, staying in the
fragment). Where none is declared, `wp` is **not computed** and the composite
carries the runtime obligation instead of implying it was discharged.

**Divergence — rust-ahead, with the counterweight.** Python has no compose-time
check at all. But `OperativeGraph` *does* validate at load time that every node
names a registered operator — real, and worth not erasing: it checks a step
**exists**, not that it can **follow**. Both notes are asserted by a test.

**Partially advances #16's neighbour, OP-3:** `wp` now exists, but using it as an
author-time check on a *single* Enzyme's `g ⟹ wp(e,Q)` needs an `EffectSummary`
on `OperatorMeta` — a separate change to every operator declaration.

### ✅ CON-1 / CON-7 — transition constraints `D(s,s')` + conservation — SHIPPED IN RUST (2026-08-12)

`sustena-core/src/transition.rs`, grounded in **Constraint §I** (three predicate
families) and **§VI** (conservation).

The gate's **third conjunct**, and the last of the three to be built:

```text
admit(o,s) ⟺ g_o(s) ∧ o(s)∈A ∧ D(s, o(s))
```

`D` is a predicate on the ordered **pair**, so it can say things no state
constraint can. All three canonical shapes ship: **rate limit**, **monotonicity**,
**conservation**.

**"Money is conserved" is expressible for the first time.** A vector proves the
point directly: a step that credits a pocket with no matching debit produces an
after-state that is *individually valid* — every balance non-negative, every
pocket well-formed — and commits happily with `D` absent. Only the step is
wrong, and only `D` can see it.

Two things are **unrepresentable** rather than checked:

- **A clamped conservation law.** `TransitionRule` has **no strategy field at
  all**. Capping a transfer's outflow while leaving the inflow untouched creates
  money — it satisfies `C` by violating `D` — and the way to prevent that is to
  leave nowhere to write the clamp down.
- **Money summed in floating point.** `Tolerance::Exact` sums as `i128` over
  whole minor units, so tolerance 0 is genuinely exact.

**One correction found by testing, not inspection:** exact mode initially
refused `95000.0`, which would have made the law undeclarable over any real
ledger — the engine's arithmetic promotes to float (Python's rule, preserved in
R1). It now accepts an **integral** float (same count of minor units, differently
written) and still refuses a **fractional** one, which is the property that
actually matters.

**Divergence — rust-ahead, with the honest counterweight.** Python has no `D`.
But it *does* give `holon.transfer` transactional **atomicity**, which is not
nothing — and is also not conservation: it never checks the debit and credit are
equal, and it is hardcoded into one operator rather than declared over every
step. `constraints.json` records both halves so the gap is neither overstated
nor understated.

**Also closed:** CON-2's third conjunct. The §4E seam's **duty 2** now covers
`D` as well as the invariants.

### ✅ EDIT-8 — edit authority, the meta-gate — SHIPPED IN RUST (2026-08-12)

`sustena-core/src/editing.rs`, grounded in **Editing §VI** and **Capstone §VI.1
duty 5**. Built on the N1 token shipped the same day.

```text
admit(e, ⟨D,s⟩) ⟺ tok(α,e) ∧ ⊢e(D) ok ∧ Safe(e,μ) ∧ applicable(e,D)
```

The meta-Sustain proposition is the point: Σ↑ has state 𝒟, Enzymes the edit
taxonomy, and `V↑ = {D' : ⊢D' ok ∧ ∀i: μ(sᵢ) ∈ V_D'}`. `admit(e,·)` **is**
§4E's gate on Σ↑, so §4E's induction applies unchanged — a conformance vector
walks it rather than asserting it.

**Authority does not lift from below**, structurally:

- `EditEffect` has **no `Unchecked` variant**. The operator gate needed one for
  R1 parity; the meta-gate is new, so no bypass had to be preserved and none
  exists.
- `EditToken::mint` **refuses** a `Governance::Shared` definition from any
  single actor. The only other route is a `CouncilMint` — private fields,
  constructible only from a decision that **passed AND met quorum**.
- *Permission to spend from a pocket is not permission to redefine what a
  pocket is* — a compile-time fact, not a convention.

**The M-MUL composition point, made honest:** quorum *arithmetic* (who counted,
against what threshold) belongs to Multiparty and is unbuilt. `CouncilMint`
takes `quorum_met` and **refuses when false**, so the dependency is an
obligation the host cannot skip silently rather than a check this module
pretends to perform.

**Also closed by this slice:** EDIT-2 (the typed taxonomy), EDIT-3 (μ = id),
EDIT-4 (fail-safe on an uncompilable candidate), EDIT-6 (loosening skips the
scan — the concrete payoff of a typed `e`), EDIT-10 (whole-definition typing).

**Scope, named:** `Safe(e, μ)` is built for **μ = id** only. A real migration
function has no representation anywhere — that is **EDIT-12**
(expand–migrate–contract), and `Migration` names it rather than stubbing it.
`ModifyOp` and the peripheral family are absent because guards and effects are
Rust fns in this core, not data.

**Divergence — partial, term by term.** `tok(α,e)` and `applicable(e,D)` are
rust-ahead. **`Safe(e,μ)` is at parity in shape** — the reference genuinely
implements §III, witness set and all — and a test asserts the vector keeps
saying so, because claiming the whole module as rust-ahead would overstate the
gap.

### 🔵 Ten further gaps found by the full article reads — see the WBD

The completed WBD (`SUSTENA_UPGRADE_SPEC.md`) records **N1–N10**, found by reading the
last eight technical articles in full. They are not duplicated here; the WBD's
*"New gaps found by the final eight reads"* table is the live list. The three worth
knowing about before opening the next slice:

- **N1 — the approval token is grep-0.** One object discharges obligations in three
  articles (Operative §XVI, Editing §VI, Capstone §VI.1). The trace invariant is
  prefix-closed, so it can be **structural rather than promised**.
- **N2 — `publish` is a route, not an Enzyme**: a second write path into the Arena.
- **N3 — a definition edit destroys its predecessor.** Sharper than #26: rollback has
  nothing to roll back *to*.

*Last updated: 2026-08-16 — **the spine is assembled.** M-TEN's mathematics completed across three slices (`tenet.rs`, `ensemble.rs`, `pincer.rs`), built on **the Signal primitive** (`signal.rs`, MUL-4/MUL-15) that MON-13, CTL-3 and TEN-8 all ride — and then **CTL-3** (`ooda.rs`) wired Monitor→Tenet→Controller into one OODA cycle with the human structurally at DECIDE. **Monitor ✅ · Tenet ✅ · Controller ✅**, and now one loop rather than three modules that could be. What is left across T1/G1: TEN-11's pawa term (parked on the economy), the MonitorEngine, holarchy escalation, the IoT bridge, panels and damping.*
