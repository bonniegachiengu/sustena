/* Sustena — Profile page component
   Personal view (default) + Public view (toggle)
   Uses core.jsx token system: var(--amber), var(--bg-surface), etc.
*/

const { useState: pUseState, useEffect: pUseEffect, useRef: pUseRef } = React;

/* ─── Sample data — replace with live state bindings ─── */
const PROFILE_DATA = {
  displayName: 'Bonnie Gachiengu',
  handle: 'bonnie',
  avatar: null, // TODO: wire to actual avatar URL
  titles: [
    { label: 'Sole Technical Co-Founder', sustain: 'Sustena XII', since: '2025' },
    { label: 'Operative Lead',             sustain: 'homestead.bonnie', since: '2024' },
    { label: 'Council Member',             sustain: 'vyyb.hive', since: '2025' },
  ],
  memberships: [
    { id: 'homestead.bonnie', label: 'Homestead · Bonnie', status: 'live', role: 'Operator' },
    { id: 'vyyb.hive',        label: 'Vyyb Hive',         status: 'live', role: 'Council' },
    { id: 'mkulima.alpha',    label: 'Mkulima Alpha',      status: 'seed', role: 'Member'  },
  ],
  // TODO: pull contributions from live operative action log
  contributions: [
    { label: 'Operative builds logged',  value: 12,    unit: '' },
    { label: 'Proposals passed',         value: 4,     unit: '' },
    { label: 'Actions completed',        value: 218,   unit: '' },
    { label: 'Pawa earned (all-time)',   value: '41.2k', unit: 'pwa' },
  ],
  // TODO: wire balances to live sustain state
  balances: [
    { sustain: 'homestead.bonnie', label: 'Homestead', pawa: 8420,  score: 87, stake: 34.2, color: 'var(--amber)' },
    { sustain: 'vyyb.hive',        label: 'Vyyb Hive', pawa: 2310,  score: 71, stake: 12.8, color: 'var(--teal)' },
    { sustain: 'mkulima.alpha',    label: 'Mkulima',   pawa: 580,   score: 54, stake: 5.1,  color: 'var(--info)' },
  ],
};

const SUSTAIN_MEMBERSHIP_COLORS = {
  live: 'var(--teal)',
  seed: 'var(--amber)',
  archived: 'var(--text-muted)',
};

/* ─── Settings slide-in panel ─────────────────────────── */
function SettingsPanel({ open, onClose }) {
  const [notifLevel, setNotifLevel] = pUseState('all');
  const [balancePublic, setBalancePublic] = pUseState(false);
  const [memberPublic, setMemberPublic] = pUseState(true);
  const [displayNameEdit, setDisplayNameEdit] = pUseState(PROFILE_DATA.displayName);

  return (
    <>
      {/* Backdrop */}
      {open && (
        <div
          onClick={onClose}
          style={{
            position: 'fixed', inset: 0, zIndex: 200,
            background: 'rgba(0,0,0,0.45)',
            animation: 'fadeUp 0.18s ease-out',
          }}
        />
      )}
      {/* Panel */}
      <div style={{
        position: 'fixed', top: 0, right: 0, bottom: 0,
        width: 'clamp(280px, 30vw, 400px)',
        background: 'var(--bg-surface)',
        borderLeft: '1px solid var(--border-mid)',
        zIndex: 201,
        display: 'flex', flexDirection: 'column',
        transform: open ? 'translateX(0)' : 'translateX(100%)',
        transition: 'transform 0.26s cubic-bezier(0.22,0.61,0.36,1)',
        boxShadow: open ? '-16px 0 40px rgba(0,0,0,0.5)' : 'none',
      }}>
        {/* Header */}
        <header style={{
          padding: '14px 18px',
          borderBottom: '1px solid var(--border)',
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          flexShrink: 0,
        }}>
          <span className="label-11">SETTINGS</span>
          <button onClick={onClose} style={{ color: 'var(--text-muted)', padding: 4, transition: 'color var(--t-fast)' }}
            onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
            onMouseLeave={e => e.currentTarget.style.color = 'var(--text-muted)'}
          >
            <Icon name="x" size={13} />
          </button>
        </header>

        <div style={{ flex: 1, overflowY: 'auto', padding: 18, display: 'flex', flexDirection: 'column', gap: 24 }}>
          {/* Display name */}
          <SettSection label="IDENTITY">
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              <label className="meta-10" style={{ color: 'var(--text-muted)' }}>Display name</label>
              <input
                value={displayNameEdit}
                onChange={e => setDisplayNameEdit(e.target.value)}
                style={{
                  background: 'var(--bg-base)', border: '1px solid var(--border-mid)',
                  borderRadius: 'var(--radius-sm)', padding: '7px 10px',
                  fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)', outline: 'none',
                }}
                onFocus={e => e.target.style.borderColor = 'var(--amber-border)'}
                onBlur={e => e.target.style.borderColor = 'var(--border-mid)'}
              />
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <label className="meta-10" style={{ color: 'var(--text-muted)' }}>Handle</label>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '7px 10px', background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)' }}>
                <span className="meta-10" style={{ color: 'var(--amber)' }}>@</span>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-secondary)' }}>{PROFILE_DATA.handle}</span>
                <span className="meta-10" style={{ marginLeft: 'auto', color: 'var(--text-dim)' }}>locked</span>
              </div>
            </div>
          </SettSection>

          {/* Privacy */}
          <SettSection label="PRIVACY">
            <SettToggle
              label="Show balances on public profile"
              sub="Aggregate only if off"
              value={balancePublic}
              onChange={setBalancePublic}
            />
            <SettToggle
              label="Show memberships publicly"
              sub="Approved-for-publicity sustains only"
              value={memberPublic}
              onChange={setMemberPublic}
            />
          </SettSection>

          {/* Notifications */}
          <SettSection label="NOTIFICATIONS">
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <label className="meta-10" style={{ color: 'var(--text-muted)' }}>Level</label>
              <div style={{ display: 'flex', gap: 4 }}>
                {['all', 'important', 'council'].map(v => (
                  <button key={v} onClick={() => setNotifLevel(v)} style={{
                    flex: 1, padding: '6px 0',
                    fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.06em', textTransform: 'uppercase',
                    color: notifLevel === v ? 'var(--amber)' : 'var(--text-muted)',
                    background: notifLevel === v ? 'var(--amber-glow)' : 'var(--bg-base)',
                    border: `1px solid ${notifLevel === v ? 'var(--amber-border)' : 'var(--border)'}`,
                    borderRadius: 'var(--radius-sm)',
                    transition: 'all var(--t-fast)',
                  }}>{v}</button>
                ))}
              </div>
            </div>
          </SettSection>

          {/* Wallet / Key management */}
          <SettSection label="WALLET · KEYS">
            <div style={{
              padding: '12px 14px',
              background: 'var(--bg-base)', border: '1px solid var(--border)',
              borderRadius: 'var(--radius-sm)',
              display: 'flex', flexDirection: 'column', gap: 6,
            }}>
              <span className="meta-10" style={{ color: 'var(--text-muted)' }}>Wallet address</span>
              {/* TODO: replace with real wallet address from state */}
              <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-secondary)', wordBreak: 'break-all' }}>
                stn1:0xBF3a…d42E
              </span>
              <div style={{ display: 'flex', gap: 6, marginTop: 4 }}>
                <button style={{
                  padding: '5px 10px',
                  fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.06em', textTransform: 'uppercase',
                  color: 'var(--text-secondary)', background: 'transparent',
                  border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
                  transition: 'all var(--t-fast)',
                }}
                onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
                onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border-mid)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
                onClick={() => window.flash?.('Key management — coming in v2', 'info')}
                >
                  MANAGE KEYS
                </button>
              </div>
            </div>
          </SettSection>
        </div>

        {/* Footer */}
        <div style={{ padding: '12px 18px', borderTop: '1px solid var(--border)', display: 'flex', justifyContent: 'flex-end', gap: 8, flexShrink: 0 }}>
          <PBtn variant="ghost" onClick={onClose}>CANCEL</PBtn>
          <PBtn onClick={() => { window.flash?.('Settings saved', 'ok'); onClose(); }}>SAVE</PBtn>
        </div>
      </div>
    </>
  );
}

function SettSection({ label, children }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <span className="label-10">{label}</span>
      {children}
    </div>
  );
}

function SettToggle({ label, sub, value, onChange }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 }}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 2, flex: 1 }}>
        <span style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-primary)' }}>{label}</span>
        {sub && <span className="meta-10" style={{ fontSize: 9, color: 'var(--text-muted)' }}>{sub}</span>}
      </div>
      <button
        onClick={() => onChange(!value)}
        style={{
          width: 36, height: 20, borderRadius: 10, flexShrink: 0,
          background: value ? 'var(--amber)' : 'var(--bg-overlay)',
          border: `1px solid ${value ? 'var(--amber)' : 'var(--border-mid)'}`,
          position: 'relative', transition: 'all var(--t-mid)', cursor: 'pointer',
        }}
      >
        <span style={{
          position: 'absolute', top: 2,
          left: value ? 'calc(100% - 18px)' : 2,
          width: 14, height: 14, borderRadius: 7,
          background: value ? 'var(--bg-base)' : 'var(--text-dim)',
          transition: 'left var(--t-mid)',
        }} />
      </button>
    </div>
  );
}

/* ─── Balance card ─────────────────────────────────────── */
function BalanceCard({ b, showDetails }) {
  const totalPawa = PROFILE_DATA.balances.reduce((sum, x) => sum + x.pawa, 0);
  return (
    <div style={{
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)',
      padding: '14px 16px',
      display: 'flex', flexDirection: 'column', gap: 10,
      position: 'relative', overflow: 'hidden',
    }}>
      {/* Accent bar */}
      <span style={{ position: 'absolute', top: 0, left: 0, right: 0, height: 2, background: b.color, opacity: 0.8 }} />
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <span style={{ fontFamily: 'var(--ui)', fontSize: 13, fontWeight: 500, color: 'var(--text-primary)' }}>{b.label}</span>
        <Badge tone="muted">{b.sustain}</Badge>
      </div>
      {/* Pawa */}
      <div>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>PAWA BALANCE</span>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 5, marginTop: 2 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 26, fontWeight: 500, color: b.color, letterSpacing: '-0.01em' }}>
            {showDetails ? b.pawa.toLocaleString() : '—'}
          </span>
          <span className="meta-10">pwa</span>
        </div>
      </div>
      {showDetails && (
        <>
          <div style={{ display: 'flex', gap: 16 }}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
              <span className="meta-10" style={{ color: 'var(--text-muted)' }}>CONTRIB SCORE</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 16, fontWeight: 500, color: b.score >= 80 ? 'var(--ok)' : b.score >= 60 ? 'var(--amber)' : 'var(--text-secondary)' }}>{b.score}</span>
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
              <span className="meta-10" style={{ color: 'var(--text-muted)' }}>STAKE %</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 16, fontWeight: 500, color: 'var(--text-primary)' }}>{b.stake}%</span>
            </div>
          </div>
          {/* Mini progress bar */}
          <div style={{ height: 3, background: 'var(--bg-base)', borderRadius: 2, overflow: 'hidden' }}>
            <div style={{ width: `${(b.pawa / totalPawa) * 100}%`, height: '100%', background: b.color, borderRadius: 2, transition: 'width 0.6s ease' }} />
          </div>
          <span className="meta-10" style={{ fontSize: 9, color: 'var(--text-muted)' }}>
            {((b.pawa / totalPawa) * 100).toFixed(1)}% of total worth
          </span>
        </>
      )}
    </div>
  );
}

/* ─── Main ProfilePage component ──────────────────────── */
function ProfilePage({ onClose }) {
  const [isPublic, setIsPublic] = pUseState(false);
  const [settingsOpen, setSettingsOpen] = pUseState(false);
  const [balancesPublic] = pUseState(false); // mirrors privacy toggle in settings
  const totalPawa = PROFILE_DATA.balances.reduce((sum, b) => sum + b.pawa, 0);

  return (
    <>
      <div style={{
        position: 'fixed', inset: 0, zIndex: 190,
        background: 'var(--bg-base)',
        overflowY: 'auto',
        animation: 'fadeUp 0.22s ease-out',
      }}>
        {/* Top bar */}
        <div style={{
          position: 'sticky', top: 0, zIndex: 10,
          background: 'var(--bg-surface)',
          borderBottom: '1px solid var(--border)',
          padding: '0 28px',
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          height: 48, flexShrink: 0,
        }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
            {onClose && (
              <button onClick={onClose} style={{ color: 'var(--text-muted)', display: 'flex', alignItems: 'center', gap: 5, transition: 'color var(--t-fast)' }}
                onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
                onMouseLeave={e => e.currentTarget.style.color = 'var(--text-muted)'}
              >
                <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
                  <path d="M7 2L3 5.5L7 9" />
                </svg>
                <span className="meta-10">BACK</span>
              </button>
            )}
            <SustenaLogo size={15} />
          </div>

          {/* Personal ↔ Public toggle */}
          <div style={{
            display: 'flex', alignItems: 'center', gap: 1,
            background: 'var(--bg-base)', border: '1px solid var(--border)',
            borderRadius: 20, padding: 3,
          }}>
            {['Personal', 'Public'].map(v => {
              const active = (v === 'Personal') ? !isPublic : isPublic;
              return (
                <button key={v} onClick={() => setIsPublic(v === 'Public')} style={{
                  padding: '4px 14px', borderRadius: 16,
                  fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.06em', textTransform: 'uppercase',
                  color: active ? 'var(--bg-base)' : 'var(--text-muted)',
                  background: active ? 'var(--amber)' : 'transparent',
                  border: 'none', transition: 'all var(--t-mid)',
                }}>{v}</button>
              );
            })}
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            {isPublic && (
              <button
                onClick={() => { window.flash?.('Link copied: sustena.app/@' + PROFILE_DATA.handle, 'ok'); }}
                style={{
                  padding: '5px 12px',
                  fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.06em', textTransform: 'uppercase',
                  color: 'var(--text-secondary)', background: 'transparent',
                  border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
                  transition: 'all var(--t-fast)',
                }}
                onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
                onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border-mid)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
              >
                SHARE ↗
              </button>
            )}
            {!isPublic && (
              <button onClick={() => setSettingsOpen(true)} style={{
                width: 32, height: 32, borderRadius: 'var(--radius-sm)',
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                color: 'var(--text-muted)', border: '1px solid var(--border)',
                background: 'transparent', transition: 'all var(--t-fast)',
              }}
              onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
              onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border)'; e.currentTarget.style.color = 'var(--text-muted)'; }}
              >
                <Icon name="settings" size={14} />
              </button>
            )}
          </div>
        </div>

        {/* Content */}
        <div style={{ maxWidth: 860, margin: '0 auto', padding: '40px 28px 80px' }}>

          {/* ── HERO HEADER ── */}
          <div className="fade-up" style={{ display: 'flex', alignItems: 'flex-start', gap: 24, marginBottom: 48 }}>
            {/* Avatar */}
            <div style={{
              width: 80, height: 80, borderRadius: '50%', flexShrink: 0,
              background: 'var(--bg-raised)',
              border: '2px solid var(--amber-border)',
              boxShadow: '0 0 24px var(--amber-glow)',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              overflow: 'hidden',
            }}>
              {/* TODO: replace with <img src={PROFILE_DATA.avatar} /> when available */}
              <SustenaMark size={36} />
            </div>

            <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 10 }}>
              <div>
                <h1 style={{
                  fontFamily: 'var(--ui)', fontSize: 28, fontWeight: 500,
                  letterSpacing: '-0.02em', color: 'var(--text-primary)', lineHeight: 1.1,
                }}>
                  {PROFILE_DATA.displayName}
                </h1>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 6 }}>
                  <span style={{ fontFamily: 'var(--mono)', fontSize: 13, color: 'var(--amber)' }}>@{PROFILE_DATA.handle}</span>
                  {isPublic && (
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)' }}>sustena.app/@{PROFILE_DATA.handle}</span>
                  )}
                </div>
              </div>

              {/* Membership pills */}
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                {PROFILE_DATA.memberships.map(m => (
                  <span key={m.id} style={{
                    display: 'inline-flex', alignItems: 'center', gap: 5,
                    padding: '3px 10px', borderRadius: 20,
                    background: 'var(--bg-raised)',
                    border: `1px solid ${SUSTAIN_MEMBERSHIP_COLORS[m.status]}33`,
                    fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.04em',
                    color: SUSTAIN_MEMBERSHIP_COLORS[m.status],
                  }}>
                    <span style={{ width: 5, height: 5, borderRadius: '50%', background: SUSTAIN_MEMBERSHIP_COLORS[m.status], flexShrink: 0 }} />
                    {m.label} · {m.role}
                  </span>
                ))}
              </div>
            </div>
          </div>

          {/* ── PUBLIC VIEW notice ── */}
          {isPublic && (
            <div className="fade-up" style={{
              padding: '10px 14px', marginBottom: 32,
              background: 'var(--amber-glow)', border: '1px solid var(--amber-border)',
              borderRadius: 'var(--radius-md)',
              display: 'flex', alignItems: 'center', gap: 10,
            }}>
              <span style={{ color: 'var(--amber)', fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.08em' }}>PUBLIC VIEW</span>
              <span style={{ width: 1, height: 12, background: 'var(--amber-border)' }} />
              <span style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-secondary)' }}>
                This is how others see your profile at <span style={{ color: 'var(--amber)' }}>sustena.app/@{PROFILE_DATA.handle}</span>
              </span>
            </div>
          )}

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 28 }}>
            {/* Left column */}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 28 }}>

              {/* ── RESUME: TITLES ── */}
              <section className="fade-up">
                <h2 style={{
                  fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500,
                  letterSpacing: '0.10em', textTransform: 'uppercase',
                  color: 'var(--text-muted)', marginBottom: 14,
                }}>TITLES</h2>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                  {PROFILE_DATA.titles.map((t, i) => (
                    <div key={i} style={{
                      padding: '12px 14px',
                      background: 'var(--bg-surface)', border: '1px solid var(--border)',
                      borderRadius: 'var(--radius-md)',
                      display: 'flex', flexDirection: 'column', gap: 4,
                    }}>
                      <span style={{ fontFamily: 'var(--ui)', fontSize: 13, fontWeight: 500, color: 'var(--text-primary)' }}>{t.label}</span>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{t.sustain}</span>
                        <span style={{ width: 3, height: 3, borderRadius: '50%', background: 'var(--text-dim)' }} />
                        <span className="meta-10" style={{ color: 'var(--text-dim)' }}>since {t.since}</span>
                      </div>
                    </div>
                  ))}
                </div>
              </section>

              {/* ── RESUME: CONTRIBUTIONS ── */}
              <section className="fade-up">
                <h2 style={{
                  fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500,
                  letterSpacing: '0.10em', textTransform: 'uppercase',
                  color: 'var(--text-muted)', marginBottom: 14,
                }}>CONTRIBUTIONS</h2>
                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
                  {PROFILE_DATA.contributions.map((c, i) => (
                    <div key={i} style={{
                      padding: '12px 14px',
                      background: 'var(--bg-surface)', border: '1px solid var(--border)',
                      borderRadius: 'var(--radius-md)',
                    }}>
                      <div style={{ display: 'flex', alignItems: 'baseline', gap: 4 }}>
                        <span style={{ fontFamily: 'var(--mono)', fontSize: 22, fontWeight: 500, color: 'var(--text-primary)', letterSpacing: '-0.01em' }}>{c.value}</span>
                        {c.unit && <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{c.unit}</span>}
                      </div>
                      <span style={{ fontFamily: 'var(--ui)', fontSize: 11, color: 'var(--text-secondary)', marginTop: 4, display: 'block', lineHeight: 1.4 }}>{c.label}</span>
                    </div>
                  ))}
                </div>
              </section>

              {/* ── MEMBERSHIPS ── */}
              {(!isPublic || true /* always show — filtered by privacy */) && (
                <section className="fade-up">
                  <h2 style={{
                    fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500,
                    letterSpacing: '0.10em', textTransform: 'uppercase',
                    color: 'var(--text-muted)', marginBottom: 14,
                  }}>MEMBERSHIPS</h2>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                    {PROFILE_DATA.memberships.map(m => (
                      <div key={m.id} style={{
                        padding: '10px 14px',
                        background: 'var(--bg-surface)', border: '1px solid var(--border)',
                        borderRadius: 'var(--radius-md)',
                        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                      }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                          <span style={{
                            width: 8, height: 8, borderRadius: '50%',
                            background: SUSTAIN_MEMBERSHIP_COLORS[m.status],
                            boxShadow: m.status === 'live' ? `0 0 6px ${SUSTAIN_MEMBERSHIP_COLORS[m.status]}` : 'none',
                          }} />
                          <div>
                            <span style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)' }}>{m.label}</span>
                            <span className="meta-10" style={{ display: 'block', color: 'var(--text-muted)', fontSize: 9 }}>{m.sustain}</span>
                          </div>
                        </div>
                        <Badge tone={m.status === 'live' ? 'teal' : 'amber'}>{m.role}</Badge>
                      </div>
                    ))}
                  </div>
                </section>
              )}
            </div>

            {/* Right column — Balances */}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 28 }}>
              <section className="fade-up">
                <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 14 }}>
                  <h2 style={{
                    fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500,
                    letterSpacing: '0.10em', textTransform: 'uppercase',
                    color: 'var(--text-muted)',
                  }}>WORTH ACROSS SUSTAINS</h2>
                  {isPublic && !balancesPublic && (
                    <span className="meta-10" style={{ color: 'var(--text-dim)' }}>aggregate only</span>
                  )}
                </div>

                {/* Aggregate hero */}
                <div style={{
                  padding: '18px 20px', marginBottom: 14,
                  background: 'var(--bg-surface)', border: '1px solid var(--amber-border)',
                  borderRadius: 'var(--radius-md)',
                  position: 'relative', overflow: 'hidden',
                }}>
                  <span style={{ position: 'absolute', top: 0, left: 0, right: 0, height: 2, background: 'var(--amber)' }} />
                  <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>TOTAL PAWA</span>
                  <div style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
                    <span style={{
                      fontFamily: 'var(--mono)', fontSize: 40, fontWeight: 500,
                      letterSpacing: '-0.02em', color: 'var(--amber)',
                    }}>
                      {isPublic && !balancesPublic ? '∗∗,∗∗∗' : totalPawa.toLocaleString()}
                    </span>
                    <span className="meta-10">pwa</span>
                  </div>
                  <span className="meta-10" style={{ color: 'var(--text-muted)', marginTop: 4, display: 'block' }}>
                    across {PROFILE_DATA.balances.length} sustains
                  </span>
                </div>

                {/* Per-sustain cards */}
                <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                  {PROFILE_DATA.balances.map(b => (
                    <BalanceCard key={b.sustain} b={b} showDetails={!isPublic || balancesPublic} />
                  ))}
                </div>

                {/* TODO: update public layout per inspiration link (Bonnie to provide) */}
              </section>
            </div>
          </div>
        </div>
      </div>

      {/* Settings slide-in */}
      <SettingsPanel open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </>
  );
}

Object.assign(window, { ProfilePage, SettingsPanel, BalanceCard });
