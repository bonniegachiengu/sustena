/**
 * apps/web/src/lib/api.js
 *
 * Thin API client for the Sustena devui + orchie endpoints.
 *   VITE_API_BASE    — defaults to http://localhost:8000
 *   VITE_ADMIN_TOKEN — defaults to 'dev-token'
 */

const BASE  = import.meta.env.VITE_API_BASE   ?? 'http://localhost:9000';
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
 * @param {string} sustainId
 * @param {(data: object) => void} onMessage
 * @param {() => void} onClose
 * @returns {WebSocket}
 */
function ws(sustainId, onMessage, onClose) {
  const wsBase = BASE.replace(/^http/, 'ws');
  const url = `${wsBase}/devui/state-stream?sustain_id=${encodeURIComponent(sustainId)}&token=${encodeURIComponent(TOKEN)}`;
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
