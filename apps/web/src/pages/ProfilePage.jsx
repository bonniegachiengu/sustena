import React, { useState, useEffect, useRef, useMemo, useCallback } from 'react';


/* ═══ profile.jsx ═══ */

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



/* ── Mock profile data ────────────────────────────────────────
   All fields fully populated — this is the filled design state.
──────────────────────────────────────────────────────────── */
const PROFILE_DATA = {
  displayName: 'Bonnie Gachiengu',
  handle: 'my.bg',
  bio: 'Sole Technical Co-founder · Building the mycelium.',
  memberSince: '2026',
  titles: ['Sustena Architect', 'Sole Technical Co-founder'],
  stats: [
    { value: 7,      label: 'Proposals\nPassed',   color: 'var(--text-primary)' },
    { value: 12,     label: 'Operators\nBuilt',     color: 'var(--text-primary)' },
    { value: '4.2K', label: 'Pawa\nEarned',         color: 'var(--text-primary)' },
  ],
  memberships: [
    { name: 'Sustena XII', role: 'Founder',  status: 'live'    },
    { name: 'Vyyb',        role: 'Founder',  status: 'seed'    },
    { name: 'Chama XII',   role: 'Member',   status: 'live'    },
    { name: 'Colosso',     role: 'Observer', status: 'pending' },
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
      { name: 'Sustena XII', pawa: 8420,  score: 0.87, stake: 4.2 },
      { name: 'Vyyb',        pawa: 2100,  score: 0.71, stake: 2.4 },
      { name: 'Chama XII',   pawa: 6300,  score: 0.92, stake: 8.1 },
    ],
  },
  privacy: { balancesPublic: false },
};

const CONTRIBUTION_COLORS = {
  decision:  'var(--text-muted)',
  build:     'var(--text-secondary)',
  proposal:  'var(--text-muted)',
  milestone: 'var(--border-light)',
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

/* ── Gamification achievement badges ─────────────────────── */
const ACHIEVEMENT_BADGES = [
  { label: 'FOUNDER',           icon: '◈' },
  { label: 'SYSTEMS ARCHITECT', icon: '⬡' },
  { label: 'MYCELIUM BUILDER',  icon: '⊕' },
  { label: 'COUNCIL CHAIR',     icon: '◇' },
];

function AchievementBadge({ label, icon }) {
  return (
    <div style={{
      display: 'inline-flex', alignItems: 'center', gap: 6,
      padding: '4px 10px',
      border: '1px solid var(--border-mid)',
      borderRadius: 4,
    }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)', lineHeight: 1 }}>{icon}</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500, letterSpacing: '0.1em', color: 'var(--text-muted)', textTransform: 'uppercase' }}>{label}</span>
    </div>
  );
}

/* ── Hero avatar — tall rectangular, full-height quadrant fill ── */
function ProfileAvatar() {
  return (
    <div style={{
      width: '100%', height: '100%',
      background: 'var(--bg-raised)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      position: 'relative', overflow: 'hidden',
    }}>
      {/* Geometric inset frames */}
      <div style={{ position: 'absolute', inset: 16, border: '0.5px solid var(--border-mid)', pointerEvents: 'none' }} />
      <div style={{ position: 'absolute', inset: 30, border: '0.5px solid var(--border)', pointerEvents: 'none' }} />
      {/* Monogram */}
      <span style={{ fontFamily: 'var(--ui)', fontSize: 72, fontWeight: 600, color: 'var(--text-muted)', letterSpacing: '-0.02em', position: 'relative', zIndex: 1, lineHeight: 1 }}>B</span>
    </div>
  );
}

/* ── Stats strip ─────────────────────────────────────────── */
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

/* ── Membership badge pills ───────────────────────────────── */
function MembershipPills({ memberships }) {
  return (
    <div style={{ display: 'flex', gap: 5, flexWrap: 'wrap' }}>
      {memberships.map(m => (
        <div key={m.name} style={{
          display: 'inline-flex', alignItems: 'center', gap: 5,
          padding: '3px 8px',
          border: '1px solid var(--border)',
          borderRadius: 4,
        }}>
          <span style={{ width: 4, height: 4, borderRadius: '50%', background: 'var(--border-light)' }} />
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', fontWeight: 500 }}>{m.name}</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)' }}>{m.role}</span>
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
              width: 5, height: 5, borderRadius: '50%',
              background: dotColor,
              boxShadow: `0 0 0 2px var(--bg-base)`,
            }} />
            {/* Content */}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
              <span style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)', lineHeight: 1.4 }}>{c.label}</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.06em', textTransform: 'uppercase' }}>
                {c.type} · {c.sustain}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

/* ── Balance cards — simplified, no backgrounds, right-aligned toward divider ── */
function AggregateBalanceCard({ data, hidden }) {
  return (
    <div style={{ borderBottom: '1px solid var(--border)', paddingBottom: 20, marginBottom: 20, textAlign: 'right' }}>
      <div style={{ fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500, letterSpacing: '0.12em', color: 'var(--text-dim)', textTransform: 'uppercase', marginBottom: 10 }}>
        TOTAL PAWA
      </div>
      {hidden ? (
        <span style={{ fontFamily: 'var(--mono)', fontSize: 28, fontWeight: 500, color: 'var(--text-dim)' }}>—</span>
      ) : (
        <>
          <div style={{ display: 'flex', alignItems: 'baseline', gap: 6, marginBottom: 4, justifyContent: 'flex-end' }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 28, fontWeight: 500, color: 'var(--text-primary)', letterSpacing: '-0.02em' }}>
              {data.pawa.toLocaleString()}
            </span>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-dim)' }}>pwa</span>
          </div>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', letterSpacing: '0.04em' }}>{data.trend}</span>
        </>
      )}
    </div>
  );
}

function SustainBalanceCard({ b, hidden }) {
  return (
    <div style={{ paddingBottom: 18, marginBottom: 18, borderBottom: '1px solid var(--border)', textAlign: 'right' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 8 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)' }}>score {hidden ? '—' : b.score.toFixed(2)}</span>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500, color: 'var(--text-muted)', letterSpacing: '0.06em', textTransform: 'uppercase' }}>{b.name}</span>
      </div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 4, marginBottom: 10, justifyContent: 'flex-end' }}>
        {!hidden && <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>pwa</span>}
        <span style={{ fontFamily: 'var(--mono)', fontSize: 18, fontWeight: 500, color: hidden ? 'var(--text-dim)' : 'var(--text-primary)', letterSpacing: '-0.01em' }}>{hidden ? '—' : b.pawa.toLocaleString()}</span>
      </div>
      <div style={{ height: 1, background: 'var(--border)', borderRadius: 1, overflow: 'hidden' }}>
        <div style={{ width: `${hidden ? 0 : b.stake}%`, height: '100%', background: 'var(--border-mid)', borderRadius: 1, transition: 'width 0.8s ease', marginLeft: 'auto' }} />
      </div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 4 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)' }}>{hidden ? '—' : `${b.stake}%`}</span>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)' }}>STAKE</span>
      </div>
    </div>
  );
}

/* ── Main ProfilePage — cross (+) layout ─────────────────── */
function ProfilePage({ onClose }) {
  const [isPublic, setIsPublic] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [data, setData] = useState(PROFILE_DATA);

  const balancesHidden = isPublic;

  /* ── Monochrome palette override (matches brand/pages/Profile.html) ── */
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
    <div style={{ ...monoVars, display: 'grid', gridTemplateRows: '44px 1fr', height: '100vh', background: 'var(--bg-base)', padding: '0 48px' }}>

      {/* ── T-nav header ── */}
      <header style={{ display: 'grid', gridTemplateColumns: '200px 1fr', borderBottom: '1px solid var(--border)' }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'flex-end', gap: 10, paddingRight: 20, borderRight: '1px solid var(--border)' }}>
          <div style={{ textAlign: 'right' }}>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.1em', color: 'var(--text-secondary)' }}>PROFILE</div>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.06em' }}>{data.handle}</div>
          </div>
          <div style={{ width: 16, height: 16, borderRadius: '50%', border: '1px solid var(--border-mid)' }} />
        </div>
        <div style={{ display: 'flex', alignItems: 'stretch', paddingLeft: 20 }}>
          {/* View toggle */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 0, marginRight: 16 }}>
            {['PERSONAL', 'PUBLIC'].map(mode => {
              const active = (isPublic ? 'PUBLIC' : 'PERSONAL') === mode;
              return (
                <button key={mode} onClick={() => setIsPublic(mode === 'PUBLIC')} style={{
                  padding: '3px 10px',
                  fontFamily: 'var(--mono)', fontSize: 8, fontWeight: active ? 600 : 400, letterSpacing: '0.1em',
                  color: active ? 'var(--text-primary)' : 'var(--text-dim)',
                  background: 'none', border: 'none', cursor: 'pointer',
                  borderBottom: active ? '1px solid var(--text-secondary)' : '1px solid transparent',
                  transition: 'all var(--t-fast)',
                }}>{mode}</button>
              );
            })}
          </div>
          <div style={{ flex: 1 }} />
          {!isPublic && (
            <button onClick={() => setSettingsOpen(true)} style={{
              display: 'flex', alignItems: 'center', padding: '0 16px',
              fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)',
              background: 'none', border: 'none', cursor: 'pointer', transition: 'color var(--t-fast)',
            }}
              onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
              onMouseLeave={e => e.currentTarget.style.color = 'var(--text-dim)'}
            >SETTINGS</button>
          )}
          <a href="Sustena Dashboard.html" style={{ display: 'flex', alignItems: 'center', padding: '0 16px', fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', textDecoration: 'none' }}>← DASHBOARD</a>
        </div>
      </header>

      {/* ── Cross (+) body: 2×2 grid ── */}
      <div style={{ display: 'grid', gridTemplateColumns: '200px 1fr', gridTemplateRows: '260px 1fr', overflow: 'hidden' }}>

        {/* TL: Avatar */}
        <div style={{ borderRight: '1px solid var(--border)', borderBottom: '1px solid var(--border)', overflow: 'hidden' }}>
          <ProfileAvatar />
        </div>

        {/* TR: Identity — name, credentials, badges, stats */}
        <div style={{ borderBottom: '1px solid var(--border)', padding: '28px 36px', display: 'flex', flexDirection: 'column', gap: 10, overflow: 'hidden' }}>
          {/* Name + handle */}
          <div style={{ display: 'flex', alignItems: 'baseline', gap: 10 }}>
            <span style={{ fontFamily: 'var(--ui)', fontSize: 26, fontWeight: 600, color: 'var(--text-primary)', letterSpacing: '-0.02em', lineHeight: 1 }}>{data.displayName}</span>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-dim)', fontWeight: 400 }}>{data.handle}</span>
          </div>

          {/* Bio */}
          <span style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-muted)' }}>{data.bio}</span>

          {/* Title pills */}
          <div style={{ display: 'flex', gap: 5, flexWrap: 'wrap' }}>
            {data.titles.map(t => (
              <span key={t} style={{ padding: '2px 8px', borderRadius: 3, fontFamily: 'var(--mono)', fontSize: 8, letterSpacing: '0.06em', color: 'var(--text-muted)', border: '1px solid var(--border)' }}>{t}</span>
            ))}
          </div>

          {/* Achievement badges */}
          <div style={{ display: 'flex', gap: 5, flexWrap: 'wrap' }}>
            {ACHIEVEMENT_BADGES.map(b => <AchievementBadge key={b.label} {...b} />)}
          </div>

          {/* Stats */}
          <div style={{ display: 'flex', gap: 0, marginTop: 6 }}>
            {data.stats.map((s, i) => (
              <div key={i} style={{
                paddingRight: i < data.stats.length - 1 ? 24 : 0,
                marginRight: i < data.stats.length - 1 ? 24 : 0,
                borderRight: i < data.stats.length - 1 ? '1px solid var(--border)' : 'none',
              }}>
                <StatPill value={s.value} label={s.label} />
              </div>
            ))}
          </div>

          {/* Memberships */}
          <MembershipPills memberships={data.memberships} />
        </div>

        {/* BL: Balances (or hidden in public view) */}
        <div style={{ borderRight: '1px solid var(--border)', overflowY: 'auto', padding: '24px 20px 24px 12px' }}>
          {balancesHidden ? (
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%' }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', letterSpacing: '0.08em', textTransform: 'uppercase' }}>PRIVATE</span>
            </div>
          ) : (
            <>
              <div style={{ fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500, letterSpacing: '0.12em', color: 'var(--text-dim)', textTransform: 'uppercase', marginBottom: 20, textAlign: 'right' }}>BALANCES</div>
              <AggregateBalanceCard data={data.balances.aggregate} hidden={false} />
              {data.balances.perSustain.map(b => (
                <SustainBalanceCard key={b.name} b={b} hidden={false} />
              ))}
            </>
          )}
        </div>

        {/* BR: History timeline (scrollable) */}
        <div style={{ overflowY: 'auto', padding: '24px 36px' }}>
          <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 24 }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500, letterSpacing: '0.12em', color: 'var(--text-dim)', textTransform: 'uppercase' }}>HISTORY</span>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)' }}>{data.contributions.length} contributions</span>
          </div>
          <ContributionTimeline contributions={data.contributions} />
        </div>

      </div>

      <SettingsPanel open={settingsOpen} onClose={() => setSettingsOpen(false)} data={data} setData={setData} />
    </div>
  );
}

/* ── Mount ── */

export default ProfilePage;
