/**
 * apps/web/src/lib/api.js
 *
 * Thin API client for the Sustena devui + orchie endpoints.
 *
 * In dev:  requests go through Vite's proxy (vite.config.ts) → localhost:9000.
 *          No absolute base URL needed — all paths are relative.
 * In prod: set VITE_API_BASE to the deployed backend URL (e.g. https://api.sustena.io).
 *          Leave unset (or set to '') to use the same origin as the frontend.
 */

const BASE  = import.meta.env.VITE_API_BASE  ?? '';
const TOKEN = import.meta.env.VITE_ADMIN_TOKEN ?? 'dev-admin-token';

const headers = () => ({
  'Content-Type': 'application/json',
  'Authorization': `Bearer ${TOKEN}`,
});

async function get(path) {
  const res = await fetch(`${BASE}${path}`, { headers: headers() });
  if (!res.ok) throw new Error(`GET ${path} → ${res.status}`);
  return res.json();
}

async function post(path, body) {
  const res = await fetch(`${BASE}${path}`, {
    method: 'POST',
    headers: headers(),
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`POST ${path} → ${res.status}`);
  return res.json();
}

/**
 * Open a WebSocket to /devui/state-stream.
 * In dev Vite proxies /devui with ws:true so a relative path works.
 */
function ws(sustainId, onMessage, onClose) {
  const base = BASE
    ? BASE.replace(/^http/, 'ws')
    : `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}`;
  const url = `${base}/devui/state-stream?sustain_id=${encodeURIComponent(sustainId)}&token=${encodeURIComponent(TOKEN)}`;
  const socket = new WebSocket(url);

  socket.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data);
      onMessage(data);
    } catch (e) {
      console.warn('[api.ws] Failed to parse frame:', e);
    }
  };

  socket.onerror  = (e) => console.warn('[api.ws] error:', e);
  socket.onclose  = () => onClose?.();

  return socket;
}

export const api = { get, post, ws };
