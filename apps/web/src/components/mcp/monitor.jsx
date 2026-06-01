import React from 'react';
import { api } from '../../lib/api.js';
/* Monitor panel — live observability. Two columns: live state stream + event log. Operative cards below. */

const { useState: dUseState, useEffect: dUseEffect, useMemo: dUseMemo, useRef: dUseRef } = React;

/* ─── helpers to normalise API state → STATE_TREE shape ─── */
function apiStateToStateTree(state) {
  if (!state) return null;
  const rows = [];

  // finances.pockets — value may be a number or {allocated, target, status}
  const pockets = state.finances?.pockets || {};
  Object.entries(pockets).forEach(([k, v]) => {
    const allocated = typeof v === 'object' ? (v.allocated ?? 0) : v;
    const target = typeof v === 'object' ? (v.target ?? null) : null;
    const st = typeof v === 'object' ? (v.status || 'ok') : 'ok';
    rows.push({
      path: `finances.pockets.${k}`,
      value: allocated,
      target,
      fmt: 'ksh',
      cstr: st === 'amber' ? 'amber' : st === 'fail' ? 'red' : 'ok',
      desc: `${k} pocket`,
    });
  });

  // pantry — numeric entries only
  const pantry = state.pantry || {};
  Object.entries(pantry).forEach(([k, v]) => {
    if (typeof v !== 'number') return;
    const fmt = k.endsWith('_L') ? 'L' : k.endsWith('_kg') ? 'kg' : '';
    rows.push({
      path: `pantry.${k}`,
      value: v,
      target: null,
      fmt,
      cstr: 'ok',
      desc: k.replace(/_/g, ' '),
    });
  });

  // system.pawa_balance
  if (state.system?.pawa_balance != null) {
    const pb = state.system.pawa_balance;
    rows.push({
      path: 'system.pawa_balance',
      value: pb,
      target: 10000,
      fmt: 'pwa',
      cstr: pb < 2000 ? 'red' : pb < 5000 ? 'amber' : 'ok',
      desc: 'Orchie tokens',
    });
  }

  // system.api_p95_ms
  if (state.system?.api_p95_ms != null) {
    const ms = state.system.api_p95_ms;
    rows.push({
      path: 'system.api_p95_ms',
      value: ms,
      target: 500,
      fmt: 'ms',
      cstr: ms > 800 ? 'red' : ms > 500 ? 'amber' : 'ok',
      desc: 'API latency p95',
      lowerBetter: true,
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
    pawa: o.pawa ?? o.pawa_session ?? 0,
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
    const payloadStr = e.payload ? JSON.stringify(e.payload) : '';
    return {
      key: e.id || i,
      time,
      sustain: e.sustain || e.sustain_id || '',
      operator: e.operator || e.event_name || e.type || '',
      delta: e.delta || e.message || payloadStr || JSON.stringify(e.data || {}),
      op: e.op || (e.event_name ? 'EVT' : e.type) || 'EVT',
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

function MonitorPanel({ tick, sustain, liveState, sustains }) {
  // liveState is pushed from shell.jsx via the WS stream (may be null on first render)
  const [apiData, setApiData]   = dUseState(null);   // last good GET /devui/state result
  const [loading, setLoading]   = dUseState(true);
  const [offline, setOffline]   = dUseState(false);
  const [wsStatus, setWsStatus] = dUseState('connecting');

  // Normalise GET /devui/state response → { state, events, operatives, constraints }
  const normaliseGetResponse = (d) => {
    const payload = d?.data || {};
    return {
      state: payload.state || null,
      events: payload.events || [],
      operatives: payload.operatives || [],
      constraints: payload.constraints || [],
    };
  };

  // Initial fetch
  dUseEffect(() => {
    let cancelled = false;
    setLoading(true);
    api.get(`/devui/state?sustain_id=${encodeURIComponent(sustain.id)}`)
      .then(d => { if (!cancelled) { setApiData(normaliseGetResponse(d)); setLoading(false); setOffline(false); } })
      .catch(() => { if (!cancelled) { setLoading(false); setOffline(true); } });
    return () => { cancelled = true; };
  }, [sustain.id]);

  // Re-fetch on tick (every ~5s at 1s tick rate)
  dUseEffect(() => {
    if (tick === 0 || tick % 5 !== 0) return;
    api.get(`/devui/state?sustain_id=${encodeURIComponent(sustain.id)}`)
      .then(d => { setApiData(normaliseGetResponse(d)); setOffline(false); setWsStatus('connected'); })
      .catch(() => setOffline(true));
  }, [tick, sustain.id]);

  // Merge liveState (from WS) — update state only, preserve events/operatives from REST
  dUseEffect(() => {
    if (liveState && liveState.sustain_id === sustain.id && liveState.state) {
      setApiData(prev => ({ ...prev, state: liveState.state }));
      setOffline(false);
      setWsStatus('connected');
    }
  }, [liveState, sustain.id]);

  const streamHealth = useStreamHealth(offline ? 'stale' : wsStatus);

  // Derive display data — fall back to mock when offline
  const stateRows = dUseMemo(() => {
    const fromApi = apiStateToStateTree(apiData?.state);
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
  const pawaBalance    = apiData?.state?.system?.pawa_balance ?? null;
  const opsPerMin      = apiData?.state?.system?.ops_per_min ?? null;
  const activeSustains = (sustains && sustains.length ? sustains : SUSTAINS).filter(s => s.status === 'live').length;

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
          <HeroTile label="ACTIVE SUSTAINS" value={activeSustains} sub="in scope" tone="ok" />
          <HeroTile label="OPERATORS / MIN" value={opsPerMin ?? '—'} live={opsPerMin != null} sub="rolling 60s" />
          <HeroTile label="API P95" value={apiData?.state?.system?.api_p95_ms != null ? `${apiData.state.system.api_p95_ms}` : '—'} unit={apiData?.state?.system?.api_p95_ms != null ? 'ms' : ''} live sub="haiku · inference" />
          <HeroTile label="PAWA BALANCE" value={pawaBalance != null ? pawaBalance : '—'} unit={pawaBalance != null ? 'pwa' : ''} sub="orchie tokens" tone="default" />
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
      body: `${o.task}. Confidence ${o.confidence}%. Pawa burn ${o.pawa} / session.`,
      ctaLabel: 'VIEW DETAILS',
    })} style={{
      display: 'flex', flexDirection: 'column', gap: 8,
      background: 'var(--bg-surface)', border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)', padding: '12px 14px',
      textAlign: 'left', width: '100%',
      transition: 'border-color var(--t-fast)',
      cursor: 'pointer',
    }}
    className="fade-up"
    style2={{ animationDelay: `${delay}ms` }}
    onMouseEnter={e => e.currentTarget.style.borderColor = statusColor}
    onMouseLeave={e => e.currentTarget.style.borderColor = 'var(--border)'}
    >
      {/* Header row */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
          <span style={{ width: 6, height: 6, borderRadius: '50%', background: statusColor, boxShadow: `0 0 5px ${statusColor}`, flexShrink: 0 }} />
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 600, color: 'var(--text-primary)', letterSpacing: '0.04em' }}>{o.name}</span>
        </div>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: statusColor, letterSpacing: '0.06em' }}>{o.status.toUpperCase()}</span>
      </div>

      {/* Role */}
      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>{o.role}</span>

      {/* Confidence bar */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <div style={{ flex: 1, height: 3, background: 'var(--bg-base)', borderRadius: 2, overflow: 'hidden' }}>
          <div style={{
            width: `${liveConfidence}%`, height: '100%',
            background: liveConfidence >= 80 ? 'var(--teal)' : liveConfidence >= 60 ? 'var(--amber)' : 'var(--danger)',
            borderRadius: 2, transition: 'width 0.4s ease',
          }} />
        </div>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-secondary)', minWidth: 28 }}>{Math.round(liveConfidence)}%</span>
      </div>

      {/* Task */}
      <span style={{
        fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-secondary)',
        overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
      }}>{o.task}</span>

      {/* Activity sparkline */}
      <svg width="100%" height="18" viewBox={`0 0 ${series.length * 6} 18`} preserveAspectRatio="none" style={{ display: 'block' }}>
        {series.map((v, i) => {
          const barH = Math.max(2, (v / 100) * 16);
          return <rect key={i} x={i * 6} y={18 - barH} width={4} height={barH} fill={statusColor} opacity={0.5 + (i / series.length) * 0.5} rx={1} />;
        })}
      </svg>
    </button>
  );
}

window.MonitorPanel = MonitorPanel;