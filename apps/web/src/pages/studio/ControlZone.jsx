/**
 * apps/web/src/pages/studio/ControlZone.jsx
 *
 * EXECUTE. Wired to the already-built GET /devui/sustain/{id}/operators
 * (the allow-listed operator set — the transition set T) and POST
 * /devui/console/execute, which runs the operator through
 * SustainEngine.execute_operator() — the real S2 enforcing gate + S3 fold,
 * identical to every other write path in this codebase. No parallel
 * execution logic lives here.
 */
import React from 'react';
import { api } from '../../lib/api.js';
import { sectionLabel, mutedText, Empty, ghostButton, primaryButton } from './shared.jsx';

const { useState, useEffect, useCallback } = React;

function coerce(raw) {
  const trimmed = raw.trim();
  if (trimmed === '') return '';
  if (trimmed === 'true') return true;
  if (trimmed === 'false') return false;
  if (!Number.isNaN(Number(trimmed)) && trimmed !== '') return Number(trimmed);
  if ((trimmed.startsWith('{') && trimmed.endsWith('}')) || (trimmed.startsWith('[') && trimmed.endsWith(']'))) {
    try { return JSON.parse(trimmed); } catch { /* fall through to plain string */ }
  }
  return trimmed;
}

export default function ControlZone({ sustainId }) {
  const [operators, setOperators] = useState([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState(null);
  const [paramValues, setParamValues] = useState({});
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState(null);

  const load = useCallback(async () => {
    if (!sustainId) return;
    setLoading(true);
    try {
      const resp = await api.get(`/devui/sustain/${encodeURIComponent(sustainId)}/operators`);
      const ops = resp?.data?.operators || [];
      setOperators(ops);
      if (ops.length > 0) setSelected(ops[0].name);
    } catch {
      setOperators([]);
    } finally {
      setLoading(false);
    }
  }, [sustainId]);

  useEffect(() => { load(); setResult(null); setParamValues({}); }, [load]);

  const chooseOperator = (name) => {
    setSelected(name);
    setParamValues({});
    setResult(null);
  };

  const run = async () => {
    if (!selected) return;
    setRunning(true);
    setResult(null);
    const params = {};
    for (const [k, v] of Object.entries(paramValues)) {
      if (v !== undefined && v !== '') params[k] = coerce(v);
    }
    try {
      const resp = await api.post('/devui/console/execute', {
        sustain_id: sustainId, operator: selected, params,
      });
      setResult(resp?.data || null);
    } catch (e) {
      setResult({ result: { status: 'failed', reason: e.message || 'unreachable' } });
    } finally {
      setRunning(false);
    }
  };

  if (!sustainId) return <Empty text="select a sustain to control" />;
  if (loading) return <Empty text="loading operators…" />;
  if (operators.length === 0) return <Empty text="no operators declared for this sustain" />;

  const op = operators.find(o => o.name === selected);
  const outcome = result?.result;

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '220px 1fr', gap: 20 }}>
      <div style={{ borderRight: '1px solid var(--border)', paddingRight: 14, maxHeight: 380, overflowY: 'auto' }}>
        <div style={sectionLabel}>operators — T</div>
        {operators.map(o => (
          <div
            key={o.name}
            onClick={() => chooseOperator(o.name)}
            style={{
              padding: '7px 8px', borderRadius: 'var(--radius-sm)', cursor: 'pointer',
              background: o.name === selected ? 'var(--bg-overlay)' : 'transparent',
              marginBottom: 2,
            }}
          >
            <div style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)' }}>{o.name}</div>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)' }}>{o.protocol} · pawa {o.pawa_cost}</div>
          </div>
        ))}
      </div>

      <div>
        {op && (
          <>
            <div style={sectionLabel}>run through the gate</div>
            <div style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)', marginBottom: 4 }}>{op.name}</div>
            <div style={{ ...mutedText, marginBottom: 14 }}>{op.description || 'no description declared'}</div>

            {op.params.length === 0
              ? <div style={{ ...mutedText, marginBottom: 14 }}>takes no parameters</div>
              : (
                <div style={{ display: 'grid', gap: 10, marginBottom: 14, maxWidth: 360 }}>
                  {op.params.map(p => (
                    <label key={p} style={{ display: 'block' }}>
                      <div style={{ fontFamily: 'var(--mono)', fontSize: 9.5, color: 'var(--text-dim)', marginBottom: 3 }}>{p}</div>
                      <input
                        value={paramValues[p] ?? ''}
                        onChange={e => setParamValues(v => ({ ...v, [p]: e.target.value }))}
                        style={{
                          width: '100%', fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-primary)',
                          background: 'var(--bg-raised)', border: '1px solid var(--border-mid)',
                          borderRadius: 'var(--radius-sm)', padding: '7px 10px',
                        }}
                      />
                    </label>
                  ))}
                </div>
              )}

            <button onClick={run} disabled={running} style={{ ...primaryButton, opacity: running ? 0.6 : 1 }}>
              {running ? 'RUNNING…' : 'RUN'}
            </button>

            {outcome && (
              <div style={{
                marginTop: 16, padding: '12px 14px', borderRadius: 'var(--radius-sm)',
                background: 'var(--bg-raised)', border: '1px solid var(--border-mid)',
              }}>
                {outcome.status === 'ok' ? (
                  <div style={{ fontFamily: 'var(--mono)', fontSize: 11.5, color: 'var(--teal)' }}>
                    ✓ executed — state updated, real event appended
                  </div>
                ) : (
                  <div style={{ fontFamily: 'var(--mono)', fontSize: 11.5, color: 'var(--danger)' }}>
                    ✗ {outcome.status === 'deferred' ? 'deferred to council' : 'refused'} — {outcome.reason || 'the gate refused this write'}
                  </div>
                )}
                {outcome.data && Object.keys(outcome.data).length > 0 && (
                  <pre style={{
                    marginTop: 8, fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)',
                    whiteSpace: 'pre-wrap', maxHeight: 140, overflowY: 'auto',
                  }}>
                    {JSON.stringify(outcome.data, null, 2)}
                  </pre>
                )}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
