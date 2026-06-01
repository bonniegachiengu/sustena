/**
 * App.tsx — Sustena XII Mycelium Control Panel
 *
 * Loading order matters: each file assigns components to window globals
 * that the next file in the chain references. Shell is last and exports App.
 */

import { BrowserRouter, Routes, Route } from 'react-router-dom'

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

// 8. App shell
import ShellApp from './components/mcp/shell.jsx'

// Page routes
import ProfilePage from './pages/ProfilePage'
import LoreLayout from './pages/LoreLayout'
import LorePage from './pages/LorePage'
import JournalPage from './pages/JournalPage'
import ArenaPage from './pages/ArenaPage'
import DocsPage from './pages/DocsPage'

function RootApp() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<ShellApp />} />
        <Route path="/profile" element={<ProfilePage />} />
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
