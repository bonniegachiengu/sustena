/**
 * apps/web/src/pages/studio/MonitorZone.jsx
 *
 * OBSERVE. Wired entirely to the already-built GET /devui/state — the same
 * single-fetch, single-source-of-truth payload the Mycelium Monitor panel
 * uses (pockets, events, constraints, widgets, needs-attention signals,
 * rollup, suggestions, egress) — no parallel computation here.
 */
import React from 'react';
import { api } from '../../lib/api.js';
import { sectionLabel, mutedText, Empty, StatusDot, fmtNum, ghostButton } from './shared.jsx';

const { useState, useEffect, useCallback } = React;

export default function MonitorZone({ sustainId }) {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  const load = useCallback(async () => {
    if (!sustainId) return;
    setLoading(true);
    setError(null);
    try {
      const resp = await api.get(`/devui/state?sustain_id=${encodeURIComponent(sustainId)}`);
      setData(resp?.data || null);
    } catch (e) {
      setError(e.message || 'could not reach the engine');
    } finally {
      setLoading(false);
    }
  }, [sustainId]);

  useEffect(() => { load(); }, [load]);

  if (!sustainId) return <Empty text="select a sustain to monitor" />;
  if (loading && !data) return <Empty text="reading live state…" />;
  if (error) return <Empty text={error} sub="the gate could not be reached" />;
  if (!data) return <Empty text="no state yet" />;

  const state = data.state || {};
  const liquid = state?.finances?.liquid?.balance;
  const pockets = state?.finances?.pockets || {};
  const constraints = data.constraints || [];
  const events = (data.events || []).slice(0, 8);

  const needsAttention = [];
  if ((data.proposals_in_voting || []).length > 0) {
    needsAttention.push(`${data.proposals_in_voting.length} proposal${data.proposals_in_voting.length === 1 ? '' : 's'} awaiting a vote`);
  }
  if ((data.ingest_attention?.messages || []).length > 0) {
    needsAttention.push(`${data.ingest_attention.messages.length} capture${data.ingest_attention.messages.length === 1 ? '' : 's'} need${data.ingest_attention.messages.length === 1 ? 's' : ''} classification`);
  }
  if ((data.ingest_attention?.stale_sources || []).length > 0) {
    needsAttention.push(`${data.ingest_attention.stale_sources.length} source${data.ingest_attention.stale_sources.length === 1 ? '' : 's'} gone quiet`);
  }
  if ((data.suggestions || []).length > 0) {
    needsAttention.push(`${data.suggestions.length} operative suggestion${data.suggestions.length === 1 ? '' : 's'} to review`);
  }
  const failingConstraints = constraints.filter(c => c.status === 'fail');
  if (failingConstraints.length > 0) {
    needsAttention.push(`${failingConstraints.length} constraint${failingConstraints.length === 1 ? '' : 's'} failing`);
  }
  const egressActionable = (data.egress || []).filter(e => e.status === 'prepared' || e.status === 'failed');
  if (egressActionable.length > 0) {
    needsAttention.push(`${egressActionable.length} outbox item${egressActionable.length === 1 ? '' : 's'} awaiting confirmation`);
  }

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 10 }}>
        <button onClick={load} style={ghostButton}>↺ REFRESH</button>
      </div>

      {/* Needs attention */}
      <div style={{ marginBottom: 22 }}>
        <div style={sectionLabel}>needs attention · why am I seeing this</div>
        {needsAttention.length === 0
          ? <div style={mutedText}>nothing needs you right now — sustain is nominal</div>
          : needsAttention.map((line, i) => (
              <div key={i} style={{ display: 'flex', alignItems: 'center', padding: '5px 0' }}>
                <StatusDot status="fail" />
                <span style={{ fontFamily: 'var(--ui)', fontSize: 12.5, color: 'var(--text-primary)' }}>{line}</span>
              </div>
            ))}
      </div>

      {/* Hero + pockets */}
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 20, marginBottom: 22 }}>
        <div>
          <div style={sectionLabel}>liquid balance</div>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 26, fontWeight: 700, color: 'var(--text-primary)' }}>
            {liquid === undefined ? '—' : fmtNum(liquid)}
          </div>
          {data.rollup?.aggregates && Object.keys(data.rollup.aggregates).length > 0 && (
            <div style={{ marginTop: 8 }}>
              {Object.entries(data.rollup.aggregates).map(([id, agg]) => (
                <div key={id} style={{ ...mutedText, fontSize: 11 }}>
                  {id.replace(/_/g, ' ')}: <b style={{ color: 'var(--amber)' }}>{fmtNum(agg.value)}</b>
                  {' '}({agg.included?.length || 0} linked{agg.excluded?.length ? `, ${agg.excluded.length} missing` : ''})
                </div>
              ))}
            </div>
          )}
        </div>
        <div>
          <div style={sectionLabel}>constraint health</div>
          {constraints.length === 0
            ? <div style={mutedText}>no constraints declared</div>
            : constraints.map((c, i) => (
                <div key={i} style={{ display: 'flex', alignItems: 'center', padding: '3px 0' }}>
                  <StatusDot status={c.status} />
                  <span style={{ fontFamily: 'var(--mono)', fontSize: 10.5, color: 'var(--text-secondary)' }}>{c.expr}</span>
                </div>
              ))}
        </div>
      </div>

      {/* Pockets */}
      <div style={{ marginBottom: 22 }}>
        <div style={sectionLabel}>pockets</div>
        {Object.keys(pockets).length === 0
          ? <div style={mutedText}>no pockets yet</div>
          : (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: 10 }}>
              {Object.entries(pockets).map(([name, p]) => {
                const allocated = p?.allocated || 0;
                const spent = p?.spent || 0;
                const pct = allocated > 0 ? Math.min(1, spent / allocated) : 0;
                const barColor = pct >= 1 ? 'var(--danger)' : pct >= 0.8 ? 'var(--warn)' : 'var(--teal)';
                return (
                  <div key={name} style={{
                    background: 'var(--bg-raised)', border: '1px solid var(--border)',
                    borderRadius: 'var(--radius-sm)', padding: '10px 12px',
                  }}>
                    <div style={{ fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-primary)', marginBottom: 4 }}>{name}</div>
                    <div style={{ height: 4, background: 'var(--bg-overlay)', borderRadius: 2, overflow: 'hidden', marginBottom: 6 }}>
                      <div style={{ height: '100%', width: `${pct * 100}%`, background: barColor }} />
                    </div>
                    <div style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>
                      {fmtNum(spent)} / {fmtNum(allocated)}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
      </div>

      {/* Event feed */}
      <div>
        <div style={sectionLabel}>event log (tail)</div>
        {events.length === 0
          ? <div style={mutedText}>no events recorded yet</div>
          : events.map((ev, i) => (
              <div key={i} style={{
                display: 'flex', gap: 10, padding: '4px 0',
                borderBottom: i < events.length - 1 ? '1px solid var(--border)' : 'none',
                fontFamily: 'var(--mono)', fontSize: 10.5,
              }}>
                <span style={{ color: 'var(--text-dim)', flexShrink: 0 }}>{(ev.timestamp || '').slice(11, 19) || '—'}</span>
                <span style={{ color: 'var(--text-secondary)' }}>{ev.event_name || '—'}</span>
              </div>
            ))}
      </div>
    </div>
  );
}
