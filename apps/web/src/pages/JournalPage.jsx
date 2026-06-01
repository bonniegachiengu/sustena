import React, { useState, useEffect, useRef, useMemo, useCallback } from 'react';


    /* ════════════════════════════════════════════════════════════
       CORE PRIMITIVES (from core.jsx)
       ════════════════════════════════════════════════════════════ */

    function SustenaMark({ size = 18, primary = 'var(--amber)', secondary = 'var(--text-primary)', mono = false }) {
      const a = mono ? secondary : primary;
      const c = secondary;
      const endX = 78 * Math.cos((28 * Math.PI) / 180);
      const endY = 78 * Math.sin((28 * Math.PI) / 180);
      const branchPath = `M 0 0 Q 52 0 ${endX} ${endY}`;
      return (
        <svg width={size} height={size} viewBox="-100 -100 200 200" style={{ display: 'block' }}>
          {[-90, 30, 150].map(r => (
            <g key={r} transform={`rotate(${r})`}>
              <path d={branchPath} stroke={c} strokeWidth="14" fill="none" strokeLinecap="round" />
              <circle cx={endX} cy={endY} r="11" fill={c} />
            </g>
          ))}
          <circle cx="0" cy="0" r="22" fill={a} />
        </svg>
      );
    }

    function Icon({ name, size = 14 }) {
      const s = { width: size, height: size, fill: 'none', stroke: 'currentColor', strokeWidth: 1.4, strokeLinecap: 'round', strokeLinejoin: 'round' };
      switch (name) {
        case 'warning': return <svg {...s} viewBox="0 0 16 16"><path d="M8 2l6.5 11h-13z" /><path d="M8 6v3.5M8 11.5v0.5" /></svg>;
        case 'replay':  return <svg {...s} viewBox="0 0 16 16"><path d="M3 8a5 5 0 1 0 5-5" /><path d="M3 3v3h3" /></svg>;
        case 'arrow-up':   return <svg {...s} viewBox="0 0 16 16"><path d="M8 13V3M4 7l4-4 4 4" /></svg>;
        case 'arrow-down': return <svg {...s} viewBox="0 0 16 16"><path d="M8 3v10M4 9l4 4 4-4" /></svg>;
        default: return null;
      }
    }

    function PBtn({ children, onClick, variant = 'primary', disabled }) {
      const [hover, setHover] = useState(false);
      const primary = variant === 'primary';
      return (
        <button
          onClick={onClick}
          disabled={disabled}
          onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
          style={{
            padding: '6px 14px',
            fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500,
            letterSpacing: '0.08em', textTransform: 'uppercase',
            color: primary ? 'var(--text-primary)' : (hover ? 'var(--text-primary)' : 'var(--text-secondary)'),
            background: primary ? (hover ? 'var(--border-light)' : 'var(--border-mid)') : (hover ? 'var(--bg-raised)' : 'transparent'),
            border: '1px solid ' + (primary ? 'var(--border-light)' : 'var(--border-mid)'),
            borderRadius: 'var(--radius-sm)',
            transition: 'all var(--t-fast)',
            display: 'inline-flex', alignItems: 'center', gap: 6,
            opacity: disabled ? 0.4 : 1,
            cursor: disabled ? 'not-allowed' : 'pointer',
          }}>
          {children}
        </button>
      );
    }

    /* ════════════════════════════════════════════════════════════
       JOURNAL (from journal.jsx)
       ════════════════════════════════════════════════════════════ */

    const ENTRY_TYPES = {
      decision:   { label: 'DECISION',   color: 'var(--text-secondary)', bg: 'transparent' },
      note:       { label: 'NOTE',       color: 'var(--text-secondary)', bg: 'transparent' },
      reflection: { label: 'REFLECTION', color: 'var(--text-secondary)', bg: 'transparent' },
      update:     { label: 'UPDATE',     color: 'var(--text-secondary)', bg: 'transparent' },
    };

    /* Journal entries — populated from API / user input */
    const JOURNAL_ENTRIES = [];

    function ComposePanel({ open, onClose, onSave }) {
      const [title, setTitle] = useState('');
      const [body, setBody] = useState('');
      const [type, setType] = useState('note');
      const [sustain, setSustain] = useState('Sustena XII');

      const handleSave = () => {
        if (!title.trim()) return;
        onSave({ title, body, type, sustain });
        setTitle(''); setBody(''); setType('note');
        onClose();
      };

      return (
        <>
          {open && (
            <div onClick={onClose} style={{
              position: 'fixed', inset: 0, zIndex: 800,
              background: 'rgba(0,0,0,0.5)', backdropFilter: 'blur(2px)',
            }} />
          )}
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
            <div style={{ display: 'flex', justifyContent: 'center', padding: '12px 0 4px' }}>
              <div style={{ width: 36, height: 3, borderRadius: 2, background: 'var(--border-mid)' }} />
            </div>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 24px 12px', borderBottom: '1px solid var(--border)' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 600, letterSpacing: '0.08em', color: 'var(--text-secondary)' }}>COMPOSE ENTRY</span>
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
            <div style={{ flex: 1, display: 'flex', flexDirection: 'column', padding: '20px 24px', gap: 12, overflow: 'auto' }}>
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
              <div style={{ height: 1, background: 'var(--border)' }} />
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

    function EntryCard({ entry, onClick, delay }) {
      const et = ENTRY_TYPES[entry.type];
      return (
        <button
          onClick={() => onClick(entry)}
          className="fade-up"
          style={{
            width: '100%', textAlign: 'left',
            background: 'transparent',
            border: 'none',
            borderBottom: '1px solid var(--border)',
            padding: '18px 0 18px 0',
            display: 'flex', flexDirection: 'column',
            transition: 'background var(--t-fast)',
            cursor: 'pointer',
            animationDelay: `${delay}ms`,
            gap: 6,
          }}
          onMouseEnter={e => e.currentTarget.style.background = 'var(--bg-raised)'}
          onMouseLeave={e => e.currentTarget.style.background = 'transparent'}
        >
          <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.1em', textTransform: 'uppercase' }}>
            {et.label} · {entry.sustain} · {entry.date}
          </div>
          <div style={{ fontFamily: 'var(--ui)', fontSize: 12, fontWeight: 500, color: 'var(--text-secondary)', lineHeight: 1.5 }}>
            {entry.title}
          </div>
          <div style={{ display: 'flex', gap: 5, justifyContent: 'flex-start', flexWrap: 'wrap' }}>
            {entry.tags.map(t => (
              <span key={t} style={{
                fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.08em', textTransform: 'uppercase',
              }}>{t}</span>
            ))}
          </div>
        </button>
      );
    }

    function EntryReader({ entry }) {
      const et = ENTRY_TYPES[entry.type];
      const hasSidenotes = entry.sidenotes && entry.sidenotes.length > 0;

      return (
        <div className="panel-enter" style={{
          display: 'grid',
          gridTemplateColumns: hasSidenotes ? 'minmax(0,1fr) 200px' : '1fr',
          gap: 40, height: '100%', overflow: 'hidden',
        }}>
          <div style={{ overflow: 'auto', padding: '36px 48px 48px 36px' }}>

            <div style={{ marginBottom: 32 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 16 }}>
                <span style={{
                  fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500, letterSpacing: '0.1em', textTransform: 'uppercase',
                  color: 'var(--text-muted)',
                }}>{et.label}</span>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)', fontWeight: 400 }}>
                  · {entry.sustain}
                </span>
              </div>

              <h1 style={{
                fontFamily: 'var(--ui)', fontSize: 28, fontWeight: 500, lineHeight: 1.25,
                color: 'var(--text-primary)', margin: '0 0 16px',
                letterSpacing: '-0.01em',
              }}>{entry.title}</h1>

              <div style={{ display: 'flex', alignItems: 'center', gap: 14, paddingBottom: 20, borderBottom: '1px solid var(--border)' }}>
                <div style={{
                  width: 28, height: 28, borderRadius: '50%',
                  border: '1px solid var(--border-mid)',
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  flexShrink: 0,
                }}>
                  <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)', fontWeight: 500 }}>B</span>
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
                  <span style={{ fontFamily: 'var(--ui)', fontSize: 12, fontWeight: 500, color: 'var(--text-primary)' }}>Bonnie Gachiengu</span>
                  <span className="meta-10" style={{ color: 'var(--text-muted)' }}>my.bg · sustena XII founder</span>
                </div>
                <span style={{ flex: 1 }} />
                <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{entry.date}</span>
                <span className="meta-10" style={{ color: 'var(--text-dim)' }}>· {entry.readTime} read</span>
              </div>
            </div>

            <div style={{ maxWidth: 660 }}>
              {(entry.body || []).map((block, i) => {
                if (block.type === 'lead') return (
                  <p key={i} style={{
                    fontFamily: 'var(--ui)', fontSize: 15, lineHeight: 1.75,
                    color: 'var(--text-secondary)', fontWeight: 400,
                    margin: '0 0 24px',
                    paddingLeft: 16, borderLeft: '2px solid var(--border-mid)',
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
                return null;
              })}
            </div>

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

          {hasSidenotes && (
            <div style={{
              padding: '32px 20px 32px 20px',
              display: 'flex', flexDirection: 'column', gap: 16,
              overflowY: 'auto',
              borderLeft: '1px solid var(--border)',
            }}>
              <span className="label-10" style={{ color: 'var(--text-dim)', marginBottom: 4 }}>STATE ANNOTATIONS</span>
              {entry.sidenotes.map((note, i) => (
                <div key={i} style={{
                  marginBottom: 20,
                  borderLeft: '1px solid var(--border-mid)',
                  paddingLeft: 10,
                }}>
                  <div style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.1em', textTransform: 'uppercase', marginBottom: 4 }}>
                    {note.anchor}
                  </div>
                  <div style={{ fontFamily: 'var(--ui)', fontSize: 11, color: 'var(--text-muted)', lineHeight: 1.65 }}>
                    {note.text}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      );
    }

    /* ── Entry navigation sidebar — shown when reading an entry ── */
    function EntryNavSidebar({ entries, activeEntry, onSelect, onBack }) {
      const currentIdx = entries.findIndex(e => e.id === activeEntry.id);
      const prevEntry = currentIdx > 0 ? entries[currentIdx - 1] : null;
      const nextEntry = currentIdx < entries.length - 1 ? entries[currentIdx + 1] : null;

      function MiniEntryBtn({ entry: e, dirLabel }) {
        const et = ENTRY_TYPES[e.type];
        const [hov, setHov] = useState(false);
        return (
          <button
            onClick={() => onSelect(e)}
            onMouseEnter={() => setHov(true)}
            onMouseLeave={() => setHov(false)}
            style={{
              width: '100%', textAlign: 'right',
              background: hov ? 'var(--bg-raised)' : 'transparent',
              border: 'none', borderBottom: '1px solid var(--border)',
              padding: '14px 20px 14px 8px',
              display: 'flex', flexDirection: 'column', gap: 4,
              cursor: 'pointer', transition: 'background var(--t-fast)',
            }}
          >
            <div style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.1em', textTransform: 'uppercase' }}>{dirLabel}</div>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.06em', textTransform: 'uppercase', marginTop: 1 }}>{et.label} · {e.date}</div>
            <div style={{ fontFamily: 'var(--ui)', fontSize: 10, color: hov ? 'var(--text-secondary)' : 'var(--text-muted)', lineHeight: 1.4, textAlign: 'right', transition: 'color var(--t-fast)' }}>{e.title}</div>
          </button>
        );
      }

      return (
        <div style={{
          width: 220, flexShrink: 0,
          borderRight: '1px solid var(--border)',
          display: 'flex', flexDirection: 'column',
          overflow: 'hidden',
        }}>
          {/* Header — matches JournalNav */}
          <div style={{ padding: '14px 20px 14px 0', borderBottom: '1px solid var(--border)', textAlign: 'right' }}>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.1em', color: 'var(--text-secondary)' }}>JOURNAL</div>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.06em', marginTop: 2 }}>sustena XII · my.bg</div>
          </div>

          {/* Back + position */}
          <div style={{ borderBottom: '1px solid var(--border)' }}>
            <button
              onClick={onBack}
              style={{
                width: '100%', textAlign: 'right',
                display: 'flex', alignItems: 'center', justifyContent: 'flex-end', gap: 6,
                padding: '9px 20px 6px',
                fontFamily: 'var(--mono)', fontSize: 8, letterSpacing: '0.1em', textTransform: 'uppercase',
                color: 'var(--text-muted)',
                background: 'transparent', border: 'none', cursor: 'pointer',
                transition: 'color var(--t-fast)',
              }}
              onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
              onMouseLeave={e => e.currentTarget.style.color = 'var(--text-muted)'}
            >← ALL</button>
            <div style={{ textAlign: 'right', padding: '0 20px 8px', fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.06em' }}>
              {currentIdx + 1} of {entries.length}
            </div>
          </div>

          {/* Prev / Current / Next */}
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
            {prevEntry
              ? <MiniEntryBtn entry={prevEntry} dirLabel="↑ PREVIOUS" />
              : <div style={{ borderBottom: '1px solid var(--border)', padding: '10px 20px', textAlign: 'right', fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)' }}>— FIRST ENTRY —</div>
            }

            {/* Current */}
            <div style={{
              padding: '14px 20px 14px 8px', textAlign: 'right',
              borderBottom: '1px solid var(--border)',
              background: 'var(--bg-raised)',
              flexShrink: 0,
            }}>
              <div style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--amber)', letterSpacing: '0.1em', textTransform: 'uppercase', marginBottom: 3 }}>READING</div>
              <div style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 4 }}>
                {ENTRY_TYPES[activeEntry.type].label} · {activeEntry.date}
              </div>
              <div style={{ fontFamily: 'var(--ui)', fontSize: 10, fontWeight: 500, color: 'var(--text-primary)', lineHeight: 1.4 }}>{activeEntry.title}</div>
            </div>

            {nextEntry
              ? <MiniEntryBtn entry={nextEntry} dirLabel="↓ NEXT" />
              : <div style={{ borderBottom: '1px solid var(--border)', padding: '10px 20px', textAlign: 'right', fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)' }}>— LATEST ENTRY —</div>
            }
          </div>
        </div>
      );
    }

    function JournalNav({ filter, setFilter, onCompose, entryCount }) {
      const filters = [
        { key: 'ALL',        label: 'All',         count: entryCount },
        { key: 'DECISION',   label: 'Decisions',   count: null },
        { key: 'UPDATE',     label: 'Updates',     count: null },
        { key: 'REFLECTION', label: 'Reflections', count: null },
        { key: 'NOTE',       label: 'Notes',       count: null },
      ];

      return (
        <div style={{
          width: 220, flexShrink: 0,
          borderRight: '1px solid var(--border)',
          display: 'flex', flexDirection: 'column',
          overflow: 'hidden',
        }}>
          {/* Title — right-aligned */}
          <div style={{ padding: '14px 0 14px', borderBottom: '1px solid var(--border)', textAlign: 'right', paddingRight: 20 }}>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.1em', color: 'var(--text-secondary)' }}>JOURNAL</div>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.06em', marginTop: 2 }}>sustena XII · my.bg</div>
          </div>

          {/* Filter list — right-aligned */}
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 0, paddingTop: 8 }}>
            {filters.map(f => {
              const isActive = filter === f.key;
              return (
                <button
                  key={f.key}
                  onClick={() => setFilter(f.key)}
                  style={{
                    width: '100%', textAlign: 'right',
                    display: 'flex', alignItems: 'center', justifyContent: 'flex-end', gap: 8,
                    padding: '7px 20px',
                    fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.06em', textTransform: 'uppercase',
                    color: isActive ? 'var(--text-primary)' : 'var(--text-muted)',
                    background: 'transparent',
                    border: 'none', cursor: 'pointer',
                    transition: 'color var(--t-fast)',
                  }}
                  onMouseEnter={e => !isActive && (e.currentTarget.style.color = 'var(--text-secondary)')}
                  onMouseLeave={e => !isActive && (e.currentTarget.style.color = 'var(--text-muted)')}
                >
                  {f.count != null && (
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', opacity: 0.6 }}>{f.count}</span>
                  )}
                  <span>{f.label}</span>
                </button>
              );
            })}
          </div>

          {/* Footer — right-aligned */}
          <div style={{ padding: '10px 20px 10px 0', borderTop: '1px solid var(--border)', textAlign: 'right' }}>
            <button onClick={onCompose} style={{
              fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500, letterSpacing: '0.1em', textTransform: 'uppercase',
              color: 'var(--text-muted)', background: 'none', border: 'none', cursor: 'pointer',
              transition: 'color var(--t-fast)',
            }}
              onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
              onMouseLeave={e => e.currentTarget.style.color = 'var(--text-muted)'}
            >+ COMPOSE</button>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.06em', marginTop: 4 }}>{entryCount} entries</div>
          </div>
        </div>
      );
    }

    function JournalPage({ onClose }) {
      const [filter, setFilter] = useState('ALL');
      const [activeEntry, setActiveEntry] = useState(null);
      const [composeOpen, setComposeOpen] = useState(false);
      const [extraEntries, setExtraEntries] = useState([]);

      const allEntries = [...extraEntries, ...JOURNAL_ENTRIES];

      const filtered = useMemo(() => {
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
          body: [{ type: 'lead', text: draft.body || '(empty)' }],
          sidenotes: [],
        };
        setExtraEntries(prev => [newEntry, ...prev]);
      };

      /* ── Monochrome palette override (matches brand/pages/journal.html) ── */
      const monoVars = {
        '--bg-base':       '#0f0f0f',
        '--bg-surface':    '#0f0f0f',
        '--bg-raised':     '#161614',
        '--bg-overlay':    '#1e1c1a',
        '--border':        '#1e1d1b',
        '--border-mid':    '#2c2a28',
        '--border-light':  '#3e3c3a',
        '--text-primary':  '#e8e4de',
        '--text-secondary':'#a09c96',
        '--text-muted':    '#5c5854',
        '--text-dim':      '#383432',
        '--amber':         '#b4b0aa',
        '--amber-dim':     '#929089',
        '--amber-glow':    'rgba(180,176,170,0.07)',
        '--amber-border':  'rgba(180,176,170,0.22)',
        '--teal':          '#9c9890',
        '--teal-dim':      '#747068',
        '--teal-glow':     'rgba(156,152,144,0.06)',
        '--teal-border':   'rgba(156,152,144,0.2)',
        '--ok':            '#8e8a84',
        '--warn':          '#b4b0aa',
        '--danger':        '#848280',
        '--info':          '#72706c',
      };

      return (
        <div style={{ height: '100%', background: 'var(--bg-base)' }}>

          {/* Body */}
          <div style={{ display: 'flex', height: '100%', overflow: 'hidden' }}>
            {activeEntry ? (
              <EntryNavSidebar
                entries={allEntries}
                activeEntry={activeEntry}
                onSelect={setActiveEntry}
                onBack={() => setActiveEntry(null)}
              />
            ) : (
              <JournalNav
                filter={filter}
                setFilter={setFilter}
                onCompose={() => setComposeOpen(true)}
                entryCount={allEntries.length}
              />
            )}

            <div style={{ flex: 1, minWidth: 0, overflow: 'hidden' }}>
              {activeEntry ? (
                <EntryReader entry={activeEntry} />
              ) : (
                <div style={{ height: '100%', overflow: 'auto', padding: '28px 32px' }}>
                  <div style={{ marginBottom: 20 }}>
                    <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between' }}>
                      <span style={{ fontFamily: 'var(--ui)', fontSize: 18, fontWeight: 500, color: 'var(--text-primary)' }}>
                        {filter === 'ALL' ? 'All entries' : ENTRY_TYPES[filter.toLowerCase()]?.label + 's'}
                      </span>
                      <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{filtered.length} entries</span>
                    </div>
                    <div style={{ display: 'flex', gap: 16, marginTop: 12 }}>
                      {[
                        { val: allEntries.length, label: 'Total entries' },
                        { val: allEntries.filter(e => e.type === 'decision').length, label: 'Decisions recorded' },
                        { val: allEntries.filter(e => e.sidenotes?.length > 0).length, label: 'State annotations' },
                        { val: 3, label: 'Active sustains' },
                      ].map((s, i) => (
                        <div key={i} style={{
                          padding: '10px 16px',
                          borderLeft: '1px solid var(--border)',
                          display: 'flex', flexDirection: 'column', gap: 3,
                          minWidth: 80,
                        }}>
                          <span style={{ fontFamily: 'var(--mono)', fontSize: 20, fontWeight: 500, color: 'var(--text-primary)' }}>{s.val}</span>
                          <span className="meta-10" style={{ color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>{s.label}</span>
                        </div>
                      ))}
                    </div>
                  </div>

                  {filtered.length === 0 ? (
                    <div style={{ textAlign: 'center', padding: '48px 0', color: 'var(--text-muted)', fontFamily: 'var(--mono)', fontSize: 11 }}>
                      No entries in this category yet.{' '}
                      <button onClick={() => setComposeOpen(true)} style={{ color: 'var(--text-secondary)', background: 'none', border: 'none', cursor: 'pointer', fontFamily: 'var(--mono)', fontSize: 11, textDecoration: 'underline' }}>
                        Compose one →
                      </button>
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

          <ComposePanel
            open={composeOpen}
            onClose={() => setComposeOpen(false)}
            onSave={handleSave}
          />
        </div>
      );
    }

    /* ── Mount ───────────────────────────────────────────────── */

    /* ── Mount ───────────────────────────────────────────────── */

export default JournalPage;
