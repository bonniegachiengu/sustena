# Sustena XII

> The operating system for Kenyan household and business sustains. Operators, Operatives, Council DAO, and Orchie — built on React and FastAPI.

Phase 0-A

---

## What is Sustena?

Sustena is an operating system for real-world economic activity — households managing budgets, food businesses running commissaries, farmers tracking harvests, chamas coordinating savings. It doesn't abstract these into dashboards. It models them: their state, rules, events, and participants, then puts a conversational AI agent (Orchie) in front of all of it.

A household joins as a **Homestead sustain**. A food business joins as a **Vyyb sustain**. A savings circle joins as a **Colosso Finance sustain**. Each sustain has its own operators, constraints, and operative agents — but they all run on the same 7-primitive engine.

The network of sustains is the **Mycelium**. Participants can discover each other, transact, and govern shared resources through a **Council DAO**. What gets built on top of Sustena is limited only by what can be described in its DSL.

---

## Architecture

Sustena has seven core primitives:

```
State · Operator · Constraint · Event · Time · Consensus · Operative
```

Everything in the system is composed from these. A budget is a State. A deposit is an Event. A spending rule is a Constraint. An Orchie conversation is an Operative executing Operators.

```
 User (React Web / React Native)
        │
        ▼
   ┌─────────┐
   │  Orchie  │  ← conversational AI agent (Claude)
   └────┬────┘
        │ speaks Sustena DSL
        ▼
 ┌──────────────┐
 │ Sustain Engine│  ← validates, resolves, executes
 └──────┬───────┘
        │
   ┌────┴────────────────────────────┐
   │         7 Primitives             │
   │  State · Operator · Constraint  │
   │  Event · Time · Consensus       │
   │  Operative                      │
   └────┬────────────────────────────┘
        │
   ┌────┴──────────────────────────────────┐
   │              Sustains                  │
   │  Homestead · Vyyb · Colosso · Biashara│
   │  Mkulima · (any new sustain)          │
   └───────────────────────────────────────┘
        │
   ┌────┴────┐
   │Mycelium │  ← public marketplace: Arena
   │  DAO    │  ← Council governance
   └─────────┘
```

**Operators** are the business-logic modules inside each sustain (budget, chama, cart, harvest). **Operatives** are the AI agents that interact with users on behalf of those operators — Mentor, Protégé, Chama Secretary. The **Council DAO** governs cross-sustain rules and network upgrades.

---

## What's in this repo

```
sustena-xii/
├── backend/                # Python / FastAPI backend
│   ├── sustena/
│   │   ├── api/            # FastAPI routes (sustains, users, webhook, ...)
│   │   ├── core/           # The 7 primitives engine
│   │   ├── db/             # SQLAlchemy schema + Alembic migrations + Firestore sync
│   │   ├── operators/      # Operator implementations per sustain
│   │   ├── operatives/     # Operative (AI agent) classes
│   │   └── sustains/       # Sustain JSON specs
│   ├── Dockerfile
│   ├── cloudbuild.yaml     # Cloud Build → Cloud Run CI/CD
│   └── Makefile
│
├── brand/                  # Design system + UI mockups
│   ├── design-system/      # Colour tokens, typography, spacing
│   ├── logos/              # Sustena, Vyyb, Colosso, 365+ SVGs
│   └── ui-components/      # Budget ring, sustain card, council UI, Orchie chat
│
├── docs/                   # Technical specs (public)
│   └── README.md
│
├── lore/                   # Public-facing pages: blog, Arena, docs, about
│
├── homestead/              # Homestead sustain — household intelligence
├── vyyb/                   # Vyyb sustain — food business / QSR
├── colosso/                # Colosso Finance sustain — chama / financial
├── biashara/               # Biashara sustain — SME intelligence
└── mkulima/                # Mkulima sustain — smallholder farming
```

---

## Running locally

### Backend (Python / FastAPI)

```bash
cd backend/

# 1. Create and activate a virtual environment
python -m venv venv
source venv/bin/activate          # Windows: .\venv\Scripts\Activate.ps1

# 2. Copy env config — all defaults work locally, no credentials needed
cp .env.example .env

# 3. Install dependencies
pip install -e ".[dev]"

# 4. Start the server (auto-creates DB tables on first run)
uvicorn sustena.api.main:app --reload --host 0.0.0.0 --port 9000
```

API docs at `http://localhost:9000/docs`.

### Tests

```bash
cd backend/
make test          # run all tests
make test-cov      # with coverage report
```

---

## Tech stack

| Layer | Technology |
|---|---|
| Backend | Python 3.12, FastAPI, SQLAlchemy, Alembic |
| AI | Anthropic Claude (Haiku for parsing, Sonnet for reasoning) |
| Database | SQLite (local dev) → Cloud Firestore (production) |
| Frontend | React + Vite + TypeScript (web), React Native (mobile, planned) |
| Deploy | Google Cloud Run via Cloud Build |
| Mobile | React Native *(planned)* |
| Design | Sustena Design System — amber/teal on near-black |

---

## Status

**Phase 0-A — actively building.**

The 7-primitive engine is implemented. Homestead and Vyyb operators are the first live sustains. Orchie (Claude-backed) parses user input from the React web app and executes operator logic. SQLite in dev, Firestore sync wired. Cloud Run deploy pipeline working.

See `brand/` for the design system and UI mockup components — that's the clearest visual preview of where this is heading.

---

*Sustena XII is open for builders. If you're working on economic infrastructure for East Africa and want to run a sustain on this network, read the docs.*

---

<sub>A 365+ Ventures project · Nairobi</sub>
