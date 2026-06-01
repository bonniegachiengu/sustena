import React from 'react';
/* Sustena — Responsive Digital Asset Widgets
   AirTicket, Voucher, Certificate, Badge, Poster, Thumbnail
   All widgets open a ResponsiveWidgetModal on click.
   Physical object feel via CSS depth/texture.
*/

const { useState: wUseState, useEffect: wUseEffect, useRef: wUseRef } = React;

/* ─── Shared QR placeholder ─────────────────────────────── */
function QRPlaceholder({ size = 64, color = 'var(--text-primary)' }) {
  // Stylised QR grid pattern drawn in SVG — no library needed
  const cells = [
    [1,1,1,1,1,1,1,0,1,0,1,0,1,1,1,1,1,1,1],
    [1,0,0,0,0,0,1,0,0,1,0,0,1,0,0,0,0,0,1],
    [1,0,1,1,1,0,1,0,1,0,1,0,1,0,1,1,1,0,1],
    [1,0,1,1,1,0,1,0,0,0,0,1,1,0,1,1,1,0,1],
    [1,0,1,1,1,0,1,0,1,1,0,0,1,0,1,1,1,0,1],
    [1,0,0,0,0,0,1,0,0,0,1,0,1,0,0,0,0,0,1],
    [1,1,1,1,1,1,1,0,1,0,1,0,1,1,1,1,1,1,1],
    [0,0,0,0,0,0,0,0,1,0,0,1,0,0,0,0,0,0,0],
    [1,0,1,1,0,1,1,1,0,1,1,0,1,0,1,1,0,1,0],
    [0,1,0,0,1,0,0,0,1,0,0,1,0,1,0,0,1,0,1],
    [1,1,0,1,0,0,1,1,1,1,0,0,1,0,0,1,1,1,0],
    [0,0,0,0,0,0,0,0,0,1,0,1,0,0,1,0,0,0,1],
    [1,1,1,1,1,1,1,0,1,0,1,0,1,0,1,0,0,1,0],
    [1,0,0,0,0,0,1,0,0,1,0,1,0,1,0,0,1,1,1],
    [1,0,1,1,1,0,1,0,1,0,1,0,1,0,1,1,0,0,0],
    [1,0,1,1,1,0,1,1,0,1,0,0,0,1,0,0,1,0,1],
    [1,0,1,1,1,0,1,0,0,0,1,0,1,0,0,1,0,1,0],
    [1,0,0,0,0,0,1,0,1,1,0,1,0,0,1,0,0,1,1],
    [1,1,1,1,1,1,1,0,0,0,1,0,0,1,0,1,1,0,0],
  ];
  const n = cells.length;
  const cell = size / n;
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} style={{ display: 'block' }}>
      {cells.map((row, r) =>
        row.map((v, c) => v ? (
          <rect key={`${r}-${c}`} x={c * cell} y={r * cell} width={cell} height={cell} fill={color} />
        ) : null)
      )}
    </svg>
  );
}

/* ─── Perforated edge SVG ───────────────────────────────── */
function PerforatedEdge({ vertical = false, color = 'var(--border-mid)' }) {
  const dashes = Array.from({ length: 24 });
  return (
    <div style={{
      display: 'flex',
      flexDirection: vertical ? 'column' : 'row',
      alignItems: 'center',
      gap: 4,
    }}>
      {dashes.map((_, i) => (
        <span key={i} style={{
          width: vertical ? 6 : 1,
          height: vertical ? 1 : 6,
          borderRadius: 1,
          background: color,
          opacity: 0.5,
        }} />
      ))}
    </div>
  );
}

/* ─── Ornate border SVG for Certificate ─────────────────── */
function OrnateBorder({ w = 540, h = 700 }) {
  const m = 14;
  return (
    <svg width={w} height={h} style={{ position: 'absolute', inset: 0, pointerEvents: 'none' }} viewBox={`0 0 ${w} ${h}`}>
      {/* Outer rect */}
      <rect x={m} y={m} width={w-m*2} height={h-m*2} fill="none" stroke="var(--amber)" strokeWidth="1" opacity="0.5" />
      {/* Inner rect */}
      <rect x={m+6} y={m+6} width={w-m*2-12} height={h-m*2-12} fill="none" stroke="var(--amber)" strokeWidth="0.5" opacity="0.3" strokeDasharray="3 6" />
      {/* Corner flourishes */}
      {[
        [m, m, 0], [w-m, m, 90], [w-m, h-m, 180], [m, h-m, 270]
      ].map(([cx, cy, rot], i) => (
        <g key={i} transform={`rotate(${rot}, ${cx}, ${cy})`}>
          <path d={`M ${cx} ${cy+18} Q ${cx} ${cy} ${cx+18} ${cy}`} fill="none" stroke="var(--amber)" strokeWidth="1.5" opacity="0.6" />
          <circle cx={cx} cy={cy} r="3" fill="var(--amber)" opacity="0.5" />
        </g>
      ))}
    </svg>
  );
}

/* ─── Metallic gradient helper ──────────────────────────── */
const metalGradient = (c1 = '#c8860a', c2 = '#e8a020', c3 = '#c8860a') =>
  `linear-gradient(135deg, ${c1} 0%, ${c2} 40%, ${c2} 60%, ${c3} 100%)`;

/* ═══════════════════════════════════════════════════════════
   WIDGET COMPONENTS
   ═══════════════════════════════════════════════════════════ */

/* ── AirTicket ──────────────────────────────────────────── */
function AirTicketWidget({ data, onClick, expanded = false }) {
  const d = data || {
    eventName: 'Vyyb Hive · Grand Opening',
    date: '14 Jun 2026',
    time: '18:00 EAT',
    tier: 'General Admission',
    seat: 'GA-142',
    sustain: 'vyyb.hive',
  };
  return (
    <button onClick={onClick} style={{
      display: 'flex', flexDirection: 'column',
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: 12,
      overflow: 'hidden',
      boxShadow: '0 4px 16px rgba(0,0,0,0.35), inset 0 1px 0 rgba(255,255,255,0.04)',
      textAlign: 'left',
      transition: 'transform var(--t-mid), box-shadow var(--t-mid)',
      cursor: 'pointer',
      width: expanded ? '100%' : 280,
    }}
    onMouseEnter={e => { e.currentTarget.style.transform = 'translateY(-2px)'; e.currentTarget.style.boxShadow = '0 8px 24px rgba(0,0,0,0.5), inset 0 1px 0 rgba(255,255,255,0.06)'; }}
    onMouseLeave={e => { e.currentTarget.style.transform = 'translateY(0)'; e.currentTarget.style.boxShadow = '0 4px 16px rgba(0,0,0,0.35), inset 0 1px 0 rgba(255,255,255,0.04)'; }}
    >
      {/* Header band */}
      <div style={{
        padding: '12px 16px',
        background: metalGradient('#1a3a0d', '#2d5016', '#1a3a0d'),
        display: 'flex', justifyContent: 'space-between', alignItems: 'center',
      }}>
        <div>
          <span className="label-10" style={{ color: 'rgba(200,220,180,0.6)', fontSize: 9 }}>SUSTENA · DIGITAL TICKET</span>
          <div style={{ fontFamily: 'var(--ui)', fontSize: 14, fontWeight: 500, color: '#e8f0d8', marginTop: 3 }}>{d.eventName}</div>
        </div>
        <SustenaMark size={22} primary="#e8a020" secondary="rgba(200,220,180,0.5)" />
      </div>

      {/* Perforated divider */}
      <div style={{ padding: '0 16px', background: 'var(--bg-surface)' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 0 }}>
          <span style={{ width: 14, height: 14, borderRadius: '50%', background: 'var(--bg-base)', flexShrink: 0, marginLeft: -22 }} />
          <PerforatedEdge color="var(--border)" />
          <span style={{ width: 14, height: 14, borderRadius: '50%', background: 'var(--bg-base)', flexShrink: 0, marginRight: -22 }} />
        </div>
      </div>

      {/* Body */}
      <div style={{ padding: '12px 16px', display: 'flex', gap: 14, alignItems: 'flex-start' }}>
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 8 }}>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
            <TicketField label="DATE" value={d.date} />
            <TicketField label="TIME" value={d.time} />
            <TicketField label="TIER" value={d.tier} />
            <TicketField label="SEAT / REF" value={d.seat} />
          </div>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', marginTop: 4 }}>
            {d.sustain} · SUSTENA VERIFIED
          </div>
        </div>
        <div style={{ flexShrink: 0, padding: 4, background: 'white', borderRadius: 4 }}>
          <QRPlaceholder size={56} color="#111" />
        </div>
      </div>
    </button>
  );
}

function TicketField({ label, value }) {
  return (
    <div>
      <span className="meta-10" style={{ fontSize: 8, color: 'var(--text-dim)', display: 'block' }}>{label}</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)', fontWeight: 500 }}>{value}</span>
    </div>
  );
}

/* ── Voucher ─────────────────────────────────────────────── */
function VoucherWidget({ data, onClick, expanded = false }) {
  const d = data || {
    title: 'Vyyb Food Credit',
    value: 'KSH 500',
    discount: '25% OFF',
    sustain: 'vyyb.hive',
    expiry: '30 Jun 2026',
    code: 'VYYB-500-X8K2',
    color: 'var(--teal)',
  };
  const accentColor = d.color || 'var(--teal)';
  return (
    <button onClick={onClick} style={{
      display: 'flex', flexDirection: 'column',
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: 10,
      overflow: 'hidden',
      boxShadow: '0 4px 14px rgba(0,0,0,0.3)',
      textAlign: 'left',
      width: expanded ? '100%' : 280,
      transition: 'transform var(--t-mid)',
      cursor: 'pointer',
    }}
    onMouseEnter={e => e.currentTarget.style.transform = 'translateY(-2px)'}
    onMouseLeave={e => e.currentTarget.style.transform = 'translateY(0)'}
    >
      {/* Color band */}
      <div style={{ height: 8, background: accentColor, opacity: 0.85 }} />
      <div style={{ padding: '14px 16px', display: 'flex', gap: 14, alignItems: 'center' }}>
        {/* Big value */}
        <div style={{
          width: 72, height: 72, flexShrink: 0,
          borderRadius: 8,
          background: `${accentColor}18`,
          border: `1px solid ${accentColor}40`,
          display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
        }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, color: accentColor }}>{d.discount}</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', marginTop: 2 }}>{d.value}</span>
        </div>
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 5 }}>
          <span style={{ fontFamily: 'var(--ui)', fontSize: 14, fontWeight: 500, color: 'var(--text-primary)' }}>{d.title}</span>
          <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{d.sustain}</span>
          <div style={{ padding: '4px 8px', background: 'var(--bg-base)', border: '1px dashed var(--border-mid)', borderRadius: 4, marginTop: 2 }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)', letterSpacing: '0.1em' }}>{d.code}</span>
          </div>
        </div>
      </div>
      <div style={{ padding: '8px 16px 12px', display: 'flex', justifyContent: 'space-between', alignItems: 'center', borderTop: '1px solid var(--border)' }}>
        <span className="meta-10" style={{ fontSize: 9, color: 'var(--text-muted)' }}>EXPIRES {d.expiry}</span>
        <div style={{ padding: '3px 8px', background: 'white', borderRadius: 3 }}>
          <QRPlaceholder size={32} color="#111" />
        </div>
      </div>
    </button>
  );
}

/* ── Certificate ─────────────────────────────────────────── */
function CertificateWidget({ data, onClick, expanded = false }) {
  const d = data || {
    title: 'Certificate of Contribution',
    recipient: 'Bonnie Gachiengu',
    issuer: 'Sustena XII · Homestead',
    date: '26 May 2026',
    signatureLabel: 'Orchie · Sustena Council',
  };
  const w = expanded ? '100%' : 240;
  return (
    <button onClick={onClick} style={{
      display: 'block',
      background: 'linear-gradient(160deg, var(--bg-surface) 0%, var(--bg-raised) 100%)',
      border: '1px solid var(--border)',
      borderRadius: 10,
      padding: '28px 24px',
      textAlign: 'center',
      boxShadow: '0 6px 20px rgba(0,0,0,0.4), inset 0 1px 0 rgba(255,255,255,0.03)',
      position: 'relative', overflow: 'hidden',
      width: w,
      cursor: 'pointer',
      transition: 'transform var(--t-mid)',
    }}
    onMouseEnter={e => e.currentTarget.style.transform = 'translateY(-2px)'}
    onMouseLeave={e => e.currentTarget.style.transform = 'translateY(0)'}
    >
      <OrnateBorder w={240} h={200} />
      <div style={{ position: 'relative', zIndex: 1, display: 'flex', flexDirection: 'column', gap: 8, alignItems: 'center' }}>
        <SustenaMark size={26} />
        <span className="label-10" style={{ color: 'var(--text-muted)', fontSize: 8, letterSpacing: '0.14em', marginTop: 4 }}>
          SUSTENA · VERIFIED CREDENTIAL
        </span>
        <div style={{ width: 40, height: 1, background: 'var(--amber)', opacity: 0.4, margin: '4px 0' }} />
        <span style={{ fontFamily: 'var(--ui)', fontSize: 15, fontWeight: 500, color: 'var(--amber)', lineHeight: 1.25 }}>{d.title}</span>
        <span style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-secondary)' }}>awarded to</span>
        <span style={{ fontFamily: 'var(--ui)', fontSize: 16, fontWeight: 500, color: 'var(--text-primary)', letterSpacing: '-0.01em' }}>{d.recipient}</span>
        <span className="meta-10" style={{ color: 'var(--text-muted)', fontSize: 9 }}>{d.issuer}</span>
        <div style={{ width: 40, height: 1, background: 'var(--border)', margin: '6px 0' }} />
        <span className="meta-10" style={{ fontSize: 9, color: 'var(--text-dim)' }}>{d.date}</span>
        {/* Signature line */}
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 2, marginTop: 4 }}>
          <div style={{ width: 64, height: 1, background: 'var(--border-mid)' }} />
          <span className="meta-10" style={{ fontSize: 8, color: 'var(--text-dim)' }}>{d.signatureLabel}</span>
        </div>
      </div>
    </button>
  );
}

/* ── Badge ───────────────────────────────────────────────── */
function BadgeWidget({ data, onClick, expanded = false }) {
  const d = data || {
    label: 'Council Member',
    sublabel: 'Sustena XII',
    icon: '⬡',
    color1: '#c8860a',
    color2: '#e8a020',
    shape: 'hex', // 'circle' | 'hex'
  };
  const size = expanded ? 160 : 100;
  const isHex = d.shape === 'hex';
  const clipPath = isHex
    ? 'polygon(50% 0%, 93% 25%, 93% 75%, 50% 100%, 7% 75%, 7% 25%)'
    : 'none';
  const border = isHex ? 'none' : `2px solid ${d.color2}`;

  return (
    <button onClick={onClick} style={{
      display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 8,
      background: 'transparent', border: 'none', cursor: 'pointer',
      transition: 'transform var(--t-mid)',
    }}
    onMouseEnter={e => e.currentTarget.style.transform = 'scale(1.06)'}
    onMouseLeave={e => e.currentTarget.style.transform = 'scale(1)'}
    >
      {/* Badge disc */}
      <div style={{
        width: size, height: size,
        background: metalGradient(d.color1, d.color2, d.color1),
        clipPath, borderRadius: isHex ? 0 : '50%', border,
        boxShadow: `0 4px 18px rgba(0,0,0,0.4), inset 0 2px 0 rgba(255,255,255,0.15)`,
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        position: 'relative',
      }}>
        {/* Inner ring */}
        <div style={{
          position: 'absolute', inset: 8,
          clipPath, borderRadius: isHex ? 0 : '50%',
          border: '1px solid rgba(255,255,255,0.2)',
        }} />
        <span style={{ fontSize: size * 0.3, lineHeight: 1, filter: 'drop-shadow(0 2px 3px rgba(0,0,0,0.5))' }}>
          {d.icon}
        </span>
      </div>
      <div style={{ textAlign: 'center' }}>
        <div style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.06em', color: 'var(--text-primary)', textTransform: 'uppercase' }}>{d.label}</div>
        {d.sublabel && <div className="meta-10" style={{ fontSize: 9, color: 'var(--text-muted)', marginTop: 2 }}>{d.sublabel}</div>}
      </div>
    </button>
  );
}

/* ── Poster ──────────────────────────────────────────────── */
function PosterWidget({ data, onClick, expanded = false }) {
  const d = data || {
    eventTitle: 'Vyyb Hive',
    subtitle: 'Grand Opening · Phase 1',
    cta: 'Get Tickets',
    date: '14 Jun 2026',
    location: 'Nairobi, KE',
    gradient: 'linear-gradient(160deg, #1a3a0d 0%, #0f1a08 55%, #3b2507 100%)',
  };
  return (
    <button onClick={onClick} style={{
      display: 'flex', flexDirection: 'column', justifyContent: 'flex-end',
      background: d.gradient,
      border: '1px solid var(--border)',
      borderRadius: 12,
      padding: '20px 18px',
      width: expanded ? '100%' : 200,
      minHeight: expanded ? 340 : 280,
      position: 'relative', overflow: 'hidden',
      textAlign: 'left',
      cursor: 'pointer',
      boxShadow: '0 6px 20px rgba(0,0,0,0.45)',
      transition: 'transform var(--t-mid)',
    }}
    onMouseEnter={e => e.currentTarget.style.transform = 'translateY(-2px)'}
    onMouseLeave={e => e.currentTarget.style.transform = 'translateY(0)'}
    >
      {/* Watermark */}
      <div style={{ position: 'absolute', top: 14, right: 14, opacity: 0.15 }}>
        <SustenaMark size={40} primary="#e8a020" secondary="#e8a020" />
      </div>
      {/* Ambient glow */}
      <div style={{
        position: 'absolute', top: 0, left: 0, right: 0, height: '60%',
        background: 'radial-gradient(ellipse at 50% 0%, rgba(232,160,32,0.12), transparent 70%)',
        pointerEvents: 'none',
      }} />
      {/* Content */}
      <div style={{ position: 'relative', zIndex: 1 }}>
        <div style={{ display: 'flex', gap: 8, marginBottom: 10 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'rgba(200,220,180,0.5)', letterSpacing: '0.08em' }}>{d.date}</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'rgba(200,220,180,0.4)' }}>·</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'rgba(200,220,180,0.5)', letterSpacing: '0.06em' }}>{d.location}</span>
        </div>
        <h2 style={{ fontFamily: 'var(--ui)', fontSize: expanded ? 36 : 28, fontWeight: 600, letterSpacing: '-0.02em', color: '#e8f0d8', lineHeight: 1.05, marginBottom: 6 }}>
          {d.eventTitle}
        </h2>
        <p style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'rgba(200,220,180,0.6)', marginBottom: 16, lineHeight: 1.4 }}>{d.subtitle}</p>
        <div style={{
          display: 'inline-flex', padding: '8px 16px',
          background: 'var(--amber)', borderRadius: 20,
          fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, letterSpacing: '0.06em',
          color: 'var(--bg-base)',
        }}>{d.cta}</div>
      </div>
    </button>
  );
}

/* ── Thumbnail/Button ────────────────────────────────────── */
function ThumbnailWidget({ data, onClick, expanded = false }) {
  const d = data || {
    label: 'Mkulima Alpha',
    sub: 'Sustain Profile',
    emoji: '🌱',
    color: 'var(--ok)',
  };
  const s = expanded ? 160 : 80;
  return (
    <button onClick={onClick} style={{
      display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 6,
      background: 'var(--bg-surface)', border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)', padding: '12px',
      cursor: 'pointer', width: expanded ? '100%' : s + 24,
      transition: 'all var(--t-mid)',
    }}
    onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.background = 'var(--bg-raised)'; }}
    onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border)'; e.currentTarget.style.background = 'var(--bg-surface)'; }}
    >
      <div style={{
        width: s, height: s, borderRadius: 'var(--radius-md)',
        background: `${d.color}18`,
        border: `1px solid ${d.color}30`,
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        fontSize: s * 0.4,
      }}>{d.emoji}</div>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, color: 'var(--text-primary)', letterSpacing: '0.04em', textAlign: 'center' }}>{d.label}</span>
      {d.sub && <span className="meta-10" style={{ fontSize: 9, color: 'var(--text-muted)' }}>{d.sub}</span>}
    </button>
  );
}

/* ═══════════════════════════════════════════════════════════
   MODAL — slides up from bottom
   ═══════════════════════════════════════════════════════════ */
function ResponsiveWidgetModal({ widget, type, data, onClose }) {
  const [copied, setCopied] = wUseState(false);

  const handleShare = () => {
    setCopied(true);
    window.flash?.('Share link copied', 'ok');
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <>
      {/* Backdrop */}
      <div onClick={onClose} style={{
        position: 'fixed', inset: 0, zIndex: 800,
        background: 'rgba(0,0,0,0.65)',
        animation: 'fadeUp 0.15s ease-out',
      }} />
      {/* Modal panel */}
      <div style={{
        position: 'fixed', left: 0, right: 0, bottom: 0, zIndex: 801,
        background: 'var(--bg-surface)',
        borderTop: '1px solid var(--border-mid)',
        borderRadius: '14px 14px 0 0',
        maxHeight: '85vh',
        overflow: 'auto',
        boxShadow: '0 -12px 40px rgba(0,0,0,0.5)',
        animation: 'widgetSlideUp 0.3s cubic-bezier(0.22, 0.61, 0.36, 1)',
      }}>
        <style>{`
          @keyframes widgetSlideUp {
            from { transform: translateY(100%); }
            to   { transform: translateY(0); }
          }
        `}</style>

        {/* Handle bar */}
        <div style={{ display: 'flex', justifyContent: 'center', padding: '10px 0 0' }}>
          <div style={{ width: 36, height: 4, borderRadius: 2, background: 'var(--border-mid)' }} />
        </div>

        {/* Header */}
        <div style={{ padding: '14px 20px 10px', display: 'flex', alignItems: 'center', justifyContent: 'space-between', borderBottom: '1px solid var(--border)' }}>
          <span className="label-11" style={{ color: 'var(--text-secondary)' }}>
            {type?.toUpperCase() || 'WIDGET'} · EXPANDED
          </span>
          <div style={{ display: 'flex', gap: 8 }}>
            <button onClick={handleShare} style={{
              padding: '5px 12px',
              fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.06em', textTransform: 'uppercase',
              color: copied ? 'var(--ok)' : 'var(--text-secondary)',
              background: 'transparent', border: `1px solid ${copied ? 'var(--ok)' : 'var(--border-mid)'}`,
              borderRadius: 'var(--radius-sm)', transition: 'all var(--t-fast)',
            }}>
              {copied ? '✓ COPIED' : 'SHARE'}
            </button>
            <button onClick={() => window.flash?.('Download — coming in v2', 'info')} style={{
              padding: '5px 12px',
              fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.06em', textTransform: 'uppercase',
              color: 'var(--text-secondary)', background: 'transparent',
              border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
              transition: 'all var(--t-fast)',
            }}
            onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
            onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border-mid)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
            >
              DOWNLOAD
            </button>
            <button onClick={onClose} style={{ color: 'var(--text-muted)', padding: 4 }}><Icon name="x" size={13} /></button>
          </div>
        </div>

        {/* Expanded widget */}
        <div style={{ padding: '28px 20px 40px', display: 'flex', justifyContent: 'center' }}>
          <div style={{ maxWidth: 560, width: '100%' }}>
            {widget}
          </div>
        </div>
      </div>
    </>
  );
}

/* ═══════════════════════════════════════════════════════════
   WIDGET GALLERY — demo page component
   ═══════════════════════════════════════════════════════════ */
function WidgetGallery() {
  const [modal, setModal] = wUseState(null); // { type, data }

  const open = (type, data, expandedWidget) => setModal({ type, data, expandedWidget });
  const close = () => setModal(null);

  const airData    = undefined; // use defaults
  const vouchData  = undefined;
  const certData   = undefined;
  const badgeData  = undefined;
  const posterData = undefined;
  const thumbData  = undefined;

  return (
    <>
      <div style={{ padding: '32px 28px', display: 'flex', flexDirection: 'column', gap: 40 }}>
        <div>
          <h1 style={{ fontFamily: 'var(--mono)', fontSize: 11, letterSpacing: '0.1em', color: 'var(--text-muted)', textTransform: 'uppercase', marginBottom: 4 }}>
            DIGITAL ASSETS · WIDGET GALLERY
          </h1>
          <p style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-secondary)' }}>Click any widget to expand.</p>
        </div>

        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 20, alignItems: 'flex-start' }}>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            <span className="meta-10" style={{ color: 'var(--text-dim)' }}>AIR TICKET</span>
            <AirTicketWidget data={airData} onClick={() => open('air-ticket', airData, <AirTicketWidget data={airData} expanded />)} />
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            <span className="meta-10" style={{ color: 'var(--text-dim)' }}>VOUCHER</span>
            <VoucherWidget data={vouchData} onClick={() => open('voucher', vouchData, <VoucherWidget data={vouchData} expanded />)} />
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            <span className="meta-10" style={{ color: 'var(--text-dim)' }}>CERTIFICATE</span>
            <CertificateWidget data={certData} onClick={() => open('certificate', certData, <CertificateWidget data={certData} expanded />)} />
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            <span className="meta-10" style={{ color: 'var(--text-dim)' }}>BADGE</span>
            <BadgeWidget data={badgeData} onClick={() => open('badge', badgeData, <BadgeWidget data={badgeData} expanded size={160} />)} />
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            <span className="meta-10" style={{ color: 'var(--text-dim)' }}>POSTER</span>
            <PosterWidget data={posterData} onClick={() => open('poster', posterData, <PosterWidget data={posterData} expanded />)} />
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            <span className="meta-10" style={{ color: 'var(--text-dim)' }}>THUMBNAIL</span>
            <ThumbnailWidget data={thumbData} onClick={() => open('thumbnail', thumbData, <ThumbnailWidget data={thumbData} expanded />)} />
          </div>
        </div>
      </div>

      {modal && (
        <ResponsiveWidgetModal
          type={modal.type}
          data={modal.data}
          widget={modal.expandedWidget}
          onClose={close}
        />
      )}
    </>
  );
}

Object.assign(window, {
  AirTicketWidget, VoucherWidget, CertificateWidget,
  BadgeWidget, PosterWidget, ThumbnailWidget,
  ResponsiveWidgetModal, WidgetGallery,
  QRPlaceholder, PerforatedEdge, OrnateBorder,
});
