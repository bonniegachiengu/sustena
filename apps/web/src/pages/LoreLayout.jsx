import React from 'react';
import { NavLink, Outlet, useNavigate } from 'react-router-dom';

/* ─── Monochrome palette (shared across all Lore pages) ─────── */
const MONO_VARS = {
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

const LORE_TABS = [
  { label: 'BLOG',    to: '/lore'    },
  { label: 'ARENA',   to: '/arena'   },
  { label: 'DOCS',    to: '/docs'    },
  { label: 'JOURNAL', to: '/journal' },
];

export default function LoreLayout() {
  const navigate = useNavigate();

  return (
    <div style={{
      ...MONO_VARS,
      display: 'grid',
      gridTemplateRows: '44px 1fr',
      height: '100vh',
      background: 'var(--bg-base)',
      padding: '0 48px',
    }}>

      {/* ── Top nav ─────────────────────────────────────────────── */}
      <header style={{
        display: 'grid',
        gridTemplateColumns: '240px 1fr',
        borderBottom: '1px solid var(--border)',
        flexShrink: 0,
      }}>

        {/* Left: LORE brand + circle toggle */}
        <div style={{
          display: 'flex', alignItems: 'center',
          justifyContent: 'flex-end', gap: 10,
          paddingRight: 20,
          borderRight: '1px solid var(--border)',
        }}>
          <div style={{ textAlign: 'right' }}>
            <div style={{
              fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500,
              letterSpacing: '0.1em', color: 'var(--text-secondary)',
            }}>LORE</div>
            <div style={{
              fontFamily: 'var(--mono)', fontSize: 8,
              color: 'var(--text-dim)', letterSpacing: '0.06em',
            }}>sustena.network</div>
          </div>
          {/* Circle toggle (decorative, matches spec) */}
          <div style={{
            width: 16, height: 16, borderRadius: '50%',
            border: '1px solid var(--border-mid)',
          }} />
        </div>

        {/* Right: BLOG | ARENA | DOCS | JOURNAL tabs + dashboard link */}
        <div style={{ display: 'flex', alignItems: 'stretch', paddingLeft: 20 }}>
          {LORE_TABS.map(({ label, to }) => (
            <NavLink
              key={label}
              to={to}
              end={to === '/lore'}
              style={({ isActive }) => ({
                display: 'flex', alignItems: 'center',
                padding: '0 14px',
                fontFamily: 'var(--mono)', fontSize: 9,
                fontWeight: isActive ? 600 : 400,
                letterSpacing: '0.1em',
                color: isActive ? 'var(--text-primary)' : 'var(--text-dim)',
                borderBottom: isActive
                  ? '1px solid var(--text-secondary)'
                  : '1px solid transparent',
                textDecoration: 'none',
                transition: 'color 0.15s',
              })}
              onMouseEnter={e => {
                if (!e.currentTarget.style.borderBottomColor.includes('text-secondary'))
                  e.currentTarget.style.color = 'var(--text-muted)';
              }}
              onMouseLeave={e => {
                // NavLink style fn will reset on next render; just reset inline override
                e.currentTarget.style.color = '';
              }}
            >
              {label}
            </NavLink>
          ))}

          <div style={{ flex: 1 }} />

          <button
            onClick={() => navigate('/')}
            style={{
              display: 'flex', alignItems: 'center',
              padding: '0 16px',
              fontFamily: 'var(--mono)', fontSize: 9,
              color: 'var(--text-dim)',
              background: 'none', border: 'none', cursor: 'pointer',
              transition: 'color 0.15s',
            }}
            onMouseEnter={e => e.currentTarget.style.color = 'var(--text-secondary)'}
            onMouseLeave={e => e.currentTarget.style.color = 'var(--text-dim)'}
          >← DASHBOARD</button>
        </div>
      </header>

      {/* ── Page content ────────────────────────────────────────── */}
      <div style={{ overflow: 'hidden', minHeight: 0, height: '100%' }}>
        <Outlet />
      </div>
    </div>
  );
}
