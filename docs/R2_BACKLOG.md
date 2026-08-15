# R2 — Article-vs-Code Backlog

*The source of truth for Phase R2: implementing what the articles specify but the code does not yet do.*

**Status: R1 (parity) complete. R2 in progress — 9 of 14 core items resolved.**

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
| Conformance vectors | R1 parity + R2 spec, incl. **22 approval cases** (rust-only, divergence recorded) |
| Rust tests | **128 unit · 14 conformance tests** across 2 binaries — all green |

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
| 16 | No **checked composition** of operator pathways | ENZYME | both — next |
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
| 23 | No Kalman / EWMA / CUSUM — urgency is a single `pct` | Monitor · PERCEPT |
| 24 | No MonitorEngine; scheduling external, no heartbeats | Monitor |
| 25 | Widget type-checker exists with **no application call site** | PERCEPT |

### Editing

| # | Gap | Article |
|---|---|---|
| 26 | **No definition version history** — edits in place, cannot roll back | SLOPE |
| 27 | 3-valued rollback (Total / Partial / Unavailable) not built | SLOPE |

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

### 🔴 SPINE — missing from the list above; required before "whole system" is true

| # | Gap | Article | Where |
|---|---|---|---|
| T1 | **Tenet optimisation engine (§4)** — backward-induction/Bellman, best/base/worst scenario ensembles, invariant-action extraction, decision-node detection, dead-drops, temporal pincer. Today only the deterministic **forward** simulator exists (ported in R1). | Tenet / Temporal Decision Architecture | new |
| G1 | **Controller decision-math (§3)** — Lyapunov distance-to-V urgency, `should_surface` (SNR filter), `is_stable_intervention`, Sheridan `AUTOMATION_LEVEL` dispatch, OODA loop, holarchy escalation. Today: execute + rollback + council exist; **none of the governance decision-math.** | Controller / The Expanse | new |

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

*Last updated: 2026-08-12 — whole-system reconciliation added (Monitor is tracked as FOLLOW #23–25; Tenet §4 and Controller §3 added as T1/G1); N1–N10 cross-referenced from the completed WBD.*
