# Sustena

**Sustena models any describable system — a household, a farm, a business, a savings group — as one recursive primitive: the Sustain.**

A Sustain contains Sustains, all the way down. The same engine that tracks one person's spending money composes upward into a household, and downward into a single pocket. There is no separate "household system" and "personal system" — there is one primitive, applied recursively.

Formally, a Sustain is **Σ = ⟨B, S, V, T, ⊕⟩** — Boundary, State, Viable region, Transitions, and composition.

---

## The idea in one paragraph

Most software models a domain by inventing a schema for it. Sustena models a domain by *declaring* it: you describe your system's dimensions, the region it must stay inside to remain viable, and the moves that change it. The engine then enforces that description. A move that would push the system outside its viable region is **refused before it commits** — not logged after the fact, not corrected later. State is never edited in place; it is the replay of everything that ever happened, so any state can be rebuilt from its own history and proven correct.

---

## The seven primitives

| Primitive | What it is |
|---|---|
| **State** | What the system currently is. Never written directly — always the fold of its events. |
| **Operators** | Guarded transitions. A declared, gated move that changes State. |
| **Constraints** | The viable region. Typed predicates the engine evaluates *before* committing. |
| **Events** | The append-only log. State is `fold(events)` — reproducible from scratch. |
| **Time** | Ordering and scheduling over the log. |
| **Consensus** | How multiple parties agree on a change to a shared Sustain. |
| **Operatives** | Semi-autonomous agents that observe and *advise*. They never decide unilaterally. |

Two properties hold throughout, by construction rather than by convention:

- **The gate.** Every state change passes the same enforcement check before it persists. A refusal changes nothing — no partial write, no silent correction.
- **The human decides.** Operatives propose; a person confirms. Nothing reaches the outside world without an explicit human confirmation step.

---

## Status

Sustena is in active development and is **pre-1.0**. It runs in production for a single household today.

The engine is currently implemented in Python. Per [ADR-0001](docs/adr/0001-phase2-restructure-and-architecture.md), it is being re-implemented as a portable **Rust** core so the same engine runs natively on Android, iOS, desktop, and the web — with the existing test suite serving as the correctness oracle. Expect the layout under `apps/` to change as that work lands.

**What is in this repository:** the framework — the engine, the primitives, the applications, and the specification.

**What is not:** the network and economy layer (settlement, marketplace, treasury, token). That is held back pending legal review and is not part of the open framework today.

---

## Repository layout

```
apps/
  api/        The engine + the FastAPI host over it
    sustena/
      core/       state · events · fold · predicates · constraints · transducer · council
      operators/  budget · calendar · tasks · holon · egress · …
      operatives/ the advisory layer
      sustains/   Sustain specifications (JSON)
      api/        HTTP routes
    tests/        the correctness suite
  web/        React frontend + the Android (Capacitor) and desktop (Tauri) hosts
docs/         architecture decisions and design
```

---

## Quickstart

Requires **Python 3.11+** and **Node 20+**.

```bash
# Backend
cd apps/api
pip install -r requirements.txt
cp .env.example .env          # defaults run fully offline, with no API spend
uvicorn sustena.api.main:app --reload --port 9000

# Frontend (separate terminal)
cd apps/web
npm install
npm run dev                    # http://localhost:5173
```

Run the test suite:

```bash
cd apps/api
python -m pytest tests/ -q
```

> Always invoke pytest as `python -m pytest` so the package path resolves correctly.

The default configuration sets `ANTHROPIC_API_KEY=mock`, which activates a local mock client. The entire stack runs with **zero API calls and zero spend**. Language models are an optional, pluggable node in an operative's graph — never a requirement for the engine to run.

---

## Documentation

- [`docs/adr/`](docs/adr/) — architecture decision records. Start with ADR-0001.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how to build, test, and submit changes.
- [`SECURITY.md`](SECURITY.md) — how to report a vulnerability.

The formal specification is a corpus of technical papers, each paired with a plain-language companion. These are being prepared for publication and will be added here in stages.

---

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

Copyright 2026 Bonnie Gachiengu.
