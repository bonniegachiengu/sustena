/*
  /journal — Sustena XII operational journal.
  Design intent: minimal editorial layout inspired by brand/inspirations/Blog/
  — Linear-style long-form with left nav rail + right-margin sidenotes
  — Hiker's Journal: large title, author strip, full-width reader
  — kaobook: margin annotations for linked state variables
  — Left editor: category/section structure in slim left panel

  Registers: JournalPage → window
*/

const { useState: jUseState, useMemo: jUseMemo, useEffect: jUseEffect, useRef: jUseRef } = React;

/* ── Entry types ─────────────────────────────────────────────── */
const ENTRY_TYPES = {
  decision:   { label: 'DECISION',   color: 'var(--amber)',  bg: 'var(--amber-glow)' },
  note:       { label: 'NOTE',       color: 'var(--teal)',   bg: 'rgba(42,184,160,0.08)' },
  reflection: { label: 'REFLECTION', color: 'var(--info)',   bg: 'rgba(80,144,224,0.08)' },
  update:     { label: 'UPDATE',     color: 'var(--ok)',     bg: 'rgba(76,175,128,0.08)' },
};

/* ── Journal entries data ────────────────────────────────────── */
const JOURNAL_ENTRIES = [
  {
    id: 'j001',
    type: 'decision',
    sustain: 'Sustena XII',
    sustainColor: 'var(--amber)',
    title: 'Chose Haiku for inference layer — Orchie cost model',
    date: '2026-05-26',
    readTime: '4 min',
    tags: ['system', 'pawa', 'architecture'],
    preview: 'After running 3 simulation branches, the Haiku model at $0.80/MTok keeps Pawa burn within the 28/hr budget while sustaining 92% intent accuracy on the test corpus.',
    body: [
      { type: 'lead', text: 'After running 3 simulation branches, the Haiku model at $0.80/MTok keeps Pawa burn within the 28/hr budget while sustaining 92% intent accuracy on the test corpus.' },
      { type: 'h2', text: 'Why this matters' },
      { type: 'p', text: 'Orchie's inference cost is the primary Pawa drain. At current usage patterns (≈ 218 ops/min rolling average), Sonnet would put us at 94 Pawa/hr — breaking the constraint reserve > 50,000 within 6 weeks.' },
      { type: 'p', text: 'Branch A.2 showed Haiku + local state cache reduces token usage by 34% with no detectable drop in structured output quality across the decision, update, and note entry types.' },
      { type: 'h2', text: 'Constraints satisfied' },
      { type: 'sidenote', anchor: 'system.pawa_balance', note: 'Current balance: 8,420 pwa' },
      { type: 'p', text: 'All four constraints pass: reserve > 50,000 ✓, pockets.sum ≤ income ✓, no_negative_balance ✓, cycle_quorum ≥ 80% (borderline at 78% — flagged).' },
      { type: 'h2', text: 'Decision' },
      { type: 'p', text: 'Lock Orchie to claude-haiku-4-5 for the next 30-day cycle. Re-evaluate if P95 exceeds 600ms three sessions in a row. The Mentor operative will monitor the anomaly queue.' },
    ],
    sidenotes: [
      { anchor: 'system.pawa_balance', text: 'Current balance: 8,420 pwa', color: 'var(--teal)' },
      { anchor: 'system.api_p95_ms', text: 'P95 today: 428ms · within target 400ms', color: 'var(--amber)' },
    ],
  },
  {
    id: 'j002',
    type: 'update',
    sustain: 'Vyyb',
    sustainColor: 'var(--teal)',
    title: 'Phase 1 setup complete — delivery dispatch queued',
    date: '2026-05-25',
    readTime: '2 min',
    tags: ['vyyb', 'phase-1', 'logistics'],
    preview: 'All entity docs filed. Delivery dispatch scheduled for Monday 08:00. Curator operative has the pantry threshold alert queued — cooking oil critical.',
    body: [
      { type: 'lead', text: 'Vyyb Phase 1 setup is complete. Entity documentation filed with the registrar. Delivery dispatch is queued for Monday 08:00 EAT.' },
      { type: 'p', text: 'The Curator flagged cooking oil at 0.4L against a target of 5.0L — critically below threshold. Batch order approval is pending from Bonnie. See alert: pantry.cooking_oil_L = 0.4.' },
      { type: 'h2', text: 'What\'s next' },
      { type: 'p', text: 'Phase 2 begins with the chama contribution cycle opening on June 1. Quorum is currently at 78% — 2% below the 80% constraint. Navigator has been assigned to close the gap.' },
    ],
    sidenotes: [
      { anchor: 'pantry.cooking_oil_L', text: '0.4 L remaining · critical', color: 'var(--danger)' },
      { anchor: 'chama.contributions', text: '198,400 KSH pool balance', color: 'var(--ok)' },
    ],
  },
  {
    id: 'j003',
    type: 'reflection',
    sustain: 'Sustena XII',
    sustainColor: 'var(--info)',
    title: 'On building a sustain from scratch — the first 90 days',
    date: '2026-05-20',
    readTime: '8 min',
    tags: ['founder', 'philosophy', 'sustain-design'],
    preview: 'There is no surer way to understand a system than to build one. The past 90 days have shown me what a sustain actually is — not a product, not a company, but a living financial organism.',
    body: [
      { type: 'lead', text: 'There is no surer way to understand a system than to build one. The past 90 days have shown me what a sustain actually is — not a product, not a company, but a living financial organism.' },
      { type: 'p', text: 'When I started Sustena XII, the concept of Pawa felt abstract. Tokens as a unit of agent effort — who thinks in those terms? But watching the Mentor operative burn 42 Pawa to produce an anomaly report that saved 10,000 KSH in misdirected pocket allocation — suddenly the ledger makes sense.' },
      { type: 'h2', text: 'What operatives actually do' },
      { type: 'p', text: 'The first surprise: operatives don\'t just execute tasks. They maintain context across sessions in a way I don\'t have to. The Navigator holds the chama schedule in working memory. The Curator monitors pantry thresholds I\'d forget about. The Mentor watches the burn rate trajectory so I can think about strategy instead of arithmetic.' },
      { type: 'p', text: 'The second surprise: the constraint system is not a cage — it\'s a conscience. reserve > 50,000 is not a restriction; it\'s a commitment I made to myself, enforced by a system that doesn\'t negotiate.' },
    ],
    sidenotes: [],
  },
  {
    id: 'j004',
    type: 'note',
    sustain: 'Chama XII',
    sustainColor: 'var(--ok)',
    title: 'Quorum risk — Navigator assigned, follow-up Tuesday',
    date: '2026-05-24',
    readTime: '1 min',
    tags: ['chama', 'quorum', 'council'],
    preview: 'Council quorum at 78%, below the 80% threshold. Navigator operative tasked with outreach. Will check back Tuesday before the cycle closes.',
    body: [
      { type: 'lead', text: 'Council quorum dropped to 78% — 2% below the constraint floor. Not critical yet, but needs resolution before the June 1 cycle opens.' },
      { type: 'p', text: 'Navigator has been assigned to outreach. Three members haven\'t confirmed participation for the next cycle. If quorum isn\'t restored, the simulation shows a 0.61 branch score — acceptable but not optimal.' },
    ],
    sidenotes: [
      { anchor: 'council.quorum_pct', text: '78% · threshold 80%', color: 'var(--amber)' },
    ],
  },
];

const FILTER_TABS = ['ALL', 'DECISION', 'UPDATE', 'REFLECTION', 'NOTE'];

/* ── Compose panel ───────────────────────────────────────────── */
function ComposePanel({ open, onClose, onSave }) {
  const [title, setTitle] = jUseState('');
  const [body, setBody] = jUseState('');
  const [type, setType] = jUseState('note');
  const [sustain, setSustain] = jUseState('Sustena XII');

  const handleSave = () => {
    if (!title.trim()) return;
    onSave({ title, body, type, sustain });
    setTitle(''); setBody(''); setType('note');
    onClose();
    window.flash?.('Entry saved to journal', 'ok');
  };

  return (
    <>
      {/* Backdrop */}
      {open && (
        <div onClick={onClose} style={{
          position: 'fixed', inset: 0, zIndex: 800,
          background: 'rgba(0,0,0,0.5)', backdropFilter: 'blur(2px)',
        }} />
      )}
      {/* Compose panel — slides up from bottom */}
      <div style={{
        position: 'fixed', left: 0, right: 0, bottom: 0,
        height: '70vh',
        zIndex: 900,
        background: 'var(--bg-surface)',
        borderTop: '1px solid var(--border-mid)',
        borderRadius: '12px 12px 0 0',
        display: 'flex', flexDirection: 'column',
        transform: open ? 'translateY(0)' : 'translateY(100%)',
        transition: 'transform 0.35s cubic-bezier(0.22,0.61,0.36,1)',
        overflow: 'hidden',
      }}>
        {/* Handle */}
        <div style={{ display: 'flex', justifyContent: 'center', padding: '12px 0 4px' }}>
          <div style={{ width: 36, height: 3, borderRadius: 2, background: 'var(--border-mid)' }} />
        </div>

        {/* Header */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 24px 12px', borderBottom: '1px solid var(--border)' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 600, letterSpacing: '0.08em', color: 'var(--text-secondary)' }}>COMPOSE ENTRY</span>
            {/* Type selector */}
            <div style={{ display: 'flex', gap: 4 }}>
              {Object.entries(ENTRY_TYPES).map(([k, v]) => (
                <button key={k} onClick={() => setType(k)} style={{
                  padding: '2px 8px', borderRadius: 12,
                  fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 600, letterSpacing: '0.06em',
                  background: type === k ? v.bg : 'transparent',
                  color: type === k ? v.color : 'var(--text-muted)',
                  border: `1px solid ${type === k ? v.color + '44' : 'var(--border)'}`,
                  transition: 'all var(--t-fast)',
                  cursor: 'pointer',
                }}>{v.label}</button>
              ))}
            </div>
          </div>
          <div style={{ display: 'flex', gap: 8 }}>
            <PBtn variant="ghost" onClick={onClose}>CANCEL</PBtn>
            <PBtn onClick={handleSave} disabled={!title.trim()}>SAVE ENTRY</PBtn>
          </div>
        </div>

        {/* Editor body */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', padding: '20px 24px', gap: 12, overflow: 'auto' }}>
          {/* Sustain selector */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span className="label-10">SUSTAIN</span>
            {['Sustena XII', 'Vyyb', 'Chama XII'].map(s => (
              <button key={s} onClick={() => setSustain(s)} style={{
                padding: '3px 10px', borderRadius: 12,
                fontFamily: 'var(--mono)', fontSize: 9,
                background: sustain === s ? 'var(--bg-raised)' : 'transparent',
                color: sustain === s ? 'var(--text-primary)' : 'var(--text-muted)',
                border: '1px solid ' + (sustain === s ? 'var(--border-mid)' : 'var(--border)'),
                cursor: 'pointer',
              }}>{s}</button>
            ))}
          </div>
          {/* Title */}
          <input
            value={title}
            onChange={e => setTitle(e.target.value)}
            placeholder="Entry title…"
            style={{
              fontFamily: 'var(--ui)', fontSize: 22, fontWeight: 500,
              color: 'var(--text-primary)',
              background: 'transparent', border: 'none', outline: 'none',
              caretColor: 'var(--amber)', width: '100%',
            }}
          />
          {/* Divider */}
          <div style={{ height: 1, background: 'var(--border)' }} />
          {/* Body */}
          <textarea
            value={body}
            onChange={e => setBody(e.target.value)}
            placeholder="Write your entry…"
            style={{
              flex: 1, width: '100%',
              fontFamily: 'var(--ui)', fontSize: 14, lineHeight: 1.7,
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

/* ── Entry card (index view) ─────────────────────────────────── */
function EntryCard({ entry, onClick, delay }) {
  const et = ENTRY_TYPES[entry.type];
  return (
    <button
      onClick={() => onClick(entry)}
      className="fade-up"
      style={{
        width: '100%', textAlign: 'left',
        background: 'var(--bg-surface)',
        border: '1px solid var(--border)',
        borderRadius: 'var(--radius-md)',
        padding: 0, overflow: 'hidden',
        display: 'flex', flexDirection: 'column',
        transition: 'border-color var(--t-fast), transform var(--t-fast)',
        cursor: 'pointer',
        animationDelay: `${delay}ms`,
        position: 'relative',
      }}
      onMouseEnter={e => {
        e.currentTarget.style.borderColor = 'var(--border-mid)';
        e.currentTarget.style.transform = 'translateY(-1px)';
      }}
      onMouseLeave={e => {
        e.currentTarget.style.borderColor = 'var(--border)';
        e.currentTarget.style.transform = 'translateY(0)';
      }}
    >
      {/* Sustain accent bar */}
      <div style={{ height: 2, background: entry.sustainColor, opacity: 0.8 }} />

      <div style={{ padding: '16px 20px', display: 'flex', flexDirection: 'column', gap: 10 }}>
        {/* Meta row */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{
            padding: '2px 8px', borderRadius: 10,
            fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 600, letterSpacing: '0.06em',
            background: et.bg, color: et.color,
          }}>{et.label}</span>
          <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{entry.sustain}</span>
          <span style={{ flex: 1 }} />
          <span className="meta-10" style={{ color: 'var(--text-dim)' }}>{entry.date}</span>
          <span className="meta-10" style={{ color: 'var(--text-dim)' }}>· {entry.readTime}</span>
        </div>

        {/* Title */}
        <span style={{
          fontFamily: 'var(--ui)', fontSize: 15, fontWeight: 500, lineHeight: 1.35,
          color: 'var(--text-primary)',
        }}>{entry.title}</span>

        {/* Preview */}
        <span style={{
          fontFamily: 'var(--ui)', fontSize: 12, lineHeight: 1.6,
          color: 'var(--text-muted)',
          display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical', overflow: 'hidden',
        }}>{entry.preview}</span>

        {/* Tags */}
        <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
          {entry.tags.map(t => (
            <span key={t} style={{
              padding: '1px 6px', borderRadius: 6,
              fontFamily: 'var(--mono)', fontSize: 9,
              color: 'var(--text-dim)',
              background: 'var(--bg-base)',
              border: '1px solid var(--border)',
            }}>{t}</span>
          ))}
        </div>
      </div>
    </button>
  );
}

/* ── Article reader ──────────────────────────────────────────── */
function EntryReader({ entry, onBack }) {
  const et = ENTRY_TYPES[entry.type];
  // Determine if there are sidenotes to show right margin
  const hasSidenotes = entry.sidenotes && entry.sidenotes.length > 0;

  return (
    <div className="panel-enter" style={{
      display: 'grid',
      gridTemplateColumns: hasSidenotes ? 'minmax(0,1fr) 200px' : '1fr',
      gap: 40, height: '100%', overflow: 'hidden',
    }}>
      {/* Main column */}
      <div style={{ overflow: 'auto', padding: '32px 40px 48px' }}>
        {/* Back */}
        <button onClick={onBack} style={{
          display: 'inline-flex', alignItems: 'center', gap: 6,
          fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500,
          color: 'var(--text-muted)', background: 'transparent', border: 'none',
          cursor: 'pointer', marginBottom: 32, padding: 0,
          transition: 'color var(--t-fast)',
        }}
          onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
          onMouseLeave={e => e.currentTarget.style.color = 'var(--text-muted)'}
        >
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
            <path d="M7 2L3 6L7 10" />
          </svg>
          JOURNAL
        </button>

        {/* Article header */}
        <div style={{ marginBottom: 32 }}>
          {/* Type badge + sustain */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 16 }}>
            <span style={{
              padding: '3px 10px', borderRadius: 12,
              fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 700, letterSpacing: '0.08em',
              background: et.bg, color: et.color,
            }}>{et.label}</span>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: entry.sustainColor, fontWeight: 500 }}>
              {entry.sustain}
            </span>
          </div>

          {/* Title */}
          <h1 style={{
            fontFamily: 'var(--ui)', fontSize: 28, fontWeight: 500, lineHeight: 1.25,
            color: 'var(--text-primary)', margin: '0 0 16px',
            letterSpacing: '-0.01em',
          }}>{entry.title}</h1>

          {/* Author + date line */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 14, paddingBottom: 20, borderBottom: '1px solid var(--border)' }}>
            {/* Avatar */}
            <div style={{
              width: 28, height: 28, borderRadius: '50%',
              background: 'linear-gradient(135deg, var(--amber), var(--teal))',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              flexShrink: 0,
            }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: '#000', fontWeight: 700 }}>B</span>
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
              <span style={{ fontFamily: 'var(--ui)', fontSize: 12, fontWeight: 500, color: 'var(--text-primary)' }}>Bonnie</span>
              <span className="meta-10" style={{ color: 'var(--text-muted)' }}>@bonnie · sustena XII founder</span>
            </div>
            <span style={{ flex: 1 }} />
            <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{entry.date}</span>
            <span className="meta-10" style={{ color: 'var(--text-dim)' }}>· {entry.readTime} read</span>
          </div>
        </div>

        {/* Body content */}
        <div style={{ maxWidth: 660 }}>
          {(entry.body || []).map((block, i) => {
            if (block.type === 'lead') return (
              <p key={i} style={{
                fontFamily: 'var(--ui)', fontSize: 15, lineHeight: 1.75,
                color: 'var(--text-secondary)', fontWeight: 400,
                margin: '0 0 24px',
                paddingLeft: 16, borderLeft: `2px solid ${et.color}`,
              }}>{block.text}</p>
            );
            if (block.type === 'h2') return (
              <h2 key={i} style={{
                fontFamily: 'var(--ui)', fontSize: 14, fontWeight: 600,
                color: 'var(--text-primary)', margin: '32px 0 12px',
                letterSpacing: '0.02em',
              }}>{block.text}</h2>
            );
            if (block.type === 'p') return (
              <p key={i} style={{
                fontFamily: 'var(--ui)', fontSize: 14, lineHeight: 1.8,
                color: 'var(--text-secondary)',
                margin: '0 0 16px',
              }}>{block.text}</p>
            );
            if (block.type === 'sidenote') return null; // rendered in margin
            return null;
          })}
        </div>

        {/* Tags footer */}
        <div style={{ display: 'flex', gap: 4, marginTop: 32, paddingTop: 20, borderTop: '1px solid var(--border)' }}>
          {entry.tags.map(t => (
            <span key={t} style={{
              padding: '3px 8px', borderRadius: 6,
              fontFamily: 'var(--mono)', fontSize: 9,
              color: 'var(--text-dim)',
              background: 'var(--bg-base)',
              border: '1px solid var(--border)',
            }}>{t}</span>
          ))}
        </div>
      </div>

      {/* Sidenotes column */}
      {hasSidenotes && (
        <div style={{
          padding: '32px 20px 32px 0',
          display: 'flex', flexDirection: 'column', gap: 16,
          overflowY: 'auto',
          borderLeft: '1px solid var(--border)',
          paddingLeft: 20,
        }}>
          <span className="label-10" style={{ color: 'var(--text-dim)', marginBottom: 4 }}>STATE ANNOTATIONS</span>
          {entry.sidenotes.map((note, i) => (
            <div key={i} style={{
              padding: '10px 12px',
              background: 'var(--bg-base)',
              border: `1px solid ${note.color}33`,
              borderLeft: `2px solid ${note.color}`,
              borderRadius: 'var(--radius-sm)',
            }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', display: 'block', marginBottom: 4 }}>
                {note.anchor}
              </span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: note.color, fontWeight: 500 }}>
                {note.text}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/* ── Left navigation rail ────────────────────────────────────── */
function JournalNav({ filter, setFilter, onCompose, entryCount }) {
  const filters = [
    { key: 'ALL', label: 'All entries', count: entryCount },
    { key: 'DECISION', label: 'Decisions', count: null },
    { key: 'UPDATE', label: 'Updates', count: null },
    { key: 'REFLECTION', label: 'Reflections', count: null },
    { key: 'NOTE', label: 'Notes', count: null },
  ];

  return (
    <div style={{
      width: 180, flexShrink: 0,
      borderRight: '1px solid var(--border)',
      display: 'flex', flexDirection: 'column',
      padding: '28px 0',
      gap: 2,
    }}>
      {/* Header */}
      <div style={{ padding: '0 18px 20px', borderBottom: '1px solid var(--border)', marginBottom: 8 }}>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 700, color: 'var(--text-primary)', letterSpacing: '0.04em' }}>
            JOURNAL
          </span>
        </div>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>sustena XII · @bonnie</span>
      </div>

      {/* Compose button */}
      <div style={{ padding: '0 12px 16px' }}>
        <button onClick={onCompose} style={{
          width: '100%', padding: '7px 12px',
          fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 600, letterSpacing: '0.06em',
          color: 'var(--bg-base)', background: 'var(--text-primary)',
          border: 'none', borderRadius: 'var(--radius-sm)',
          cursor: 'pointer', display: 'flex', alignItems: 'center', gap: 6,
          justifyContent: 'center',
          transition: 'opacity var(--t-fast)',
        }}
          onMouseEnter={e => e.currentTarget.style.opacity = '0.85'}
          onMouseLeave={e => e.currentTarget.style.opacity = '1'}
        >
          <svg width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round">
            <path d="M4.5 1V8M1 4.5H8" />
          </svg>
          COMPOSE
        </button>
      </div>

      {/* Divider */}
      <div style={{ height: 1, background: 'var(--border)', margin: '0 12px 8px' }} />

      {/* Filter list */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 0, flex: 1 }}>
        {filters.map(f => {
          const et = f.key !== 'ALL' ? ENTRY_TYPES[f.key.toLowerCase()] : null;
          const isActive = filter === f.key;
          return (
            <button
              key={f.key}
              onClick={() => setFilter(f.key)}
              style={{
                width: '100%', textAlign: 'left',
                display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                padding: '7px 18px',
                fontFamily: 'var(--ui)', fontSize: 12,
                color: isActive ? 'var(--text-primary)' : 'var(--text-muted)',
                background: isActive ? 'var(--bg-raised)' : 'transparent',
                borderLeft: `2px solid ${isActive ? (et ? et.color : 'var(--text-primary)') : 'transparent'}`,
                border: 'none', cursor: 'pointer',
                transition: 'all var(--t-fast)',
              }}
              onMouseEnter={e => !isActive && (e.currentTarget.style.color = 'var(--text-secondary)')}
              onMouseLeave={e => !isActive && (e.currentTarget.style.color = 'var(--text-muted)')}
            >
              <span>{f.label}</span>
              {f.count != null && (
                <span style={{
                  fontFamily: 'var(--mono)', fontSize: 9,
                  color: isActive ? 'var(--text-secondary)' : 'var(--text-dim)',
                  background: 'var(--bg-base)', padding: '1px 5px', borderRadius: 8,
                }}>{f.count}</span>
              )}
            </button>
          );
        })}
      </div>

      {/* Bottom stats */}
      <div style={{ padding: '16px 18px 0', borderTop: '1px solid var(--border)', display: 'flex', flexDirection: 'column', gap: 6 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between' }}>
          <span className="meta-10">ENTRIES</span>
          <span className="meta-10" style={{ color: 'var(--text-secondary)' }}>{entryCount}</span>
        </div>
        <div style={{ display: 'flex', justifyContent: 'space-between' }}>
          <span className="meta-10">MEMBER SINCE</span>
          <span className="meta-10" style={{ color: 'var(--text-secondary)' }}>2026</span>
        </div>
      </div>
    </div>
  );
}

/* ── Main JournalPage ────────────────────────────────────────── */
function JournalPage({ onClose }) {
  const [filter, setFilter] = jUseState('ALL');
  const [activeEntry, setActiveEntry] = jUseState(null);
  const [composeOpen, setComposeOpen] = jUseState(false);
  const [extraEntries, setExtraEntries] = jUseState([]);

  const allEntries = [...extraEntries, ...JOURNAL_ENTRIES];

  const filtered = jUseMemo(() => {
    if (filter === 'ALL') return allEntries;
    return allEntries.filter(e => e.type === filter.toLowerCase());
  }, [filter, extraEntries]);

  const handleSave = (draft) => {
    const newEntry = {
      id: `j${Date.now()}`,
      type: draft.type,
      sustain: draft.sustain,
      sustainColor: 'var(--amber)',
      title: draft.title,
      date: new Date().toISOString().slice(0, 10),
      readTime: '< 1 min',
      tags: [draft.type],
      preview: draft.body.slice(0, 160) || '(no body)',
      body: [
        { type: 'lead', text: draft.body || '(empty)' },
      ],
      sidenotes: [],
    };
    setExtraEntries(prev => [newEntry, ...prev]);
  };

  return (
    <div style={{
      position: 'fixed', inset: 0, zIndex: 600,
      background: 'var(--bg-base)',
      display: 'flex', flexDirection: 'column',
    }}>
      {/* Top bar */}
      <div style={{
        height: 44, flexShrink: 0,
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        padding: '0 20px',
        borderBottom: '1px solid var(--border)',
        background: 'var(--bg-surface)',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          {onClose && (
            <button onClick={onClose} style={{
              display: 'flex', alignItems: 'center', gap: 6,
              fontFamily: 'var(--mono)', fontSize: 10,
              color: 'var(--text-muted)', background: 'transparent', border: 'none',
              cursor: 'pointer',
            }}>
              <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
                <path d="M7 2L3 6L7 10" />
              </svg>
              BACK
            </button>
          )}
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 600, color: 'var(--text-primary)', letterSpacing: '0.06em' }}>
            /journal
          </span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{allEntries.length} entries</span>
          <button onClick={() => setComposeOpen(true)} style={{
            padding: '4px 12px',
            fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 700, letterSpacing: '0.06em',
            color: 'var(--amber)', background: 'var(--amber-glow)',
            border: '1px solid var(--amber)44', borderRadius: 'var(--radius-sm)',
            cursor: 'pointer', transition: 'all var(--t-fast)',
          }}>+ COMPOSE</button>
        </div>
      </div>

      {/* Body: nav + content */}
      <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
        {/* Left nav — hidden in reader mode */}
        {!activeEntry && (
          <JournalNav
            filter={filter}
            setFilter={setFilter}
            onCompose={() => setComposeOpen(true)}
            entryCount={allEntries.length}
          />
        )}

        {/* Main content area */}
        <div style={{ flex: 1, minWidth: 0, overflow: 'hidden' }}>
          {activeEntry ? (
            <EntryReader entry={activeEntry} onBack={() => setActiveEntry(null)} />
          ) : (
            <div style={{ height: '100%', overflow: 'auto', padding: '28px 32px' }}>
              {/* Index header */}
              <div style={{ marginBottom: 20 }}>
                <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between' }}>
                  <span style={{ fontFamily: 'var(--ui)', fontSize: 18, fontWeight: 500, color: 'var(--text-primary)' }}>
                    {filter === 'ALL' ? 'All entries' : ENTRY_TYPES[filter.toLowerCase()]?.label + 's'}
                  </span>
                  <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{filtered.length} entries</span>
                </div>
                {/* Stats strip — inspired by "200+ / 50+" stat cards */}
                <div style={{ display: 'flex', gap: 16, marginTop: 12 }}>
                  {[
                    { val: allEntries.length, label: 'Total entries' },
                    { val: allEntries.filter(e => e.type === 'decision').length, label: 'Decisions recorded' },
                    { val: allEntries.filter(e => e.sidenotes?.length > 0).length, label: 'State annotations' },
                    { val: 3, label: 'Active sustains' },
                  ].map((s, i) => (
                    <div key={i} style={{
                      padding: '10px 16px',
                      background: 'var(--bg-surface)',
                      border: '1px solid var(--border)',
                      borderRadius: 'var(--radius-sm)',
                      display: 'flex', flexDirection: 'column', gap: 3,
                      minWidth: 80,
                    }}>
                      <span style={{ fontFamily: 'var(--mono)', fontSize: 20, fontWeight: 700, color: 'var(--text-primary)' }}>{s.val}</span>
                      <span className="meta-10" style={{ color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>{s.label}</span>
                    </div>
                  ))}
                </div>
              </div>

              {/* Entry cards grid */}
              {filtered.length === 0 ? (
                <div style={{ textAlign: 'center', padding: '48px 0', color: 'var(--text-muted)', fontFamily: 'var(--mono)', fontSize: 11 }}>
                  No entries in this category yet. <button onClick={() => setComposeOpen(true)} style={{ color: 'var(--amber)', background: 'none', border: 'none', cursor: 'pointer', fontFamily: 'var(--mono)', fontSize: 11, textDecoration: 'underline' }}>Compose one →</button>
                </div>
              ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
                  {filtered.map((entry, i) => (
                    <EntryCard key={entry.id} entry={entry} onClick={setActiveEntry} delay={i * 40} />
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Compose panel */}
      <ComposePanel
        open={composeOpen}
        onClose={() => setComposeOpen(false)}
        onSave={handleSave}
      />
    </div>
  );
}

Object.assign(window, { JournalPage });
