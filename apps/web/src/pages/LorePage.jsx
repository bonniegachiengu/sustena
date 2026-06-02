import React, { useState, useEffect, useMemo, useCallback } from 'react';
import { api } from '../lib/api.js';

const API_BASE = import.meta.env.VITE_API_BASE ?? '';

const ENTRY_TYPES = {
  vision:     { label: 'VISION'     },
  technical:  { label: 'TECHNICAL'  },
  reflection: { label: 'REFLECTION' },
  update:     { label: 'UPDATE'     },
};

const KINDS = ['VISION', 'TECHNICAL', 'REFLECTION', 'UPDATE'];

// ── Helpers ──────────────────────────────────────────────────

function authFetch(path, method, body, token) {
  return fetch(`${API_BASE}${path}`, {
    method,
    headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
    ...(body != null ? { body: JSON.stringify(body) } : {}),
  }).then(async r => {
    const data = await r.json();
    if (!r.ok) throw new Error(data.detail ?? String(r.status));
    return data;
  });
}

function textToBlocks(text) {
  const paragraphs = text.split(/\n\n+/).map(p => p.trim()).filter(Boolean);
  let firstText = true;
  return paragraphs.map(p => {
    if (p.startsWith('## ')) return { type: 'h2', text: p.slice(3).trim() };
    const type = firstText ? 'lead' : 'p';
    firstText = false;
    return { type, text: p };
  });
}

function normalizeEntry(raw) {
  let body = [];
  try {
    const parsed = JSON.parse(raw.body_json);
    body = Array.isArray(parsed) ? parsed : [{ type: 'p', text: String(parsed) }];
  } catch {
    body = [{ type: 'p', text: raw.body_json ?? '' }];
  }
  const date = raw.published_at
    ? raw.published_at.slice(0, 10)
    : (raw.created_at ?? '').slice(0, 10);
  const preview = body.find(b => b.type === 'lead' || b.type === 'p')?.text ?? '';
  return {
    id: raw.id,
    type: (raw.kind ?? 'REFLECTION').toLowerCase(),
    title: raw.title,
    date,
    tags: Array.isArray(raw.tags) ? raw.tags : [],
    preview,
    body,
    sidenotes: Array.isArray(raw.sidenotes) ? raw.sidenotes : [],
  };
}

// ── Shared button ─────────────────────────────────────────────

function PBtn({ children, onClick, disabled, variant = 'primary' }) {
  const [hov, setHov] = useState(false);
  const ghost = variant === 'ghost';
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      onMouseEnter={() => setHov(true)}
      onMouseLeave={() => setHov(false)}
      style={{
        padding: '6px 14px',
        fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500,
        letterSpacing: '0.08em', textTransform: 'uppercase',
        color: ghost
          ? (hov ? 'var(--text-primary)' : 'var(--text-secondary)')
          : 'var(--text-primary)',
        background: ghost
          ? (hov ? 'var(--bg-raised)' : 'transparent')
          : (hov ? 'var(--border-light)' : 'var(--border-mid)'),
        border: `1px solid ${ghost ? 'var(--border-mid)' : 'var(--border-light)'}`,
        borderRadius: 4,
        transition: 'all var(--t-fast)',
        opacity: disabled ? 0.4 : 1,
        cursor: disabled ? 'not-allowed' : 'pointer',
      }}
    >
      {children}
    </button>
  );
}

// ── Sign-in panel ─────────────────────────────────────────────

function SignInPanel({ open, onClose, onSuccess }) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  const inputStyle = {
    fontFamily: 'var(--ui)', fontSize: 14,
    color: 'var(--text-primary)',
    background: 'var(--bg-raised)',
    border: '1px solid var(--border-mid)',
    borderRadius: 4, padding: '10px 14px',
    outline: 'none', width: '100%',
    caretColor: 'var(--amber)',
  };

  const handleSubmit = async () => {
    if (!email.trim() || !password) return;
    setLoading(true); setError('');
    try {
      const res = await fetch(`${API_BASE}/api/v1/users/login`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email: email.trim(), password }),
      });
      const data = await res.json();
      if (!res.ok) { setError(data.detail ?? 'Login failed'); return; }
      localStorage.setItem('sustena_token', data.data.token);
      onSuccess(data.data.token);
    } catch {
      setError('Connection error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <>
      {open && <div onClick={onClose} style={{ position: 'fixed', inset: 0, zIndex: 800, background: 'rgba(0,0,0,0.5)', backdropFilter: 'blur(2px)' }} />}
      <div style={{
        position: 'fixed', left: 0, right: 0, bottom: 0,
        height: '44vh', zIndex: 900,
        background: 'var(--bg-surface)',
        borderTop: '1px solid var(--border-mid)',
        borderRadius: '12px 12px 0 0',
        display: 'flex', flexDirection: 'column',
        transform: open ? 'translateY(0)' : 'translateY(100%)',
        transition: 'transform 0.35s cubic-bezier(0.22,0.61,0.36,1)',
        overflow: 'hidden',
      }}>
        <div style={{ display: 'flex', justifyContent: 'center', padding: '12px 0 4px' }}>
          <div style={{ width: 36, height: 3, borderRadius: 2, background: 'var(--border-mid)' }} />
        </div>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 24px 12px', borderBottom: '1px solid var(--border)' }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 600, letterSpacing: '0.08em', color: 'var(--text-secondary)' }}>SIGN IN</span>
          <PBtn variant="ghost" onClick={onClose}>CANCEL</PBtn>
        </div>
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', padding: '24px 24px 0', gap: 12, maxWidth: 420 }}>
          <input
            type="email" value={email} onChange={e => setEmail(e.target.value)}
            placeholder="email address" style={inputStyle}
          />
          <input
            type="password" value={password} onChange={e => setPassword(e.target.value)}
            placeholder="password" style={inputStyle}
            onKeyDown={e => e.key === 'Enter' && handleSubmit()}
          />
          {error && (
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--danger)' }}>{error}</span>
          )}
          <PBtn onClick={handleSubmit} disabled={!email.trim() || !password || loading}>
            {loading ? 'SIGNING IN…' : 'SIGN IN →'}
          </PBtn>
        </div>
      </div>
    </>
  );
}

// ── Compose panel ─────────────────────────────────────────────

function ComposePanel({ open, token, onClose, onPublished }) {
  const [title, setTitle] = useState('');
  const [body, setBody] = useState('');
  const [kind, setKind] = useState('VISION');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');

  const reset = () => { setTitle(''); setBody(''); setKind('VISION'); setError(''); };

  const handlePublish = async () => {
    if (!title.trim() || !body.trim()) return;
    setSaving(true); setError('');
    try {
      const blocks = textToBlocks(body);
      const created = await authFetch('/api/v1/lore/entries', 'POST', {
        title: title.trim(),
        body_json: JSON.stringify(blocks),
        kind,
      }, token);
      await authFetch(`/api/v1/lore/entries/${created.data.id}/publish`, 'POST', null, token);
      reset();
      onPublished();
    } catch (e) {
      setError(e.message ?? 'Failed to publish');
    } finally {
      setSaving(false);
    }
  };

  const handleClose = () => { reset(); onClose(); };

  return (
    <>
      {open && <div onClick={handleClose} style={{ position: 'fixed', inset: 0, zIndex: 800, background: 'rgba(0,0,0,0.5)', backdropFilter: 'blur(2px)' }} />}
      <div style={{
        position: 'fixed', left: 0, right: 0, bottom: 0,
        height: '78vh', zIndex: 900,
        background: 'var(--bg-surface)',
        borderTop: '1px solid var(--border-mid)',
        borderRadius: '12px 12px 0 0',
        display: 'flex', flexDirection: 'column',
        transform: open ? 'translateY(0)' : 'translateY(100%)',
        transition: 'transform 0.35s cubic-bezier(0.22,0.61,0.36,1)',
        overflow: 'hidden',
      }}>
        {/* Drag handle */}
        <div style={{ display: 'flex', justifyContent: 'center', padding: '12px 0 4px' }}>
          <div style={{ width: 36, height: 3, borderRadius: 2, background: 'var(--border-mid)' }} />
        </div>
        {/* Header row */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 24px 12px', borderBottom: '1px solid var(--border)' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 600, letterSpacing: '0.08em', color: 'var(--text-secondary)' }}>NEW POST</span>
            <div style={{ display: 'flex', gap: 4 }}>
              {KINDS.map(k => (
                <button key={k} onClick={() => setKind(k)} style={{
                  padding: '2px 8px', borderRadius: 12,
                  fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 600, letterSpacing: '0.06em',
                  background: kind === k ? 'var(--bg-raised)' : 'transparent',
                  color: kind === k ? 'var(--text-primary)' : 'var(--text-muted)',
                  border: `1px solid ${kind === k ? 'var(--border-light)' : 'var(--border)'}`,
                  transition: 'all var(--t-fast)', cursor: 'pointer',
                }}>{k}</button>
              ))}
            </div>
          </div>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            {error && <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--danger)' }}>{error}</span>}
            <PBtn variant="ghost" onClick={handleClose}>CANCEL</PBtn>
            <PBtn onClick={handlePublish} disabled={!title.trim() || !body.trim() || saving}>
              {saving ? 'PUBLISHING…' : 'PUBLISH →'}
            </PBtn>
          </div>
        </div>
        {/* Body */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', padding: '24px 52px', gap: 16, overflow: 'auto', maxWidth: 860, width: '100%', margin: '0 auto' }}>
          <input
            value={title}
            onChange={e => setTitle(e.target.value)}
            placeholder="Post title…"
            style={{
              fontFamily: 'var(--ui)', fontSize: 26, fontWeight: 500,
              color: 'var(--text-primary)',
              background: 'transparent', border: 'none', outline: 'none',
              caretColor: 'var(--amber)', width: '100%',
            }}
          />
          <div style={{ height: 1, background: 'var(--border)' }} />
          <textarea
            value={body}
            onChange={e => setBody(e.target.value)}
            placeholder={'Write your post…\n\nDouble line break starts a new paragraph.\n## Heading starts a section.'}
            style={{
              flex: 1, width: '100%', minHeight: 280,
              fontFamily: 'var(--ui)', fontSize: 15, lineHeight: 1.75,
              color: 'var(--text-secondary)',
              background: 'transparent', border: 'none', outline: 'none',
              resize: 'none', caretColor: 'var(--amber)',
            }}
          />
        </div>
      </div>
    </>
  );
}

// ── Entry body renderer ───────────────────────────────────────

function BodyBlock({ block }) {
  const base = { fontFamily: 'var(--ui)', color: 'var(--text-secondary)', lineHeight: 1.75 };
  if (block.type === 'lead') return <p style={{ ...base, fontSize: 15, color: 'var(--text-primary)', fontWeight: 400, marginBottom: 20 }}>{block.text}</p>;
  if (block.type === 'p')    return <p style={{ ...base, fontSize: 14, marginBottom: 16 }}>{block.text}</p>;
  if (block.type === 'h2')   return <h2 style={{ fontFamily: 'var(--ui)', fontSize: 14, fontWeight: 600, color: 'var(--text-primary)', marginTop: 28, marginBottom: 12, letterSpacing: '-0.01em' }}>{block.text}</h2>;
  return null;
}

// ── Entry card ────────────────────────────────────────────────

function EntryCard({ entry, active, onSelect }) {
  const t = ENTRY_TYPES[entry.type] || ENTRY_TYPES.reflection;
  return (
    <button onClick={() => onSelect(entry.id)} style={{
      width: '100%', textAlign: 'right',
      padding: '18px 0 18px 16px',
      borderBottom: '1px solid var(--border)',
      background: active ? 'var(--bg-raised)' : 'transparent',
      transition: 'background var(--t-fast)',
    }}
    onMouseEnter={e => { if (!active) e.currentTarget.style.background = 'var(--bg-raised)'; }}
    onMouseLeave={e => { if (!active) e.currentTarget.style.background = 'transparent'; }}
    >
      <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.1em', textTransform: 'uppercase', marginBottom: 7 }}>
        {t.label} · {entry.date}
      </div>
      <div style={{ fontFamily: 'var(--ui)', fontSize: 12, fontWeight: active ? 500 : 400, color: active ? 'var(--text-primary)' : 'var(--text-secondary)', lineHeight: 1.5, marginBottom: 6 }}>
        {entry.title}
      </div>
      {entry.tags.length > 0 && (
        <div style={{ display: 'flex', gap: 5, justifyContent: 'flex-end', flexWrap: 'wrap' }}>
          {entry.tags.map(tag => (
            <span key={tag} style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.08em', textTransform: 'uppercase' }}>{tag}</span>
          ))}
        </div>
      )}
    </button>
  );
}

// ── Entry reader ──────────────────────────────────────────────

function EntryReader({ entry }) {
  const t = ENTRY_TYPES[entry.type] || ENTRY_TYPES.reflection;
  return (
    <div className="panel-enter" style={{ display: 'grid', gridTemplateColumns: '1fr 180px', height: '100%', overflow: 'hidden' }}>
      <div style={{ overflowY: 'auto', padding: '56px 52px 80px 52px' }}>
        <div style={{ maxWidth: 580 }}>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.12em', textTransform: 'uppercase', marginBottom: 16, display: 'flex', gap: 10, alignItems: 'center' }}>
            <span>{t.label}</span>
            <span style={{ opacity: 0.4 }}>·</span>
            <span>{entry.date}</span>
          </div>
          <h1 style={{ fontFamily: 'var(--ui)', fontSize: 22, fontWeight: 500, lineHeight: 1.35, letterSpacing: '-0.015em', color: 'var(--text-primary)', marginBottom: 20 }}>
            {entry.title}
          </h1>
          {entry.tags.length > 0 && (
            <div style={{ display: 'flex', gap: 5, marginBottom: 32, paddingBottom: 24, borderBottom: '1px solid var(--border)', flexWrap: 'wrap' }}>
              {entry.tags.map(tag => (
                <span key={tag} style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)', letterSpacing: '0.08em', textTransform: 'uppercase', padding: '2px 6px', border: '1px solid var(--border)', borderRadius: 3 }}>{tag}</span>
              ))}
            </div>
          )}
          {entry.body.map((block, i) => <BodyBlock key={i} block={block} />)}
        </div>
      </div>
      <div style={{ borderLeft: '1px solid var(--border)', padding: '56px 20px 80px 22px', overflowY: 'auto' }}>
        {entry.sidenotes.length > 0 && (
          <>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.14em', textTransform: 'uppercase', marginBottom: 20 }}>CONTEXT</div>
            {entry.sidenotes.map((n, i) => (
              <div key={i} style={{ marginBottom: 28, borderLeft: '1px solid var(--border-mid)', paddingLeft: 10 }}>
                <div style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.1em', textTransform: 'uppercase', marginBottom: 6 }}>{n.anchor}</div>
                <div style={{ fontFamily: 'var(--ui)', fontSize: 11, color: 'var(--text-muted)', lineHeight: 1.65 }}>{n.text}</div>
              </div>
            ))}
          </>
        )}
      </div>
    </div>
  );
}

// ── Main page ─────────────────────────────────────────────────

function LorePage() {
  const [entries, setEntries] = useState([]);
  const [loading, setLoading] = useState(true);
  const [activeId, setActiveId] = useState(null);
  const [category, setCategory] = useState('all');
  const [token, setToken] = useState(() => localStorage.getItem('sustena_token'));
  const [showSignIn, setShowSignIn] = useState(false);
  const [showCompose, setShowCompose] = useState(false);

  const fetchEntries = useCallback(() => {
    api.get('/api/v1/lore/entries')
      .then(res => {
        const normalized = (res.data?.entries ?? []).map(normalizeEntry);
        setEntries(normalized);
        if (normalized.length > 0) setActiveId(id => id ?? normalized[0].id);
      })
      .catch(() => setEntries([]))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => { fetchEntries(); }, [fetchEntries]);

  const categories = useMemo(() => [
    { id: 'all',        label: 'ALL',        count: entries.length },
    { id: 'vision',     label: 'VISION',     count: entries.filter(e => e.type === 'vision').length },
    { id: 'technical',  label: 'TECHNICAL',  count: entries.filter(e => e.type === 'technical').length },
    { id: 'reflection', label: 'REFLECTION', count: entries.filter(e => e.type === 'reflection').length },
    { id: 'update',     label: 'UPDATE',     count: entries.filter(e => e.type === 'update').length },
  ], [entries]);

  const filtered = useMemo(() =>
    category === 'all' ? entries : entries.filter(e => e.type === category),
  [entries, category]);

  const active = entries.find(e => e.id === activeId) ?? null;

  useEffect(() => {
    if (filtered.length > 0 && !filtered.find(e => e.id === activeId)) {
      setActiveId(filtered[0].id);
    }
  }, [category, filtered]);

  const handleSignedIn = (t) => {
    setToken(t);
    setShowSignIn(false);
  };

  const handleSignOut = () => {
    localStorage.removeItem('sustena_token');
    setToken(null);
  };

  const handlePublished = () => {
    setShowCompose(false);
    setLoading(true);
    fetchEntries();
  };

  return (
    <div style={{ height: '100%', background: 'var(--bg-base)' }}>
      <div style={{ display: 'grid', gridTemplateColumns: '240px 1fr', height: '100%', overflow: 'hidden' }}>

        {/* Left sidebar */}
        <div style={{ borderRight: '1px solid var(--border)', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
          {/* Category rail */}
          <div style={{ borderBottom: '1px solid var(--border)', padding: '14px 0', display: 'flex', gap: 2, flexWrap: 'wrap', justifyContent: 'flex-end' }}>
            {categories.map(cat => (
              <button key={cat.id} onClick={() => setCategory(cat.id)} style={{
                padding: '3px 8px',
                fontFamily: 'var(--mono)', fontSize: 8, letterSpacing: '0.1em', textTransform: 'uppercase',
                color: category === cat.id ? 'var(--text-primary)' : 'var(--text-dim)',
                background: 'transparent', border: 'none',
                transition: 'color var(--t-fast)', cursor: 'pointer',
              }}>{cat.label} <span style={{ opacity: 0.4 }}>{cat.count}</span></button>
            ))}
          </div>

          {/* Entry list */}
          <div style={{ overflowY: 'auto', flex: 1 }}>
            {loading
              ? <div style={{ padding: '24px 12px', textAlign: 'right', fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)' }}>loading</div>
              : filtered.length === 0
                ? <div style={{ padding: '24px 12px', textAlign: 'right', fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)' }}>no entries published yet</div>
                : filtered.map(entry => (
                    <EntryCard key={entry.id} entry={entry} active={entry.id === activeId} onSelect={setActiveId} />
                  ))
            }
          </div>

          {/* Footer — auth + write */}
          <div style={{ borderTop: '1px solid var(--border)', padding: '10px 0 10px', display: 'flex', flexDirection: 'column', alignItems: 'flex-end', gap: 6 }}>
            {token ? (
              <div style={{ display: 'flex', gap: 10, alignItems: 'center', paddingRight: 2 }}>
                <button onClick={handleSignOut} style={{
                  fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)',
                  background: 'none', border: 'none', cursor: 'pointer', letterSpacing: '0.08em',
                }}
                onMouseEnter={e => e.currentTarget.style.color = 'var(--text-muted)'}
                onMouseLeave={e => e.currentTarget.style.color = 'var(--text-dim)'}
                >SIGN OUT</button>
                <button onClick={() => setShowCompose(true)} style={{
                  fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 600, color: 'var(--text-secondary)',
                  background: 'none', border: 'none', cursor: 'pointer', letterSpacing: '0.1em',
                }}
                onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
                onMouseLeave={e => e.currentTarget.style.color = 'var(--text-secondary)'}
                >+ WRITE</button>
              </div>
            ) : (
              <button onClick={() => setShowSignIn(true)} style={{
                fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)',
                background: 'none', border: 'none', cursor: 'pointer', letterSpacing: '0.08em', paddingRight: 2,
              }}
              onMouseEnter={e => e.currentTarget.style.color = 'var(--text-muted)'}
              onMouseLeave={e => e.currentTarget.style.color = 'var(--text-dim)'}
              >sign in to write</button>
            )}
            <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.08em' }}>
              {entries.length} entries · lore.sustena
            </span>
          </div>
        </div>

        {/* Reader */}
        <div style={{ overflow: 'hidden' }}>
          {active
            ? <EntryReader entry={active} />
            : (
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%' }}>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>
                  {loading ? 'loading' : 'no entries published yet'}
                </span>
              </div>
            )
          }
        </div>
      </div>

      <SignInPanel
        open={showSignIn}
        onClose={() => setShowSignIn(false)}
        onSuccess={handleSignedIn}
      />
      <ComposePanel
        open={showCompose}
        token={token}
        onClose={() => setShowCompose(false)}
        onPublished={handlePublished}
      />
    </div>
  );
}

export default LorePage;
