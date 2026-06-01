import React from 'react';
import { api } from '../../lib/api.js';
/* Monitor panel — live observability. Two columns: live state stream + event log. Operative cards below. */

const { useState: dUseState, useEffect: dUseEffect, useMemo: dUseMemo, useRef: dUseRef } = React;

/* ─── helpers to normalise API state → STATE_TREE shape ─── */
function apiStateToStateTree(apiState) {
  if (!apiState) return null;
  const rows = [];
  // pockets
  const pockets = apiState.pockets || {};
  Object.entries(pockets).forEach(([k, v]) => {
    rows.push({
      path: `finances.pockets.${k}`,
      value: v,
      target: null,
      fmt: 'ksh',
      cstr: 'ok',
      desc: `${k} pocket`,
    });
  });
  // pawa_balance
  if (apiState.pawa_balance != null) {
    rows.push({
      path: 'system.pawa_balance',
      value: apiState.pawa_balance,
      target: 10000,
      fmt: 'pwa',
      cstr: apiState.pawa_balance < 2000 ? 'red' : apiState.pawa_balance < 5000 ? 'amber' : 'ok',
      desc: 'Orchie tokens',
    });
  }
  // score
  if (apiState.score != null) {
    rows.push({
      path: 'system.score',
      value: apiState.score,
      target: 1,
      fmt: '',
      cstr: apiState.score >= 0.8 ? 'ok' : apiState.score >= 0.6 ? 'amber' : 'red',
      desc: 'Sustain score',
    });
  }
  return rows.length ? rows : null;
}

function apiOperativesToCards(operatives) {
  if (!operatives || !operatives.length) return null;
  return operatives.map(o => ({
    id: o.id || o.name,
    name: o.name || o.id,
    role: o.role || '',
    status: o.status || 'active',
    confidence: o.confidence ?? 80,
    pawa: o.pawa ?? 0,
    task: o.task || o.current_task || '',
    subtasks: o.subtasks || [],
  }));
}

function apiEventsToLogEntries(events) {
  if (!events || !events.length) return null;
  const pad = n => n.toString().padStart(2, '0');
  return events.slice(0, 24).map((e, i) => {
    const d = e.timestamp ? new Date(e.timestamp) : new Date();
    const time = `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
    return {
      key: e.id || i,
      time,
      sustain: e.sustain || '',
      operator: e.operator || e.type || '',
      delta: e.delta || e.message || JSON.stringify(e.data || {}),
      op: e.op || e.type || 'EVT',
      tone: e.tone || 'teal',
    };
  });
}

/* Stream health: LIVE when ws is connected, STALE/DELAYED otherwise */
function useStreamHealth(wsStatus) {
  if (wsStatus === 'connected') return { status: 'LIVE',    color: 'var(--teal)',   lag: 'ws' };
  if (wsStatus === 'stale')     return { status: 'STALE',   color: 'var(--danger)', lag: '>10s' };
  return                               { status: 'OFFLINE', color: 'var(--text-muted)', lag: '—' };
}

function MonitorPanel({ tick, sustain, liveState }) {
  // liveState is pushed from shell.jsx via the WS stream (may be null on first render)
  const [apiData, setApiData]   = dUseState(null);   // last good GET /devui/state result
  const [loading, setLoading]   = dUseState(true);
  const [offline, setOffline]   = dUseState(false);
  const [wsStatus, setWsStatus] = dUseState('connecting');

  // Initial fetch
  dUseEffect(() => {
    let cancelled = false;
    setLoading(true);
    api.get(`/devui/state?sustain_id=${encodeURIComponent(sustain.id)}`)
      .then(d => { if (!cancelled) { setApiData(d); setLoading(false); setOffline(false); } })
      .catch(() => { if (!cancelled) { setLoading(false); setOffline(true); } });
    return () => { cancelled = true; };
  }, [sustain.id]);

  // Re-fetch on tick (every ~5s at 1s tick rate)
  dUseEffect(() => {
    if (tick === 0 || tick % 5 !== 0) return;
    api.get(`/devui/state?sustain_id=${encodeURIComponent(sustain.id)}`)
      .then(d => { setApiData(d); setOffline(false); setWsStatus('connected'); })
      .catch(() => setOffline(true));
  }, [tick, sustain.id]);

  // Merge liveState (from WS) when available
  dUseEffect(() => {
    if (liveState && liveState.sustain_id === sustain.id) {
      setApiData(liveState);
      setOffline(false);
      setWsStatus('connected');
    }
  }, [liveState, sustain.id]);

  const streamHealth = useStreamHealth(offline ? 'stale' : wsStatus);

  // Derive display data — fall back to mock when offline
  const stateRows = dUseMemo(() => {
    const fromApi = apiStateToStateTree(apiData);
    if (fromApi) {
      return fromApi.map((s, i) => {
        const live = s.value;
        const sparkline = Array.from({ length: 7 }).map((_, t) => {
          const ago = 6 - t;
          return s.value + Math.sin((tick - ago * 4) * 0.1 + i) * (s.value * 0.003);
        });
        return { ...s, live, sparkline };
      });
    }
    // Fallback: use MOCK data
    return STATE_TREE.map((s, i) => {
      const live = s.value + (s.fmt === 'ksh' ? Math.sin(tick * 0.1 + i) * (s.value * 0.001) : 0);
      const sparkline = Array.from({ length: 7 }).map((_, t) => {
        const ago = 6 - t;
        const base = s.value;
        if (s.fmt === 'ksh' || s.fmt === 'pwa') return base + Math.sin((tick - ago * 4) * 0.1 + i) * (base * 0.005);
        if (s.fmt === '%' || s.fmt === 'L' || s.fmt === 'kg') return base + Math.sin((tick - ago * 4) * 0.15 + i) * (base * 0.03);
        return base + Math.sin((tick - ago * 4) * 0.12 + i) * (base * 0.02);
      });
      return { ...s, live, sparkline };
    });
  }, [apiData, tick]);

  const operatives = dUseMemo(() =>
    apiOperativesToCards(apiData?.operatives) || OPERATIVES,
  [apiData]);

  // Event log: accumulate API events, fall back to synthetic
  const [logEntries, setLogEntries] = dUseState(() =>
    Array.from({ length: 8 }).map((_, i) => pickEvent(i + 1000))
  );
  dUseEffect(() => {
    const fromApi = apiEventsToLogEntries(apiData?.events);
    if (fromApi && fromApi.length) {
      setLogEntries(fromApi);
    }
  }, [apiData]);
  // Keep synthetic trickle when offline
  dUseEffect(() => {
    if (!offline || tick === 0 || tick % 3 !== 0) return;
    const next = pickEvent(tick + 4321);
    setLogEntries(prev => [next, ...prev].slice(0, 24));
  }, [tick, offline]);

  // Hero metrics from API or fallback
  const pawaBalance = apiData?.pawa_balance ?? (8420 - tick % 60);
  const activeSustains = SUSTAINS.filter(s => s.status === 'live').length;

  return (
    <div className="panel-enter" style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: 18, padding: '24px 28px' }}>
      {/* Stream health pill row */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <StreamHealthPill health={streamHealth} tick={tick} />
        {offline && (
          <span style={{
            fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)',
            padding: '2px 7px', border: '1px solid var(--border)',
            borderRadius: 10, opacity: 0.7,
          }}>OFFLINE · LAST KNOWN DATA</span>
        )}
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>SUSTENA XII · MCP MONITOR · ALL SUSTAINS</span>
      </div>

      {/* Loading skeleton */}
      {loading && (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 14 }}>
          {[0,1,2,3].map(i => (
            <div key={i} style={{
              height: 80, borderRadius: 'var(--radius-md)',
              background: 'var(--bg-surface)', border: '1px solid var(--border)',
              animation: 'skeletonPulse 1.4s ease-in-out infinite',
              animationDelay: `${i * 120}ms`,
            }} />
          ))}
          <style>{`
            @keyframes skeletonPulse {
              0%,100% { opacity: 0.5; }
              50%      { opacity: 1; }
            }
          `}</style>
        </div>
      )}

      {/* Hero strip: 4 sparse live metrics */}
      {!loading && (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 14 }}>
          <HeroTile label="ACTIVE SUSTAINS" value={activeSustains} sub="of 5 in scope" tone="ok" />
          <HeroTile label="OPERATORS / MIN" value={Math.round(218 + Math.sin(tick * 0.2) * 14)} live sub="rolling 60s" />
          <HeroTile label="API P95" value={`${apiData?.api_p95_ms ?? Math.round(412 + Math.sin(tick * 0.15) * 18)}`} unit="ms" live sub="haiku · inference" tone={tick % 17 < 3 ? 'amber' : 'default'} />
          <HeroTile label="PAWA BALANCE" value={pawaBalance} unit="pwa" sub="orchie tokens" tone="default" />
        </div>
      )}

      {/* 2-col main: state stream + event log */}
      {!loading && (
        <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1fr) minmax(0, 1.3fr)', gap: 14, flex: 1, minHeight: 0 }}>
          {/* Live State Stream */}
          <Card title="LIVE STATE STREAM" sub={`${sustain.label}`} padded={false} scroll
            actions={
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span className={offline ? '' : 'pulse'} style={{ width: 5, height: 5, borderRadius: '50%', background: offline ? 'var(--text-muted)' : 'var(--teal)', opacity: offline ? 0.5 : 1 }} />
                <span className="meta-10" style={{ color: offline ? 'var(--text-muted)' : 'var(--teal)' }}>{offline ? 'OFFLINE' : 'SYNC · live'}</span>
              </div>
            }
          >
            {stateRows.map((s, i) => <StateRow key={s.path} s={s} delay={i * 30} />)}
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
      )}

      {/* Operative activity strip */}
      {!loading && (
        <div>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 8 }}>
            <span className="label-11">OPERATIVE ACTIVITY</span>
            <span className="meta-10">{operatives.filter(o => o.status === 'active').length} ACTIVE · {operatives.filter(o => o.status === 'alert').length} ALERT</span>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12 }}>
            {operatives.map((o, i) => <OperativeCard key={o.id} o={o} delay={i * 60} tick={tick} />)}
          </div>
        </div>
      )}
    </div>
  );
}

function StreamHealthPill({ health, tick }) {
  return (
    <div style={{
      display: 'inline-flex', alignItems: 'center', gap: 7,
      padding: '4px 10px 4px 8px',
      background: 'var(--bg-surface)',
      border: `1px solid ${health.color}44`,
      borderRadius: 20,
    }}>
      <span className={health.status === 'LIVE' ? 'pulse' : ''} style={{
        width: 6, height: 6, borderRadius: '50%',
        background: health.color,
        boxShadow: `0 0 5px ${health.color}`,
      }} />
      <span style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 600, color: health.color, letterSpacing: '0.06em' }}>
        {health.status}
      </span>
      <span className="meta-10" style={{ color: 'var(--text-muted)' }}>·</span>
      <span className="meta-10" style={{ color: 'var(--text-secondary)' }}>{health.lag}</span>
      <span className="meta-10" style={{ color: 'var(--text-muted)' }}>
        {formatClock(new Date())}
      </span>
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
    if (s.fmt === 'ksh' || s.fmt === 'pwa') return Math.round(v).toLocaleString();
    if (s.fmt === '%') return Math.round(v).toString();
    if (s.fmt === 'L' || s.fmt === 'kg') return v.toFixed(1);
    if (s.fmt === 'ms') return Math.round(v).toString();
    return Math.round(v).toLocaleString();
  };

  const fmtTarget = (v) => {
    if (!v) return '—';
    if (s.fmt === 'ksh' || s.fmt === 'pwa') return Math.round(v).toLocaleString();
    if (s.fmt === '%' || s.fmt === 'ms') return Math.round(v).toString();
    if (s.fmt === 'L' || s.fmt === 'kg') return v.toFixed(1);
    return Math.round(v).toLocaleString();
  };

  // Progress bar fill ratio — for lowerBetter, invert the ratio
  const fillRatio = s.target
    ? s.lowerBetter
      ? Math.min(1, Math.max(0, s.target / Math.max(s.live, 0.001)))
      : Math.min(1, Math.max(0, s.live / s.target))
    : 0;

  // Target tick position on bar (always at 100% mark for lowerBetter, else at s.target)
  const tickPct = 100; // tick is at the target line — always rightmost

  // Sparkline min/max for scaling
  const spark = s.sparkline || [];
  const sparkMin = Math.min(...spark) * 0.98;
  const sparkMax = Math.max(...spark) * 1.02;
  const sparkRange = Math.max(sparkMax - sparkMin, 0.001);
  const SPARK_W = 56, SPARK_H = 18;
  const sparkPts = spark.map((v, i) => {
    const x = (i / (spark.length - 1)) * SPARK_W;
    const y = SPARK_H - ((v - sparkMin) / sparkRange) * SPARK_H;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(' ');

  const shortPath = s.path.split('.').slice(-1)[0].replace(/_/g, ' ');

  return (
    <div className="fade-up" style={{
      display: 'grid',
      gridTemplateColumns: '6px 130px 86px 1fr 60px',
      gap: 10,
      padding: '8px 16px',
      borderBottom: '1px solid var(--border)',
      animationDelay: `${delay}ms`,
      alignItems: 'center',
    }}>
      {/* Status dot */}
      <span style={{
        width: 5, height: 5, borderRadius: '50%',
        background: c,
        boxShadow: s.cstr === 'red' ? `0 0 5px ${c}` : 'none',
        flexShrink: 0,
      }} />

      {/* Name + desc */}
      <div style={{ display: 'flex', flexDirection: 'column', minWidth: 0 }}>
        <span style={{
          fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500,
          color: 'var(--text-primary)',
          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          textTransform: 'uppercase', letterSpacing: '0.04em',
        }}>{shortPath}</span>
        {s.desc && (
          <span style={{
            fontFamily: 'var(--mono)', fontSize: 9,
            color: 'var(--text-muted)',
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          }}>{s.desc}</span>
        )}
      </div>

      {/* Value / target */}
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 3 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 600, color: c }}>
          {fmtVal(s.live)}
        </span>
        {s.target && (
          <>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>/</span>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>
              {fmtTarget(s.target)}
            </span>
          </>
        )}
        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)' }}>{s.fmt}</span>
      </div>

      {/* Progress bar with target tick */}
      <div style={{ position: 'relative', height: 6 }}>
        {/* Track */}
        <div style={{
          position: 'absolute', inset: 0,
          background: 'var(--bg-base)',
          borderRadius: 3,
          border: '1px solid var(--border)',
          overflow: 'hidden',
        }}>
          {/* Fill */}
          <div style={{
            position: 'absolute', top: 0, left: 0, bottom: 0,
            width: `${fillRatio * 100}%`,
            background: c,
            borderRadius: 3,
            opacity: s.cstr === 'red' ? 1 : 0.85,
            transition: 'width 0.6s ease',
            boxShadow: s.cstr === 'red' ? `0 0 4px ${c}` : 'none',
          }} />
        </div>
        {/* Target tick mark */}
        {s.target && (
          <div style={{
            position: 'absolute', top: -2, right: 0, bottom: -2,
            width: 2,
            background: 'var(--text-muted)',
            borderRadius: 1,
            opacity: 0.6,
          }} />
        )}
        {/* lowerBetter arrow indicator */}
        {s.lowerBetter && (
          <span style={{
            position: 'absolute', right: 4, top: '50%', transform: 'translateY(-50%)',
            fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-muted)',
          }}>▼</span>
        )}
      </div>

      {/* 7-point sparkline */}
      {spark.length >= 2 ? (
        <svg width={SPARK_W} height={SPARK_H} viewBox={`0 0 ${SPARK_W} ${SPARK_H}`}
          style={{ display: 'block', overflow: 'visible' }}>
          <polyline
            points={sparkPts}
            fill="none"
            stroke={c}
            strokeWidth="1.2"
            opacity="0.8"
          />
          {/* Last reading dot */}
          <circle
            cx={(SPARK_W).toFixed(1)}
            cy={(SPARK_H - ((spark[spark.length - 1] - sparkMin) / sparkRange) * SPARK_H).toFixed(1)}
            r="2" fill={c}
          />
        </svg>
      ) : (
        <span style={{ width: SPARK_W, color: 'var(--text-dim)', fontFamily: 'var(--mono)', fontSize: 9 }}>—</span>
      )}
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
