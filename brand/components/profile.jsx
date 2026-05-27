/*
  Profile — person or entity profile page.
  Design language: dark hero + earthy palette (soil brown, warm amber, forest green).
  Inspired by brand/inspirations/Profile/:
  — Mahmudur Rahman: dark hero, bold stats strip (42/712/92 pattern), tile grid
  — product.jpg: warm leather-brown earthy palette for balance cards
  — download(5).jpg: vertical timeline CV with single connecting line
  — black.jpg: full-bleed dark hero with geometric editorial accents
  — firm.jpg: sparse dramatic typography + dominant mark
  Registers: ProfilePage → window
*/

const { useState: pUseState, useMemo: pUseMemo, useRef: pUseRef } = React;

/* ── Mock profile data ────────────────────────────────────────
   All fields fully populated — this is the filled design state.
──────────────────────────────────────────────────────────── */
const PROFILE_DATA = {
  displayName: 'Bonnie',
  handle: '@bonnie',
  bio: 'Sole Technical Co-founder · Building the mycelium.',
  memberSince: '2026',
  titles: ['Sustena Architect', 'Sole Technical Co-founder'],
  stats: [
    { value: 7,      label: 'Proposals\nPassed',   color: 'var(--amber)' },
    { value: 12,     label: 'Operators\nBuilt',     color: 'var(--teal)' },
    { value: '4.2K', label: 'Pawa\nEarned',         color: 'var(--ok)' },
  ],
  memberships: [
    { name: 'Sustena XII', role: 'Founder',  status: 'live',    color: 'var(--teal)'  },
    { name: 'Vyyb',        role: 'Founder',  status: 'seed',    color: 'var(--amber)' },
    { name: 'Chama XII',   role: 'Member',   status: 'live',    color: 'var(--ok)'    },
    { name: 'Colosso',     role: 'Observer', status: 'pending', color: 'var(--info)'  },
  ],
  contributions: [
    { date: '2026·05', label: 'Filed Sustena XII entity documentation', type: 'decision', sustain: 'Sustena XII' },
    { date: '2026·05', label: 'Deployed Navigator operative — quorum tracking', type: 'build', sustain: 'Chama XII' },
    { date: '2026·04', label: 'Passed SUS-0148 · pockets reallocation proposal', type: 'proposal', sustain: 'Sustena XII' },
    { date: '2026·04', label: 'Deployed Mentor operative — burn rate monitoring', type: 'build', sustain: 'Sustena XII' },
    { date: '2026·03', label: 'Deployed Curator operative — pantry & procurement', type: 'build', sustain: 'Vyyb' },
    { date: '2026·03', label: 'Founded Sustena XII · 5 sustains initialised', type: 'milestone', sustain: 'Sustena XII' },
    { date: '2026·01', label: 'Launched Vyyb Phase 1 · delivery dispatch system', type: 'milestone', sustain: 'Vyyb' },
  ],
  balances: {
    aggregate: { pawa: 16820, label: 'Total across sustains', trend: '+340 this cycle' },
    perSustain: [
      { name: 'Sustena XII', color: '#8B5E3C', accent: 'var(--amber)', pawa: 8420,  score: 0.87, stake: 4.2 },
      { name: 'Vyyb',        color: '#2C4A3E', accent: 'var(--teal)',  pawa: 2100,  score: 0.71, stake: 2.4 },
      { name: 'Chama XII',   color: '#2A3A1E', accent: 'var(--ok)',    pawa: 6300,  score: 0.92, stake: 8.1 },
    ],
  },
  privacy: { balancesPublic: false },
};

const CONTRIBUTION_COLORS = {
  decision:  'var(--amber)',
  build:     'var(--teal)',
  proposal:  'var(--info)',
  milestone: 'var(--ok)',
};

/* ── Settings slide-in panel ──────────────────────────────── */
function SettingsPanel({ open, onClose, data, setData }) {
  const tog = (key) => setData(d => ({ ...d, privacy: { ...d.privacy, [key]: !d.privacy[key] } }));

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
          <SettingsSection title="IDENTITY">
            <SettingsField label="Display name">
              <input defaultValue={data.displayName} style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)', background: 'var(--bg-base)', border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)', padding: '6px 10px', width: '100%', outline: 'none' }} />
            </SettingsField>
            <SettingsField label="Handle">
              <span style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-muted)', padding: '6px 0', display: 'block' }}>{data.handle} <span style={{ fontSize: 9, color: 'var(--text-dim)' }}>· locked</span></span>
            </SettingsField>
          </SettingsSection>
          <SettingsSection title="PRIVACY">
            <SettingsToggle
              label="Show balances on public profile"
              sub="Pawa amounts + stake visible to others"
              checked={data.privacy.balancesPublic}
              onChange={() => tog('balancesPublic')}
            />
          </SettingsSection>
          <SettingsSection title="NOTIFICATIONS">
            <span className="meta-10" style={{ color: 'var(--text-muted)' }}>Managed per-sustain in each sustain's settings.</span>
          </SettingsSection>
          <SettingsSection title="WALLET / KEYS">
            <div style={{ background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)', padding: '12px', display: 'flex', flexDirection: 'column', gap: 6 }}>
              <span className="meta-10">PAWA KEY</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-secondary)', letterSpacing: '0.04em' }}>sk-pwa-••••••••••••••3f8a</span>
              <button style={{ alignSelf: 'flex-start', fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--amber)', background: 'none', border: 'none', cursor: 'pointer', padding: 0, marginTop: 4 }}>ROTATE KEY →</button>
            </div>
          </SettingsSection>
        </div>
      </div>
    </>
  );
}

function SettingsSection({ title, children }) {
  return (
    <div style={{ marginBottom: 28 }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 700, letterSpacing: '0.1em', color: 'var(--text-dim)', display: 'block', marginBottom: 12 }}>{title}</span>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>{children}</div>
    </div>
  );
}

function SettingsField({ label, children }) {
  return (
    <div>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>{label}</span>
      {children}
    </div>
  );
}

function SettingsToggle({ label, sub, checked, onChange }) {
  return (
    <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 12 }}>
      <div>
        <span style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-secondary)', display: 'block' }}>{label}</span>
        {sub && <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{sub}</span>}
      </div>
      <button onClick={onChange} style={{
        width: 32, height: 18, borderRadius: 9, flexShrink: 0,
        background: checked ? 'var(--teal)' : 'var(--bg-base)',
        border: '1px solid ' + (checked ? 'var(--teal)' : 'var(--border-mid)'),
        position: 'relative', cursor: 'pointer', transition: 'all var(--t-fast)',
      }}>
        <span style={{
          position: 'absolute', top: 2, left: checked ? 14 : 2, width: 12, height: 12,
          borderRadius: '50%', background: checked ? '#fff' : 'var(--text-dim)',
          transition: 'left var(--t-fast)',
        }} />
      </button>
    </div>
  );
}

/* ── Hero monogram ────────────────────────────────────────── */
function ProfileMonogram({ size = 80 }) {
  return (
    <div style={{
      width: size, height: size, borderRadius: '50%',
      background: 'linear-gradient(135deg, var(--amber) 0%, #8B5E3C 50%, #2C4A3E 100%)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      flexShrink: 0, position: 'relative',
      boxShadow: '0 0 0 3px rgba(232,160,32,0.2), 0 8px 32px rgba(0,0,0,0.5)',
    }}>
      {/* Geometric accent ring — black.jpg / firm.jpg */}
      <svg width={size} height={size} viewBox="0 0 80 80" style={{ position: 'absolute', inset: 0, opacity: 0.22 }}>
        <circle cx="40" cy="40" r="36" fill="none" stroke="white" strokeWidth="0.8" />
        <circle cx="40" cy="40" r="28" fill="none" stroke="white" strokeWidth="0.4" />
      </svg>
      <span style={{ fontFamily: 'var(--ui)', fontSize: Math.round(size * 0.38), fontWeight: 700, color: '#fff', letterSpacing: '-0.02em', position: 'relative', zIndex: 1 }}>B</span>
    </div>
  );
}

/* ── Stats strip — Mahmudur Rahman 42/712/92 pattern ─────── */
function StatPill({ value, label, color }) {
  const lines = label.split('\n');
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 3, minWidth: 80 }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 38, fontWeight: 700, color, letterSpacing: '-0.02em', lineHeight: 1 }}>{value}</span>
      {lines.map((l, i) => (
        <span key={i} style={{ fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500, letterSpacing: '0.08em', color: 'rgba(255,255,255,0.4)', textTransform: 'uppercase', lineHeight: 1.3 }}>{l}</span>
      ))}
    </div>
  );
}

/* ── Membership badge pills ───────────────────────────────── */
function MembershipPills({ memberships }) {
  const statusColor = (s) => s === 'live' ? 'var(--teal)' : s === 'seed' ? 'var(--amber)' : s === 'pending' ? 'var(--text-muted)' : 'var(--ok)';
  return (
    <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
      {memberships.map(m => (
        <div key={m.name} style={{
          display: 'inline-flex', alignItems: 'center', gap: 5,
          padding: '4px 10px',
          background: 'rgba(255,255,255,0.05)',
          border: '1px solid rgba(255,255,255,0.1)',
          borderRadius: 20,
        }}>
          <span style={{ width: 5, height: 5, borderRadius: '50%', background: statusColor(m.status) }} />
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'rgba(255,255,255,0.65)', fontWeight: 500 }}>{m.name}</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'rgba(255,255,255,0.3)' }}>{m.role}</span>
        </div>
      ))}
    </div>
  );
}

/* ── Contribution timeline — download(5).jpg pattern ─────── */
function ContributionTimeline({ contributions }) {
  return (
    <div style={{ position: 'relative', paddingLeft: 72 }}>
      {/* Single vertical connecting line */}
      <div style={{
        position: 'absolute', left: 54, top: 8, bottom: 8,
        width: 1, background: 'var(--border)',
      }} />
      {contributions.map((c, i) => {
        const dotColor = CONTRIBUTION_COLORS[c.type] || 'var(--text-muted)';
        return (
          <div key={i} className="fade-up" style={{
            display: 'flex', alignItems: 'flex-start',
            marginBottom: i < contributions.length - 1 ? 24 : 0,
            position: 'relative',
            animationDelay: `${i * 50}ms`,
          }}>
            {/* Date — left of line */}
            <span style={{
              position: 'absolute', left: -72, top: 3,
              fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500,
              color: 'var(--text-dim)', whiteSpace: 'nowrap', letterSpacing: '0.04em',
            }}>{c.date}</span>
            {/* Dot on line */}
            <span style={{
              position: 'absolute', left: -20, top: 5,
              width: 7, height: 7, borderRadius: '50%',
              background: dotColor,
              boxShadow: `0 0 0 2px var(--bg-base), 0 0 0 3.5px ${dotColor}55`,
            }} />
            {/* Content */}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
              <span style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)', lineHeight: 1.4 }}>{c.label}</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: dotColor, letterSpacing: '0.06em', fontWeight: 600, textTransform: 'uppercase' }}>
                {c.type} · {c.sustain}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

/* ── Balance cards — product.jpg earthy palette ───────────── */
function AggregateBalanceCard({ data, hidden }) {
  return (
    <div style={{
      background: 'linear-gradient(135deg, #1a1208 0%, #2C1F0E 100%)',
      border: '1px solid rgba(232,160,32,0.22)',
      borderRadius: 'var(--radius-md)',
      padding: '18px 20px',
      marginBottom: 12,
      position: 'relative', overflow: 'hidden',
    }}>
      <div style={{ position: 'absolute', top: -24, right: -24, width: 90, height: 90, borderRadius: '50%', background: 'radial-gradient(circle, rgba(232,160,32,0.14) 0%, transparent 70%)' }} />
      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 700, letterSpacing: '0.1em', color: 'rgba(232,160,32,0.55)', display: 'block', marginBottom: 8 }}>
        TOTAL ACROSS SUSTAINS
      </span>
      {hidden ? (
        <span style={{ fontFamily: 'var(--mono)', fontSize: 32, fontWeight: 700, color: 'var(--text-dim)' }}>—</span>
      ) : (
        <>
          <div style={{ display: 'flex', alignItems: 'baseline', gap: 6, marginBottom: 4 }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 36, fontWeight: 700, color: 'var(--amber)', letterSpacing: '-0.02em' }}>
              {data.pawa.toLocaleString()}
            </span>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 13, color: 'rgba(232,160,32,0.55)' }}>pwa</span>
          </div>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--teal)' }}>{data.trend}</span>
        </>
      )}
    </div>
  );
}

function SustainBalanceCard({ b, hidden }) {
  const R = 14, C = 18, strokeW = 2.5;
  const circ = 2 * Math.PI * R;
  const dash = circ * (hidden ? 0 : b.score);

  return (
    <div style={{
      background: 'var(--bg-surface)',
      border: `1px solid ${b.accent}2a`,
      borderTop: `2px solid ${b.accent}`,
      borderRadius: 'var(--radius-md)',
      padding: '14px 16px',
      display: 'flex', flexDirection: 'column', gap: 12,
      position: 'relative', overflow: 'hidden',
    }}>
      <div style={{ position: 'absolute', bottom: -14, right: -14, width: 54, height: 54, borderRadius: '50%', background: `radial-gradient(circle, ${b.accent}14 0%, transparent 70%)` }} />
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 700, color: 'var(--text-secondary)', letterSpacing: '0.06em' }}>{b.name.toUpperCase()}</span>
        {/* Score ring */}
        <svg width={C * 2} height={C * 2} viewBox={`0 0 ${C * 2} ${C * 2}`}>
          <circle cx={C} cy={C} r={R} fill="none" stroke="var(--bg-base)" strokeWidth={strokeW} />
          <circle cx={C} cy={C} r={R} fill="none" stroke={b.accent} strokeWidth={strokeW}
            strokeDasharray={`${dash} ${circ}`} strokeLinecap="round"
            style={{ transform: 'rotate(-90deg)', transformOrigin: `${C}px ${C}px`, transition: 'stroke-dasharray 0.8s ease' }}
            opacity={hidden ? 0.2 : 0.9}
          />
          <text x={C} y={C + 3.5} textAnchor="middle" style={{ fontFamily: 'var(--mono)', fontSize: 8, fill: hidden ? 'var(--text-dim)' : b.accent, fontWeight: 700 }}>
            {hidden ? '—' : b.score.toFixed(2)}
          </text>
        </svg>
      </div>
      {hidden ? (
        <span style={{ fontFamily: 'var(--mono)', fontSize: 22, fontWeight: 700, color: 'var(--text-dim)' }}>—</span>
      ) : (
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 4 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 22, fontWeight: 700, color: 'var(--text-primary)' }}>{b.pawa.toLocaleString()}</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-dim)' }}>pwa</span>
        </div>
      )}
      <div>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
          <span className="meta-10">STAKE</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: hidden ? 'var(--text-dim)' : b.accent, fontWeight: 600 }}>{hidden ? '—' : `${b.stake}%`}</span>
        </div>
        <div style={{ height: 3, background: 'var(--bg-base)', borderRadius: 2, overflow: 'hidden' }}>
          <div style={{ width: `${hidden ? 0 : b.stake}%`, height: '100%', background: b.accent, borderRadius: 2, transition: 'width 0.8s ease', opacity: 0.8 }} />
        </div>
      </div>
    </div>
  );
}

/* ── Main ProfilePage ─────────────────────────────────────── */
function ProfilePage({ onClose }) {
  const [isPublic, setIsPublic] = pUseState(false);
  const [settingsOpen, setSettingsOpen] = pUseState(false);
  const [data, setData] = pUseState(PROFILE_DATA);

  const balancesHidden = isPublic && !data.privacy.balancesPublic;

  return (
    <div style={{ position: 'fixed', inset: 0, zIndex: 600, background: 'var(--bg-base)', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>

      {/* ── Top bar ── */}
      <div style={{
        height: 44, flexShrink: 0,
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        padding: '0 20px',
        background: 'rgba(18,14,9,0.95)',
        borderBottom: '1px solid var(--border)',
        backdropFilter: 'blur(8px)',
        position: 'relative', zIndex: 2,
      }}>
        {/* Left */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 14, minWidth: 140 }}>
          {onClose && (
            <button onClick={onClose} style={{
              display: 'flex', alignItems: 'center', gap: 6,
              fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500,
              color: 'var(--text-muted)', background: 'transparent', border: 'none', cursor: 'pointer', padding: 0,
            }}
              onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
              onMouseLeave={e => e.currentTarget.style.color = 'var(--text-muted)'}
            >
              <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M7 2L3 6L7 10" /></svg>
              BACK
            </button>
          )}
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-dim)', letterSpacing: '0.04em' }}>
            PROFILE <span style={{ color: 'var(--text-muted)' }}>· {data.handle}</span>
          </span>
        </div>

        {/* Center: toggle */}
        <div style={{ display: 'flex', alignItems: 'center', background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 20, padding: 2 }}>
          {['PERSONAL', 'PUBLIC'].map(mode => {
            const active = (isPublic ? 'PUBLIC' : 'PERSONAL') === mode;
            return (
              <button key={mode} onClick={() => setIsPublic(mode === 'PUBLIC')} style={{
                padding: '4px 14px', borderRadius: 16,
                fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 700, letterSpacing: '0.08em',
                background: active ? 'var(--bg-raised)' : 'transparent',
                color: active ? 'var(--text-primary)' : 'var(--text-muted)',
                border: 'none', cursor: 'pointer', transition: 'all var(--t-fast)',
              }}>{mode}</button>
            );
          })}
        </div>

        {/* Right: settings */}
        <div style={{ minWidth: 140, display: 'flex', justifyContent: 'flex-end' }}>
          {!isPublic && (
            <button onClick={() => setSettingsOpen(true)} style={{
              display: 'flex', alignItems: 'center', gap: 6,
              fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 600, letterSpacing: '0.06em',
              color: 'var(--text-muted)', background: 'transparent',
              border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)',
              padding: '5px 10px', cursor: 'pointer', transition: 'all var(--t-fast)',
            }}
              onMouseEnter={e => { e.currentTarget.style.color = 'var(--text-primary)'; e.currentTarget.style.borderColor = 'var(--border-mid)'; }}
              onMouseLeave={e => { e.currentTarget.style.color = 'var(--text-muted)'; e.currentTarget.style.borderColor = 'var(--border)'; }}
            >
              <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round">
                <circle cx="5.5" cy="5.5" r="1.5" />
                <path d="M5.5 1v1M5.5 9v1M1 5.5h1M9 5.5h1M2.4 2.4l.7.7M7.9 7.9l.7.7M7.9 2.4l-.7.7M2.4 7.9l.7-.7" />
              </svg>
              SETTINGS
            </button>
          )}
        </div>
      </div>

      {/* ── Scrollable body ── */}
      <div style={{ flex: 1, overflowY: 'auto', overflowX: 'hidden' }}>

        {/* ── HERO — Mahmudur Rahman + black.jpg + product.jpg palette ── */}
        <div style={{
          background: 'linear-gradient(160deg, #1a1208 0%, #0f0c08 55%, var(--bg-base) 100%)',
          padding: '36px 36px 28px',
          position: 'relative', overflow: 'hidden', minHeight: 260,
        }}>
          {/* Earthy amber glow — product.jpg palette */}
          <div style={{ position: 'absolute', top: -60, left: -40, width: 320, height: 320, borderRadius: '50%', background: 'radial-gradient(circle, rgba(139,94,60,0.18) 0%, transparent 65%)', pointerEvents: 'none' }} />
          <div style={{ position: 'absolute', bottom: -40, right: 80, width: 200, height: 200, borderRadius: '50%', background: 'radial-gradient(circle, rgba(42,184,160,0.08) 0%, transparent 65%)', pointerEvents: 'none' }} />
          {/* Geometric accent — black.jpg / firm.jpg */}
          <svg style={{ position: 'absolute', top: 0, right: 0, opacity: 0.07, pointerEvents: 'none' }} width="220" height="220" viewBox="0 0 220 220" fill="none">
            <rect x="60" y="20" width="140" height="140" stroke="white" strokeWidth="0.6" />
            <rect x="80" y="40" width="100" height="100" stroke="white" strokeWidth="0.4" />
          </svg>

          {/* Public URL badge */}
          {isPublic && (
            <div style={{ display: 'inline-flex', alignItems: 'center', gap: 7, padding: '5px 12px', background: 'rgba(42,184,160,0.07)', border: '1px solid rgba(42,184,160,0.2)', borderRadius: 20, marginBottom: 20 }}>
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="var(--teal)" strokeWidth="1.3" strokeLinecap="round">
                <circle cx="5" cy="5" r="4" /><path d="M1 5h8M5 1s-2 1.5-2 4 2 4 2 4M5 1s2 1.5 2 4-2 4-2 4" />
              </svg>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--teal)' }}>sustena.app/{data.handle}</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)' }}>· public view</span>
            </div>
          )}

          {/* Identity */}
          <div style={{ display: 'flex', alignItems: 'flex-start', gap: 24, marginBottom: 28 }}>
            <ProfileMonogram size={80} />
            <div style={{ display: 'flex', flexDirection: 'column', gap: 7 }}>
              <div style={{ display: 'flex', alignItems: 'baseline', gap: 10 }}>
                <span style={{ fontFamily: 'var(--ui)', fontSize: 30, fontWeight: 700, color: '#fff', letterSpacing: '-0.02em', lineHeight: 1 }}>{data.displayName}</span>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 13, color: 'rgba(255,255,255,0.3)', fontWeight: 400 }}>{data.handle}</span>
              </div>
              <span style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'rgba(255,255,255,0.48)' }}>{data.bio}</span>
              <div style={{ display: 'flex', gap: 5, marginTop: 2 }}>
                {data.titles.map(t => (
                  <span key={t} style={{ padding: '2px 8px', borderRadius: 4, fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 600, letterSpacing: '0.06em', background: 'rgba(255,255,255,0.06)', color: 'rgba(255,255,255,0.4)', border: '1px solid rgba(255,255,255,0.09)' }}>{t}</span>
                ))}
              </div>
            </div>
          </div>

          {/* Stats strip */}
          <div style={{ display: 'flex', gap: 0, borderTop: '1px solid rgba(255,255,255,0.07)', paddingTop: 20, marginBottom: 20 }}>
            {data.stats.map((s, i) => (
              <div key={i} style={{
                paddingRight: i < data.stats.length - 1 ? 32 : 0,
                marginRight: i < data.stats.length - 1 ? 32 : 0,
                borderRight: i < data.stats.length - 1 ? '1px solid rgba(255,255,255,0.07)' : 'none',
              }}>
                <StatPill value={s.value} label={s.label} color={s.color} />
              </div>
            ))}
          </div>

          {/* Membership pills */}
          <MembershipPills memberships={data.memberships} />
        </div>

        {/* ── Body 2-col ── */}
        <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1.15fr) minmax(0, 0.85fr)', gap: 0 }}>

          {/* Left: timeline */}
          <div style={{ padding: '32px 36px', borderRight: '1px solid var(--border)' }}>
            <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 28 }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 700, letterSpacing: '0.1em', color: 'var(--text-secondary)' }}>HISTORY</span>
              <span className="meta-10" style={{ color: 'var(--text-dim)' }}>{data.contributions.length} contributions</span>
            </div>
            <ContributionTimeline contributions={data.contributions} />
          </div>

          {/* Right: balances */}
          <div style={{ padding: '32px 28px' }}>
            <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 16 }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 700, letterSpacing: '0.1em', color: 'var(--text-secondary)' }}>BALANCES</span>
              {balancesHidden && <span className="meta-10" style={{ color: 'var(--text-dim)' }}>hidden · privacy</span>}
            </div>
            <AggregateBalanceCard data={data.balances.aggregate} hidden={balancesHidden} />
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              {data.balances.perSustain.map(b => (
                <SustainBalanceCard key={b.name} b={b} hidden={balancesHidden} />
              ))}
            </div>
          </div>
        </div>
      </div>

      <SettingsPanel open={settingsOpen} onClose={() => setSettingsOpen(false)} data={data} setData={setData} />
    </div>
  );
}

Object.assign(window, { ProfilePage });
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    