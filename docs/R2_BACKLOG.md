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
| Conformance vectors | R1 parity + R2 spec, incl. **22 approval**, **26 editing**, **29 constraint**, **31 compose**, **16 version**, **15 migrate**, **20 region**, **17 detect**, **20 controller**, **18 kernel**, **16 tenet**, **18 ensemble**, **17 signal**, **16 pincer**, **17 ooda**, **17 monitor**, **15 holarchy**, **14 population**, **17 consensus**, **13 router**, **16 vclock**, **16 disaggregation**, **21 division**, **20 crdt**, **17 operative**, **16 goodhart+presentation**, **14 mixture**, **16 criticality**, **15 cynefin**, **15 boundary**, **14 flow**, **8 obligation**, **18 clocks**, **17 watermark**, **20 period**, **16 checkpoint**, **19 dimension**, **14 belief**, **16 harmonics**, **13 windowing**, **14 observability**, **14 damping** cases (divergences recorded) |
| Rust tests | **825 unit · 586 conformance tests** across 43 binaries, plus 2 doctests (one a `compile_fail` proof) — all green |

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
| ✅ 24 | ~~No MonitorEngine; scheduling external, no heartbeats~~ — **the engine is DONE in Rust** (`monitor.rs`, 2026-08-16): per-sustain chains keyed by id with `flatten_holarchy`, the **native** `ingest()` (`W = d(s,V) → EWMA → CUSUM`, no Kalman), and the escalation that makes **OBSERVE self-driving** into the OODA loop. The diagnosis behind this row was exactly right and is worth keeping: the reference watches by RECOMPUTING per request, so an accumulator had nowhere to live. **Scheduling stays external on purpose** — no clock or `tick()` in core, the same boundary CTL-9 draws. Python unchanged. | Monitor |
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

**Monitor ✅ · Tenet ✅ · Controller ✅.** The spine the master spec puts at the centre is no longer a gap: `region.rs` + `detect.rs` (Monitor), `tenet.rs` + `ensemble.rs` (Tenet), `controller.rs` (Controller), with `kernel.rs` underneath all three. **And as of 2026-08-16 they are wired into one cycle** — `ooda.rs` (CTL-3) sequences them, with the human structurally at DECIDE. **And as of 2026-08-16 the loop feeds itself**: `monitor.rs` (MON-9) owns the detection chain per sustain and drives OBSERVE, so a state update reaches a surfaced decision without anyone stepping the machine by hand. **And as of CTL-5 the cycle reaches every level**: a breach at a nano-sustain climbs the holarchy until it finds a level that can act. What remains is the belief tracker, the preattentive encoder, the IoT bridge and panels — not the mathematics, no longer the loop, no longer its input, and no longer its reach.

| # | Gap | Article | Where |
|---|---|---|---|
| ✅ T1 | ~~**Tenet optimisation engine (§4)**~~ — **MATHEMATICS COMPLETE in Rust across three slices** (2026-08-16). `tenet.rs`: `T:S×A→Δ(S)`, the inversion point, **backward induction** → `J` + a complete policy, and the **CTL-12 join** (the `H=1` sweep IS the Controller's greedy Lyapunov step, checked by a vector). `ensemble.rs`: **scenario ensembles, invariant actions, decision nodes, dead drops** — the sweep run unmodified once per declared future, read for its disagreements. `pincer.rs`: **signal functions over the trajectory + the temporal pincer** — forward on a sampled `T`, backward on a declared cadence or a fired signal, consuming the Signal primitive so a persistent warning is debounced rather than re-triggering forever. **Only TEN-11 (pawa-efficiency) remains, and it is parked on the economy rather than on Tenet.** Python unchanged. | Tenet / Temporal Decision Architecture | Python |
| ✅ G1 | ~~**Controller decision-math (§3)**~~ — **the math, the loop AND the escalation are DONE in Rust** (`controller.rs` 2026-08-12; `ooda.rs`, `holarchy.rs` 2026-08-16): Lyapunov distance-to-V urgency, `should_surface`, `is_stable_intervention`, the Sheridan dispatch; **CTL-3's OODA machine** assembling OBSERVE→ORIENT→DECIDE→ACT with §III's separation made structural and no method that mints a token; and **CTL-5's holarchic escalation**, walking the `MonitorEngine`'s parent chain until a level can act — *"the escalation path IS the holarchy"*, so no escalation graph was introduced. **Still open: the IoT bridge (CTL-6), persistent panels (CTL-8), damping (CTL-11)** — none of which changes the loop or the escalation. Python unchanged. | Controller / The Expanse | Python |

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

*Last updated: 2026-08-17 — **the spine is assembled and the coordination layer is built end to end.** M-TEN's mathematics completed on **the Signal primitive**; **CTL-3** wired Monitor→Tenet→Controller into one cycle with the human at DECIDE; **MON-9** gave OBSERVE an engine that drives it; **CTL-5** made a violation climb until a level can act; and Multiparty went substrate → **Signal** → **Population** → **Consensus** (closing EDIT-8's `CouncilMint` loop) → **Router** (**completing MUL-15**) → **vector clocks** → **disaggregation**, which closed the aggregation bracket the Population opened: a hysteresis band that turns eight readings into two transitions instead of five, and *departure removes an edge, never a node* asserted rather than described → **division of labour**, which answered the question the coordination layer had been assuming away — *why a node would spend itself* — with `rb > c` enforced as a **refusal** (an unstable split cannot be constructed, only diagnosed), `r` **derived from the composition roll-up**, and a cheaper-covered-redundant split refused with `price_of_stability()` reporting what refusing it cost → **the wider CRDT family**, which closed the last row: G-counter, PN-counter, OR-set and RGA, with §VI's three laws *executed* (every permutation folded, then folded again with every update duplicated, one state out of all of it) and *never erase the node* turned into a **theorem about `max`** rather than a property of how the fold happens to be written. **Monitor ✅ · Tenet ✅ · Controller ✅ — and ★ M-MUL is COMPLETE, every row, the first article module finished end to end.** **★ M-OPV is now OPEN**: `ω` is built with no `S` and no `T` on it, so the sharing constraint is unrepresentable to violate — Prop 1 structural (a move outside `T` is unconstructible), Prop 2 at the type level (`reachable` is never passed a population) — and **`u_i` is a vector that stays one**, with a non-dominated option in a non-convex notch kept by the frontier and selected by **none of 1001 weight vectors**. That unblocked three agent-layer rows at once, and **two of them shipped the same day**: the **Goodhart guard** (OPV-27) — a gate-enforced invariant over `U`, in its own module beside the gate because §XIV's whole point is that the fix leaves the agent layer, with damage to `U` proven invisible to **every** operative and refused only by the gate — and **present-the-frontier** (OPV-29), where no method returns one option, collapsing needs a declared rule and reports what it hid, and the proof ties §XV back to §II by computation: the option 1001 weight vectors deleted is the option a single-winner surface hides. **And the MoE router (OPV-28) followed the same day**, closing §XV's third duty: a declared-weight top-`k` gate whose **sparsity is structural** — the router performs the expensive call, so an unselected operative's path is unreachable rather than merely skipped (a counting closure enters 2 of 4, never the other two) — with the **routing collapse walled off from the surface and the wall proven**, not asserted: the frontier is byte-identical before and after routing. It surfaces a coverage gap rather than routing to the least-bad scorer, and takes the mixture for **selection only**, since averaging expert outputs would be §II's collapse one layer on. **§XV's three duties are now all at least partial.** **And the criticality read (OPV-14) followed**, with the risk arithmetic built as a **type**: there is no method that returns a bare expected cascade size, because §VIII's whole point is that a mean at or above criticality would mislead rather than merely be uncertain — withheld *at* the band too, since being inside it is not knowing which side of `τ ≤ 2` you are on. It is read-only by construction (`&[Event]` in, reading out) and it **corrected three premises by checking them**: the reference has no `causes` column (the causal edge is known at runtime and thrown away — one column, not a redesign); the Monitor keeps no second moment, so detector 3 works over a supplied window; and **this backlog's own prior claim that OPV-14 would finish two duties was wrong** — σ̂ is *one input* to a domain reading. ★★★ **OPV-16 closed them, and §XV's THREE DUTIES ARE NOW ALL BUILT** — Orchie routes sparsely on a domain reading, holds the integrating view including the criticality signal, and presents the frontier rather than a winner. What closed Route specifically: an **indeterminate** `dom(s)` became representable, so a caller with no reading surfaces **disorder** instead of fabricating a regime to route at all. `dom(s)` itself stays **supplied** — §IX assigns the classifier to the Monitor (CAP-13), a separate **layer** rather than an agent-layer gap, and that distinction is what makes `Built` honest. The same slice made **disorder undeclarable by construction** (`Cynefin` has four variants, so there is no fifth to write) and **reconciled coverage to one source of truth** — `mixture`'s copy was removed, not left to drift. ★ **Phase 1 opened 2026-08-17 with SUS-7, the boundary `μ` + the autopoietic closure law** (`boundary.rs`): `B = ⟨scope, μ⟩` replaces the flat `owner_ids` the article names by its symptoms, and the closure law `μ_{o(s)} = μ_s ∧ schema(o(s)) = schema(s)` is enforced **at the gate in both halves** — a registered operator that appends to the roster is refused with state untouched. Boundary change is a **separate higher gate that reuses `editing.rs`'s authority** rather than forking it, so two paths mean **nothing to exempt**. It closes SUS-7, SUS-8 and OP-10 together, and **unblocked the firewall `F`** (CON-1) — **which shipped the same day** (`flow.rs`): all three predicate families now exist, and `F`'s irreducibility to `C` and `D` is **proven rather than asserted** — 500 between two own pockets versus 500 out and 500 back give byte-identical endpoints that **both conserve** under `Tolerance::Exact`, and only `F` separates them. A flow is defined relative to `μ` (the operator declares a movement, `μ` decides whether it crossed), flows are **declared not inferred**, composition is conjunction only, and the clamp trap is honoured by construction — `FlowRule` has no clamp strategy to declare. **And OP-3 closed the Operator layer the same day** (`obligation.rs`): the author-time obligation `g ⟹ wp(e,Q)` on a single Enzyme, reusing `compose.rs`'s `wp` and building no prover. ★ **It FLAGS, it does not block** — §III asks for a diagnostic, so nothing is called from `execute` and a test asserts an unsound operator is flagged at author time **and still runs**, caught dynamically after the fact. Three honest outcomes, and the third is the point: `Unavailable` exists because silently passing an unsummarisable effect is the *silently trusted* the row exists to prevent, and flagging one would report a defect not shown. ★★ **The finding:** every shipped operator is `Unavailable`, because Sustena's effects are parameter-driven — so the static check's reach today is zero operators proven either way, which is precisely why §III calls the dynamic check the honest position. Running it is what demonstrates that. ★ **And Phase 1's last block opened with EVT-5, the two clocks and the skew between them** (`clocks.rs`) — a **different** two-clock split from EVT-8's: that one separated physical from causal time, this separates **when it happened in the world** from **when the system heard about it**. ★★ The load-bearing property is that a **negative** skew survives: §III requires it *recorded and surfaced, never silently clamped*, and the clamp is made **unspellable** rather than merely refused — `Skew::Observed` is signed, `skew_of` is one subtraction that floors nothing, and `clock_findings` takes an **immutable** slice, so the surfacing path has no mutable access to the record it reports on. ★ Three readings, and two are deliberately not numbers: an absent arrival reads `Unknown`, a backfilled row reads `Inferred` (its two clocks read equal, so the arithmetic is 0 and *means nothing*), and `millis()` returns `Option`, so neither can decay into a `0` that passes for a punctual arrival. Skew is unbounded on purpose — no threshold, no rejection, no error type anywhere in the module — so §III's week-long forwarding-macro outage is accepted, a week-late event still orders back into its own past, and **EVT-6 is unblocked without being pre-empted**. ★★ **The divergence is the sharpest one yet, and most of the record is already there:** the reference has a real arrival clock (`ingest_messages.received_at`) and a real source (`source_id`), so the gap is the OTHER clock — and its transducer **captures the SMS's own date and time in 13 patterns and discards both one line later**, which makes ING-13 a `parsed_fields` change rather than a parser one. `trust`/`provenance` also already ship, on `ParseRule` rather than on the event: unapplied, not missing. ★ **Reconciling before building corrected three of this tracker's own claims:** `causes` and `provenance` were listed pending and were already present (re-read, not re-built), and `payload` was listed as present and **is not** — it lives on `EmittedEvent` and never reaches the durable record, so **EVT-1 stays 🔨 at six of seven fields** with that residual named rather than closed on paper. ★★ **And EVT-6 followed the same day — the row the whole substrate had been pointing at** (`watermark.rs`). *No window closes by wall-clock hope* is now **structural**: `close` takes a `&Watermark` and nothing else, there is no `close_at(tau)` and no `From<i64>`, so a caller holding only a processing clock cannot ask the question. §4J.3's failure is exactly `W(τ) = τ`, and it stays **possible but stops being accidental** — it requires declaring `SourceGuarantee { max_out_of_orderness: 0 }`, the claim *this source never delivers out of order*, in as many words. A case runs that zero bound and an honest 2h bound at the same τ over the same window: one closes, the other does not, and **the difference is entirely a declaration someone had to write down.** ★ Monotonicity is structural and **deliberately distinguished from EVT-5's clamp rule** — flooring a measurement hides a fact; non-decreasing *is* the definition of a watermark, a retreat would un-close a window that had already answered, and the rejected proposal is reported rather than discarded. ★ The heuristic is **estimated from EVT-5's surfaced skew** rather than invented, honouring its three readings — `Unknown` and `Inferred` are not zero skew, and with none observed the estimator **errors rather than returning a zero bound**, because *an unmeasured source is not a punctual one*. ★★ And **no volume of data promotes an estimate to a guarantee**: heavy-tailed skew means the largest ever seen is not a bound on the next, at 3 samples or 500. That is what keeps the **lateness policy mandatory** — `Lateness` implements no `Default`, so the compiler stops a window existing without one, and each cost is real in the type: only `AccumulateAndRetract` retains its emitted value, so `Drop` cannot retract even by mistake, while a dropped fact still lands in the late log. ★ **The divergence is a specific failure rather than an absence, with a real credit attached:** the reference has three genuine windows (`calendar.upcoming`, `tasks.due_soon`, `protege.urgent_window_hours`) and every one closes on `ctx.timestamp` — but that clock is **injected, not ambient**, so EVT-13's purity holds and the defect is the value assumed rather than an ambient read. A second correction to this tracker: **MON-4's row names `SlidingWindowAggregator` as if it ships and it is grep-0** — the row was describing the article, not code. ★★ **And EVT-11's mechanism landed the same day, completing §VIII's *same object seen twice* in one place** (`period.rs`): `Period` **wraps EVT-6's `Window`** rather than inventing a second interval type, so the half that says *which events belong* and the half that says *when we may answer* now meet, and EVT-6's mandatory `Lateness` rides through from the rule. ★★ The partition is **structural** — the expansion emits `[occ[i], occ[i+1])`, so each period's end IS the next one's start and a gap or overlap is unrepresentable — which buys Dijkstra's point (EWD831) exactly where roll-up needs it: **a year is the sum of its twelve months, not the months plus twelve boundary instants**, asserted both by summing durations against the span and by checking every boundary lands in exactly one month. ★★ **Store the rule, recompute the expansion** is a property of the type: `Recurrence` holds no instants, has no constructor accepting any, and the reason is *run* rather than described — **one rule under two tzdata releases yields two different instants with an identical local time**, so a persisted expansion would keep the older answer with nothing to detect it. That is exactly what `state.calendar.events` does in the reference today. ★★ **Zone id over bare offset**, with `Anchor::FixedOffset` kept as a labelled lesser thing — real data does arrive carrying only an offset — that consults no tz data and carries `tzdata_version: None`; its trap is demonstrated rather than warned about, since it agrees with the zone today and diverges under the revised release. ★ **The tz data is INJECTED, not bundled**, and §VIII's own argument is the reason rather than the ADR alone: a bundled database pins one release *invisibly*, hiding the very versioning the section warns about. The line is drawn where the versioning is — civil-calendar arithmetic is a fixed algorithm and is computed (Hinnant, round-trip tested); timezone offsets are versioned data and are only ever asked for. **`Cargo.toml` is unchanged.** ★ DST is exercised by a Europe/Berlin counterparty because Nairobi's case is genuinely vacuous: a real **23-hour** and **25-hour** day, both still partitioning, with a nonexistent local start refused and an ambiguous one resolved by a **declared** `DstPolicy` that implements no `Default`. ★ **The honest residual, kept 🔨:** the rest of the RRULE grammar (`COUNT`/`UNTIL`/`BYSETPOS`/`YEARLY`/`BYMONTH`/`EXDATE`/multi-value BY- lists), an `RRULE:` text parser, and migrating a real consumer onto the primitive — it is ready rather than in use. ★★ **And EVT-12 closed the same day** (`checkpoint.rs`): a checkpoint `(s_k, k)` out of §II's homomorphism, verified at **every** k rather than one convenient one, with `events_applied()` making the `O(n−k)` claim **countable** rather than asserted. ★★ Both of §IX's properties are **structural**: *always discardable* because `replay` takes an `Option<&Checkpoint>` and the fast and from-scratch paths are the **same function**; and *derived, never authored* because `Checkpoint::at` folds the log and there is **no constructor taking a state**. ★★ And **replay cannot reach an effect channel** — `&[Event]` in, `Value` out, no sink, no registry, no `execute` — so §IX's `ASSERT no_external_effects_were_issued()` is a fact about the signature rather than a check at the end; the contrast is the proof, since an operator body receives `&mut Vec<EmittedEvent>` precisely because emitting is what it is for. ★★ **It also found a real gap in §IX's own pseudocode:** `seen = set()` is per call, so ids already folded into the checkpoint never enter it, and a **late duplicate of an early event is skipped from scratch and applied from a checkpoint** — 380 versus 100, demonstrated by running the printed pseudocode literally. That quietly breaks the same section's discardability claim. Carrying the folded id set would make the checkpoint grow with `k`, which is most of what it was for, so the fix is taken where §V already puts dedupe, and `replay` **refuses** an un-deduped log rather than silently picking one of the two wrong answers. ★ **The counterweight is generous and exact:** the reference **already runs the homomorphism** — `sustain_states` is advanced one call at a time rather than re-folded, which is `fold(s_k, L[k+1..n])` at `k = n−1` — and its discardability is real and **tested**, since `rebuild_state()` regenerates the cache from the log and the law is asserted in its own suite. The genuine difference is small and sharp: `_persist_state()` makes a wrong cache **writable** and forbids it by docstring and project rule, where `Checkpoint::at` makes it unwritable. The `snapshot` grep (89 hits) is named as the false positive most likely to mislead — almost all of it is `StateAccessor.snapshot()`, a read, not a persisted `(s_k, k)`. ★★★ **And EVT-10 closed the block the same day** (`dimension.rs`) — the module's centrepiece stops being remembered and starts being checked. ★★ **Delta-accumulation on a snapshot dimension is a genuine TYPE ERROR, proven by a `compile_fail` doctest** that `cargo test --doc` executes: `SnapshotDim` has no `add`, so calling one on a balance read off an SMS does not build. The mistake is **unspellable**, not caught. ★ And the honest completion, because a kind declared in a *spec* with an update off a *wire* cannot be a Rust type error: `DimensionSchema::check` refuses it there too, with the reason travelling along. Both halves are needed, and claiming the compile-time one covered the other would have been the dishonest version. ★★ ***Never accumulate deltas* is derived and MEASURED**, not decreed: non-idempotence run as arithmetic (`x+2δ ≠ x+δ`), and made safe an accumulation **is** the grow-only counter — a redelivered delta leaves the state byte-identical, at an applied-id map of 200 entries after 200 deltas against LWW's constant **1** after 500 readings. §VII's cost argument as two numbers. ★★ **§VII's emphatic limit is structural:** three distinct kinds, `Contested` refusing automatic apply **entirely** and backed by `crdt.rs`'s OR-set, so the same two concurrent edits keep **both** as contested and exactly **one** as snapshot — **LWW cannot eat a contested edit**. ★ **The counterweight is the strongest in the whole set, and it belongs to the reference:** §VII's own field note records that the LWW-for-snapshot behaviour shipped in VOS **first**, derived empirically after out-of-order SMS clobbered a balance and before it had a name — *"the convergence theorem is not being proposed to VOS; it is being written down about VOS."* This row brought no behaviour; it turned a hard-won rule into a checked one. A second credit worth naming: `balance_after` is **already parsed out of every M-Pesa SMS** there and discarded, so the data for the snapshot kind is arriving and only the declaration is missing. ★ **Reconciled, not forked:** `vclock::Dimension` gained its third variant rather than a parallel enum appearing beside it — which forced `MergeResolution::resolve` open and turned a bare `Vec` meaning two things into a typed `Resolved::{Superseded, BothCount, NeedsReconciliation}`, three call sites corrected in place. The `(τ,id)` tie-break and the law checkers were reused, not re-derived.

★★★ **PHASE 1's ATTACK LIST IS COMPLETE.** Boundary (SUS-7/SUS-8/OP-10), firewall (CON-1/CON-2/IMM-1), Operator (OP-3/OP-5) and the whole Events & Time substrate (EVT-5/6/11/12/10) all closed on 2026-08-17. What remains in that layer is **declared residual rather than unstarted work**: EVT-11's RRULE-grammar remainder and EVT-10's DSL authoring surface (both Phase 4 / M-DSL), EVT-1's `payload`, EVT-14 effect journaling, and **EVT-15 replay-under-`D′`, which blocks M-TEN and M-EDIT** and so is the one to schedule deliberately rather than let drift. **→ Phase 2 opened the same day, on MON-8** (`belief.rs`). ★★ The decisive move was declining §VIII's pseudocode where it calls `kalman_filter.update(observation)` **by name**: MON-2 stays the declined import, and the surviving `point_mass` is served by **EVT-10's snapshot collapse** — `Belief` *holds* a `SnapshotDim` and collapses through it, so *an observation arrived* means one thing in this codebase rather than two, and **a stale reading arriving late does not collapse the belief**. ★★ Silence widens monotonically (`σ = elapsed × DRIFT_RATE`, asserted across 24 consecutive hours) **without inventing dynamics** — `Dynamics::Unknown` holds the mean and grows only the band, since filling `predict_forward`'s shape with a fabricated model would put a made-up trajectory behind a real-looking mean. ★ The `SENSOR_SILENT` alert **reuses the boundary without reusing the shape** — it rides `Severity::escalates()` but is deliberately not a `detect::Alert`, because that type carries a `Shift` and nothing shifted; it is *we have lost sight of this*, not *this moved*, and it can never be `Info`. ★ The counterweight is real and correctly scoped: `ingest_engine`'s `is_stale` already flags a quiet source against a **declared** cadence and never guesses one — so the gap is precisely three things, of which only the third is new: **per-dimension** rather than per-source, **continuous** rather than binary, and **the estimate degrades** rather than a flag flipping. ★ Two limits worth carrying: the point-mass collapse **assumes the observation is authoritative**, which for a noisy sensor is a modelling assumption made and not checked — and modelling measurement noise is exactly where Kalman would come in, so the trap and the limit are the same decision seen twice; and the tracker is **not yet wired into `MonitorEngine`**.

★★ **MON-13 followed the same day** (`harmonics.rs`), and it **corrected the row that sent it there**: the row said harmonics *consumes `signal.rs`*, and it cannot — `signal.rs` is the excitable-medium primitive, a wave across a **graph of nodes**, with no time series in it. The Additions paragraph runs the harmonics sentence and a Signal **cross-reference** together, and the row read the second as a dependency of the first. What this reads is the `W = d(s,V)` window `detect.rs` already runs over, **supplied** rather than re-windowed — the same discipline `criticality.rs` follows. ★★ **The payoff is proven, not argued:** a conformance case runs the **real `Cusum`** over the on-schedule monthly bill and asserts it **fires**, while the frequency-domain reading calls the identical series an `ExpectedCycle`. *Complement, not veto.* The same magnitude **off-cycle** surfaces as a `GenuineShift`, and a **new rhythm on top of the known one** still surfaces — so suppression is not a blanket mute. ★★ **A cycle must be earned twice over:** **authorised** (declared — an EVT-11 monthly period is exactly this — or established through enough observed cycles) **and resolvable** (the window must carry two full periods). ★ Declaration authorises a cycle; **it does not make a short window able to measure it**, and subtracting energy at an unresolvable bin would remove a real shift under the name of a season. Both failures give `Indeterminate`, the same third-answer family as `Skew::Unknown` and `Soundness::Unavailable`. ★★ **And testing forced a correction inspection would not have:** attributing one bin per cycle reported an **on-schedule bill as a shift**, because a bill is a periodic **impulse** and an impulse train puts energy at every multiple of its fundamental — which is what the row is *named* for. Fixed by attributing the harmonic series, **with the cost named where it lives**: a genuine shift landing exactly on a harmonic of a known cycle is absorbed with it, inherent rather than a defect. ★ Counterweight: the **substrate is at parity** (`detect.rs` + `MonitorEngine`'s histories) and the **need is real there today** — `budget.allocate` carries `period: 'monthly'` and `record_income` a `frequency: 'monthly'`, so the household's rhythms are already named in the reference while nothing can see them. Direct `O(N²)` DFT, **no dependency**; an FFT is a named scaling slot. ★ **MON-7 moved to Phase 3** — a rendering row whose real consumer is the Curated UI's UI-13.

★★ **MON-4's typology followed the same day** (`windowing.rs`), and the reconciliation *is* the substance: **tumbling is not a third mechanism**. Calendar-aligned tumbling is a **one-line delegate** to EVT-11's `Recurrence::expand` — which matters concretely, since a month is not a fixed number of milliseconds and a duplicate would have had to get calendar length right a second time or quietly get it wrong — and **fixed-duration tumbling is the degenerate sliding case where `step == size`**, proven by `Sliding{DAY, DAY}` and an EVT-11 daily expansion producing **byte-identical** windows. So the genuinely new work was sliding and session. ★★ **Sliding overlaps honestly:** `windows_containing` returns a `Vec` and an event is counted in **each** window containing it, because that is what makes a rolling rate roll; a **`step > size` is refused**, since gaps between windows would silently drop every event that fell in one. ★★ **Session is data-driven:** a session ends **τ after its own last event**, not at the event that revealed the gap, and the last session in a batch is `Open` — *not* an empty session and *not* a closed one. ★★ **And EVT-6's guarantee carries to the one type that could have lost it:** a session's end is unknown in advance, so `close_on` takes a **`&Watermark` and nothing else** — a processing clock two hours past the gap, carrying an honest 90-minute bound, does **not** close it. **A clock is not evidence**, and there is deliberately no *expire after a while* path, which would be the wall clock wearing a different name. ★ **MON-4's own `SlidingWindowAggregator` wording corrected a second time** — grep-0 **on both sides**, and deliberately unbuilt: aggregation is a *consumer* of the typology, not part of it, and building one in would tie a window shape to a particular thing being summed.

★★★ **And MON-1 closed the Monitor block the same day** (`observability.rs`) — as a **declination plus a translation**, which is the honest shape for this row. §I's `rank([C; CA; …; CAⁿ⁻¹]) = n` is a **declined import**, the same position MON-2 takes on Kalman and on MON-11's own grounds — and with a concrete reason rather than only the categorical one: **`CAᵏ` needs a single `A` to take powers of, and there is not one**, because the dynamics here is a *choice* (which operator someone runs) rather than a fixed linear map. Picking an `A` would be a fabrication, and the row's own warning settles it: *a rough answer to a rank question is worse than none.* ★★ **The replacement is term-for-term, not an approximation:** `C` → dimensions a declared source measures; `A` → one operator (reads `X`, writes `Y` ⇒ `X → Y`); `CAᵏ` → a measurement reachable in `k` operator hops; `rank(O)=n` → every governed dimension reaches one. The verdict **names the operator chain** that carries a value, so the claim is checkable rather than trusted, and the search is breadth-first because *which block row* is a question about the smallest `k`. ★★ **Conservative where effects are unsummarised, and it says so in the verdict:** an operator with no `EffectSummary` contributes **no edges**, so a dimension can read unobservable that a summary would have shown observable — the safe direction, and `Unobservable` carries the unsummarised operators with `is_conservative()` to distinguish *unobservable* from *we could not see*. ★ **Over today's registry that is the whole picture** — no shipped operator declares a summary, which is OP-3's finding seen from a second angle, asserted rather than noted. ★ And it completes §I's implication where MON-8 began it, with the distinction kept sharp: `ungoverned()` is **runtime** (*nothing has spoken yet*), MON-1 is **structural** (*nothing ever could*). ★★★ **The Monitor block of Phase 2 is complete** — MON-8, MON-13, MON-4 and MON-1 all closed 2026-08-17, leaving MON-3 (traces; its Logs half already ✅) and MON-7 (moved to Phase 3).

★★★ **And CTL-11 closed Phase 2 the same day** (`damping.rs`). The row's substance is that **CTL-2 has a blind spot the article names**: under `e_{k+1} = (1−g)·e_k`, a gain in `(1,2)` makes `W` fall every step **while the state flips side every step**, and a gain of exactly **2** makes `W` never change — so `W(after) ≤ W(before)` holds **for ever, with equality**, while nothing settles. A conformance case runs **`is_stable_intervention` itself** over both trajectories and asserts `stable` at every step. Lyapunov is not wrong; it is answering a different question, which is exactly what *'Lyapunov says the gap closes; damping says it closes without oscillation'* means. ★★ **Honest fit on MON-1's precedent:** where a gain is **declared** the pole is exact and is computed (`Damped`/`Deadbeat`/`Underdamped`/`Undamped`, with `sustained` marking the pole exactly on the circle) — declining a real model would be as dishonest as fabricating an absent one; and **where there is none, nothing is fitted**, because there is deliberately **no `fit_gain(trajectory)`** and taking poles of a fitted model is the fabricated-`A` trap. The native form reads ringing from the trajectory instead, with `Indeterminate` below three points, and **overshoot** as the one-step form: 50 → 230 on a [100,200] band drops `W` and lands on the far side. ★★ **A finding from testing rather than design:** `W` is a **magnitude**, so a converging-but-ringing loop has a *monotonically falling* `W` — `read_trajectory` calls it `Damped` and is not wrong to; the **side** series is what carries the ringing. Both native readings ship because each sees what the other cannot, and shipping only the magnitude one would have **reproduced Lyapunov's own blind spot**. ★ **A framing correction the greps forced, recorded rather than smoothed:** the obvious counterweight would be *the time-domain half is at parity*, and it is **not** — `lyapunov` is grep-0 on the reference side, and its four `stable` hits are a stable sort, a stable fingerprint and two testables. CTL-2 is a **Rust** row, so the real counterweight is **intra-Rust**: the per-step check already existed here, and CTL-11 adds only the half Lyapunov cannot provide. ★ **Row phasing settled:** **CTL-6 relocated to a host-integration pass** (ADR-0001 keeps I/O out of the core; the core-side half is already the ingest door) and **CTL-8 moved to Phase 3** as a surface row, on the same argument that moved MON-7.

★★★ **PHASE 2 IS COMPLETE.** What remains in the cognition layer is declared residual: MON-3's Metrics/Traces (Logs already ✅), and the **`MonitorEngine` consolidation wiring** now named by four rows at once — MON-8, MON-13, MON-4 and MON-1 — which makes it a slice of its own rather than four footnotes. **→ Phase 3 (Surface + Immune) is next**, and has gained MON-7 and CTL-8 on the way. What is left elsewhere: TEN-11's pawa term (parked on the economy), the belief tracker, the preattentive encoder, the IoT bridge, panels and damping.*
