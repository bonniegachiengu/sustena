import React from 'react';
import { api } from '../../lib/api.js';
/* Monitor panel — live observability. Two columns: live state stream + event log. Operative cards below. */

const { useState: dUseState, useEffect: dUseEffect, useMemo: dUseMemo, useRef: dUseRef } = React;

/* ─── helpers to normalise API state → STATE_TREE shape ─── */
function apiStateToStateTree(state) {
  if (!state) return null;
  const rows = [];

  // finances.pockets — pocket is {allocated, spent, limit}. Show live REMAINING
  // (allocated − spent) against allocated, so the stream reflects real spending.
  const pockets = state.finances?.pockets || {};
  Object.entries(pockets).forEach(([k, v]) => {
    const obj = typeof v === 'object' && v != null;
    const allocated = obj ? (v.allocated ?? 0) : v;
    const spent = obj ? (v.spent ?? 0) : 0;
    const remaining = allocated - spent;
    const pct = allocated > 0 ? spent / allocated : 0;
    rows.push({
      path: `finances.pockets.${k}`,
      value: remaining,
      target: allocated,
      fmt: 'ksh',
      cstr: pct >= 1 ? 'red' : pct >= 0.8 ? 'amber' : 'ok',
      desc: `${k} · ${Math.round(spent).toLocaleString()} spent of ${Math.round(allocated).toLocaleString()}`,
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
    confidence: typeof o.confidence === 'number' ? o.confidence : null,
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

  // Re-fetch every heartbeat (1s tick) — keep all cards fresh, no lag
  dUseEffect(() => {
    if (tick === 0) return;
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

  // Derive display data — fall back to empty when offline
  const stateRows = dUseMemo(() => {
    const fromApi = apiStateToStateTree(apiData?.state);
    if (fromApi) {
      return fromApi
        .map((s, i) => {
          const live = s.value;
          const sparkline = Array.from({ length: 7 }).map((_, t) => {
            const ago = 6 - t;
            return s.value + Math.sin((tick - ago * 4) * 0.1 + i) * (s.value * 0.003);
          });
          return { ...s, live, sparkline };
        })
        .sort((a, b) => (b.value ?? 0) - (a.value ?? 0));  // highest balance first
    }
    // Fallback: nothing to show
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
    apiOperativesToCards(apiData?.operatives) || [],
  [apiData]);

  // Add Orchie (the orchestrator) as a 6th card — idle until its scenario engine runs.
  const operativeCards = dUseMemo(() => ([
    ...operatives,
    { id: 'orchie', name: 'Orchie', role: 'orchestrator · scenario engine', status: 'active', confidence: null, pawa: 0, task: '' },
  ]), [operatives]);

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
    <div className="panel-enter" style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: 18, padding: '24px 28px', overflowY: 'auto' }}>
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

      {/* Top region: Active Sustains (count + list) ⅓ · operatives 3×2 ⅔ */}
      {!loading && (
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 2fr', gap: 14, alignItems: 'start' }}>
          <ActiveSustainsCard count={activeSustains} sustains={sustains} currentId={sustain.id} />
          <div>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 8 }}>
              <span className="label-11">OPERATIVE ACTIVITY</span>
              <span className="meta-10">
                {operativeCards.filter(o => o.status === 'active').length} ACTIVE · {operativeCards.filter(o => o.status === 'alert').length} ALERT
              </span>
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, minmax(0, 1fr))', gap: 12 }}>
              {operativeCards.map((o, i) => <OperativeCard key={o.id} o={o} delay={i * 60} />)}
            </div>
          </div>
        </div>
      )}

      {/* visualize.* widget grid */}
      {!loading && <VisualizeWidgetGrid sustain={sustain} tick={tick} />}

      {/* 2-col: state stream + event log — at the bottom */}
      {!loading && (
        <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1fr) minmax(0, 1.3fr)', gap: 14, minHeight: 380 }}>
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
    </div>
  );
}

/* ── VisualizeWidgetGrid — calls GET /devui/monitor-widgets ──────────────── */
function VisualizeWidgetGrid({ sustain, tick }) {
  const [widgets, setWidgets] = dUseState(null);
  const [loading, setLoading] = dUseState(true);

  dUseEffect(() => {
    let cancelled = false;
    setLoading(true);
    api.get(`/devui/monitor-widgets?sustain_id=${encodeURIComponent(sustain.id)}`)
      .then(d => {
        if (!cancelled) { setWidgets(d?.data?.widgets || null); setLoading(false); }
      })
      .catch(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [sustain.id]);

  // Refresh widgets every heartbeat (1s tick) so the grid never lags state.
  dUseEffect(() => {
    if (!tick) return;
    api.get(`/devui/monitor-widgets?sustain_id=${encodeURIComponent(sustain.id)}`)
      .then(d => setWidgets(d?.data?.widgets || null))
      .catch(() => {});
  }, [tick, sustain.id]);

  if (loading) return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      <span style={{ animation: 'pulse 0.8s ease-in-out infinite', fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>⟳ loading widgets…</span>
    </div>
  );
  if (!widgets) return null;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 8 }}>
        <span className="label-11">WIDGET GRID · VISUALIZE OPERATORS</span>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>visualize.pocket_ring · event_feed · constraint_health</span>
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 12 }}>
        {widgets.pocket_ring       && <PocketRingWidget w={widgets.pocket_ring} />}
        {widgets.event_feed        && <EventFeedWidget  w={widgets.event_feed} />}
        {widgets.constraint_health && <ConstraintHealthWidget w={widgets.constraint_health} />}
      </div>
    </div>
  );
}

function PocketRingWidget({ w }) {
  const d = w.data || {};
  const pockets = d.pockets || [];
  const pctSpent = d.pct_spent ?? 0;
  const liquid = d.liquid ?? 0;
  const color = pctSpent > 90 ? 'var(--danger)' : pctSpent > 70 ? 'var(--amber)' : 'var(--teal)';

  return (
    <div style={{
      background: 'var(--bg-surface)', border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)', padding: 14,
      display: 'flex', flexDirection: 'column', gap: 10,
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
        <span className="label-10">BUDGET RING</span>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>visualize.pocket_ring</span>
      </div>

      {/* Donut ring summary */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <svg width="60" height="60" viewBox="0 0 60 60">
          <circle cx="30" cy="30" r="22" fill="none" stroke="var(--bg-overlay)" strokeWidth="8" />
          <circle cx="30" cy="30" r="22" fill="none" stroke={color} strokeWidth="8"
            strokeDasharray={`${2 * Math.PI * 22 * pctSpent / 100} 999`}
            transform="rotate(-90 30 30)"
            strokeLinecap="round"
            style={{ filter: `drop-shadow(0 0 4px ${color})` }}
          />
          <text x="30" y="35" textAnchor="middle"
            style={{ fontFamily: 'var(--mono)', fontSize: 12, fill: color, fontWeight: 600 }}>
            {Math.round(pctSpent)}%
          </text>
        </svg>
        <div style={{ flex: 1 }}>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 18, fontWeight: 500, color, lineHeight: 1.1 }}>
            {Math.round(pctSpent)}%
          </div>
          <div className="meta-10" style={{ color: 'var(--text-muted)', marginTop: 2 }}>of budget spent</div>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-secondary)', marginTop: 4 }}>
            KES {liquid.toLocaleString()} liquid
          </div>
        </div>
      </div>

      {w.summary && <span className="meta-10" style={{ color: 'var(--text-muted)', borderTop: '1px solid var(--border)', paddingTop: 6 }}>{w.summary}</span>}
    </div>
  );
}

function EventFeedWidget({ w }) {
  const d = w.data || {};
  const events = d.events || [];
  const total = d.total ?? events.length;

  return (
    <div style={{
      background: 'var(--bg-surface)', border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)', padding: 14,
      display: 'flex', flexDirection: 'column', gap: 8,
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
        <span className="label-10">EVENT FEED</span>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>visualize.event_feed · streaming</span>
      </div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 6 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 28, fontWeight: 500, color: 'var(--node-event)' }}>{total}</span>
        <span className="meta-10">recent events</span>
      </div>
      {total === 0 && (
        <span className="meta-10" style={{ color: 'var(--text-dim)' }}>No events yet — run operators to generate events</span>
      )}
      <span className="meta-10" style={{ color: 'var(--text-muted)', borderTop: '1px solid var(--border)', paddingTop: 6 }}>
        see EVENT LOG · STREAM below for detail
      </span>
    </div>
  );
}

function ConstraintHealthWidget({ w }) {
  const d = w.data || {};
  const passing = d.passing ?? 0;
  const total = d.total ?? 0;
  const constraints = d.constraints || [];
  const allPass = total > 0 && passing === total;
  const color = allPass ? 'var(--teal)' : passing > 0 ? 'var(--amber)' : 'var(--danger)';

  return (
    <div style={{
      background: 'var(--bg-surface)', border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)', padding: 14,
      display: 'flex', flexDirection: 'column', gap: 8,
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
        <span className="label-10">CONSTRAINT HEALTH</span>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>visualize.constraint_health</span>
      </div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 6 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 28, fontWeight: 500, color }}>{passing}</span>
        <span className="meta-10">/ {total} passing</span>
      </div>
      {constraints.length === 0 ? (
        <span className="meta-10" style={{ color: 'var(--text-dim)' }}>No constraints configured</span>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          {constraints.map((c, i) => (
            <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '4px 0', borderBottom: i < constraints.length - 1 ? '1px solid var(--border)' : 'none' }}>
              <span style={{ color: c.passing ? 'var(--teal)' : 'var(--danger)', fontSize: 11, flexShrink: 0 }}>
                {c.passing ? '✓' : '✗'}
              </span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {c.constraint}
              </span>
            </div>
          ))}
        </div>
      )}
      {w.summary && <span className="meta-10" style={{ color: 'var(--text-muted)', borderTop: '1px solid var(--border)', paddingTop: 6 }}>{w.summary}</span>}
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

function ActiveSustainsCard({ count, sustains, currentId }) {
  const live = (sustains || []).filter(s => s.status === 'live');
  return (
    <div className="fade-up" style={{
      background: 'var(--bg-surface)', border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)', padding: '14px 16px',
      display: 'flex', flexDirection: 'column', gap: 10, height: '100%',
    }}>
      <span className="label-10">ACTIVE SUSTAINS</span>
      <div style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 40, fontWeight: 600, color: 'var(--teal)', lineHeight: 1 }}>{count}</span>
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 4, maxHeight: 72, overflowY: 'auto' }}>
          {live.length === 0 ? (
            <span className="meta-10" style={{ color: 'var(--text-dim)' }}>none live</span>
          ) : live.map(s => (
            <div key={s.id} style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <span style={{ width: 5, height: 5, borderRadius: '50%', background: 'var(--teal)', flexShrink: 0 }} />
              <span style={{
                fontFamily: 'var(--mono)', fontSize: 10,
                color: s.id === currentId ? 'var(--amber)' : 'var(--text-secondary)',
                overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
              }}>{s.label}</span>
            </div>
          ))}
        </div>
      </div>
      <span className="meta-10" style={{ color: 'var(--text-muted)' }}>in scope</span>
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

function OperativeCard({ o, delay }) {
  const statusColor = o.status === 'active' ? 'var(--teal)' : o.status === 'alert' ? 'var(--amber)' : 'var(--text-muted)';
  const hasConf = typeof o.confidence === 'number';
  const conf = hasConf ? Math.max(0, Math.min(100, o.confidence)) : null;
  const hasTask = !!(o.task && String(o.task).trim());

  return (
    <button onClick={() => window.confirmAction?.({
      title: `${o.name} · ${o.role}`,
      body: hasTask
        ? `${o.task}.${hasConf ? ` Confidence ${conf}%.` : ''} Pawa burn ${o.pawa} / session.`
        : `${o.name} is idle — no task assigned. Operatives act when a Council proposal touches their domain (${o.role}).`,
      ctaLabel: 'VIEW DETAILS',
    })}
    className="fade-up"
    style={{
      display: 'flex', flexDirection: 'column', gap: 8,
      background: 'var(--bg-surface)', border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)', padding: '12px 14px',
      textAlign: 'left', width: '100%', animationDelay: `${delay}ms`,
      transition: 'border-color var(--t-fast)', cursor: 'pointer',
    }}
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

      {/* Confidence — only when the operative reports a real value */}
      {hasConf ? (
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <div style={{ flex: 1, height: 3, background: 'var(--bg-base)', borderRadius: 2, overflow: 'hidden' }}>
            <div style={{
              width: `${conf}%`, height: '100%',
              background: conf >= 80 ? 'var(--teal)' : conf >= 60 ? 'var(--amber)' : 'var(--danger)',
              borderRadius: 2, transition: 'width 0.4s ease',
            }} />
          </div>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-secondary)', minWidth: 28 }}>{Math.round(conf)}%</span>
        </div>
      ) : (
        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)' }}>confidence · n/a</span>
      )}

      {/* Task or idle */}
      <span style={{
        fontFamily: 'var(--mono)', fontSize: 9,
        color: hasTask ? 'var(--text-secondary)' : 'var(--text-dim)',
        overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
      }}>{hasTask ? o.task : 'idle · awaiting council trigger'}</span>
    </button>
  );
}

window.MonitorPanel = MonitorPanel;