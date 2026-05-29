/* Sustena — core primitives: logo, icons, frames, badges, hero readouts */

const { useState, useEffect, useRef, useMemo } = React;

/* ───────────────────────────────────────────────────────────
   LOGO — network node (center disc + 3 branching arcs)
   ─────────────────────────────────────────────────────────── */
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

function SustenaLogo({ size = 16, gap = 8 }) {
  return (
    <div style={{ display: 'inline-flex', alignItems: 'center', gap }}>
      <SustenaMark size={size} />
      <span style={{
        fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 500,
        letterSpacing: '0.08em', textTransform: 'uppercase',
        color: 'var(--text-primary)', lineHeight: 1,
      }}>
        <span style={{ color: 'var(--amber)' }}>S</span>USTENA
      </span>
    </div>
  );
}

/* ───────────────────────────────────────────────────────────
   ICONS — line set
   ─────────────────────────────────────────────────────────── */
function Icon({ name, size = 14 }) {
  const s = { width: size, height: size, fill: 'none', stroke: 'currentColor', strokeWidth: 1.4, strokeLinecap: 'round', strokeLinejoin: 'round' };
  switch (name) {
    case 'pulse':    return <svg {...s} viewBox="0 0 16 16"><path d="M1 8h3l2-5 3 10 2-5h4" /></svg>;
    case 'ledger':   return <svg {...s} viewBox="0 0 16 16"><rect x="3" y="2" width="10" height="12" /><path d="M5 5h6M5 8h6M5 11h4" /></svg>;
    case 'council':  return <svg {...s} viewBox="0 0 16 16"><circle cx="5" cy="5.5" r="1.6" /><circle cx="11" cy="5.5" r="1.6" /><path d="M2 13c0-2 1.4-3.5 3-3.5s3 1.5 3 3.5" /><path d="M8 13c0-2 1.4-3.5 3-3.5s3 1.5 3 3.5" /></svg>;
    case 'leaf':     return <svg {...s} viewBox="0 0 16 16"><path d="M3 13c0-6 4-10 10-10 0 6-4 10-10 10z" /><path d="M3 13c2-2 4-4 7-6" /></svg>;
    case 'agent':    return <svg {...s} viewBox="0 0 16 16"><rect x="3" y="4" width="10" height="9" rx="1.5" /><circle cx="6.5" cy="8.5" r="0.8" fill="currentColor" stroke="none" /><circle cx="9.5" cy="8.5" r="0.8" fill="currentColor" stroke="none" /><path d="M8 4V2M6 2h4" /></svg>;
    case 'vault':    return <svg {...s} viewBox="0 0 16 16"><rect x="2" y="3" width="12" height="10" /><circle cx="8" cy="8" r="2.5" /><path d="M8 5.5v0.5M8 10v0.5M5.5 8h0.5M10 8h0.5" /></svg>;
    case 'settings': return <svg {...s} viewBox="0 0 16 16"><circle cx="8" cy="8" r="2" /><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3 3l1.5 1.5M11.5 11.5L13 13M3 13l1.5-1.5M11.5 4.5L13 3" /></svg>;
    case 'send':     return <svg {...s} viewBox="0 0 16 16"><path d="M14 2L2 7l5 2 2 5z" /></svg>;
    case 'arrow-up':   return <svg {...s} viewBox="0 0 16 16"><path d="M8 13V3M4 7l4-4 4 4" /></svg>;
    case 'arrow-down': return <svg {...s} viewBox="0 0 16 16"><path d="M8 3v10M4 9l4 4 4-4" /></svg>;
    case 'chevron':  return <svg {...s} viewBox="0 0 16 16"><path d="M6 4l4 4-4 4" /></svg>;
    case 'check':    return <svg {...s} viewBox="0 0 16 16"><path d="M3 8.5L6.5 12 13 4.5" /></svg>;
    case 'x':        return <svg {...s} viewBox="0 0 16 16"><path d="M4 4l8 8M12 4l-8 8" /></svg>;
    case 'plus':     return <svg {...s} viewBox="0 0 16 16"><path d="M8 3v10M3 8h10" /></svg>;
    case 'expand':   return <svg {...s} viewBox="0 0 16 16"><path d="M3 6V3h3M13 6V3h-3M3 10v3h3M13 10v3h-3" /></svg>;
    case 'replay':   return <svg {...s} viewBox="0 0 16 16"><path d="M3 8a5 5 0 1 0 5-5" /><path d="M3 3v3h3" /></svg>;
    case 'crosshair': return <svg {...s} viewBox="0 0 16 16"><circle cx="8" cy="8" r="3" /><path d="M8 1v3M8 12v3M1 8h3M12 8h3" /></svg>;
    case 'warning':  return <svg {...s} viewBox="0 0 16 16"><path d="M8 2l6.5 11h-13z" /><path d="M8 6v3.5M8 11.5v0.5" /></svg>;
    case 'dot':      return <svg {...s} viewBox="0 0 16 16"><circle cx="8" cy="8" r="2.5" fill="currentColor" stroke="none" /></svg>;
    default: return null;
  }
}

/* ───────────────────────────────────────────────────────────
   BADGE — status pill
   ─────────────────────────────────────────────────────────── */
function Badge({ tone = 'muted', dot, children }) {
  return (
    <span className={`badge badge-${tone}`}>
      {dot && <span className="badge-dot" />}
      {children}
    </span>
  );
}

/* ───────────────────────────────────────────────────────────
   FRAME — bracket-cornered container
   ─────────────────────────────────────────────────────────── */
function Frame({ children, amber, style, label, sub }) {
  return (
    <div className={`frame ${amber ? 'frame--amber' : ''}`} style={style}>
      {label && (
        <div style={{ position: 'absolute', top: -7, left: 14, background: 'var(--bg-base)', padding: '0 6px', display: 'flex', gap: 8, alignItems: 'baseline' }}>
          <span className="label-11">{label}</span>
          {sub && <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{sub}</span>}
        </div>
      )}
      <span className="frame-corner-bl" />
      <span className="frame-corner-br" />
      {children}
    </div>
  );
}

/* ───────────────────────────────────────────────────────────
   CARD — bg-surface card with header
   ─────────────────────────────────────────────────────────── */
function Card({ title, sub, actions, children, padded = true, scroll, style }) {
  return (
    <section style={{
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)',
      display: 'flex', flexDirection: 'column',
      overflow: 'hidden',
      minHeight: 0,
      ...style,
    }}>
      {title && (
        <header style={{
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          padding: '10px 14px',
          borderBottom: '1px solid var(--border)',
          flexShrink: 0,
          minHeight: 38,
        }}>
          <div style={{ display: 'flex', alignItems: 'baseline', gap: 10, minWidth: 0 }}>
            <span className="label-11">{title}</span>
            {sub && <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{sub}</span>}
          </div>
          {actions}
        </header>
      )}
      <div style={{ flex: 1, minHeight: 0, padding: padded ? 14 : 0, overflow: scroll ? 'auto' : 'visible' }}>
        {children}
      </div>
    </section>
  );
}

/* ───────────────────────────────────────────────────────────
   HERO READOUT — the big number
   ─────────────────────────────────────────────────────────── */
function Hero({ label, value, unit, delta, sub, accent, prefix }) {
  const tone = accent || 'var(--text-primary)';
  return (
    <div className="hero-settle" style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      <div className="label-10" style={{ color: 'var(--text-muted)' }}>{label}</div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 12, flexWrap: 'wrap' }}>
        {prefix && (
          <span style={{
            fontFamily: 'var(--mono)', fontSize: 28, fontWeight: 400,
            color: 'var(--text-muted)', alignSelf: 'flex-start', paddingTop: 6,
          }}>{prefix}</span>
        )}
        <span className="val-hero" style={{ color: tone }}>{value}</span>
        {unit && (
          <span style={{
            fontFamily: 'var(--mono)', fontSize: 22, fontWeight: 400,
            color: 'var(--text-secondary)', alignSelf: 'flex-end', paddingBottom: 16,
          }}>{unit}</span>
        )}
      </div>
      {(delta || sub) && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 14, marginTop: 4 }}>
          {delta && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 4, color: delta.positive ? 'var(--teal)' : 'var(--danger)' }}>
              <Icon name={delta.positive ? 'arrow-up' : 'arrow-down'} size={12} />
              <span className="val-12" style={{ color: 'inherit' }}>{delta.value}</span>
              <span className="meta-11" style={{ color: 'var(--text-muted)' }}>{delta.label}</span>
            </div>
          )}
          {sub && <span className="meta-11" style={{ color: 'var(--text-secondary)' }}>{sub}</span>}
        </div>
      )}
    </div>
  );
}

/* ───────────────────────────────────────────────────────────
   PULSE TICK — tiny live number with flash
   ─────────────────────────────────────────────────────────── */
function LiveNum({ value, prev }) {
  const [flash, setFlash] = useState(false);
  useEffect(() => {
    if (prev != null && prev !== value) {
      setFlash(true);
      const t = setTimeout(() => setFlash(false), 600);
      return () => clearTimeout(t);
    }
  }, [value]);
  return (
    <span className={`tick-num ${flash ? 'tick-pulse' : ''}`}>{value}</span>
  );
}

/* ───────────────────────────────────────────────────────────
   SPARKLINE
   ─────────────────────────────────────────────────────────── */
function Spark({ series, w = 100, h = 28, color = 'var(--text-secondary)', dot = true, animate = true }) {
  const min = Math.min(...series);
  const max = Math.max(...series);
  const range = max - min || 1;
  const pts = series.map((v, i) => {
    const x = (i / (series.length - 1)) * w;
    const y = h - ((v - min) / range) * h;
    return `${x},${y}`;
  }).join(' ');
  const last = series[series.length - 1];
  const lastY = h - ((last - min) / range) * h;
  return (
    <svg width={w} height={h} style={{ overflow: 'visible' }}>
      <polyline
        points={pts} fill="none" stroke={color} strokeWidth="1.2"
        className={animate ? 'draw-line' : undefined}
        style={{ '--dash-len': w * 3 }}
      />
      {dot && <circle cx={w} cy={lastY} r="2" fill={color} />}
    </svg>
  );
}

/* ───────────────────────────────────────────────────────────
   AREA CHART
   ─────────────────────────────────────────────────────────── */
function AreaChart({ series, w = 600, h = 160, color = 'var(--amber)', fill = 'var(--amber-glow)', animate = true, axis = true }) {
  if (!series || series.length < 2) return null;
  const min = 0;
  const max = Math.max(...series) * 1.1;
  const range = max - min || 1;
  const pts = series.map((v, i) => {
    const x = (i / (series.length - 1)) * w;
    const y = h - ((v - min) / range) * h;
    return [x, y];
  });
  const linePath = pts.map(([x, y], i) => `${i === 0 ? 'M' : 'L'} ${x} ${y}`).join(' ');
  const areaPath = `M 0 ${h} ` + pts.map(([x, y]) => `L ${x} ${y}`).join(' ') + ` L ${w} ${h} Z`;
  return (
    <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" width="100%" height="100%" style={{ display: 'block', overflow: 'visible' }}>
      {axis && [0.25, 0.5, 0.75].map(p => (
        <line key={p} x1="0" y1={h * p} x2={w} y2={h * p} stroke="var(--border)" strokeDasharray="2 4" strokeWidth="0.5" />
      ))}
      <path d={areaPath} fill={fill} className={animate ? 'fade-up' : undefined} style={{ animationDelay: '0.6s', animationFillMode: 'both', opacity: animate ? 0 : 1, ...(animate ? { animation: 'fadeUp 0.6s ease-out 0.6s forwards' } : {}) }} />
      <path d={linePath} fill="none" stroke={color} strokeWidth="1.4"
        className={animate ? 'draw-line' : undefined}
        style={{ '--dash-len': w * 3 }}
      />
      {/* End marker */}
      <circle
        cx={pts[pts.length - 1][0]} cy={pts[pts.length - 1][1]} r="3"
        fill={color}
        style={{ opacity: animate ? 0 : 1, animation: animate ? 'fadeUp 0.3s ease-out 1.4s forwards' : undefined }}
      />
    </svg>
  );
}

/* ───────────────────────────────────────────────────────────
   PERSONA HEADER
   ─────────────────────────────────────────────────────────── */
function PersonaHeader({ persona, cycle }) {
  return (
    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-end', marginBottom: 28 }}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <span className="label-10" style={{ color: 'var(--text-muted)' }}>{persona.role}</span>
          <span style={{ width: 4, height: 4, borderRadius: '50%', background: 'var(--text-dim)' }} />
          <span className="label-10" style={{ color: 'var(--text-muted)' }}>{persona.context}</span>
          <span style={{ width: 4, height: 4, borderRadius: '50%', background: 'var(--text-dim)' }} />
          <span className="label-10" style={{ color: 'var(--text-muted)' }}>{persona.location} · {persona.coords}</span>
        </div>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 16 }}>
          <span style={{ fontFamily: 'var(--ui)', fontSize: 22, fontWeight: 500, letterSpacing: '-0.01em', color: 'var(--text-primary)' }}>
            {persona.name}
          </span>
          <span className="meta-11" style={{ color: 'var(--text-muted)' }}>
            ↳ {persona.org}
          </span>
        </div>
      </div>
      {cycle && (
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-end', gap: 4 }}>
          <span className="label-10">{cycle.label}</span>
          <span className="val-12" style={{ fontSize: 13 }}>{cycle.value}</span>
        </div>
      )}
    </div>
  );
}

/* ───────────────────────────────────────────────────────────
   TOOLBAR BUTTON
   ─────────────────────────────────────────────────────────── */
function TBtn({ children, active, onClick }) {
  const [hover, setHover] = useState(false);
  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{
        padding: '3px 8px',
        fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500,
        letterSpacing: '0.08em', textTransform: 'uppercase',
        color: active ? 'var(--amber)' : (hover ? 'var(--text-primary)' : 'var(--text-muted)'),
        border: '1px solid ' + (active ? 'var(--amber-border)' : 'var(--border)'),
        background: active ? 'var(--amber-glow)' : 'transparent',
        borderRadius: 'var(--radius-sm)',
        transition: 'all var(--t-fast)',
      }}>
      {children}
    </button>
  );
}

/* Primary action button */
function PBtn({ children, onClick, variant = 'primary' }) {
  const [hover, setHover] = useState(false);
  const primary = variant === 'primary';
  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{
        padding: '6px 14px',
        fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500,
        letterSpacing: '0.08em', textTransform: 'uppercase',
        color: primary ? 'var(--bg-base)' : (hover ? 'var(--text-primary)' : 'var(--text-secondary)'),
        background: primary ? (hover ? '#f5b030' : 'var(--amber)') : (hover ? 'var(--bg-raised)' : 'transparent'),
        border: '1px solid ' + (primary ? 'var(--amber)' : 'var(--border-mid)'),
        borderRadius: 'var(--radius-sm)',
        transition: 'all var(--t-fast)',
        display: 'inline-flex', alignItems: 'center', gap: 6,
      }}>
      {children}
    </button>
  );
}

/* ───────────────────────────────────────────────────────────
   EMPTY / LOADING / ERROR STATES
   ─────────────────────────────────────────────────────────── */
function Loading({ label = 'STREAMING' }) {
  return (
    <div style={{ height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center', flexDirection: 'column', gap: 12 }}>
      <div style={{ display: 'flex', gap: 5 }}>
        {[0, 1, 2, 3, 4].map(i => (
          <span key={i} style={{
            width: 6, height: 6, borderRadius: '50%', background: 'var(--amber)',
            animation: `pulse 1.2s ease-in-out infinite`,
            animationDelay: `${i * 0.15}s`,
          }} />
        ))}
      </div>
      <span className="label-10" style={{ color: 'var(--text-muted)' }}>{label}<span className="blink">_</span></span>
    </div>
  );
}

function Empty({ label = 'NO SIGNAL', sub, action }) {
  return (
    <div style={{ height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center', flexDirection: 'column', gap: 14, padding: 24 }}>
      <pre style={{
        fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-dim)', lineHeight: 1.3, margin: 0,
        textAlign: 'center',
      }}>{`╭─ · · · ─╮
│ ∙       ∙ │
│           │
│ ∙       ∙ │
╰─ · · · ─╯`}</pre>
      <span className="label-11" style={{ color: 'var(--text-secondary)' }}>{label}</span>
      {sub && <span className="meta-11" style={{ color: 'var(--text-muted)', textAlign: 'center', maxWidth: 280 }}>{sub}</span>}
      {action}
    </div>
  );
}

function ErrorState({ label = 'STREAM DEGRADED', code = 'ERR-0427', sub, onRetry }) {
  return (
    <div style={{
      height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center', flexDirection: 'column', gap: 12, padding: 20,
      borderTop: '1px solid var(--danger)', borderBottom: '1px solid var(--danger)',
      background: 'rgba(224,80,80,0.04)',
    }}>
      <div style={{ color: 'var(--danger)' }}>
        <Icon name="warning" size={20} />
      </div>
      <span className="label-11" style={{ color: 'var(--danger)' }}>{label}</span>
      <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{code}</span>
      {sub && <span className="meta-11" style={{ color: 'var(--text-secondary)', textAlign: 'center', maxWidth: 320 }}>{sub}</span>}
      {onRetry && <PBtn variant="ghost" onClick={onRetry}><Icon name="replay" size={11}/> RETRY</PBtn>}
    </div>
  );
}

/* ───────────────────────────────────────────────────────────
   Export
   ─────────────────────────────────────────────────────────── */
Object.assign(window, {
  SustenaMark, SustenaLogo, Icon, Badge, Frame, Card,
  Hero, LiveNum, Spark, AreaChart,
  PersonaHeader, TBtn, PBtn,
  Loading, Empty, ErrorState,
});
