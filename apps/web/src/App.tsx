/**
 * App.tsx — Sustena XII Mycelium Control Panel
 *
 * Loading order matters: each file assigns components to window globals
 * that the next file in the chain references. Shell is last and exports App.
 */

import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { Capacitor } from '@capacitor/core'

// 1. Core primitives
import './components/core/index.jsx'

// 2. Tweaks panel
import './components/ui/tweaks-panel.jsx'

// 3. Orchie chat
import './components/orchie/chat.jsx'

// 4. MCP data models + DAG primitives
import './components/mcp/data.jsx'

// 5. Monitor panel
import './components/mcp/monitor.jsx'

// 6. Editor + Controller + Library panels
import './components/mcp/other.jsx'

// 6b. Profile page (legacy window global)
import './components/ui/profile.jsx'

// 6c. Journal / Lore page (legacy window global)
import './components/ui/journal.jsx'

// 7. Simulator panel
import './components/mcp/simulator.jsx'

// 7b. Define panel — create/edit sustain definitions (Slice 6)
import './components/mcp/define.jsx'

// 8. App shell
import ShellApp from './components/mcp/shell.jsx'

// Page routes
import ProfilePage from './pages/ProfilePage'
import LoreLayout from './pages/LoreLayout'
import LorePage from './pages/LorePage'
import JournalPage from './pages/JournalPage'
import ArenaPage from './pages/ArenaPage'
import DocsPage from './pages/DocsPage'
import OrchePanel from './pages/OrchePanel'
import OrchieShell from './pages/OrchieShell'

// The native Android wrapper (Capacitor) opens the same routes as the
// hosted web app, but "/" is Mycelium — the orchestrator/dev cockpit,
// laptop-first per its own design (see CLAUDE.md's Mycelium/Orchie split).
// The phone-first, event-first Orchie surface at "/orchie" is what the
// native app is actually FOR, so a native launch redirects there instead
// of landing on a desktop-oriented panel. Capacitor.isNativePlatform() is
// false in every browser context (dev, hosted PWA, any tab), so this has
// no effect on the web app at all.
function HomeRoute() {
  if (Capacitor.isNativePlatform()) return <Navigate to="/orchie" replace />
  return <ShellApp />
}

function RootApp() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<HomeRoute />} />
        <Route path="/profile" element={<ProfilePage />} />
        {/* Orchie Panel — the Mycelium-side operative dashboard (unchanged) */}
        <Route path="/orchie-panel" element={<OrchePanel />} />
        {/* Orchie — the new curated, phone-first, event-first surface (Curated UI engine, §4H) */}
        <Route path="/orchie" element={<OrchieShell />} />
        {/* Arena — standalone with its own full header */}
        <Route path="/arena" element={<ArenaPage />} />
        {/* Lore section — shared top nav via LoreLayout */}
        <Route element={<LoreLayout />}>
          <Route path="/lore" element={<LorePage />} />
          <Route path="/journal" element={<JournalPage />} />
          <Route path="/docs" element={<DocsPage />} />
        </Route>
      </Routes>
    </BrowserRouter>
  )
}

export default RootApp
