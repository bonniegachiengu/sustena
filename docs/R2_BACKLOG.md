# R2 — Article-vs-Code Backlog

*The source of truth for Phase R2: implementing what the articles specify but the code does not yet do.*

**Status: R1 (parity) complete. R2 in progress — 5 of 14 core items resolved.**

> **Read this before opening an R2 slice.** Every item traces to an article. The
> articles are the spec; the code catches up to them, never the reverse. Where
> an article and the code disagree, level UP to whichever is the better design —
> do not quietly make the doc match the code.

---

## Where things stand

| | |
|---|---|
| Reference engine (Python) | live, in daily use, **2,319 tests** |
| Portable core (Rust, `sustena-core`) | **R1 parity complete** — all 5 slices |
| Conformance vectors | **118**, replayed by both engines |
| Rust tests | 44 unit + 7 conformance suites |

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
| 15 | Operators have no `inverse` | ENZYME | both |
| 16 | No **checked composition** of operator pathways | ENZYME | both |
| 17 | Ordering is `seq`; articles specify **event-time convergence** | RECORD · GAIA | **HELD — awaiting Bonnie's decision** (architecture fork; shapes event storage permanently) |
| 18 | No **substrate dedupe** — a repeated event id raises | RECORD | **HELD with #17** — both touch the event storage model; building one before the other decides is rework |

### Consistency

| # | Gap | Article | Where |
|---|---|---|---|
| 19 | Council keeps **two stores** (state-based and SQL) that can diverge | SLIME | Python |
| 20 | Rollback registry is **in-memory** — snapshots lost on restart | Controller | Python |
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

## Two items that need a decision before they are built

**#17 — event-time vs `seq` ordering.** The articles specify convergence on
event time; the code orders by a per-sustain sequence number. This shapes the
Rust data model, so getting it wrong means rewriting the core later. Flagged as
CORE for that reason.

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

*Last updated: 2026-08-12, at R1 completion.*
