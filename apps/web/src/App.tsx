import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'

// Placeholder pages — to be implemented
const MCPMonitor = () => <div>MCP Monitor</div>
const MCPEditor = () => <div>MCP Editor</div>
const MCPSimulator = () => <div>MCP Simulator</div>
const MCPController = () => <div>MCP Controller</div>
const MCPLibrary = () => <div>MCP Library</div>
const OrchiePage = () => <div>Orchie</div>
const ArenaPage = () => <div>Arena</div>
const ProfilePage = () => <div>Profile</div>
const LorePage = () => <div>Lore</div>

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Navigate to="/mcp/monitor" replace />} />
        <Route path="/mcp/monitor" element={<MCPMonitor />} />
        <Route path="/mcp/editor" element={<MCPEditor />} />
        <Route path="/mcp/simulator" element={<MCPSimulator />} />
        <Route path="/mcp/controller" element={<MCPController />} />
        <Route path="/mcp/library" element={<MCPLibrary />} />
        <Route path="/orchie" element={<OrchiePage />} />
        <Route path="/arena" element={<ArenaPage />} />
        <Route path="/profile" element={<ProfilePage />} />
        <Route path="/lore" element={<LorePage />} />
      </Routes>
    </BrowserRouter>
  )
}
