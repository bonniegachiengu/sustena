/**
 * apps/web/src/components/LoginGate.jsx
 *
 * Shared sign-in gate for every packaged app that has no browser session
 * to inherit -- the hosted web app's own auth screens (this app's
 * ancestor: ProfilePage.jsx's SignInPanel) rely on whatever's already in
 * this same origin's localStorage, which a packaged native shell
 * (Capacitor/Orchie on Android, Tauri/Studio on desktop) never has on
 * first launch. Built for Orchie first (1 Aug 2026), then extracted here
 * so Studio reuses the exact same login mechanism rather than a second,
 * drifting copy of it.
 *
 * Deliberately sign-in only, no register mode -- unlike ProfilePage's
 * SignInPanel (which offers both), anyone opening a packaged app already
 * has a Sustena account from the web app; adding registration here wasn't
 * asked for and isn't needed.
 */
import React, { useState } from 'react';
import { Link } from 'react-router-dom';
import { api } from '../lib/api';
import { isNativeShell } from '../lib/platform';

const confirmButton = {
  fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 700, letterSpacing: '0.06em',
  color: 'var(--bg-base)', background: 'var(--teal)',
  border: 'none', borderRadius: 'var(--radius-sm)', padding: '12px 20px', cursor: 'pointer',
};

export default function LoginGate({ onSignedIn, title = 'sign in to sustena' }) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  const inputStyle = {
    fontFamily: 'var(--ui)', fontSize: 14, color: 'var(--text-primary)',
    background: 'var(--bg-raised)', border: '1px solid var(--border-mid)',
    borderRadius: 'var(--radius-sm)', padding: '10px 14px', outline: 'none', width: '100%',
    boxSizing: 'border-box',
  };

  const canSubmit = email.trim() && password && !loading;

  const submit = async () => {
    if (!canSubmit) return;
    setLoading(true);
    setError('');
    try {
      await api.login(email.trim(), password);
      onSignedIn();
    } catch (e) {
      // api.login() throws the real FastAPI error detail ("Invalid email
      // or password" on bad creds) or a network-reachability message --
      // shown inline, never a page reload (see api.js's login() docstring
      // for why post()'s shared 401 handling is wrong for this case).
      setError(e.message || 'could not reach sustena');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ padding: '60px 4px', display: 'flex', flexDirection: 'column', gap: 12, maxWidth: 320, margin: '0 auto' }}>
      <div style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-muted)', textAlign: 'center', marginBottom: 10 }}>
        {title}
      </div>
      <input
        type="email" inputMode="email" autoCapitalize="none" autoCorrect="off"
        value={email} onChange={e => setEmail(e.target.value)}
        placeholder="email address" style={inputStyle}
      />
      <input
        type="password" value={password} onChange={e => setPassword(e.target.value)}
        placeholder="password" style={inputStyle}
        onKeyDown={e => { if (e.key === 'Enter') submit(); }}
      />
      {error && <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--danger)' }}>{error}</span>}
      <button
        onClick={submit} disabled={!canSubmit}
        style={{ ...confirmButton, textAlign: 'center', opacity: canSubmit ? 1 : 0.4 }}
      >
        {loading ? 'SIGNING IN…' : 'SIGN IN →'}
      </button>
      {/* Inside any packaged app "/" just redirects back to the app's own
          entry route (App.tsx's HomeRoute) -- this link only means
          anything in a real browser. */}
      {!isNativeShell() && (
        <div style={{ textAlign: 'center', marginTop: 10 }}>
          <Link to="/" style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--amber)' }}>go to sustena →</Link>
        </div>
      )}
    </div>
  );
}
