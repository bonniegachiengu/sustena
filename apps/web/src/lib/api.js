/**
 * apps/web/src/lib/api.js
 *
 * Thin API client for the Sustena devui + orchie endpoints.
 *
 * In dev:  requests go through Vite's proxy (vite.config.ts) → localhost:9000.
 *          No absolute base URL needed — all paths are relative.
 * In prod: set VITE_API_BASE to the deployed backend URL (e.g. https://api.sustena.io).
 *          Leave unset (or set to '') to use the same origin as the frontend.
 *
 * Auth: reads the real per-user session token from localStorage on every
 * call (not a module-level constant) — the token changes at login/logout,
 * and this file has to see that without a page reload forcing it to.
 * There is deliberately no baked-in fallback token here: shell.jsx's
 * AuthGate won't render anything that calls this module until a session
 * exists, so an absent token here means something is genuinely wrong
 * (session expired/revoked between checks), not "no one's logged in yet."
 */

const BASE = import.meta.env.VITE_API_BASE ?? '';

function getToken() {
  return localStorage.getItem('sustena_token');
}

function headers() {
  const token = getToken();
  return {
    'Content-Type': 'application/json',
    ...(token ? { 'Authorization': `Bearer ${token}` } : {}),
  };
}

// A 401 here means the session that got us past AuthGate has since expired
// or been revoked (logout in another tab, token_version bumped, natural
// expiry). Clear it and reload — the cleanest way to land back on AuthGate
// without this plain module reaching into React state directly.
function handleUnauthorized() {
  localStorage.removeItem('sustena_token');
  window.location.reload();
}

async function get(path) {
  const res = await fetch(`${BASE}${path}`, { headers: headers() });
  if (res.status === 401) { handleUnauthorized(); throw new Error(`GET ${path} → 401 (session expired)`); }
  if (!res.ok) throw new Error(`GET ${path} → ${res.status}`);
  return res.json();
}

async function post(path, body) {
  const res = await fetch(`${BASE}${path}`, {
    method: 'POST',
    headers: headers(),
    body: JSON.stringify(body),
  });
  if (res.status === 401) { handleUnauthorized(); throw new Error(`POST ${path} → 401 (session expired)`); }
  if (!res.ok) throw new Error(`POST ${path} → ${res.status}`);
  return res.json();
}

/**
 * POST /api/v1/users/login — deliberately NOT built on post() above.
 * post()'s 401 handling assumes a session already existed and was revoked
 * (clears the token and reloads the page) -- correct for a call made
 * DURING a session, wrong for a login attempt that never had one: a wrong
 * password would otherwise silently reload the page instead of showing an
 * inline error. On success, stores the token the exact same way every
 * other sign-in path in this app does (SignInPanel in ProfilePage.jsx)
 * and returns {user_id, email, display_name} for the caller to use.
 */
async function login(email, password) {
  const res = await fetch(`${BASE}/api/v1/users/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  });
  let data;
  try { data = await res.json(); } catch { data = {}; }
  if (!res.ok) {
    const detail = typeof data?.detail === 'string' ? data.detail
      : Array.isArray(data?.detail) ? data.detail[0]?.msg : null;
    throw new Error(detail || `sign-in failed (${res.status})`);
  }
  const token = data?.data?.token;
  if (!token) throw new Error('sign-in succeeded but no token was returned');
  localStorage.setItem('sustena_token', token);
  return data.data;
}

async function patch(path, body) {
  const res = await fetch(`${BASE}${path}`, {
    method: 'PATCH',
    headers: headers(),
    body: JSON.stringify(body),
  });
  if (res.status === 401) { handleUnauthorized(); throw new Error(`PATCH ${path} → 401 (session expired)`); }
  if (!res.ok) throw new Error(`PATCH ${path} → ${res.status}`);
  return res.json();
}

/**
 * Open a WebSocket to /devui/state-stream.
 * In dev Vite proxies /devui with ws:true so a relative path works.
 */
function ws(sustainId, onMessage, onClose) {
  const token = getToken();
  const base = BASE
    ? BASE.replace(/^http/, 'ws')
    : `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}`;
  const url = `${base}/devui/state-stream?sustain_id=${encodeURIComponent(sustainId)}&token=${encodeURIComponent(token ?? '')}`;
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

export const api = { get, post, patch, ws, login };
