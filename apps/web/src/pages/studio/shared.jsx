/**
 * apps/web/src/pages/studio/shared.jsx
 *
 * Shared style tokens + small primitives for the Modeling Studio. Studio is
 * desktop-first, high-craft, and organized around ONE selected Sustain —
 * these are the visual building blocks every verb zone (Monitor / Control /
 * Simulate / Network / Edit) and the sustain graph reuse, so the whole
 * surface reads as one coherent frame rather than five unrelated panels.
 */
import React from 'react';

export const panelCard = {
  background: 'var(--bg-surface)',
  border: '1px solid var(--border-mid)',
  borderRadius: 'var(--radius-lg)',
  padding: '18px 20px',
};

export const sectionLabel = {
  fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.1em',
  color: 'var(--text-dim)', textTransform: 'uppercase', marginBottom: 10,
};

export const mutedText = { fontFamily: 'var(--ui)', fontSize: 12.5, color: 'var(--text-muted)' };

export const primaryButton = {
  fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 700, letterSpacing: '0.06em',
  color: 'var(--bg-base)', background: 'var(--teal)',
  border: 'none', borderRadius: 'var(--radius-sm)', padding: '9px 16px', cursor: 'pointer',
};

export const ghostButton = {
  fontFamily: 'var(--mono)', fontSize: 10.5, letterSpacing: '0.05em',
  color: 'var(--text-secondary)', background: 'var(--bg-overlay)',
  border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)', padding: '8px 14px', cursor: 'pointer',
};

export const dangerButton = {
  ...primaryButton, background: 'var(--danger)', color: '#fff',
};

export function fmtNum(v) {
  if (v === null || v === undefined) return '—';
  if (typeof v !== 'number') return String(v);
  return v.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

export function Empty({ text, sub }) {
  return (
    <div style={{ padding: '32px 16px', textAlign: 'center' }}>
      <div style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-muted)' }}>{text}</div>
      {sub && <div style={{ ...mutedText, marginTop: 6, fontSize: 11, color: 'var(--text-dim)' }}>{sub}</div>}
    </div>
  );
}

export function StatusDot({ status }) {
  const color = status === 'ok' || status === 'live'
    ? 'var(--ok)'
    : status === 'fail' || status === 'missing' || status === 'failed'
      ? 'var(--danger)'
      : 'var(--text-dim)';
  return (
    <span style={{
      display: 'inline-block', width: 7, height: 7, borderRadius: '50%',
      background: color, marginRight: 6, flexShrink: 0,
    }} />
  );
}

export function WhyReveal({ children }) {
  const [open, setOpen] = React.useState(false);
  return (
    <div style={{ marginTop: 8 }}>
      <button
        onClick={() => setOpen(o => !o)}
        style={{
          background: 'none', border: 'none', padding: 0, cursor: 'pointer',
          fontFamily: 'var(--mono)', fontSize: 9.5, letterSpacing: '0.04em',
          color: 'var(--text-dim)', textDecoration: 'underline dotted',
        }}
      >
        why am I seeing this?
      </button>
      {open && (
        <div style={{
          marginTop: 6, padding: '8px 10px', borderRadius: 'var(--radius-sm)',
          background: 'var(--bg-raised)', border: '1px solid var(--border)',
          fontFamily: 'var(--ui)', fontSize: 11.5, color: 'var(--text-secondary)', lineHeight: 1.5,
        }}>
          {children}
        </div>
      )}
    </div>
  );
}

export function ZoneFrame({ title, sub, children }) {
  return (
    <div style={{ ...panelCard, minHeight: 260 }}>
      <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginBottom: 16 }}>
        <div>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 700, letterSpacing: '0.06em', color: 'var(--text-primary)' }}>
            {title}
          </div>
          {sub && <div style={{ ...mutedText, marginTop: 3 }}>{sub}</div>}
        </div>
      </div>
      {children}
    </div>
  );
}
