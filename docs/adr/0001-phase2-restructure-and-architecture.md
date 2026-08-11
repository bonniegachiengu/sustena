# ADR-0001 — Phase 2 Restructure & Architecture Decisions

- **Status:** Accepted (ratified by Bonnie, 2026-08-07)
- **Context:** Following the complete-awareness pass (`docs/COMPLETE_AWARENESS_MAP.md`) and the article↔code alignment (`docs/ARTICLE_CODE_ALIGNMENT.md`), the restructure design (`docs/RESTRUCTURE_DESIGN.md`) surfaced five decisions. This ADR records what was ratified and why, for posterity and for contributors.
- **Supersedes:** the earlier "Option 1 (bundle Python) now → Rust later" recommendation, which was withdrawn as an easy-path hedge that would have caused throwaway work and a hard ceiling.

---

## Decision 1 — The engine's portable core is **Rust**
**Decision.** Re-implement the Sustena engine (state · fold · predicates · constraints · transducer · operators · council) as a portable **Rust** core that compiles to Android (JNI/uniffi), iOS, desktop (Tauri, itself Rust), and web (WASM). SQLite via `rusqlite`.

**Why (not Python-bundle).** Bundling Python on-device hits: a **hard wall at iOS**, a **soft wall at mobile perf/battery** (the fold replays the event log), **packaging fragility** on any C-dependency, **two runtimes on desktop**, and **throwaway native glue** if Rust is adopted later. Rust has none of these ceilings — one core, every surface, small + fast + native.

**De-risking.** The spec is already correct (Phase 1) and the **1,868-test suite is a ready-made correctness oracle** — this is translation of a known-good design, checked against tests, not a research project.

**Interim (so nothing is blocked).** Keep multi-device working via the hosted engine behind a stable interface + thin **offline-capable native shells** (NOT a heavy Python bundle). Apps swap to the local Rust core when it's ready.

**Effort/owner.** Claude across build sprints + Bonnie's review. Bounded, test-gated.

**Consequences.** Biggest single build commitment; highest payoff; the only option without a later patch-over-a-limit.

## Decision 2 — Unify persistence on **one** database layer
**Decision.** Retire the parallel SQLAlchemy-async write path; standardize on the engine's own connection with a thin typed accessor for the routes (one writer, shared transactions). Move migrations to a real tool (Alembic or a small versioned runner).

**Why.** Today the engine (raw sqlite3) and the routes (SQLAlchemy async) write the same file with no shared transaction — a real consistency risk; `seed.py` writes `sustains` rows directly, bypassing the engine. **Consequences.** One source of truth; removes the two-writer hazard.

## Decision 3 — Clean deploy configuration (prod ≠ dev)
**Decision.** Split dev/prod for real: prod disables wildcard CORS, `/docs`, SQL echo, and the dev routers (`devui`/`seed`/`dev`); secrets come from the environment; the app **refuses to boot** in prod without real secrets.

**Why.** The live deployment currently runs `is_development=True` with the default `dev-secret` JWT key. **Consequences.** Security-correct public/multi-device deploy.

## Decision 4 — Quarantine legacy code (do NOT delete)
**Decision.** Move dead/legacy code into a clearly-marked `legacy/` area (kept in history, not deleted): the 4 dormant operatives (Navigator/Attaché/Curator/Protégé), the WhatsApp channel (handler/sender/webhook/dev routes), the `operators.py` stub. Protégé flagged as later-salvageable (a state-path fix). **Bonnie approves the exact list before anything moves.**

**Why.** They read non-existent state / propose non-existent operators / never persist — silent dead code that shouldn't ship in a clean public repo. **Consequences.** Cleaner surface; nothing lost (recoverable).

## Decision 5 — Open-core licensing
**Decision.** **Open the framework** — engine, primitives, native apps, and the full article corpus — under **Apache-2.0** (permissive + patent grant). **Hold back the network/economy layer** (Mycelium settlement, Arena marketplace, pawa/Juul, treasury): keep it out of the public repo for now, license later (likely source-available/BSL-style) **after legal counsel**.

**Why.** Sustena is a *framework* (thrives on openness/contribution) plus a *network + economy* (the business, and clonable). Open the first, protect the second. **Licenses are sticky**, so the economy decision is deferred, not rushed.

**Caveats (recorded).** Not legal advice; a lawyer must review before finalizing, especially because Juul/pawa sits under Kenya's crypto/VASP rules — license and economic model reviewed together. The framework license (Apache-2.0) can proceed now; the economy license stays open.

---

## Where these decisions are persisted (for posterity & reference)
1. **This ADR** — `docs/adr/0001-phase2-restructure-and-architecture.md` (the canonical, contributor-facing record).
2. **`docs/RESTRUCTURE_DESIGN.md`** — the full design + options + migration plan (this ADR records what was chosen from it).
3. **Agent memory** — `[[complete-awareness-program]]` (so it survives across sessions).
4. **The IO vault** — a backup copy alongside the pre-restructure repo snapshot.
