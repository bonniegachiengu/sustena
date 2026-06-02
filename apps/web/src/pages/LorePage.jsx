import React, { useState, useEffect, useMemo } from 'react';
import { api } from '../lib/api.js';

const ENTRY_TYPES = {
  vision:     { label: 'VISION'     },
  technical:  { label: 'TECHNICAL'  },
  reflection: { label: 'REFLECTION' },
  update:     { label: 'UPDATE'     },
};

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

/* ── Entry body renderer ─────────────────────────────── */
function BodyBlock({ block }) {
  const base = { fontFamily: 'var(--ui)', color: 'var(--text-secondary)', lineHeight: 1.75 };
  if (block.type === 'lead')    return <p style={{ ...base, fontSize: 15, color: 'var(--text-primary)', fontWeight: 400, marginBottom: 20 }}>{block.text}</p>;
  if (block.type === 'p')       return <p style={{ ...base, fontSize: 14, marginBottom: 16 }}>{block.text}</p>;
  if (block.type === 'h2')      return <h2 style={{ fontFamily: 'var(--ui)', fontSize: 14, fontWeight: 600, color: 'var(--text-primary)', marginTop: 28, marginBottom: 12, letterSpacing: '-0.01em' }}>{block.text}</h2>;
  return null;
}

/* ── Entry card — right-aligned, minimal ────────────── */
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

/* ── Entry reader — kaobook: main column + right margin ─ */
function EntryReader({ entry }) {
  const t = ENTRY_TYPES[entry.type] || ENTRY_TYPES.reflection;
  return (
    <div className="panel-enter" style={{ display: 'grid', gridTemplateColumns: '1fr 180px', height: '100%', overflow: 'hidden' }}>
      {/* Main text column */}
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
      {/* Right margin — sidenotes */}
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

/* ── Main page — T boundary layout ──────────────────── */
function LorePage() {
  const [entries, setEntries] = useState([]);
  const [loading, setLoading] = useState(true);
  const [activeId, setActiveId] = useState(null);
  const [category, setCategory] = useState('all');

  useEffect(() => {
    api.get('/api/v1/lore/entries')
      .then(res => {
        const normalized = (res.data?.entries ?? []).map(normalizeEntry);
        setEntries(normalized);
        if (normalized.length > 0) setActiveId(normalized[0].id);
      })
      .catch(() => setEntries([]))
      .finally(() => setLoading(false));
  }, []);

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
          {/* Footer */}
          <div style={{ borderTop: '1px solid var(--border)', padding: '10px 0', textAlign: 'right' }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.08em' }}>{entries.length} entries · lore.sustena</span>
          </div>
        </div>

        {/* Reader */}
        <div style={{ overflow: 'hidden' }}>
          {active
            ? <EntryReader entry={active} />
            : (
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-muted)' }}>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>
                  {loading ? 'loading' : 'no entries published yet'}
                </span>
              </div>
            )
          }
        </div>
      </div>
    </div>
  );
}

export default LorePage;
