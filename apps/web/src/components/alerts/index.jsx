import React from 'react';
/* Sustena — Alerts & Modal System
   AlertModal: full-overlay urgent alerts with priority queue
   SustainPanel: full-page slide-in panel from right (75vw desktop, 100vw mobile)
   Both exported to window.
*/

const { useState: aUseState, useEffect: aUseEffect, useRef: aUseRef, useCallback: aUseCallback } = React;

/* ═══════════════════════════════════════════════════════════
   ALERT MODAL — stacking, swipe/button dismiss, z-index 9999
   ═══════════════════════════════════════════════════════════ */

const TONE_CONFIG = {
  warning:         { icon: 'warning', color: 'var(--danger)',  bg: 'rgba(224,80,80,0.06)',   label: 'ALERT' },
  info:            { icon: 'dot',     color: 'var(--info)',    bg: 'rgba(80,144,224,0.06)',  label: 'INFO'  },
  'action-required':{ icon: 'agent',  color: 'var(--amber)',   bg: 'var(--amber-glow)',      label: 'ACTION REQUIRED' },
  ok:              { icon: 'check',   color: 'var(--ok)',      bg: 'rgba(76,175,128,0.06)',  label: 'SUCCESS' },
};

/* Single AlertModal card */
function AlertCard({ alert, onDismiss, stackIndex, totalAlerts }) {
  const cfg = TONE_CONFIG[alert.tone] || TONE_CONFIG.info;
  const cardRef = aUseRef(null);
  const dragStartX = aUseRef(null);
  const [dragging, setDragging] = aUseState(false);
  const [offsetX, setOffsetX] = aUseState(0);
  const [exiting, setExiting] = aUseState(false);

  const isTop = stackIndex === totalAlerts - 1;
  const stackOffset = (totalAlerts - 1 - stackIndex) * 6;
  const stackScale = 1 - (totalAlerts - 1 - stackIndex) * 0.04;

  /* Swipe-to-dismiss handlers */
  const onPointerDown = (e) => {
    if (!isTop) return;
    dragStartX.current = e.clientX;
    setDragging(true);
    cardRef.current?.setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e) => {
    if (!dragging || dragStartX.current == null) return;
    setOffsetX(e.clientX - dragStartX.current);
  };
  const onPointerUp = () => {
    if (!dragging) return;
    setDragging(false);
    if (Math.abs(offsetX) > 90) {
      dismiss();
    } else {
      setOffsetX(0);
    }
  };

  const dismiss = () => {
    setExiting(true);
    setTimeout(() => onDismiss(alert.id), 250);
  };

  return (
    <div
      ref={cardRef}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      style={{
        position: 'absolute',
        left: '50%',
        top: '50%',
        transform: `
          translateX(calc(-50% + ${offsetX}px))
          translateY(calc(-50% + ${-stackOffset}px))
          scale(${stackScale})
        `,
        transition: dragging ? 'none' : 'all 0.25s cubic-bezier(0.22,0.61,0.36,1)',
        opacity: exiting ? 0 : (isTop ? 1 : 0.7 - stackIndex * 0.1),
        zIndex: stackIndex,
        width: 'min(460px, 90vw)',
        background: 'var(--bg-surface)',
        border: `1px solid ${cfg.color}50`,
        borderTop: `3px solid ${cfg.color}`,
        borderRadius: 'var(--radius-lg)',
        boxShadow: `0 20px 60px rgba(0,0,0,0.6), 0 0 0 1px ${cfg.color}18`,
        overflow: 'hidden',
        cursor: isTop ? (dragging ? 'grabbing' : 'grab') : 'default',
        userSelect: 'none',
        animation: exiting ? 'none' : 'alertSlideIn 0.3s cubic-bezier(0.22,0.61,0.36,1)',
      }}
    >
      <style>{`
        @keyframes alertSlideIn {
          from { opacity: 0; transform: translateX(-50%) translateY(calc(-50% - 24px)) scale(0.94); }
          to   { opacity: 1; transform: translateX(-50%) translateY(-50%) scale(1); }
        }
      `}</style>

      {/* Ambient tinted background */}
      <div style={{ position: 'absolute', inset: 0, background: cfg.bg, pointerEvents: 'none' }} />

      {/* Content */}
      <div style={{ position: 'relative', padding: '18px 20px' }}>
        {/* Header */}
        <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12, marginBottom: 12 }}>
          <div style={{
            width: 34, height: 34, flexShrink: 0,
            borderRadius: 'var(--radius-md)',
            background: `${cfg.color}18`,
            border: `1px solid ${cfg.color}40`,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            color: cfg.color,
          }}>
            <Icon name={cfg.icon} size={16} />
          </div>
          <div style={{ flex: 1 }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.10em', color: cfg.color, display: 'block' }}>
              {cfg.label}
            </span>
            <h3 style={{ fontFamily: 'var(--ui)', fontSize: 16, fontWeight: 500, color: 'var(--text-primary)', marginTop: 3, lineHeight: 1.2 }}>
              {alert.title}
            </h3>
          </div>
          {isTop && (
            <button onClick={dismiss} style={{ color: 'var(--text-dim)', padding: 4, flexShrink: 0, transition: 'color var(--t-fast)' }}
              onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
              onMouseLeave={e => e.currentTarget.style.color = 'var(--text-dim)'}
            >
              <Icon name="x" size={13} />
            </button>
          )}
        </div>

        {/* Body */}
        <p style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-secondary)', lineHeight: 1.6, marginBottom: alert.cta ? 16 : 4 }}>
          {alert.body}
        </p>

        {/* CTA */}
        {alert.cta && isTop && (
          <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end', paddingTop: 12, borderTop: '1px solid var(--border)' }}>
            <button onClick={dismiss} style={{
              padding: '7px 14px',
              fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.08em', textTransform: 'uppercase',
              color: 'var(--text-muted)', background: 'transparent',
              border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
              transition: 'all var(--t-fast)',
            }}
            onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
            onMouseLeave={e => e.currentTarget.style.color = 'var(--text-muted)'}
            >DISMISS</button>
            <button onClick={() => { alert.onCta?.(); dismiss(); }} style={{
              padding: '7px 16px',
              fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.08em', textTransform: 'uppercase',
              color: 'var(--bg-base)', background: cfg.color,
              border: `1px solid ${cfg.color}`,
              borderRadius: 'var(--radius-sm)',
              transition: 'all var(--t-fast)',
            }}>
              {alert.cta}
            </button>
          </div>
        )}

        {/* Swipe hint */}
        {isTop && !alert.cta && (
          <p style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', textAlign: 'center', marginTop: 10 }}>
            swipe to dismiss
          </p>
        )}
      </div>

      {/* Stack indicator */}
      {totalAlerts > 1 && isTop && (
        <div style={{
          position: 'absolute', top: 12, right: 44,
          padding: '2px 7px', borderRadius: 8,
          background: 'var(--bg-raised)', border: '1px solid var(--border)',
          fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)',
        }}>
          {totalAlerts} alerts
        </div>
      )}
    </div>
  );
}

/* Alert manager — renders the full scrim + stacked cards */
function AlertModal({ alerts, onDismiss }) {
  if (!alerts || alerts.length === 0) return null;
  return (
    <div style={{
      position: 'fixed', inset: 0, zIndex: 9999,
      background: 'rgba(0,0,0,0.7)',
      backdropFilter: 'blur(2px)',
      animation: 'fadeUp 0.18s ease-out',
    }}>
      {alerts.map((alert, i) => (
        <AlertCard
          key={alert.id}
          alert={alert}
          onDismiss={onDismiss}
          stackIndex={i}
          totalAlerts={alerts.length}
        />
      ))}
    </div>
  );
}

/* ─── AlertSystem hook + provider ─────────────────────────
   Usage:
     const { pushAlert, dismissAlert, alerts } = useAlertSystem();
     window.pushAlert({ tone:'warning', title:'...', body:'...', cta:'VIEW' });
*/
function useAlertSystem() {
  const [alerts, setAlerts] = aUseState([]);

  const pushAlert = aUseCallback((alert) => {
    const id = Math.random().toString(36).slice(2);
    setAlerts(prev => [...prev, { ...alert, id }]);
  }, []);

  const dismissAlert = aUseCallback((id) => {
    setAlerts(prev => prev.filter(a => a.id !== id));
  }, []);

  // Expose globally
  aUseEffect(() => {
    window.pushAlert = pushAlert;
  }, [pushAlert]);

  return { alerts, pushAlert, dismissAlert };
}

/* ═══════════════════════════════════════════════════════════
   SUSTAIN PANEL — slide in from right, pushes main UI left
   ═══════════════════════════════════════════════════════════ */

/*  Props:
    open: bool
    onClose: fn
    sustainName: string
    sustainLogo: ReactNode (optional)
    status: 'live' | 'seed' | 'archived'
    children: ReactNode — custom branded content slot
*/
function SustainPanel({ open, onClose, sustainName, sustainLogo, status = 'live', children }) {
  const statusColor = status === 'live' ? 'var(--teal)' : status === 'seed' ? 'var(--amber)' : 'var(--text-muted)';
  const panelWidth = 'clamp(320px, 75vw, 900px)';

  return (
    <>
      {/* Scrim — does NOT overlay main UI, just sits behind panel */}
      {open && (
        <div
          onClick={onClose}
          style={{
            position: 'fixed', inset: 0, zIndex: 400,
            background: 'rgba(0,0,0,0.3)',
            animation: 'fadeUp 0.18s ease-out',
          }}
        />
      )}

      {/* Panel */}
      <div style={{
        position: 'fixed',
        top: 0, right: 0, bottom: 0,
        width: panelWidth,
        background: 'var(--bg-base)',
        borderLeft: '1px solid var(--border-mid)',
        zIndex: 401,
        display: 'flex', flexDirection: 'column',
        transform: open ? 'translateX(0)' : 'translateX(100%)',
        transition: 'transform 0.3s cubic-bezier(0.22,0.61,0.36,1)',
        boxShadow: open ? '-20px 0 60px rgba(0,0,0,0.5)' : 'none',
      }}>
        {/* Nav header */}
        <header style={{
          height: 52, flexShrink: 0,
          borderBottom: '1px solid var(--border)',
          background: 'var(--bg-surface)',
          padding: '0 18px',
          display: 'flex', alignItems: 'center', gap: 12,
        }}>
          {/* Back arrow */}
          <button onClick={onClose} style={{
            display: 'flex', alignItems: 'center', gap: 6,
            color: 'var(--text-muted)', padding: '4px 0',
            transition: 'color var(--t-fast)',
          }}
          onMouseEnter={e => e.currentTarget.style.color = 'var(--text-primary)'}
          onMouseLeave={e => e.currentTarget.style.color = 'var(--text-muted)'}
          >
            <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
              <path d="M7 2L3 5.5L7 9" />
            </svg>
            <span className="meta-10">CLOSE</span>
          </button>

          <span style={{ width: 1, height: 18, background: 'var(--border)' }} />

          {/* Sustain logo/icon */}
          <div style={{
            width: 28, height: 28, borderRadius: 'var(--radius-sm)',
            background: 'var(--bg-raised)', border: '1px solid var(--border)',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            flexShrink: 0, overflow: 'hidden',
          }}>
            {sustainLogo || <SustenaMark size={16} />}
          </div>

          <div style={{ flex: 1, minWidth: 0 }}>
            <span style={{ fontFamily: 'var(--ui)', fontSize: 14, fontWeight: 500, color: 'var(--text-primary)', display: 'block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {sustainName || 'Sustain'}
            </span>
          </div>

          {/* Status pill */}
          <span style={{
            display: 'inline-flex', alignItems: 'center', gap: 5,
            padding: '3px 9px', borderRadius: 12,
            background: `${statusColor}12`,
            border: `1px solid ${statusColor}40`,
            fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500, letterSpacing: '0.08em', textTransform: 'uppercase',
            color: statusColor, flexShrink: 0,
          }}>
            <span style={{ width: 5, height: 5, borderRadius: '50%', background: statusColor, animation: status === 'live' ? 'pulse 1.6s ease-in-out infinite' : 'none' }} />
            {status}
          </span>
        </header>

        {/* Custom content slot */}
        <div style={{ flex: 1, overflowY: 'auto', overflowX: 'hidden' }}>
          {children || (
            <div style={{ height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center', flexDirection: 'column', gap: 14, padding: 40 }}>
              <SustenaMark size={40} />
              <span className="label-11" style={{ color: 'var(--text-secondary)' }}>{sustainName}</span>
              <span className="meta-11" style={{ color: 'var(--text-muted)', textAlign: 'center', maxWidth: 280, lineHeight: 1.6 }}>
                Custom sustain UI will render here. Drop any branded content into the children slot.
              </span>
            </div>
          )}
        </div>
      </div>
    </>
  );
}

/* ─── Demo component ───────────────────────────────────── */
function AlertDemoButton() {
  const tones = ['warning', 'info', 'action-required', 'ok'];
  const samples = [
    { tone: 'warning', title: 'Cash position breach', body: 'Homestead cash position fell below the minimum reserve floor of KSH 50,000. Immediate review required.', cta: 'REVIEW NOW' },
    { tone: 'action-required', title: 'Council vote closing', body: 'SUS-0148 requires your vote before quorum closes in 1h 22m. 7 for, 2 against.', cta: 'CAST VOTE' },
    { tone: 'info', title: 'Pawa top-up received', body: '500 Pawa tokens credited to homestead.bonnie from Council reward pool.' },
    { tone: 'ok', title: 'Operator completed', body: 'budget.allocate executed successfully. KSH 1,120 transferred. Pantry updated.' },
  ];
  return (
    <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
      {samples.map((s, i) => (
        <button key={i} onClick={() => window.pushAlert?.({ ...s, onCta: () => window.flash?.(`${s.title} actioned`, 'ok') })} style={{
          padding: '6px 12px',
          fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.06em', textTransform: 'uppercase',
          color: 'var(--text-secondary)', background: 'var(--bg-raised)',
          border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
          transition: 'all var(--t-fast)',
        }}
        onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
        onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border-mid)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
        >{s.tone}</button>
      ))}
    </div>
  );
}

Object.assign(window, {
  AlertModal, AlertCard, SustainPanel,
  useAlertSystem, AlertDemoButton,
});
