# Sustena — Restructure & Architecture Design (Phase 2)
*Purpose: a clean, multi-device-installable, contributor-ready structure — designed from the complete-awareness pass (`COMPLETE_AWARENESS_MAP.md`).*

> **✅ RATIFIED 2026-08-07 → see `docs/adr/0001-phase2-restructure-and-architecture.md` for the accepted decisions** (Rust engine · one DB layer · clean deploy · legacy quarantine · open-core Apache-2.0 framework, economy held back). This doc holds the full options/tradeoffs/migration plan; the ADR records what was chosen.

## Guardrails (non-negotiable)
- **Nothing is deleted or moved without explicit authorization.** ([[restructure-deletion-protocol]])
- **Full backup first.** A complete repo snapshot to the vault before the first move; every step reversible.
- **Incremental + verified.** The test suite (1,868 tests) must stay green after each step — it's our safety net.
- **The articles are the spec** and are already aligned (Phase 1). The code moves toward them; they don't change here.

---

## The problems this restructure solves (from the awareness map)
1. **Repo is not clean-public-ready** — a committed `venv/` (~3,900 files), build artifacts in the tree (`node_modules`, `dist`, `.gradle`, tauri `target`, `.idea`, `.obsidian`, pytest cache), and an awkward nested root `Sustena XII/Sustena XII`.
2. **Two databases on one file** — the engine (raw sqlite3) and the routes (SQLAlchemy async) write the same file with no shared transaction.
3. **Web-first monolith vs the native-apps direction** — one FastAPI process is both API and web-host; the native apps are thin wrappers around it. The goal is genuine native installs with a local engine.
4. **Prod runs in dev mode + default secrets** — no clean deploy config.
5. **Legacy dead code** — 4 dormant operatives, the WhatsApp stub, `operators.py` stub, mock Firestore sync.
6. **Redundancy** — 4 widget-render systems, 2 in-memory registries, duplicated logic.
7. **Frontend monolith** — mixed `.jsx/.js` (no TS), 3,410-line files, no tests.
8. **Not contributor-ready** — no LICENSE, README, CONTRIBUTING, CI, or clean module boundaries.

---

## PART A — Repo hygiene & target layout *(low-risk, clear; the easy wins)*

**Immediate hygiene (reversible, no logic change):**
- Add a real `.gitignore`; remove `venv/`, `node_modules/`, `dist/`, `.gradle/`, tauri `target/`, `.idea/`, `.obsidian/`, `.pytest_cache/` from version control (keep them on disk locally — untracked, not deleted).
- Flatten the nested `Sustena XII/Sustena XII/` root to a single `sustena/` root.
- Add `LICENSE`, `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, a `.env.example`.

**Target monorepo layout (proposed):**
```
sustena/
├── engine/            # the durable core — pure Python, installable as a package
│   ├── core/          # state, operator, constraints, predicates, events, fold, council, curated_ui, …
│   ├── operators/     # budget, calendar, tasks, holon, egress, …
│   ├── operatives/    # base + mentor (+ legacy, quarantined — see Part C)
│   ├── specs/         # homestead.json, habitat.json, graphs/
│   └── pyproject.toml # so the engine installs cleanly anywhere (server, desktop sidecar, tests)
├── server/            # the FastAPI host over the engine (hosted cockpit + dev)
│   ├── routes/        # sustains, orchie, ingest, users, arena, lore, journal, council, devui, …
│   └── db/            # ONE persistence layer (see Part B)
├── apps/
│   ├── orchie/        # Android — native, local engine (see Part B decision)
│   ├── studio/        # Desktop (Tauri) — native, local engine
│   └── cockpit/       # Mycelium web dev-cockpit (optional/dev surface)
├── ui/                # shared React components, decomposed + TypeScript
├── articles/          # the 18 technical + 18 inspiration papers (the spec)
├── docs/              # WBD, awareness map, ledgers, this design
├── tests/             # the 1,868-test suite
├── .github/           # CI (lint + test on every PR)
└── LICENSE · README · CONTRIBUTING · SECURITY · .gitignore · .env.example
```

---

## PART B — The architecture decisions *(each is YOUR call — I'll recommend, you choose)*

### ★ Decision 1 — how the "local engine" ships to native devices (the crux)
The engine is Python + SQLite. Getting it onto Orchie (Android) and Studio (desktop) as a *local* engine has four honest options:

- **Option 1 — Bundle a local Python engine on-device.** Android via Chaquopy / embedded Python; desktop via a Tauri Python sidecar. Each app runs the real engine + a local sqlite, talks to it over localhost.
  - *Pro:* reuse the **entire existing engine unchanged**; fastest path to "it runs offline."
  - *Con:* heavy runtime, fiddly mobile packaging, larger installs.
- **Option 2 — Port the engine to a portable core (Rust).** Re-implement `SustainEngine` as a Rust library compiling to Android (JNI), desktop (native Tauri), and web (WASM).
  - *Pro:* one true portable, fast, clean local engine across every surface.
  - *Con:* a real re-implementation of ~25k lines — but against an **already-correct spec** (the articles) with an **existing 1,868-test oracle**, so it's feasible, just the biggest effort.
- **Option 3 — Local server process.** Each app bundles the current Python backend as an on-device local server against a local sqlite.
  - *Pro:* reuse the whole backend; app is a thin shell over localhost.
  - *Con:* bundling a Python server in a mobile app is heavy (battery/lifecycle).
- **Option 4 — Hosted backend + proper offline-capable native shells.** Not a true local engine; a real native app with an offline cache/queue (the SMS capture already queues offline).
  - *Pro:* simplest; reuses everything now.
  - *Con:* needs connectivity; against the pivot's "local engine" intent.

**My recommendation (corrected, 2026-08-07 — after Bonnie challenged the easy-path hedge): Option 2 — port the engine to Rust as the one portable local core.** It is the ONLY option with no ceiling: Python-bundle (Option 1) hits a hard wall at iOS, a soft wall at mobile perf/battery as history grows, packaging fragility on any C-dependency, and two runtimes on desktop — AND the native-integration work is throwaway if we go Rust afterward (build it twice = the patch-on-patch trap). Rust compiles to Android (JNI/uniffi), iOS, desktop (Tauri is *already* Rust), and web (WASM) from one core; SQLite via `rusqlite`; the fold/predicates/constraints/transducer are pure deterministic logic that ports cleanly. It's de-risked because the **spec is already correct** (Phase 1) and the **1,868-test suite is a ready-made correctness oracle**. **Interim (so nothing is blocked while the Rust core is built): keep multi-device working via the hosted engine behind a stable interface + thin offline-capable native shells — NOT a heavy Python bundle** (which would be the throwaway work). Effort is mine across build sprints + Bonnie's review — a bounded re-implementation against a known spec, not a research project or an external team.

### Decision 2 — unify the two database layers
Pick one persistence layer for the whole backend. **Recommendation:** standardize on the engine's own path and give the routes a thin typed accessor over the *same* connection (one writer, shared transactions), retiring the parallel SQLAlchemy-async layer for writes. Migrations move to a real tool (Alembic or a small versioned migration runner).

### Decision 3 — clean deploy config
Split `dev` vs `prod` for real: prod turns off wildcard CORS, `/docs`, SQL echo, and the dev routers; secrets come from the environment (no default `dev-secret`), enforced at startup (fail to boot if a prod secret is missing).

---

## PART C — Legacy / dead-code decisions *(need your authorization — nothing removed without it)*
For each, the options are **build-out**, **convert+wire**, or **quarantine** (move to a clearly-marked `legacy/` or a separate branch, not deleted):
- **4 dormant operatives** (Navigator/Attaché/Curator/Protégé) — recommend **quarantine** now (they read non-existent state, propose non-existent operators); Protégé is the one salvageable later (a path fix).
- **WhatsApp channel** (handler + sender + webhook + dev routes) — recommend **quarantine** (superseded by native Orchie).
- **`operators.py` "not yet implemented" stub** — remove or implement.
- **Mock Firestore sync** — keep as the dev stub, but the real cross-device sync is Decision 1's job.
- **Redundant widget-render systems** (4 → 1) and **ephemeral registries** (rollback/forks → event-log-backed) — consolidate in the build phase.

---

## PART D — Contributor-readiness
- **LICENSE** (your choice — e.g. a permissive MIT/Apache-2.0, or a source-available licence if you want to guard commercial use).
- **README** (what Sustena is, the 7 primitives, quickstart), **CONTRIBUTING** (how to run the engine + tests, the code style, the PR flow), **SECURITY.md** (how to report issues).
- **CI** — lint + the full test suite on every PR (and the new fold-invariant guardrail from Phase 0).
- **`.env.example`** + a one-command dev setup.

---

## PART E — Migration plan *(phased, reversible, backup-first)*
1. **Full backup** of the repo to the vault (a complete snapshot, not just the map).
2. **Hygiene pass** (Part A) — `.gitignore` + untrack artifacts + flatten root + contributor files. No logic change; tests stay green.
3. **Backend unify** (Decision 2 + 3) — one DB layer, clean deploy config. Tests green.
4. **Package the engine** (`engine/` + `pyproject.toml`) so it installs standalone.
5. **Native installs** (Decision 1) — stand up Orchie/Studio on the packaged engine.
6. **Legacy quarantine** (Part C) — only what you authorize.
7. **Frontend decomposition + TS + tests** — incremental.
Each step is its own reviewable change; we can stop or roll back at any point.

---

## PART F — What needs your explicit authorization before I execute
1. The **layout** (Part A target tree) — approve or adjust.
2. **Decision 1** (native engine strategy) — pick an option.
3. **Decisions 2 & 3** (DB unify, deploy config) — approve the recommendations or redirect.
4. **Part C quarantine list** — approve what gets moved to `legacy/` (nothing deleted).
5. The **LICENSE** choice (Part D).

*Once you sign off on these, I take the full backup, then execute Part E one reversible step at a time, showing you each before moving on.*
