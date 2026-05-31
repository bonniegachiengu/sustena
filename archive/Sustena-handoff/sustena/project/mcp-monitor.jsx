/* Monitor panel — live observability. Two columns: live state stream + event log. Operative cards below. */

const { useState: dUseState, useEffect: dUseEffect, useMemo: dUseMemo, useRef: dUseRef } = React;

function MonitorPanel({ tick, sustain }) {
  // Animated state values that occasionally tick
  const stateVals = dUseMemo(() => STATE_TREE.map((s, i) => ({
    ...s,
    live: s.value + (s.fmt === 'ksh' ? Math.sin(tick * 0.1 + i) * (s.value * 0.001) : 0),
  })), [tick]);

  // Synthetic event stream — accumulate
  const [logEntries, setLogEntries] = dUseState(() =>
    Array.from({ length: 8 }).map((_, i) => pickEvent(i + 1000))
  );
  dUseEffect(() => {
    if (tick === 0) return;
    if (tick % 3 === 0) {
      const next = pickEvent(tick + 4321);
      setLogEntries(prev => [next, ...prev].slice(0, 24));
    }
  }, [tick]);

  return (
    <div className="panel-enter" style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: 18, padding: '24px 28px' }}>
      {/* Hero strip: 4 sparse live metrics */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 14 }}>
        <HeroTile label="ACTIVE SUSTAINS" value={SUSTAINS.filter(s => s.status === 'live').length} sub="of 5 in scope" tone="ok" />
        <HeroTile label="OPERATORS / MIN" value={Math.round(218 + Math.sin(tick * 0.2) * 14)} live sub="rolling 60s" />
        <HeroTile label="API P95" value={`${Math.round(412 + Math.sin(tick * 0.15) * 18)}`} unit="ms" live sub="haiku · inference" tone={tick % 17 < 3 ? 'amber' : 'default'} />
        <HeroTile label="PAWA BURN" value={Math.round(28 + Math.sin(tick * 0.18) * 4)} unit="/hr" sub={`bal ${8420 - tick % 60}`} tone="default" />
      </div>

      {/* 2-col main: state stream + event log */}
      <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1fr) minmax(0, 1.3fr)', gap: 14, flex: 1, minHeight: 0 }}>
        {/* Live State Stream */}
        <Card title="LIVE STATE STREAM" sub={`${sustain.label}`} padded={false} scroll
          actions={
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <span className="pulse" style={{ width: 5, height: 5, borderRadius: '50%', background: 'var(--teal)' }} />
              <span className="meta-10" style={{ color: 'var(--teal)' }}>SYNC · 28ms</span>
            </div>
          }
        >
          {stateVals.map((s, i) => <StateRow key={s.path} s={s} delay={i * 30} />)}
        </Card>

        {/* Event Log Stream */}
        <Card title="EVENT LOG · STREAM" sub={`LAST ${logEntries.length} · ALL SUSTAINS`} padded={false} scroll
          actions={
            <div style={{ display: 'flex', gap: 6 }}>
              <TBtn active>ALL</TBtn>
              <TBtn>OPS</TBtn>
              <TBtn>AGENT</TBtn>
              <TBtn>CSTR</TBtn>
            </div>
          }
        >
          {logEntries.map((e, i) => <LogRow key={e.key} e={e} first={i === 0} />)}
        </Card>
      </div>

      {/* Operative activity strip */}
      <div>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 8 }}>
          <span className="label-11">OPERATIVE ACTIVITY</span>
          <span className="meta-10">{OPERATIVES.filter(o => o.status === 'active').length} ACTIVE · {OPERATIVES.filter(o => o.status === 'alert').length} ALERT</span>
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12 }}>
          {OPERATIVES.map((o, i) => <OperativeCard key={o.id} o={o} delay={i * 60} tick={tick} />)}
        </div>
      </div>
    </div>
  );
}

function HeroTile({ label, value, unit, sub, tone, live }) {
  const c = tone === 'ok' ? 'var(--teal)' : tone === 'amber' ? 'var(--amber)' : 'var(--text-primary)';
  return (
    <div className="fade-up" style={{
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)',
      padding: '14px 16px',
      display: 'flex', flexDirection: 'column', gap: 6,
      position: 'relative', overflow: 'hidden',
    }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <span className="label-10">{label}</span>
        {live && <span className="pulse" style={{ width: 4, height: 4, borderRadius: '50%', background: 'var(--teal)' }} />}
      </div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 6 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 28, fontWeight: 500, color: c, letterSpacing: '-0.01em' }}>{value}</span>
        {unit && <span className="meta-10">{unit}</span>}
      </div>
      <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{sub}</span>
    </div>
  );
}

function StateRow({ s, delay }) {
  const c = s.cstr === 'ok' ? 'var(--ok)' : s.cstr === 'amber' ? 'var(--amber)' : 'var(--danger)';
  const fmtVal = (v) => {
    if (s.fmt === 'ksh') return Math.round(v).toLocaleString();
    if (s.fmt === '%') return v.toFixed(0);
    if (s.fmt === 'L' || s.fmt === 'kg') return v.toFixed(1);
    return Math.round(v).toLocaleString();
  };
  return (
    <div className="fade-up" style={{
      display: 'grid', gridTemplateColumns: '8px 1fr auto',
      gap: 12, padding: '10px 16px',
      borderBottom: '1px solid var(--border)',
      animationDelay: `${delay}ms`,
      alignItems: 'center',
    }}>
      <span style={{ width: 6, height: 6, borderRadius: '50%', background: c, boxShadow: s.cstr === 'red' ? '0 0 6px var(--danger)' : 'none' }} />
      <div style={{ display: 'flex', flexDirection: 'column', minWidth: 0 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{s.path}</span>
        {s.desc && <span className="meta-10" style={{ color: 'var(--text-muted)', fontSize: 9 }}>{s.desc}</span>}
      </div>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 500, color: 'var(--text-primary)' }}>
        {fmtVal(s.live)} <span style={{ color: 'var(--text-muted)', fontWeight: 400, fontSize: 11 }}>{s.fmt}</span>
      </span>
    </div>
  );
}

function LogRow({ e, first }) {
  const c = e.tone === 'ok' ? 'var(--ok)' : e.tone === 'amber' ? 'var(--amber)' : e.tone === 'danger' ? 'var(--danger)' : 'var(--teal)';
  return (
    <div className={first ? 'log-in' : ''} style={{
      display: 'grid', gridTemplateColumns: '70px 6px 1.2fr 1.6fr 90px',
      gap: 10, padding: '8px 16px',
      borderBottom: '1px solid var(--border)',
      alignItems: 'center',
      fontFamily: 'var(--mono)', fontSize: 11,
    }}>
      <span style={{ color: 'var(--text-muted)' }}>{e.time}</span>
      <span style={{ width: 4, height: 4, borderRadius: '50%', background: c }} />
      <span style={{ color: 'var(--text-secondary)' }}>{e.sustain}</span>
      <span style={{ color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
        <span style={{ color: 'var(--amber)' }}>{e.operator}</span> <span style={{ color: 'var(--text-muted)' }}>→</span> {e.delta}
      </span>
      <span style={{ color: c, textAlign: 'right' }}>{e.op}</span>
    </div>
  );
}

function OperativeCard({ o, delay, tick }) {
  const statusColor = o.status === 'active' ? 'var(--teal)' : o.status === 'alert' ? 'var(--amber)' : 'var(--text-muted)';
  // Animate the "in progress" subtask
  const inProg = o.subtasks.find(t => t.progress > 0 && t.progress < 100);
  const animatedProgress = inProg ? Math.min(100, inProg.progress + ((tick % 20) * 0.5)) : null;
  // Live confidence drift
  const liveConfidence = Math.max(0, Math.min(100, o.confidence + Math.sin(tick * 0.18 + o.id.charCodeAt(0)) * 3));
  // Activity sparkline — synthesize from tick
  const series = dUseMemo(() => {
    const N = 16;
    return Array.from({ length: N }).map((_, i) => {
      const base = o.status === 'idle' ? 8 : o.status === 'alert' ? 70 : 45;
      return base + Math.sin((tick + i * 7) * 0.3 + o.id.charCodeAt(0)) * 18 + (Math.sin(i * 11.3) * 0.5 + 0.5) * 8;
    });
  }, [tick]);

  return (
    <button onClick={() => window.confirmAction?.({
      title: `${o.name} · ${o.role}`,
      body: `${o.task}. Confidence ${o.confidence}%. Pawa burn ${o.pawa} / session. ${o.subtasks.length} subtasks tracked.`,
      detail: o.subtasks.map(s => `${s.done ? '✓' : s.progress > 0 ? '⟳' : '○'} ${s.name}${s.progress > 0 && s.progress < 100 ? ` · ${s.progress}%` : ''}`).join('\n'),
      ctaLabel: o.status === 'idle' ? 'WAKE' : 'PAUSE',
      tone: 'amber',
      onConfirm: () => window.flash?.(`${o.name} ${o.status === 'idle' ? 'woken' : 'paused'}`, 'info'),
    })} className="fade-up lift" style={{
      textAlign: 'left',
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)',
      padding: '12px 14px',
      display: 'flex', flexDirection: 'column', gap: 10,
      animationDelay: `${delay}ms`,
      position: 'relative',
      overflow: 'hidden',
    }}>
      {/* Status accent bar */}
      <span style={{
        position: 'absolute', top: 0, left: 0, right: 0, height: 2,
        background: statusColor,
        opacity: o.status === 'idle' ? 0.4 : 1,
      }} />
      {/* Live pulse for alert state */}
      {o.status === 'alert' && (
        <span style={{
          position: 'absolute', top: 0, left: 0, right: 0, height: 2,
          background: statusColor,
          animation: 'pulse 1.4s ease-in-out infinite',
          filter: 'blur(2px)',
        }} />
      )}

      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span className="pulse" style={{
            width: 6, height: 6, borderRadius: '50%', background: statusColor,
            boxShadow: o.status !== 'idle' ? `0 0 6px ${statusColor}` : 'none',
          }} />
          <span style={{ fontFamily: 'var(--ui)', fontSize: 13, fontWeight: 500 }}>{o.name}</span>
          <Badge tone={o.status === 'active' ? 'teal' : o.status === 'alert' ? 'amber' : 'muted'}>{o.status}</Badge>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, color: 'var(--text-primary)' }}>
            {Math.round(liveConfidence)}
          </span>
          <span className="meta-10">%</span>
        </div>
      </div>
      <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{o.role}</span>

      {/* Activity sparkline */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <svg width="100%" height="22" viewBox="0 0 120 22" preserveAspectRatio="none" style={{ flex: 1, overflow: 'visible' }}>
          <polyline
            points={series.map((v, i) => `${(i / (series.length - 1)) * 120},${22 - (v / 100) * 22}`).join(' ')}
            fill="none" stroke={statusColor} strokeWidth="1"
            opacity={o.status === 'idle' ? 0.4 : 0.85}
          />
          <circle
            cx={120} cy={22 - (series[series.length - 1] / 100) * 22}
            r="2" fill={statusColor}
            style={{ filter: o.status !== 'idle' ? `drop-shadow(0 0 3px ${statusColor})` : 'none' }}
          />
        </svg>
        <span className="meta-10" style={{ fontSize: 9, color: statusColor, minWidth: 30, textAlign: 'right' }}>
          {Math.round(series[series.length - 1])} ops/s
        </span>
      </div>

      <span style={{ fontSize: 12, color: 'var(--text-secondary)', minHeight: 30, lineHeight: 1.45 }}>{o.task}</span>

      {/* Subtasks rail */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
        {o.subtasks.map((t, i) => {
          const p = t === inProg ? animatedProgress : t.progress;
          const isActive = p > 0 && p < 100;
          return (
            <div key={i} style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
                <span style={{
                  fontFamily: 'var(--mono)', fontSize: 9,
                  color: t.done ? 'var(--ok)' : (isActive ? statusColor : 'var(--text-muted)'),
                  display: 'inline-flex', alignItems: 'center', gap: 4,
                }}>
                  <span style={{
                    display: 'inline-block',
                    transform: isActive ? `rotate(${tick * 30}deg)` : 'none',
                    transition: 'transform 200ms linear',
                  }}>{t.done ? '✓' : (isActive ? '⟳' : '○')}</span>
                  {t.name}
                </span>
                {isActive && <span className="meta-10" style={{ fontSize: 9, color: statusColor }}>{Math.round(p)}%</span>}
              </div>
              {isActive && (
                <div style={{ height: 1.5, background: 'var(--bg-base)', borderRadius: 1, overflow: 'hidden' }}>
                  <div style={{
                    width: `${p}%`, height: '100%', background: statusColor, transition: 'width 300ms',
                    boxShadow: `0 0 4px ${statusColor}`,
                  }} />
                </div>
              )}
            </div>
          );
        })}
      </div>

      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', borderTop: '1px solid var(--border)', paddingTop: 8 }}>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>PAWA</span>
        <span className="val-12" style={{ color: 'var(--text-secondary)', fontSize: 11 }}>
          {(o.pawa + Math.sin(tick * 0.3 + o.id.charCodeAt(0)) * 4).toFixed(0)} <span style={{ color: 'var(--text-muted)' }}>/ session</span>
        </span>
      </div>
    </button>
  );
}

Object.assign(window, { MonitorPanel });
