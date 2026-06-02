import React, { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

const API_BASE = import.meta.env.VITE_API_BASE ?? 'http://localhost:8000';

const CONTRIBUTION_COLORS = {
  decision:  'var(--text-muted)',
  build:     'var(--text-secondary)',
  proposal:  'var(--text-muted)',
  milestone: 'var(--border-light)',
};

const ACHIEVEMENT_BADGES = [
  { label: 'FOUNDER',           icon: '◈' },
  { label: 'SYSTEMS ARCHITECT', icon: '⬡' },
  { label: 'MYCELIUM BUILDER',  icon: '⊕' },
  { label: 'COUNCIL CHAIR',     icon: '◇' },
];

// ── Settings slide-in panel ───────────────────────────────────

function SettingsPanel({ open, onClose, data, onSignOut }) {
  return (
    <>
      {open && <div onClick={onClose} style={{ position: 'fixed', inset: 0, zIndex: 799, background: 'rgba(0,0,0,0.4)', backdropFilter: 'blur(1px)' }} />}
      <div style={{
        position: 'fixed', top: 0, right: 0, bottom: 0,
        width: 'clamp(280px, 30vw, 380px)',
        zIndex: 800,
        background: 'var(--bg-surface)',
        borderLeft: '1px solid var(--border-mid)',
        display: 'flex', flexDirection: 'column',
        transform: open ? 'translateX(0)' : 'translateX(100%)',
        transition: 'transform 0.3s cubic-bezier(0.22,0.61,0.36,1)',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '18px 20px', borderBottom: '1px solid var(--border)' }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 700, letterSpacing: '0.08em', color: 'var(--text-primary)' }}>SETTINGS</span>
          <button onClick={onClose} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', padding: 4 }}>
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M2 2L12 12M12 2L2 12"/></svg>
          </button>
        </div>
        <div style={{ flex: 1, overflowY: 'auto', padding: '20px' }}>
          <div style={{ marginBottom: 28 }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 700, letterSpacing: '0.1em', color: 'var(--text-dim)', display: 'block', marginBottom: 12 }}>IDENTITY</span>
            <div style={{ marginBottom: 10 }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Display name</span>
              <span style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)' }}>{data.displayName}</span>
            </div>
            <div>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Email</span>
              <span style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-secondary)' }}>{data.email}</span>
            </div>
          </div>
          <div style={{ marginBottom: 28 }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 700, letterSpacing: '0.1em', color: 'var(--text-dim)', display: 'block', marginBottom: 12 }}>SESSION</span>
            <button onClick={onSignOut} style={{
              padding: '8px 16px',
              fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
              color: 'var(--text-secondary)', background: 'var(--bg-raised)',
              border: '1px solid var(--border-mid)', borderRadius: 4,
              cursor: 'pointer',
            }}>SIGN OUT</button>
          </div>
        </div>
      </div>
    </>
  );
}

// ── Sub-components ────────────────────────────────────────────

function AchievementBadge({ label, icon }) {
  return (
    <div style={{ display: 'inline-flex', alignItems: 'center', gap: 6, padding: '4px 10px', border: '1px solid var(--border-mid)', borderRadius: 4 }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)', lineHeight: 1 }}>{icon}</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500, letterSpacing: '0.1em', color: 'var(--text-muted)', textTransform: 'uppercase' }}>{label}</span>
    </div>
  );
}

function ProfileAvatar() {
  return (
    <div style={{ width: '100%', height: '100%', background: 'var(--bg-raised)', display: 'flex', alignItems: 'center', justifyContent: 'center', position: 'relative', overflow: 'hidden' }}>
      <div style={{ position: 'absolute', inset: 16, border: '0.5px solid var(--border-mid)', pointerEvents: 'none' }} />
      <div style={{ position: 'absolute', inset: 30, border: '0.5px solid var(--border)', pointerEvents: 'none' }} />
      <span style={{ fontFamily: 'var(--ui)', fontSize: 72, fontWeight: 600, color: 'var(--text-muted)', letterSpacing: '-0.02em', position: 'relative', zIndex: 1, lineHeight: 1 }}>B</span>
    </div>
  );
}

function StatPill({ value, label }) {
  const lines = label.split('\n');
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 3, minWidth: 60 }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 28, fontWeight: 500, color: 'var(--text-primary)', letterSpacing: '-0.02em', lineHeight: 1 }}>{value}</span>
      {lines.map((l, i) => (
        <span key={i} style={{ fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500, letterSpacing: '0.08em', color: 'var(--text-dim)', textTransform: 'uppercase', lineHeight: 1.3 }}>{l}</span>
      ))}
    </div>
  );
}

function ContributionTimeline({ contributions }) {
  if (contributions.length === 0) {
    return <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>no activity recorded yet</span>;
  }
  return (
    <div style={{ position: 'relative', paddingLeft: 72 }}>
      <div style={{ position: 'absolute', left: 54, top: 8, bottom: 8, width: 1, background: 'var(--border)' }} />
      {contributions.map((c, i) => {
        const dotColor = CONTRIBUTION_COLORS[c.type] || 'var(--text-muted)';
        return (
          <div key={i} className="fade-up" style={{ display: 'flex', alignItems: 'flex-start', marginBottom: i < contributions.length - 1 ? 24 : 0, position: 'relative', animationDelay: `${i * 50}ms` }}>
            <span style={{ position: 'absolute', left: -72, top: 3, fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500, color: 'var(--text-dim)', whiteSpace: 'nowrap', letterSpacing: '0.04em' }}>{c.date}</span>
            <span style={{ position: 'absolute', left: -20, top: 5, width: 5, height: 5, borderRadius: '50%', background: dotColor, boxShadow: '0 0 0 2px var(--bg-base)' }} />
            <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
              <span style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)', lineHeight: 1.4 }}>{c.label}</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.06em', textTransform: 'uppercase' }}>{c.type} · {c.sustain}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

function AggregateBalanceCard({ pawa }) {
  return (
    <div style={{ borderBottom: '1px solid var(--border)', paddingBottom: 20, marginBottom: 20, textAlign: 'right' }}>
      <div style={{ fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500, letterSpacing: '0.12em', color: 'var(--text-dim)', textTransform: 'uppercase', marginBottom: 10 }}>TOTAL PAWA</div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 6, justifyContent: 'flex-end' }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 28, fontWeight: 500, color: 'var(--text-primary)', letterSpacing: '-0.02em' }}>{pawa.toLocaleString()}</span>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-dim)' }}>pwa</span>
      </div>
    </div>
  );
}

// ── Sign-in / Register panel ─────────────────────────────────

function SignInPanel({ open, onClose, onSuccess, defaultMode = 'login' }) {
  const [mode, setMode] = useState(defaultMode);
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  const inputStyle = { fontFamily: 'var(--ui)', fontSize: 14, color: 'var(--text-primary)', background: 'var(--bg-raised)', border: '1px solid var(--border-mid)', borderRadius: 4, padding: '10px 14px', outline: 'none', width: '100%', caretColor: 'var(--amber)' };

  const handleSubmit = async () => {
    if (!email.trim() || !password) return;
    setLoading(true); setError('');
    try {
      if (mode === 'login') {
        const res = await fetch(`${API_BASE}/api/v1/users/login`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email: email.trim(), password }) });
        const data = await res.json();
        if (!res.ok) { setError(data.detail ?? 'Login failed'); return; }
        localStorage.setItem('sustena_token', data.data.token);
        onSuccess(data.data.token);
      } else {
        if (!displayName.trim()) { setError('Display name required'); setLoading(false); return; }
        const res = await fetch(`${API_BASE}/api/v1/users/register`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email: email.trim(), password, display_name: displayName.trim() }) });
        const data = await res.json();
        if (!res.ok) { setError(data.detail ?? 'Registration failed'); return; }
        localStorage.setItem('sustena_token', data.data.token);
        onSuccess(data.data.token);
      }
    } catch { setError('Connection error'); } finally { setLoading(false); }
  };

  const switchMode = (m) => { setMode(m); setError(''); };

  const panelHeight = mode === 'register' ? '52vh' : '44vh';

  return (
    <>
      {open && <div onClick={onClose} style={{ position: 'fixed', inset: 0, zIndex: 800, background: 'rgba(0,0,0,0.5)', backdropFilter: 'blur(2px)' }} />}
      <div style={{ position: 'fixed', left: 0, right: 0, bottom: 0, height: panelHeight, zIndex: 900, background: 'var(--bg-surface)', borderTop: '1px solid var(--border-mid)', borderRadius: '12px 12px 0 0', display: 'flex', flexDirection: 'column', transform: open ? 'translateY(0)' : 'translateY(100%)', transition: 'transform 0.35s cubic-bezier(0.22,0.61,0.36,1)', overflow: 'hidden' }}>
        <div style={{ display: 'flex', justifyContent: 'center', padding: '12px 0 4px' }}><div style={{ width: 36, height: 3, borderRadius: 2, background: 'var(--border-mid)' }} /></div>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 24px 12px', borderBottom: '1px solid var(--border)' }}>
          <div style={{ display: 'flex', gap: 0, background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 4, overflow: 'hidden' }}>
            {['login', 'register'].map(m => (
              <button key={m} onClick={() => switchMode(m)} style={{ padding: '5px 14px', fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 600, letterSpacing: '0.08em', color: mode === m ? 'var(--text-primary)' : 'var(--text-muted)', background: mode === m ? 'var(--border-mid)' : 'transparent', border: 'none', cursor: 'pointer', transition: 'all var(--t-fast)' }}>
                {m === 'login' ? 'SIGN IN' : 'REGISTER'}
              </button>
            ))}
          </div>
          <button onClick={onClose} style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)', background: 'none', border: 'none', cursor: 'pointer' }}>CANCEL</button>
        </div>
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', padding: '20px 24px', gap: 10, maxWidth: 420 }}>
          {mode === 'register' && (
            <input type="text" value={displayName} onChange={e => setDisplayName(e.target.value)} placeholder="display name" style={inputStyle} />
          )}
          <input type="email" value={email} onChange={e => setEmail(e.target.value)} placeholder="email address" style={inputStyle} />
          <input type="password" value={password} onChange={e => setPassword(e.target.value)} placeholder="password" style={inputStyle} onKeyDown={e => e.key === 'Enter' && handleSubmit()} />
          {error && <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--danger)' }}>{error}</span>}
          <button onClick={handleSubmit} disabled={!email.trim() || !password || loading} style={{ padding: '8px 16px', fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, letterSpacing: '0.08em', color: 'var(--text-primary)', background: 'var(--border-mid)', border: '1px solid var(--border-light)', borderRadius: 4, cursor: 'pointer', opacity: (!email.trim() || !password || loading) ? 0.4 : 1 }}>
            {loading ? (mode === 'login' ? 'SIGNING IN…' : 'CREATING…') : (mode === 'login' ? 'SIGN IN →' : 'CREATE ACCOUNT →')}
          </button>
        </div>
      </div>
    </>
  );
}

// ── Main page ─────────────────────────────────────────────────

function ProfilePage() {
  const navigate = useNavigate();
  const [isPublic, setIsPublic] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [showSignIn, setShowSignIn] = useState(false);
  const [signInMode, setSignInMode] = useState('login');
  const [token, setToken] = useState(() => localStorage.getItem('sustena_token'));
  const [profile, setProfile] = useState(null);
  const [stats, setStats] = useState(null);
  const [activity, setActivity] = useState([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!token) return;
    setLoading(true);
    const h = { 'Authorization': `Bearer ${token}` };
    Promise.all([
      fetch(`${API_BASE}/api/v1/users/me`, { headers: h }).then(r => r.ok ? r.json() : null),
      fetch(`${API_BASE}/api/v1/users/me/stats`, { headers: h }).then(r => r.ok ? r.json() : null),
      fetch(`${API_BASE}/api/v1/users/me/activity`, { headers: h }).then(r => r.ok ? r.json() : null),
    ]).then(([me, st, act]) => {
      if (me?.data) setProfile(me.data);
      if (st?.data) setStats(st.data);
      if (act?.data) setActivity(act.data.activity ?? []);
    }).catch(() => {}).finally(() => setLoading(false));
  }, [token]);

  const handleSignedIn = (t) => { setToken(t); setShowSignIn(false); };
  const handleSignOut = () => {
    localStorage.removeItem('sustena_token');
    setToken(null); setProfile(null); setStats(null); setActivity([]);
    setSettingsOpen(false);
  };

  const displayName = profile?.display_name ?? 'Bonnie Gachiengu';
  const email = profile?.email ?? '—';
  const handle = email !== '—' ? email.split('@')[0] : 'my.bg';
  const pawaBalance = profile?.pawa_balance ?? stats?.pawa_balance ?? 0;

  const statPills = [
    { value: stats ? stats.sustain_count : '—', label: 'Sustains\nActive'       },
    { value: stats ? stats.operator_executions : '—', label: 'Operators\nRun'   },
    { value: pawaBalance, label: 'Pawa\nBalance'                                 },
  ];

  const contributions = activity.map(a => ({
    date: (a.timestamp ?? '').slice(0, 10),
    type: 'build',
    label: a.operator_name,
    sustain: a.sustain_id ?? 'sustena XII',
  }));

  const monoVars = {
    '--bg-base': '#0f0f0f', '--bg-surface': '#0f0f0f', '--bg-raised': '#161614',
    '--bg-overlay': '#1e1c1a', '--border': '#1e1d1b', '--border-mid': '#2c2a28',
    '--border-light': '#3e3c3a', '--text-primary': '#e8e4de', '--text-secondary': '#a09c96',
    '--text-muted': '#5c5854', '--text-dim': '#383432', '--amber': '#b4b0aa',
    '--amber-dim': '#929089', '--amber-glow': 'rgba(180,176,170,0.07)',
    '--amber-border': 'rgba(180,176,170,0.22)', '--teal': '#9c9890',
    '--teal-dim': '#747068', '--teal-glow': 'rgba(156,152,144,0.06)',
    '--teal-border': 'rgba(156,152,144,0.2)', '--ok': '#8e8a84',
    '--warn': '#b4b0aa', '--danger': '#848280', '--info': '#72706c',
  };

  return (
    <div style={{ ...monoVars, display: 'grid', gridTemplateRows: '44px 1fr', minHeight: '100vh', background: 'var(--bg-base)', padding: '0 48px' }}>

      {/* Header */}
      <header style={{ display: 'grid', gridTemplateColumns: '200px 1fr', borderBottom: '1px solid var(--border)' }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'flex-end', gap: 10, paddingRight: 20, borderRight: '1px solid var(--border)' }}>
          <div style={{ textAlign: 'right' }}>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.1em', color: 'var(--text-secondary)' }}>PROFILE</div>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.06em' }}>{handle}</div>
          </div>
          <div style={{ width: 16, height: 16, borderRadius: '50%', border: '1px solid var(--border-mid)' }} />
        </div>
        <div style={{ display: 'flex', alignItems: 'stretch', paddingLeft: 20 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 0, marginRight: 16 }}>
            {['PERSONAL', 'PUBLIC'].map(mode => {
              const active = (isPublic ? 'PUBLIC' : 'PERSONAL') === mode;
              return (
                <button key={mode} onClick={() => setIsPublic(mode === 'PUBLIC')} style={{ padding: '3px 10px', fontFamily: 'var(--mono)', fontSize: 8, fontWeight: active ? 600 : 400, letterSpacing: '0.1em', color: active ? 'var(--text-primary)' : 'var(--text-dim)', background: 'none', border: 'none', cursor: 'pointer', borderBottom: active ? '1px solid var(--text-secondary)' : '1px solid transparent', transition: 'all var(--t-fast)' }}>{mode}</button>
              );
            })}
          </div>
          <div style={{ flex: 1 }} />
          {token ? (
            <button onClick={() => setSettingsOpen(true)} style={{ display: 'flex', alignItems: 'center', padding: '0 16px', fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', background: 'none', border: 'none', cursor: 'pointer', transition: 'color var(--t-fast)' }}
              onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
              onMouseLeave={e => e.currentTarget.style.color = 'var(--text-dim)'}
            >SETTINGS</button>
          ) : (
            <div style={{ display: 'flex', alignItems: 'stretch' }}>
              <button onClick={() => { setSignInMode('login'); setShowSignIn(true); }} style={{ display: 'flex', alignItems: 'center', padding: '0 12px', fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', background: 'none', border: 'none', cursor: 'pointer', transition: 'color var(--t-fast)' }}
                onMouseEnter={e => e.currentTarget.style.color = 'var(--text-secondary)'}
                onMouseLeave={e => e.currentTarget.style.color = 'var(--text-dim)'}
              >SIGN IN</button>
              <button onClick={() => { setSignInMode('register'); setShowSignIn(true); }} style={{ display: 'flex', alignItems: 'center', padding: '0 12px', fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-primary)', background: 'var(--bg-raised)', border: 'none', borderLeft: '1px solid var(--border)', cursor: 'pointer', transition: 'color var(--t-fast)' }}
                onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
                onMouseLeave={e => e.currentTarget.style.color = 'var(--text-primary)'}
              >REGISTER</button>
            </div>
          )}
          <button onClick={() => navigate('/')} style={{ display: 'flex', alignItems: 'center', padding: '0 16px', fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', background: 'none', border: 'none', cursor: 'pointer' }}>← DASHBOARD</button>
        </div>
      </header>

      {/* Body */}
      {!token ? (
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100%', gap: 16 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-dim)', letterSpacing: '0.08em' }}>
            sign in to view your profile
          </span>
          <div style={{ display: 'flex', gap: 8 }}>
            <button onClick={() => { setSignInMode('login'); setShowSignIn(true); }} style={{ padding: '7px 18px', fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em', color: 'var(--text-secondary)', background: 'transparent', border: '1px solid var(--border-mid)', borderRadius: 4, cursor: 'pointer', transition: 'all var(--t-fast)' }}
              onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--border-light)'; e.currentTarget.style.color = 'var(--text-primary)'; }}
              onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border-mid)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
            >SIGN IN</button>
            <button onClick={() => { setSignInMode('register'); setShowSignIn(true); }} style={{ padding: '7px 18px', fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em', color: 'var(--text-primary)', background: 'var(--border-mid)', border: '1px solid var(--border-light)', borderRadius: 4, cursor: 'pointer', transition: 'all var(--t-fast)' }}
              onMouseEnter={e => e.currentTarget.style.background = 'var(--border-light)'}
              onMouseLeave={e => e.currentTarget.style.background = 'var(--border-mid)'}
            >CREATE ACCOUNT</button>
          </div>
        </div>
      ) : loading ? (
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%' }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-dim)', letterSpacing: '0.08em' }}>loading</span>
        </div>
      ) : (
        <div style={{ display: 'grid', gridTemplateColumns: '200px 1fr', gridTemplateRows: '260px 1fr', overflow: 'hidden' }}>

          {/* TL: Avatar */}
          <div style={{ borderRight: '1px solid var(--border)', borderBottom: '1px solid var(--border)', overflow: 'hidden' }}>
            <ProfileAvatar />
          </div>

          {/* TR: Identity */}
          <div style={{ borderBottom: '1px solid var(--border)', padding: '28px 36px', display: 'flex', flexDirection: 'column', gap: 10, overflow: 'hidden' }}>
            <div style={{ display: 'flex', alignItems: 'baseline', gap: 10 }}>
              <span style={{ fontFamily: 'var(--ui)', fontSize: 26, fontWeight: 600, color: 'var(--text-primary)', letterSpacing: '-0.02em', lineHeight: 1 }}>{displayName}</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-dim)', fontWeight: 400 }}>{handle}</span>
            </div>
            <span style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-muted)' }}>{email}</span>
            <div style={{ display: 'flex', gap: 5, flexWrap: 'wrap' }}>
              {ACHIEVEMENT_BADGES.map(b => <AchievementBadge key={b.label} {...b} />)}
            </div>
            <div style={{ display: 'flex', gap: 0, marginTop: 6 }}>
              {statPills.map((s, i) => (
                <div key={i} style={{ paddingRight: i < statPills.length - 1 ? 24 : 0, marginRight: i < statPills.length - 1 ? 24 : 0, borderRight: i < statPills.length - 1 ? '1px solid var(--border)' : 'none' }}>
                  <StatPill value={s.value} label={s.label} />
                </div>
              ))}
            </div>
          </div>

          {/* BL: Balances */}
          <div style={{ borderRight: '1px solid var(--border)', overflowY: 'auto', padding: '24px 20px 24px 12px' }}>
            {isPublic ? (
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%' }}>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', letterSpacing: '0.08em', textTransform: 'uppercase' }}>PRIVATE</span>
              </div>
            ) : (
              <>
                <div style={{ fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500, letterSpacing: '0.12em', color: 'var(--text-dim)', textTransform: 'uppercase', marginBottom: 20, textAlign: 'right' }}>BALANCES</div>
                <AggregateBalanceCard pawa={pawaBalance} />
              </>
            )}
          </div>

          {/* BR: History */}
          <div style={{ overflowY: 'auto', padding: '24px 36px' }}>
            <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 24 }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500, letterSpacing: '0.12em', color: 'var(--text-dim)', textTransform: 'uppercase' }}>HISTORY</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)' }}>{contributions.length} entries</span>
            </div>
            <ContributionTimeline contributions={contributions} />
          </div>
        </div>
      )}

      <SettingsPanel
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        data={{ displayName, email }}
        onSignOut={handleSignOut}
      />
      <SignInPanel open={showSignIn} onClose={() => setShowSignIn(false)} onSuccess={handleSignedIn} defaultMode={signInMode} />
    </div>
  );
}

export default ProfilePage;
