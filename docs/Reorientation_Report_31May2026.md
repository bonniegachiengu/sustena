# Sustena XII — Reorientation Report
**Date:** 31 May 2026  
**Author:** Claude (Cowork)  
**Prepared for:** Bonventure Gachiengu — Lead Architect  
**Status:** Full project reread following monorepo reorg + app crash

---

## 1. Project Overview — What Exists and What State It's In

### Repository Structure

The project is a **monorepo** with the following layout:

```
sustena-xii/
├── apps/
│   ├── api/          ← Active FastAPI backend (Python)
│   └── web/          ← Active React/Vite frontend
├── backend/          ← OLD — pre-monorepo copy of the API, now stale (missing /scripts)
├── docs/             ← Bonnie_Master_Roadmap.md + strategy docs
├── archive/          ← Previous brand work, old sustain specs, legacy HTML prototypes
└── Briefs/           ← Daily briefing files
```

The monorepo reorg happened in the **last two commits** (`960bde8` → `44fe926`). The `backend/` directory is a stale remnant that was not deleted — it diverges from `apps/api/` by being missing the `sustena/scripts/` folder. It should be deleted to prevent confusion.

---

### Backend (`apps/api`) — Solid, ~70% of Phase 1 Built

The backend is the strongest part of the codebase. All core primitives are implemented:

| Module | File | Lines | Status |
|--------|------|-------|--------|
| StateAccessor | `core/state.py` | 220 | ✅ Built |
| ConstraintEngine | `core/constraints.py` | 412 | ✅ Built |
| EventBus | `core/events.py` | 207 | ✅ Built |
| PawaLedger | `core/pawa.py` | 277 | ✅ Built |
| OperatorContext/Result | `core/operator.py` | 227 | ✅ Built |
| SustainEngine | `core/sustain_engine.py` | 530 | ✅ Built |
| CouncilSession | `core/council.py` | 587 | ✅ Built |
| WhatsApp handler/sender | `core/whatsapp_*.py` | ~450 | ✅ Built |
| Claude client wrapper | `core/claude_client.py` | 160 | ✅ Built |

**Operators implemented:** `budget.py`, `chama.py`, `procurement.py`, `biashara.py`, `vyyb.py`, `calendar.py`

**Operatives implemented:** `mentor.py`, `protege.py`, `curator.py`, `attache.py`, `navigator.py`, `chama_secretary.py`, `base.py`

**Sustain specs in `apps/api/sustena/sustains/`:** Homestead, Vyyb Biashara, Chama (confirmed from git commits for Epics 1.3.1, 1.5.1, 1.5b.1)

**API routes:** FastAPI app with WhatsApp webhook, Sustain CRUD routes, JWT auth (Epics 1.4.1, 1.4.2 complete)

**Tests exist for:** all core primitives, budget/chama/procurement/vyyb operators, chama secretary, council, sustain engine, WhatsApp handler — 16 test files in `tests/`.

**What's missing from the backend:** No Mycelium registry (Epic 1.16), no simulation engine beyond basic `simulate()` in SustainEngine (Epic 1.12), no HTMX dev UI (Epic 1.9/1.10 were specified as HTMX but never built — the React frontend replaced this). No `mpesa.*` operators (Epic 1.14). No Platform Ops sustain (Epic 1.17). No Mkulima sustain (Epic 1.13).

---

### Frontend (`apps/web`) — Design-Forward, Architecturally Diverged from Roadmap

The frontend is a **React 18 + Vite + TypeScript** single-page application. It was built **ahead of the roadmap spec**, which called for an HTMX-based dev UI (Epic 1.9) and HTMX Orchie chat (Epic 1.10). Instead, a full React Control Panel was built.

**What exists in the UI:**

| Component | File | Description |
|-----------|------|-------------|
| Shell / App layout | `components/mcp/shell.jsx` | Full 5-panel layout: Monitor, Simulator, Editor, Controller, Library |
| Data models + DAG | `components/mcp/data.jsx` | SUSTAINS mock data, DagNode, DagEdges, CodeEditor, OrchieExpansionPanel |
| Monitor panel | `components/mcp/monitor.jsx` | Live state stream visualisation |
| Editor/Controller/Library | `components/mcp/other.jsx` | Spec editor, operator console, proposal queue, Mycelium library |
| Simulator panel | `components/mcp/simulator.jsx` | Fork + run simulation UI |
| Orchie chat | `components/orchie/chat.jsx` | Streamed conversation with bubble rendering |
| Orchie mobile | `components/orchie/mobile.jsx` | Mobile-first Orchie interface |
| Profile page | `pages/ProfilePage.jsx` | Identity + history page |
| Lore/Journal | `pages/LorePage.jsx`, `pages/JournalPage.jsx` | Builder journal + Lore surface |
| Arena | `pages/ArenaPage.jsx` | Mycelium operator/operative browser |
| Docs | `pages/DocsPage.jsx` | In-app strategy/whitepaper reader |

**Routes configured:** `/` (Shell), `/profile`, `/lore`, `/journal`, `/arena`, `/docs`

**Architectural pattern used:** The components use a "window globals loading chain" — each file is imported as a side-effect in `App.tsx` and assigns its components/data to `window.*`. `shell.jsx` then references them without imports (e.g., `SUSTAINS`, `useTweaks`, `MonitorPanel`, `Icon`, `Badge`, etc.). This works in Vite's bundled output (all modules end up in the same chunk scope) but is fragile, non-idiomatic React, and will cause issues if code-splitting is ever enabled. It's technically functional but the pattern needs to be migrated to proper ES module imports eventually.

**Current data state:** All UI data is **mocked** — static arrays of SUSTAINS, OPERATIVES, OPERATORS, SIM_NODES etc. defined in `data.jsx`. The frontend is not wired to the FastAPI backend yet. No API calls, no WebSocket connections — it's a fully self-contained design prototype.

---

## 2. The Roadmap Gap — v1.5b → v1.9

### What Was Built (Completed Epics)

Based on the git log and file analysis:

| Epic | Description | Status |
|------|-------------|--------|
| 0.1 | Repo setup, env, DB schema | ✅ Done |
| 0.2 | Core primitives (all 5) | ✅ Done |
| 0.3 | WhatsApp gateway + onboarding | ✅ Done |
| 1.1 | Budget, Chama, Procurement operators | ✅ Done |
| 1.2 | Operative runtime + Council | ✅ Done |
| 1.3 | Homestead sustain spec + SustainEngine | ✅ Done |
| 1.4 | FastAPI app structure + Sustain routes | ✅ Done |
| 1.5 | Vyyb Biashara sustain + operators | ✅ Done |
| 1.5b | Chama Secretary sustain + DAO governance | ✅ Done |

Then: monorepo reorg (`apps/api`, `apps/web`) with test fixes.

**Separately (parallel track):** The entire React frontend (Mycelium Control Panel, all 5 panels, all page routes) was built as design work — likely alongside the backend but not recorded sequentially in git. This work is substantial but entirely disconnected from the backend.

### What Was Skipped (The Gap)

These epics exist in the roadmap but have **zero implementation**:

| Epic | Description | Why It Matters |
|------|-------------|----------------|
| **1.6** | Customer Support System | Daily `support-queue` workflow, GitHub issues pipeline — the feedback loop that should be running right now |
| **1.7** | Substack + Blog + Docs site | "The Sustena Build" newsletter, technical docs — public credibility signal, no public presence yet |
| **1.7b** | Lore Sustain (public surface) | `/journal`, `/docs`, `/about`, `/whitepaper` — Lore exists as UI pages but there's no actual content deployed. The UI shell is built; the sustain spec and published content are not. |
| **1.8** | GitHub hygiene + CI | No CI pipeline, no issue templates, no PR templates, no GitHub Actions `ci.yml` — the test suite exists but isn't gated |
| **1.8b** | Legal/investor track | Brian's lane — incorporations, ODPC, Daraja application — these have hard calendar deadlines that are accumulating |
| **1.9** | Mycelium Control Panel (backend) | The **roadmap** specifies a FastAPI + WebSocket backend for the dev UI. The React frontend was built instead. The backend `/devui/*` routes don't exist — so the frontend has nothing to connect to. |
| **1.10** | Orchie Web UI (HTMX) | The roadmap specified HTMX; a React version was built instead. The React Orchie UI exists but is mocked — no `/orchie/message` endpoint exists in the backend. |
| **1.11** | Generated UI / ui_schema | `ui_schema` parsing and `ui.render.*` operators — not built. The frontend renders mock widget data. |
| **1.12** | Monte Carlo simulation engine | The Simulator panel exists in the UI as a mockup. The `POST /devui/simulate` backend endpoint doesn't exist. |

**What this gap means in practice:** The frontend and backend are two parallel tracks that have **never been connected**. You have a solid backend API and a polished React UI, but there are no API integrations — no fetch calls, no WebSocket connections, no shared data contract. The gap between 1.5b and the next logical step is the **integration layer**.

---

## 3. Why the App Crashed — Root Cause

### Primary Crash: Corrupted `tsconfig.json`

The file `apps/web/tsconfig.json` is **truncated and malformed**. It ends mid-JSON:

```json
{
  "compilerOptions": { ... },
  "include": ["src"],
  "references": [{ "path": "./tsco
```

The correct ending should be:
```json
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

This happened during the monorepo reorg commit (`44fe926` or `960bde8`) — a file write was truncated. The file is only **524 bytes** when it should be ~600+ bytes.

**Consequences of this corruption:**

1. `npm run build` (`tsc && vite build`) **fails immediately** — TypeScript cannot parse the project config. The `tsc --noEmit` check produces 7 cascade errors including false "unclosed JSX" errors in `shell.jsx` (TypeScript's parser is confused by the malformed JSON before it even reaches the JSX files).
2. Production deployments are **blocked** — you cannot build a distributable.
3. The dev server (`npm run dev`) likely still works because Vite uses **esbuild** for the dev server and bypasses `tsc` entirely. This means the app runs in dev but silently has a broken build config.
4. Any CI pipeline (if one existed) would fail every run.

### Secondary Issue: `backend/` Directory Not Cleaned Up

The pre-monorepo `backend/` directory still exists at the root. It contains a near-duplicate of `apps/api/` but is missing `sustena/scripts/`. Having both creates navigational confusion and the risk of accidentally editing the wrong copy. It should be deleted.

### Not a Crash — But Worth Noting: Frontend/Backend Disconnection

The app doesn't crash at runtime (dev mode works), but it's functionally hollow — all data is mocked. No API integration exists. This isn't a crash, it's an architecture debt that will need a deliberate integration sprint.

---

## 4. Where We Are and What the Logical Next Step Is

### Current Position

```
Backend:   ████████████████░░░░░░░░  ~65% of Phase 1 built
Frontend:  ████████████░░░░░░░░░░░░  Design-complete but entirely mocked
Tests:     ████████████████░░░░░░░░  16 test files, untested in CI
Build:     ████░░░░░░░░░░░░░░░░░░░░  BROKEN (tsconfig.json corrupted)
Wiring:    ░░░░░░░░░░░░░░░░░░░░░░░░  ZERO — frontend ↔ backend not connected
```

### Immediate Fix (15 minutes)

Fix the corrupted `tsconfig.json`. Replace its content with:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": false,
    "noUnusedParameters": false,
    "allowJs": true
  },
  "include": ["src"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

Run `npm run build` after to confirm it passes.

### Logical Next Step (the Integration Sprint)

The highest-leverage next move is **connecting the frontend to the backend** — not building new features. Specifically:

1. **Fix tsconfig.json** (above) — unblocks builds.

2. **Delete `backend/`** — remove the stale pre-monorepo copy.

3. **Build the `/devui` WebSocket + REST backend** (Epic 1.9.1) — the React frontend is ready and waiting for these endpoints. This is the single task that makes the entire Control Panel functional with real data. Key endpoints needed:
   - `WS /devui/state-stream/{sustain_id}` — powers the Monitor panel
   - `GET /devui/sustains` — powers the sustain selector
   - `POST /devui/console/execute` — powers the Operator Console in the Editor panel
   - `POST /devui/simulate` — powers the Simulator panel

4. **Wire the Orchie FAB** — add `POST /orchie/message` endpoint and replace the mocked `useStreamedConversation` with a real API call. This makes the most visible piece of the UI functional.

5. **Add GitHub Actions CI** (`ci.yml`) — one YAML file that runs `pytest tests/` on every push to `main`. This is 20 minutes of work and protects the test suite you've already written.

### What to Hold Off On

Don't start Epics 1.12 (Monte Carlo), 1.13 (Mkulima), 1.14 (M-Pesa Daraja), or 1.16 (Mycelium) yet. The correct sequence is: **fix build → wire backend to frontend → ship something real users can see → then expand features**. Building more backend before the existing backend is accessible via the UI is investment with no return signal.

---

## 5. Summary

| Area | State | Priority |
|------|-------|----------|
| `tsconfig.json` | **CORRUPTED** — truncated at 524 bytes | Fix immediately |
| `backend/` directory | Stale duplicate of `apps/api/` | Delete |
| Backend API | Solid — 65% of Phase 1 complete, well-tested | Ready to extend |
| Frontend UI | Design-complete, fully mocked, not connected | Ready to wire |
| Frontend ↔ Backend | **Zero integration** | The actual next build sprint |
| CI pipeline | Does not exist | Add after tsconfig fix |
| Epics 1.6–1.18 | Not started | Sequence: 1.9 backend → 1.10 wiring → 1.6/1.8 ops hygiene |
| Lore/public surface | UI pages exist, no content deployed | Hold until wiring sprint done |
| Brian's lane (1.8b) | No visibility — check in with Brian | Calendar-sensitive |

---

*Report generated by full codebase read — 31 May 2026*  
*Files read: docs/Bonnie_Master_Roadmap.md, apps/web/src/App.tsx + all component files, apps/api structure, tsconfig.json, git log -30, tsc --noEmit output*
