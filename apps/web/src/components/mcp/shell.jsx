import React from 'react';
import { useNavigate } from 'react-router-dom';
import { api } from '../../lib/api.js';
/* Mycelium Control Panel — app shell, nav, state, Tweaks integration */

const { useState: dUseState, useEffect: dUseEffect, useMemo: dUseMemo, useRef: dUseRef } = React;

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "sustain": "homestead.bonnie",
  "panel": "monitor",
  "tickRate": 1000,
  "accent": "amber",
  "density": "regular",
  "showOrchie": true,
  "showGrain": true
}/*EDITMODE-END*/;

const PANELS = [
  { id: 'monitor',    label: 'MONITOR',    icon: 'pulse',    sub: 'live observability' },
  { id: 'simulator',  label: 'SIMULATOR',  icon: 'crosshair', sub: 'forked execution' },
  { id: 'editor',     label: 'EDITOR',     icon: 'agent',    sub: 'spec & graph' },
  { id: 'controller', label: 'CONTROLLER', icon: 'vault',    sub: 'approved execution' },
  { id: 'library',    label: 'LIBRARY',    icon: 'leaf',     sub: 'mycelium network' },
  { id: 'seed',       label: 'SEED',       icon: 'plus',     sub: 'inject real data' },
];

const PAGE_LINKS = [
  { id: 'lore',    label: 'LORE',    icon: 'ledger',  sub: 'operational journal', href: '/lore' },
  { id: 'profile', label: 'PROFILE', icon: 'council', sub: 'identity & history',  href: '/profile' },
];

function App() {
  const navigate = useNavigate();
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const [panel, setPanel] = dUseState(t.panel || 'monitor');
  const [sustainId, setSustainId] = dUseState(t.sustain || 'homestead.bonnie');
  const [tick, setTick] = dUseState(0);
  const [clock, setClock] = dUseState(formatClock(new Date()));
  const [modal, setModal] = dUseState(null);   // {kind:'proposal'|'library'|'confirm'|'pin', data}
  const [toasts, setToasts] = dUseState([]);   // [{id, text, tone}]
  const [orchieOpen, setOrchieOpen] = dUseState(t.showOrchie ?? false);
  const [navCollapsed, setNavCollapsed] = dUseState(false);
  const [liveState, setLiveState] = dUseState(null);  // pushed from WS
  const [apiSustains, setApiSustains] = dUseState([]);
  const [userPawa, setUserPawa] = dUseState(null);   // real pawa balance from /me
  const wsRef = dUseRef(null);
  const lastEventCountRef = dUseRef(null);  // heartbeat: last seen event count
  const onlineRef = dUseRef(true);          // heartbeat: backend reachability

  // Per-panel side-rail visibility (Sim & Editor)
  const [simLeft, setSimLeft] = dUseState(true);
  const [simRight, setSimRight] = dUseState(true);
  const [editLeft, setEditLeft] = dUseState(true);
  const [editRight, setEditRight] = dUseState(true);

  // Wire up globals for any component to flash a toast or confirm an action
  dUseEffect(() => {
    window.flash = (text, tone = 'ok') => {
      const id = Math.random().toString(36).slice(2);
      setToasts(ts => [...ts, { id, text, tone }]);
      setTimeout(() => setToasts(ts => ts.filter(t => t.id !== id)), 3600);
    };
    window.confirmAction = (opts) => setModal({ kind: 'confirm', data: opts });
    window.openPinPad = (opts) => setModal({ kind: 'pin', data: opts });
  }, []);

  dUseEffect(() => setPanel(t.panel), [t.panel]);
  dUseEffect(() => setSustainId(t.sustain), [t.sustain]);

  // Fetch sustain list once on mount
  dUseEffect(() => {
    api.get('/devui/sustains')
      .then(d => { const list = d?.data?.sustains || []; if (list.length) setApiSustains(list); })
      .catch(() => {});
  }, []);

  // Fetch the signed-in user's real pawa balance for the sidebar gauge.
  // Falls back to null (—) when signed out.
  dUseEffect(() => {
    const token = localStorage.getItem('sustena_token');
    if (!token) { setUserPawa(null); return; }
    fetch(`${import.meta.env.VITE_API_BASE ?? ''}/api/v1/users/me`, { headers: { Authorization: `Bearer ${token}` } })
      .then(r => (r.ok ? r.json() : null))
      .then(d => setUserPawa(d?.data?.pawa_balance ?? null))
      .catch(() => {});
  }, []);

  // Refetch the sustain list; optionally select one by id.
  async function refreshSustains(selectId) {
    try {
      const d = await api.get('/devui/sustains');
      const list = d?.data?.sustains || [];
      setApiSustains(list);
      if (selectId) { setSustainId(selectId); setTweak('sustain', selectId); }
      return list;
    } catch (e) {
      return [];
    }
  }

  // Birth a fully-hydrated sustain from a template via the engine, then select it.
  async function createSustain(templateId) {
    try {
      const d = await api.post('/devui/sustains', { template_id: templateId, user_id: 'bonventure' });
      const sid = d?.data?.sustain_id;
      await refreshSustains(sid);
      window.flash?.(`${templateId} sustain created`, 'ok');
      return sid;
    } catch (e) {
      window.flash?.(`create failed · ${String(e?.message || e)}`, 'err');
      return null;
    }
  }

  // WebSocket — open on mount and whenever sustainId changes, close on unmount/change
  dUseEffect(() => {
    // Close any existing socket
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
    const socket = api.ws(
      sustainId,
      (data) => setLiveState(data),
      () => {
        // Reconnect after 3s if closed unexpectedly
        setTimeout(() => {
          if (wsRef.current === socket) wsRef.current = null;
        }, 3000);
      }
    );
    wsRef.current = socket;
    return () => {
      socket.close();
      wsRef.current = null;
    };
  }, [sustainId]);

  // Tick loop
  dUseEffect(() => {
    const rate = t.tickRate || 1000;
    const id = setInterval(() => {
      setTick(x => x + 1);
      setClock(formatClock(new Date()));
    }, rate);
    return () => clearInterval(id);
  }, [t.tickRate]);

  // Heartbeat — every 1s, poll the backend for this sustain. Honest, real-data
  // notifications: flash on new events and on connection drop/restore. No mock pulses.
  dUseEffect(() => {
    if (!sustainId) return;
    lastEventCountRef.current = null;  // reset baseline when the sustain changes
    let cancelled = false;
    const beat = async () => {
      try {
        const d = await api.get(`/devui/monitor-widgets?sustain_id=${encodeURIComponent(sustainId)}`);
        if (cancelled) return;
        if (!onlineRef.current) { onlineRef.current = true; window.flash?.('backend reconnected', 'ok'); }
        const total = d?.data?.widgets?.event_feed?.data?.total ?? 0;
        const prev = lastEventCountRef.current;
        if (prev != null && total > prev) {
          const n = total - prev;
          window.flash?.(`${n} new event${n > 1 ? 's' : ''} · ${sustain?.label ?? 'sustain'}`, 'info');
        }
        lastEventCountRef.current = total;
      } catch {
        if (!cancelled && onlineRef.current) { onlineRef.current = false; window.flash?.('backend unreachable', 'danger'); }
      }
    };
    beat();
    const id = setInterval(beat, 1000);
    return () => { cancelled = true; clearInterval(id); };
  }, [sustainId]);

  const sustains   = apiSustains.length ? apiSustains : SUSTAINS;
  const sustain    = sustains.find(s => s.id === sustainId) || sustains[0] || { id: '', label: '—', sub: 'no sustains', status: 'seed' };
  const sysStats   = liveState?.state?.system || {};
  const pawaBalance = sysStats.pawa_balance ?? userPawa ?? null;

  const switchPanel = (id) => {
    setPanel(id);
    setTweak('panel', id);
  };

  // Esc closes modal
  dUseEffect(() => {
    const onKey = (e) => { if (e.key === 'Escape') setModal(null); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const navWidth = navCollapsed ? 56 : 200;

  return (
    <div data-density={t.density} style={{
      display: 'grid',
      gridTemplateAreas: `
        "header header"
        "rail   main"
        "footer footer"
      `,
      gridTemplateRows: '44px 1fr 30px',
      gridTemplateColumns: `${navWidth}px 1fr`,
      height: '100vh', width: '100vw',
      background: 'var(--bg-base)',
    }}>
      <TopBar
        clock={clock} sustain={sustain}
        sustains={sustains} onSustainChange={(id) => { setSustainId(id); setTweak('sustain', id); }}
        onCreate={createSustain}
      />
      <LeftNav
        panels={PANELS} active={panel} onSelect={switchPanel}
        collapsed={navCollapsed} onToggle={() => setNavCollapsed(c => !c)}
        pageLinks={PAGE_LINKS}
        pawaBalance={pawaBalance}
      />

      <main style={{ gridArea: 'main', overflow: 'hidden', minHeight: 0, position: 'relative' }}>
        {panel === 'monitor'    && <MonitorPanel tick={tick} sustain={sustain} sustains={sustains} />}
        {panel === 'simulator'  && <SimulatorPanel tick={tick} sustain={sustain}
          leftOpen={simLeft} rightOpen={simRight}
          onToggleLeft={() => setSimLeft(v => !v)}
          onToggleRight={() => setSimRight(v => !v)} />}
        {panel === 'editor'     && <EditorPanel sustain={sustain} />}
        {panel === 'controller' && <ControllerPanel tick={tick} sustain={sustain} openModal={(p) => setModal({ kind: 'proposal', data: p })} />}
        {panel === 'library'    && <LibraryPanel openModal={(it, kind) => setModal({ kind: 'library', data: { item: it, kind } })} />}
        {panel === 'seed'       && <SeedPanel sustain={sustain} />}
      </main>

      {/* Floating Orchie button + drawer */}
      <OrchieFAB
        open={orchieOpen}
        onToggle={() => setOrchieOpen(o => !o)}
        sustainId={sustainId}
      />

      <Footer tick={tick} stats={sysStats} />

      {modal?.kind === 'proposal' && <ProposalModal p={modal.data} onClose={() => setModal(null)} />}
      {modal?.kind === 'library' && <LibraryModal data={modal.data} onClose={() => setModal(null)} />}
      {modal?.kind === 'confirm' && <ConfirmModal opts={modal.data} onClose={() => setModal(null)} />}
      {modal?.kind === 'pin' && <PinPad opts={modal.data} onClose={() => setModal(null)} />}

      {/* Toast stack */}
      <div className="toast-stack" style={{ bottom: orchieOpen ? 'calc(100vh - 80px)' : 80 }}>
        {toasts.map(t => (
          <div key={t.id} className="toast" style={{
            borderLeft: `2px solid ${t.tone === 'ok' ? 'var(--ok)' : t.tone === 'danger' ? 'var(--danger)' : t.tone === 'amber' ? 'var(--amber)' : 'var(--info)'}`,
            pointerEvents: 'auto',
          }}>
            <span style={{
              width: 6, height: 6, borderRadius: '50%',
              background: t.tone === 'ok' ? 'var(--ok)' : t.tone === 'danger' ? 'var(--danger)' : t.tone === 'amber' ? 'var(--amber)' : 'var(--info)',
              flexShrink: 0,
            }} />
            <span style={{ flex: 1, color: 'var(--text-primary)' }}>{t.text}</span>
          </div>
        ))}
      </div>

      <TweaksPanel>
        <TweakSection label="Workspace" />
        <TweakSelect label="Sustain" value={t.sustain}
          options={SUSTAINS.map(s => ({ value: s.id, label: `${s.label} · ${s.sub}` }))}
          onChange={(v) => setTweak('sustain', v)} />
        <TweakRadio label="Panel" value={t.panel}
          options={['monitor', 'simulator', 'editor', 'controller', 'library']}
          onChange={(v) => setTweak('panel', v)} />
        <TweakRadio label="Density" value={t.density}
          options={['compact', 'regular', 'comfy']}
          onChange={(v) => setTweak('density', v)} />
        <TweakSection label="Telemetry" />
        <TweakSlider label="Tick rate" value={t.tickRate} min={250} max={3000} step={250} unit="ms"
          onChange={(v) => setTweak('tickRate', v)} />
        <TweakSection label="Orchie" />
        <TweakButton label="Open Orchie panel" onClick={() => window.location.href = `/orchie-panel?sustain=${encodeURIComponent(t.sustain || 'homestead.bonnie')}`} />
        <TweakSection label="Atmosphere" />
        <TweakToggle label="Background grain" value={t.showGrain}
          onChange={(v) => setTweak('showGrain', v)} />
      </TweaksPanel>
    </div>
  );
}

/* ─── Top bar ──────────────────────────────────────────────── */
function TopBar({ clock, sustain, sustains, onSustainChange, onCreate }) {
  const [open, setOpen] = dUseState(false);
  const [templates, setTemplates] = dUseState(null);  // null = not yet loaded
  const [busy, setBusy] = dUseState(false);

  // Load creatable templates the first time the dropdown opens.
  dUseEffect(() => {
    if (open && templates === null) {
      api.get('/devui/templates')
        .then(d => setTemplates(d?.data?.templates || []))
        .catch(() => setTemplates([]));
    }
  }, [open]);
  return (
    <header style={{
      gridArea: 'header',
      borderBottom: '1px solid var(--border)',
      background: 'var(--bg-surface)',
      display: 'flex', alignItems: 'stretch',
      paddingLeft: 16, paddingRight: 16,
      flexShrink: 0,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', paddingRight: 20, borderRight: '1px solid var(--border)' }}>
        <SustenaLogo size={16} />
      </div>

      {/* Sustain selector */}
      <div style={{ position: 'relative', display: 'flex', alignItems: 'stretch' }}>
        <button onClick={() => setOpen(!open)} style={{
          padding: '0 16px',
          display: 'flex', alignItems: 'center', gap: 8,
          color: 'var(--text-secondary)',
          borderRight: '1px solid var(--border)',
          transition: 'color var(--t-fast)',
        }}
        onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
        onMouseLeave={e => e.currentTarget.style.color = 'var(--text-secondary)'}
        >
          <span className="pulse" style={{ width: 5, height: 5, borderRadius: '50%', background: sustain.status === 'live' ? 'var(--teal)' : 'var(--text-muted)' }} />
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, letterSpacing: '0.06em', color: 'var(--text-primary)' }}>
            {sustain.label}
          </span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)' }}>{sustain.sub}</span>
          <Icon name="chevron" size={10} />
        </button>
        {open && (
          <>
            <div style={{ position: 'fixed', inset: 0, zIndex: 50 }} onClick={() => setOpen(false)} />
            <div style={{
              position: 'absolute', top: '100%', left: 0, marginTop: 4,
              background: 'var(--bg-raised)', border: '1px solid var(--border-mid)',
              borderRadius: 'var(--radius-md)', padding: 4, zIndex: 51,
              minWidth: 320,
              boxShadow: '0 8px 24px rgba(0,0,0,0.4)',
            }}>
              {sustains.map(s => (
                <button key={s.id} onClick={() => { onSustainChange(s.id); setOpen(false); }} style={{
                  width: '100%', textAlign: 'left',
                  padding: '8px 10px',
                  display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                  background: s.id === sustain.id ? 'var(--amber-glow)' : 'transparent',
                  borderRadius: 'var(--radius-sm)',
                  transition: 'background var(--t-fast)',
                }}
                onMouseEnter={e => s.id !== sustain.id && (e.currentTarget.style.background = 'var(--bg-overlay)')}
                onMouseLeave={e => s.id !== sustain.id && (e.currentTarget.style.background = 'transparent')}
                >
                  <div style={{ display: 'flex', flexDirection: 'column' }}>
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: s.id === sustain.id ? 'var(--amber)' : 'var(--text-primary)' }}>{s.label}</span>
                    <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{s.sub}</span>
                  </div>
                  <span style={{ width: 5, height: 5, borderRadius: '50%', background: s.status === 'live' ? 'var(--teal)' : 'var(--text-muted)' }} />
                </button>
              ))}

              {sustains.length === 0 && (
                <div style={{ padding: '8px 10px', fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)' }}>
                  no sustains yet · create one below
                </div>
              )}

              {/* Create section */}
              <div style={{ height: 1, background: 'var(--border)', margin: '4px 0' }} />
              <div style={{ padding: '6px 10px 2px', fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.12em', color: 'var(--text-muted)' }}>
                CREATE SUSTAIN
              </div>
              {templates === null && (
                <div style={{ padding: '6px 10px', fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)' }}>loading templates…</div>
              )}
              {Array.isArray(templates) && templates.map(tpl => (
                <button key={tpl.template_id} disabled={busy}
                  onClick={async () => { setBusy(true); await onCreate?.(tpl.template_id); setBusy(false); setOpen(false); }}
                  style={{
                    width: '100%', textAlign: 'left', padding: '8px 10px',
                    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                    borderRadius: 'var(--radius-sm)', background: 'transparent',
                    opacity: busy ? 0.5 : 1, cursor: busy ? 'default' : 'pointer',
                  }}
                  onMouseEnter={e => !busy && (e.currentTarget.style.background = 'var(--bg-overlay)')}
                  onMouseLeave={e => (e.currentTarget.style.background = 'transparent')}
                >
                  <div style={{ display: 'flex', flexDirection: 'column' }}>
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)' }}>{tpl.display_name}</span>
                    <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{(tpl.operatives || []).length} operatives</span>
                  </div>
                  <span style={{ fontFamily: 'var(--mono)', fontSize: 14, color: 'var(--amber)' }}>+</span>
                </button>
              ))}
            </div>
          </>
        )}
      </div>

      <div style={{ flex: 1 }} />

      {/* Right cluster */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '3px 8px', border: '1px solid var(--amber-border)', background: 'var(--amber-glow)', borderRadius: 'var(--radius-sm)' }}>
          <span className="pulse" style={{ width: 5, height: 5, borderRadius: '50%', background: 'var(--amber)' }} />
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.1em', color: 'var(--amber)' }}>PROD · LIVE</span>
        </div>
        <div className="meta-11" style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <span className="label-10">OP</span>
          <span style={{ color: 'var(--text-primary)' }}>B.GACHIENGU</span>
        </div>
        <span className="val-12" style={{ color: 'var(--text-secondary)' }}>{clock} <span style={{ color: 'var(--text-muted)' }}>EAT</span></span>
      </div>
    </header>
  );
}

/* ─── Left nav ────────────────────────────────────────────── */
function LeftNav({ panels, active, onSelect, collapsed, onToggle, pageLinks, pawaBalance }) {
  const navigate = useNavigate();
  return (
    <aside style={{
      gridArea: 'rail',
      borderRight: '1px solid var(--border)',
      background: 'var(--bg-surface)',
      display: 'flex', flexDirection: 'column',
      padding: '10px 0',
      transition: 'width var(--t-mid)',
      overflow: 'hidden',
    }}>
      {/* Header with collapse toggle */}
      <div style={{
        padding: collapsed ? '4px 12px 10px' : '4px 14px 10px',
        marginBottom: 4,
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        gap: 8,
      }}>
        {!collapsed && (
          <div style={{ display: 'flex', flexDirection: 'column' }}>
            <span className="label-10" style={{ color: 'var(--text-muted)' }}>MYCELIUM</span>
            <span className="val-12" style={{ color: 'var(--text-primary)', marginTop: 2, fontSize: 12 }}>Control Panel</span>
          </div>
        )}
        <button onClick={onToggle} title={collapsed ? 'Expand' : 'Collapse'} style={{
          width: 26, height: 26,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          color: 'var(--text-muted)',
          background: 'transparent',
          border: '1px solid var(--border)',
          borderRadius: 'var(--radius-sm)',
          transition: 'all var(--t-fast)',
        }}
        onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
        onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border)'; e.currentTarget.style.color = 'var(--text-muted)'; }}
        >
          <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round">
            {collapsed
              ? <path d="M3 2 L7 5.5 L3 9" />
              : <path d="M8 2 L4 5.5 L8 9" />
            }
          </svg>
        </button>
      </div>

      <nav style={{ display: 'flex', flexDirection: 'column', gap: 2, padding: collapsed ? '0 6px' : 0 }}>
        {panels.map(p => collapsed ? (
          <button key={p.id} className={`nav-link ${active === p.id ? 'active' : ''}`}
            title={`${p.label} · ${p.sub}`}
            onClick={() => onSelect(p.id)}
            style={{
              padding: '9px 0', justifyContent: 'center',
              borderRadius: 'var(--radius-sm)',
            }}>
            <Icon name={p.icon} size={15} />
          </button>
        ) : (
          <button key={p.id} className={`nav-link ${active === p.id ? 'active' : ''}`} onClick={() => onSelect(p.id)}>
            <Icon name={p.icon} size={14} />
            <span>{p.label}</span>
          </button>
        ))}
        {(pageLinks || []).map(p => collapsed ? (
          <button key={p.id} className="nav-link"
            title={`${p.label} · ${p.sub}`}
            onClick={() => navigate(p.href)}
            style={{ padding: '9px 0', justifyContent: 'center', borderRadius: 'var(--radius-sm)' }}>
            <Icon name={p.icon} size={15} />
          </button>
        ) : (
          <button key={p.id} className="nav-link" onClick={() => navigate(p.href)}>
            <Icon name={p.icon} size={14} />
            <span>{p.label}</span>
          </button>
        ))}
      </nav>

      <div style={{ flex: 1 }} />

      {/* Self-stat block */}
      {!collapsed ? (
        <div style={{ padding: '12px 16px', borderTop: '1px solid var(--border)', display: 'flex', flexDirection: 'column', gap: 6 }}>
          <span className="label-10">PAWA</span>
          <div style={{ display: 'flex', alignItems: 'baseline', gap: 4 }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 18, fontWeight: 500, color: 'var(--amber)' }}>
              {pawaBalance != null ? pawaBalance.toLocaleString() : '—'}
            </span>
            <span className="meta-10">balance</span>
          </div>
          <div style={{ height: 2, background: 'var(--bg-base)', borderRadius: 1, marginTop: 4 }}>
            <div style={{ width: pawaBalance != null ? `${Math.min(100, (pawaBalance / 10000) * 100)}%` : '0%', height: '100%', background: 'var(--amber)', transition: 'width 0.6s ease' }} />
          </div>
          <span className="meta-10" style={{ fontSize: 9, color: 'var(--text-muted)' }}>
            {pawaBalance != null ? `${Math.round((pawaBalance / 10000) * 100)}% capacity` : '—'}
          </span>
        </div>
      ) : (
        <div style={{ padding: '10px 0', borderTop: '1px solid var(--border)', display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4 }}>
          <span className="meta-10" style={{ fontSize: 9, color: 'var(--text-muted)' }}>PWA</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 12, fontWeight: 500, color: 'var(--amber)' }}>
            {pawaBalance != null ? pawaBalance.toLocaleString() : '—'}
          </span>
        </div>
      )}
    </aside>
  );
}

/* ─── Orchie FAB + Drawer ─────────────────────────────────── */
function OrchieFAB({ open, onToggle, sustainId = 'homestead.bonnie' }) {
  const navigate = useNavigate();
  const { messages, thinking, onMessageDone, send, restart } = useStreamedConversation(ORCHIE_DEMO, true, 1400, sustainId);
  const [input, setInput] = dUseState('');
  const [unread, setUnread] = dUseState(0);
  const scrollerRef = dUseRef(null);

  dUseEffect(() => {
    if (open) {
      scrollerRef.current?.scrollTo({ top: 999999, behavior: 'smooth' });
      setUnread(0);
    } else {
      setUnread(u => u + 1);
    }
  }, [messages.length]);

  dUseEffect(() => {
    if (open) scrollerRef.current?.scrollTo({ top: 999999, behavior: 'smooth' });
  }, [open, thinking]);

  const submit = () => {
    if (!input.trim()) return;
    send(input.trim());
    setInput('');
  };

  return (
    <>
      {/* Drawer */}
      {open && (
        <div style={{
          position: 'fixed',
          right: 16, bottom: 84,
          width: 360, height: 'min(620px, calc(100vh - 160px))',
          zIndex: 950,
          background: 'var(--bg-surface)',
          border: '1px solid var(--border-light)',
          borderRadius: 'var(--radius-md)',
          display: 'flex', flexDirection: 'column',
          boxShadow: '0 24px 48px rgba(0,0,0,0.45)',
          animation: 'modalSlideIn 0.28s cubic-bezier(0.22, 0.61, 0.36, 1)',
          overflow: 'hidden',
        }}>
          {/* Header */}
          <header style={{
            padding: '10px 14px',
            borderBottom: '1px solid var(--border)',
            display: 'flex', alignItems: 'center', justifyContent: 'space-between',
            flexShrink: 0,
          }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <div style={{
                width: 28, height: 28, borderRadius: '50%',
                background: 'var(--bg-base)', border: '1px solid var(--amber-border)',
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                boxShadow: '0 0 10px var(--amber-glow)',
              }}>
                <SustenaMark size={16} />
              </div>
              <div style={{ display: 'flex', flexDirection: 'column' }}>
                <span style={{ fontFamily: 'var(--ui)', fontSize: 13, fontWeight: 500 }}>Orchie</span>
                <span style={{ display: 'flex', alignItems: 'center', gap: 5, fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--teal)' }}>
                  <span className="pulse" style={{ width: 4, height: 4, borderRadius: '50%', background: 'var(--teal)' }} />
                  ONLINE
                </span>
              </div>
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
              <FabBtn onClick={restart} title="Restart"><Icon name="replay" size={11} /></FabBtn>
              <FabBtn onClick={() => navigate(`/orchie-panel?sustain=${encodeURIComponent(sustainId)}`)} title="Open Orchie panel"><Icon name="expand" size={11} /></FabBtn>
              <FabBtn onClick={onToggle} title="Close"><Icon name="x" size={11} /></FabBtn>
            </div>
          </header>

          {/* Messages */}
          <div ref={scrollerRef} className="no-scrollbar" style={{
            flex: 1, minHeight: 0, overflowY: 'auto', overflowX: 'hidden',
            padding: 14, display: 'flex', flexDirection: 'column', gap: 14,
          }}>
            {messages.map((m, i) => (
              <OrchieBubble
                key={i} m={m} size="sm"
                onDone={i === messages.length - 1 ? onMessageDone : undefined}
              />
            ))}
            {thinking && <OrchieThinking size="sm" />}
          </div>

          {/* Suggestions */}
          <div className="no-scrollbar" style={{
            padding: '6px 12px 8px', display: 'flex', gap: 5, overflow: 'auto', flexShrink: 0,
          }}>
            {['Show pockets', 'Why is burn rising?', 'Reallocate 400k', 'Pantry'].map(s => (
              <button key={s} onClick={() => send(s)} style={{
                padding: '5px 10px',
                fontFamily: 'var(--mono)', fontSize: 10,
                color: 'var(--text-secondary)',
                background: 'var(--bg-base)', border: '1px solid var(--border)',
                borderRadius: 12, whiteSpace: 'nowrap', flexShrink: 0,
                transition: 'all var(--t-fast)',
              }}
              onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
              onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
              >{s}</button>
            ))}
          </div>

          {/* Input */}
          <div style={{ padding: 10, borderTop: '1px solid var(--border)', display: 'flex', gap: 6, flexShrink: 0 }}>
            <input
              value={input} onChange={e => setInput(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter') submit(); }}
              placeholder="Ask Orchie..."
              autoFocus
              style={{
                flex: 1, background: 'var(--bg-base)',
                border: '1px solid var(--border-mid)',
                borderRadius: 'var(--radius-sm)', padding: '7px 10px',
                fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-primary)', outline: 'none',
              }}
              onFocus={e => e.target.style.borderColor = 'var(--amber-border)'}
              onBlur={e => e.target.style.borderColor = 'var(--border-mid)'}
            />
            <button onClick={submit} style={{
              width: 30, height: 30,
              background: input.trim() ? 'var(--amber)' : 'var(--bg-raised)',
              color: input.trim() ? 'var(--bg-base)' : 'var(--text-muted)',
              borderRadius: 'var(--radius-sm)',
              border: '1px solid ' + (input.trim() ? 'var(--amber)' : 'var(--border-mid)'),
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              transition: 'all var(--t-fast)',
            }}><Icon name="send" size={12} /></button>
          </div>
        </div>
      )}

      {/* LISTEN pill — shown only when FAB drawer is closed */}
      {!open && (
        <button title="Listen" style={{
          position: 'fixed', right: 20, bottom: 98, zIndex: 951,
          padding: '5px 14px',
          fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 600, letterSpacing: '0.08em',
          color: 'var(--teal)',
          background: 'var(--bg-raised)',
          border: '1px solid var(--teal)',
          borderRadius: 12,
          boxShadow: '0 0 10px rgba(0,200,180,0.15)',
          cursor: 'pointer',
          transition: 'all var(--t-fast)',
        }}
        onMouseEnter={e => { e.currentTarget.style.background = 'var(--teal)'; e.currentTarget.style.color = 'var(--bg-base)'; }}
        onMouseLeave={e => { e.currentTarget.style.background = 'var(--bg-raised)'; e.currentTarget.style.color = 'var(--teal)'; }}
        >LISTEN</button>
      )}

      {/* FAB button */}
      <button onClick={onToggle} title={open ? 'Close Orchie' : 'Ask Orchie'} style={{
        position: 'fixed', right: 16, bottom: 36, zIndex: 951,
        width: 52, height: 52, borderRadius: '50%',
        background: open ? 'var(--bg-surface)' : 'var(--bg-raised)',
        border: '1px solid var(--amber-border)',
        boxShadow: open ? '0 4px 16px rgba(0,0,0,0.4)' : '0 0 24px var(--amber-glow), 0 6px 18px rgba(0,0,0,0.5)',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        transition: 'all var(--t-mid)',
        cursor: 'pointer',
      }}
      onMouseEnter={e => e.currentTarget.style.transform = 'scale(1.06)'}
      onMouseLeave={e => e.currentTarget.style.transform = 'scale(1)'}
      >
        {open ? <Icon name="x" size={16} /> : <SustenaMark size={24} />}
        {!open && unread > 0 && (
          <span style={{
            position: 'absolute', top: -2, right: -2,
            minWidth: 18, height: 18, padding: '0 5px',
            background: 'var(--amber)', color: 'var(--bg-base)',
            borderRadius: 9, border: '2px solid var(--bg-base)',
            fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 600,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
          }}>{unread}</span>
        )}
      </button>
    </>
  );
}

function FabBtn({ children, onClick, title }) {
  return (
    <button onClick={onClick} title={title} style={{
      width: 24, height: 24, borderRadius: 'var(--radius-sm)',
      color: 'var(--text-muted)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      transition: 'all var(--t-fast)',
    }}
    onMouseEnter={e => { e.currentTarget.style.background = 'var(--bg-raised)'; e.currentTarget.style.color = 'var(--text-primary)'; }}
    onMouseLeave={e => { e.currentTarget.style.background = 'transparent'; e.currentTarget.style.color = 'var(--text-muted)'; }}
    >{children}</button>
  );
}

/* ─── Footer telemetry ────────────────────────────────────── */
function Footer({ tick, stats = {} }) {
  const latency    = stats.api_p95_ms != null ? `${stats.api_p95_ms}ms` : '—';
  const opsPerMin  = stats.ops_per_min != null ? stats.ops_per_min : '—';
  const orchieLoad = stats.orchie_load_pct != null ? `${stats.orchie_load_pct}%` : '—';
  const events     = stats.event_count != null ? stats.event_count : '—';
  return (
    <footer style={{
      gridArea: 'footer',
      borderTop: '1px solid var(--border)',
      background: 'var(--bg-surface)',
      display: 'flex', alignItems: 'center', gap: 18,
      paddingLeft: 14, paddingRight: 14,
      flexShrink: 0, overflow: 'hidden',
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
        <span className="pulse" style={{ width: 5, height: 5, borderRadius: '50%', background: 'var(--teal)' }} />
        <span className="meta-10" style={{ color: 'var(--teal)' }}>NOMINAL</span>
      </div>
      <FSep />
      <FTick label="LATENCY" value={latency} />
      <FSep />
      <FTick label="OPS / MIN" value={opsPerMin} />
      <FSep />
      <FTick label="ORCHIE LOAD" value={orchieLoad} />
      <FSep />
      <FTick label="EVENTS" value={events} />
      <FSep />
      <FTick label="GAS" value="—" />
      <div style={{ flex: 1 }} />
      <FTick label="LAST SYNC" value={`T-${tick % 60}s`} />
      <FSep />
      <FTick label="BUILD" value="2026.05.25-a4f8c1" />
    </footer>
  );
}
function FSep() { return <span style={{ width: 1, height: 12, background: 'var(--border)' }} />; }
function FTick({ label, value }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, whiteSpace: 'nowrap' }}>
      <span className="label-10" style={{ fontSize: 9 }}>{label}</span>
      <span className="val-12" style={{ fontSize: 11 }}>{value}</span>
    </div>
  );
}

/* ─── Proposal modal ──────────────────────────────────────── */
function ProposalModal({ p, onClose }) {
  const votes  = p.council || { for: 0, against: 0, abstain: 0, total: 0, breakdown: [] };
  const total  = votes.for + votes.against + votes.abstain;
  const hasSim = p.sim != null;

  const doExecute = async () => {
    try {
      await api.post('/devui/console/execute', {
        sustain_id: p.sustain,
        operator_id: 'control.execute_approved',
        params: { proposal_id: p.id },
      });
      onClose();
      window.flash?.(`${p.id} executed · committed to ${p.sustain}`, 'ok');
    } catch (err) {
      window.flash?.(`Execute failed: ${err.message}`, 'danger');
    }
  };

  return (
    <div className="modal-scrim" onClick={onClose}>
      <div className="modal-card" onClick={e => e.stopPropagation()}>
        <header style={{
          padding: '16px 20px', borderBottom: '1px solid var(--border)',
          display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 16,
        }}>
          <div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 6 }}>
              <Badge tone="amber" dot>AWAITING EXECUTION</Badge>
              <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{p.id} · {p.sustain}</span>
            </div>
            <h2 style={{ fontFamily: 'var(--ui)', fontSize: 18, fontWeight: 500, color: 'var(--text-primary)', letterSpacing: '-0.01em' }}>{p.title}</h2>
          </div>
          <button onClick={onClose} style={{ color: 'var(--text-muted)', padding: 6 }}>
            <Icon name="x" size={14} />
          </button>
        </header>
        <div style={{ padding: '20px 20px 4px', overflow: 'auto', display: 'flex', flexDirection: 'column', gap: 18 }}>
          <p style={{ fontSize: 13, color: 'var(--text-secondary)', lineHeight: 1.6 }}>{p.summary}</p>

          {/* Sim metrics — only when simulation data present */}
          {hasSim ? (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 14, paddingTop: 12, borderTop: '1px solid var(--border)' }}>
              <ModalMetric label="OUTCOME SCORE" value={p.sim.outcomeScore.toFixed(2)} sub={`rank 1 of ${p.sim.runs}`} tone="teal" />
              <ModalMetric label="CSTR PASS" value={`${(p.sim.constraintPassRate * 100).toFixed(0)}%`} sub="100 simulations" />
              <ModalMetric label="PAWA COST" value={p.cost} sub="per execution" />
              <ModalMetric label="AUTONOMY" value={p.autonomy} sub="thresholds" tone="amber" />
            </div>
          ) : (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 14, paddingTop: 12, borderTop: '1px solid var(--border)' }}>
              <ModalMetric label="PAWA COST" value={p.cost} sub="per execution" />
              <ModalMetric label="AUTONOMY" value={p.autonomy} sub="thresholds" tone="amber" />
            </div>
          )}

          {/* Council tally */}
          <div>
            <span className="label-10" style={{ display: 'block', marginBottom: 8 }}>COUNCIL TALLY · {total} VOTES</span>
            {total > 0 ? (
              <>
                <div style={{ display: 'flex', height: 6, borderRadius: 3, overflow: 'hidden', background: 'var(--bg-base)' }}>
                  <div style={{ width: `${(votes.for / total) * 100}%`, background: 'var(--teal)' }} />
                  <div style={{ width: `${(votes.against / total) * 100}%`, background: 'var(--danger)' }} />
                  <div style={{ width: `${(votes.abstain / total) * 100}%`, background: 'var(--border)' }} />
                </div>
                <div style={{ display: 'flex', gap: 16, marginTop: 6 }}>
                  <span className="meta-10" style={{ color: 'var(--teal)' }}>FOR {votes.for}</span>
                  <span className="meta-10" style={{ color: 'var(--danger)' }}>AGAINST {votes.against}</span>
                  <span className="meta-10" style={{ color: 'var(--text-muted)' }}>ABSTAIN {votes.abstain}</span>
                </div>
              </>
            ) : (
              <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>no vote in progress</span>
            )}
          </div>

          {/* Per-councillor breakdown */}
          {votes.breakdown?.length > 0 && (
            <div>
              <span className="label-10" style={{ display: 'block', marginBottom: 8 }}>COUNCILLOR BREAKDOWN</span>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                {votes.breakdown.map(v => (
                  <div key={v.operative_id} style={{
                    display: 'flex', alignItems: 'flex-start', gap: 10,
                    padding: '7px 10px', background: 'var(--bg-base)',
                    border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)',
                  }}>
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)', flex: 1 }}>{v.operative_id}</span>
                    <span style={{
                      fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500,
                      color: v.vote === 'YES' ? 'var(--teal)' : v.vote === 'NO' ? 'var(--danger)' : 'var(--text-muted)',
                      flexShrink: 0,
                    }}>{v.vote}</span>
                    {v.reasoning && (
                      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', maxWidth: 240, lineHeight: 1.5 }}>
                        {v.reasoning}
                      </span>
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Simulation projection */}
          {hasSim && (
            <div style={{ background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)', padding: 14 }}>
              <span className="label-10" style={{ color: 'var(--amber)' }}>SIMULATION PROJECTION</span>
              <div style={{ fontFamily: 'var(--mono)', fontSize: 13, color: 'var(--text-primary)', marginTop: 4 }}>
                {p.sim.projection}
              </div>
            </div>
          )}
        </div>
        <footer style={{ padding: '14px 20px', borderTop: '1px solid var(--border)', display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <PBtn variant="ghost" onClick={onClose}>CANCEL</PBtn>
          <PBtn variant="ghost" onClick={() => {
            window.confirmAction?.({
              title: `Reject ${p.id}?`,
              body: `This rolls back the council vote and returns the proposal to draft. Authors will be notified.`,
              ctaLabel: 'REJECT',
              tone: 'danger',
              onConfirm: () => { onClose(); window.flash?.(`${p.id} rejected · authors notified`, 'danger'); },
            });
          }}>REJECT</PBtn>
          <PBtn onClick={() => {
            const projection = p.sim?.projection || `Execute ${p.title}`;
            if (p.autonomy === 'HIGH') {
              window.openPinPad?.({
                title: `${p.cta || 'EXECUTE'} · ${p.id}`,
                body: `${p.title}.`,
                onConfirm: doExecute,
              });
            } else {
              window.confirmAction?.({
                title: `${p.cta || 'EXECUTE'} ${p.id}?`,
                body: `${projection}. Pawa cost ${p.cost}.`,
                ctaLabel: p.cta || 'EXECUTE',
                tone: 'amber',
                onConfirm: doExecute,
              });
            }
          }}>{p.cta || 'EXECUTE'}</PBtn>
        </footer>
      </div>
    </div>
  );
}
function ModalMetric({ label, value, sub, tone }) {
  const c = tone === 'teal' ? 'var(--teal)' : tone === 'amber' ? 'var(--amber)' : 'var(--text-primary)';
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 3 }}>
      <span className="label-10">{label}</span>
      <span className="val-22" style={{ fontSize: 20, color: c }}>{value}</span>
      <span className="meta-10" style={{ color: 'var(--text-muted)', fontSize: 9 }}>{sub}</span>
    </div>
  );
}

/* formatClock defined in data.jsx */

/* ─── Generic Confirm Modal ──────────────────────────────── */
function ConfirmModal({ opts, onClose }) {
  const tone = opts.tone || 'amber';
  const toneColor = tone === 'danger' ? 'var(--danger)' : tone === 'teal' ? 'var(--teal)' : 'var(--amber)';
  return (
    <div className="modal-scrim" onClick={onClose} style={{ zIndex: 1100 }}>
      <div className="modal-card" onClick={e => e.stopPropagation()} style={{ maxWidth: 460 }}>
        <header style={{ padding: '16px 20px', borderBottom: '1px solid var(--border)', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <span style={{ width: 8, height: 8, borderRadius: '50%', background: toneColor, boxShadow: `0 0 8px ${toneColor}` }} />
            <h2 style={{ fontFamily: 'var(--ui)', fontSize: 16, fontWeight: 500 }}>{opts.title}</h2>
          </div>
          <button onClick={onClose} style={{ color: 'var(--text-muted)', padding: 6 }}><Icon name="x" size={12} /></button>
        </header>
        <div style={{ padding: '18px 20px', display: 'flex', flexDirection: 'column', gap: 10 }}>
          <p style={{ fontSize: 13, color: 'var(--text-secondary)', lineHeight: 1.6 }}>{opts.body}</p>
          {opts.detail && (
            <div style={{ background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)', padding: 12, fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)', whiteSpace: 'pre-wrap' }}>
              {opts.detail}
            </div>
          )}
        </div>
        <footer style={{ padding: '12px 20px', borderTop: '1px solid var(--border)', display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <PBtn variant="ghost" onClick={onClose}>CANCEL</PBtn>
          <button onClick={() => { opts.onConfirm?.(); onClose(); }} style={{
            padding: '6px 14px',
            fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, letterSpacing: '0.08em',
            color: tone === 'danger' ? 'var(--bg-base)' : 'var(--bg-base)',
            background: toneColor,
            border: `1px solid ${toneColor}`,
            borderRadius: 'var(--radius-sm)',
            display: 'inline-flex', alignItems: 'center', gap: 6,
          }}>{opts.ctaLabel || 'CONFIRM'}</button>
        </footer>
      </div>
    </div>
  );
}

/* ─── PIN Pad Modal ──────────────────────────────────────── */
function PinPad({ opts, onClose }) {
  const [pin, setPin] = dUseState('');
  const [stage, setStage] = dUseState('enter'); // enter | verifying | success
  dUseEffect(() => {
    if (pin.length === 4 && stage === 'enter') {
      setStage('verifying');
      setTimeout(() => {
        setStage('success');
        setTimeout(() => { opts.onConfirm?.(); onClose(); }, 700);
      }, 900);
    }
  }, [pin, stage]);
  const append = (d) => stage === 'enter' && pin.length < 4 && setPin(p => p + d);
  const back = () => stage === 'enter' && setPin(p => p.slice(0, -1));
  return (
    <div className="modal-scrim" onClick={onClose} style={{ zIndex: 1100 }}>
      <div className="modal-card" onClick={e => e.stopPropagation()} style={{ maxWidth: 380 }}>
        <header style={{ padding: '16px 20px', borderBottom: '1px solid var(--border)', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div>
            <div className="label-10" style={{ color: 'var(--amber)' }}>BIOMETRIC · PIN REQUIRED</div>
            <h2 style={{ fontFamily: 'var(--ui)', fontSize: 16, fontWeight: 500, marginTop: 4 }}>{opts.title}</h2>
          </div>
          <button onClick={onClose} style={{ color: 'var(--text-muted)', padding: 6 }}><Icon name="x" size={12} /></button>
        </header>
        <div style={{ padding: '18px 20px', display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 18 }}>
          <p style={{ fontSize: 12, color: 'var(--text-secondary)', textAlign: 'center', lineHeight: 1.5 }}>{opts.body}</p>
          {/* PIN dots */}
          <div style={{ display: 'flex', gap: 16 }}>
            {[0,1,2,3].map(i => (
              <span key={i} style={{
                width: 14, height: 14, borderRadius: '50%',
                background: pin.length > i ? 'var(--amber)' : 'transparent',
                border: `1.5px solid ${stage === 'success' ? 'var(--teal)' : pin.length > i ? 'var(--amber)' : 'var(--border-mid)'}`,
                boxShadow: pin.length > i ? '0 0 6px var(--amber-glow)' : 'none',
                transition: 'all var(--t-fast)',
              }} />
            ))}
          </div>
          {stage === 'verifying' && (
            <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--amber)', letterSpacing: '0.1em' }}>VERIFYING<span className="blink">_</span></span>
          )}
          {stage === 'success' && (
            <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--teal)', letterSpacing: '0.1em' }}>✓ AUTHORIZED</span>
          )}
          {/* Keypad */}
          {stage === 'enter' && (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 56px)', gap: 8 }}>
              {[1,2,3,4,5,6,7,8,9].map(n => (
                <button key={n} onClick={() => append(n)} style={pinKeyStyle}>{n}</button>
              ))}
              <span />
              <button onClick={() => append(0)} style={pinKeyStyle}>0</button>
              <button onClick={back} style={{ ...pinKeyStyle, fontSize: 14 }}>⌫</button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
const pinKeyStyle = {
  width: 56, height: 48,
  fontFamily: 'var(--mono)', fontSize: 20, color: 'var(--text-primary)',
  background: 'var(--bg-raised)',
  border: '1px solid var(--border-mid)',
  borderRadius: 'var(--radius-sm)',
  transition: 'all var(--t-fast)',
};

/* ─── Library item detail modal ──────────────────────────── */
function LibraryModal({ data, onClose }) {
  const { item, kind } = data;
  const kindLabel = { operatives: 'OPERATIVE', operators: 'OPERATOR', spores: 'SUSTAIN SPORE', widgets: 'WIDGET' }[kind];
  const accent = { operatives: 'var(--node-operative)', operators: 'var(--node-operator)', spores: 'var(--node-state)', widgets: 'var(--node-event)' }[kind];
  const installLabel = kind === 'spores' ? 'FORK SPORE' : kind === 'widgets' ? 'INSTALL WIDGET' : 'INSTALL';

  return (
    <div className="modal-scrim" onClick={onClose}>
      <div className="modal-card" onClick={e => e.stopPropagation()} style={{ maxWidth: 720 }}>
        <header style={{
          padding: '18px 22px', borderBottom: '1px solid var(--border)',
          display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 14,
        }}>
          <div style={{ display: 'flex', alignItems: 'flex-start', gap: 14 }}>
            <div style={{
              width: 48, height: 48, borderRadius: 'var(--radius-md)',
              background: 'var(--bg-base)', border: `1px solid ${accent}`,
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              boxShadow: `0 0 18px ${accent}33`,
              flexShrink: 0,
            }}>
              <LibraryMark kind={kind} size={26} />
            </div>
            <div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 6 }}>
                <span className="label-10" style={{ color: accent }}>{kindLabel}</span>
                <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{item.author} · v{item.version}</span>
              </div>
              <h2 style={{ fontFamily: 'var(--ui)', fontSize: 22, fontWeight: 500, letterSpacing: '-0.01em' }}>{item.name}</h2>
            </div>
          </div>
          <button onClick={onClose} style={{ color: 'var(--text-muted)', padding: 6 }}><Icon name="x" size={14} /></button>
        </header>

        <div style={{ padding: '20px 22px 4px', overflow: 'auto', display: 'flex', flexDirection: 'column', gap: 18 }}>
          <p style={{ fontSize: 13, color: 'var(--text-secondary)', lineHeight: 1.6 }}>{item.desc}</p>

          {/* Stats grid */}
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 14, paddingTop: 12, borderTop: '1px solid var(--border)' }}>
            <ModalMetric label="TRUST RATING" value={item.trust} sub="of 100" tone="teal" />
            <ModalMetric label="INSTALLS" value={item.downloads.toLocaleString()} sub="active uses" />
            <ModalMetric label="LICENSE" value={typeof item.pawa === 'string' ? item.pawa : `${item.pawa} pwa`} sub="per execution" tone="amber" />
            <ModalMetric label="LATEST" value={`v${item.version}`} sub="14 Feb 2026" />
          </div>

          {/* Kind-specific detail block */}
          {kind === 'operatives' && (
            <div>
              <span className="label-10" style={{ display: 'block', marginBottom: 8 }}>CAPABILITIES</span>
              <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 6 }}>
                {['Reads state.finances.*', 'Drafts council proposals', 'Listens to mpesa.parse events', 'Calls 3 sub-operators', 'Bounded by 5 constraints', 'pawa.budget = 200 / session'].map(c => (
                  <div key={c} style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 10px', background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)' }}>
                    <span style={{ color: accent }}>·</span>
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)' }}>{c}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
          {kind === 'operators' && (
            <div>
              <span className="label-10" style={{ display: 'block', marginBottom: 8 }}>SIGNATURE</span>
              <pre style={{ background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)', padding: 14, fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)', overflow: 'auto', lineHeight: 1.6 }}>
{`(state: State, inputs: { pocket_name: str, amount: num }) → 
  delta: Δ, events: Event[]

constraints:
  sum_constraint     : sum(allocated) ≤ income_monthly
  balance_constraint : state.cash >= amount
  
emits: budget.allocated`}
              </pre>
            </div>
          )}
          {kind === 'spores' && (
            <div>
              <span className="label-10" style={{ display: 'block', marginBottom: 8 }}>STATE SCHEMA</span>
              <pre style={{ background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)', padding: 14, fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-secondary)', overflow: 'auto', lineHeight: 1.6 }}>
{`state.${item.name.toLowerCase()}:
  finances:
    pockets:        { food, transport, savings, rent }
    burn_rate:      number
    cash_position:  number
  pantry:           { …items }
  chama_links:      array
  preferences:      object
operators: 24 included
constraints: 12 included
operatives: 4 default (Mentor, Protégé, Curator, Navigator)`}
              </pre>
            </div>
          )}
          {kind === 'widgets' && (
            <div>
              <span className="label-10" style={{ display: 'block', marginBottom: 8 }}>LIVE PREVIEW</span>
              <div style={{
                height: 200, background: 'var(--bg-base)', border: '1px solid var(--border)',
                borderRadius: 'var(--radius-md)', display: 'flex', alignItems: 'center', justifyContent: 'center',
              }}>
                <WidgetPreview name={item.name} />
              </div>
            </div>
          )}
        </div>

        <footer style={{ padding: '14px 22px', borderTop: '1px solid var(--border)', display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <PBtn variant="ghost" onClick={onClose}>CANCEL</PBtn>
          <PBtn onClick={() => { onClose(); window.flash?.(`${item.name} installed`, 'ok'); }}>{installLabel}</PBtn>
        </footer>
      </div>
    </div>
  );
}

export default App;