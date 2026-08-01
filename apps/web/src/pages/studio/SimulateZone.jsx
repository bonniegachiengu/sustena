/**
 * apps/web/src/pages/studio/SimulateZone.jsx
 *
 * SIMULATE + OPTIMISE. Wired to the already-built POST /devui/simulate
 * (forks live state, replays a step sequence under the same S2/S7 gates,
 * never persists) and POST /devui/sustain/{id}/promote-simulation (re-runs
 * the sequence for real, against CURRENT live state — not a shortcut that
 * trusts the earlier simulation). Every simulated result is marked
 * unmistakably hypothetical, matching the honesty discipline the existing
 * Simulator panel already established.
 */
import React from 'react';
import { api } from '../../lib/api.js';
import { sectionLabel, mutedText, Empty, ghostButton, primaryButton, dangerButton } from './shared.jsx';

const { useState, useEffect, useCallback } = React;

function coerce(raw) {
  const trimmed = raw.trim();
  if (trimmed === '') return '';
  if (!Number.isNaN(Number(trimmed)) && trimmed !== '') return Number(trimmed);
  return trimmed;
}

export default function SimulateZone({ sustainId }) {
  const [operators, setOperators] = useState([]);
  const [chosenOp, setChosenOp] = useState(null);
  const [paramValues, setParamValues] = useState({});
  const [steps, setSteps] = useState([]); // [{operator, params}]
  const [running, setRunning] = useState(false);
  const [simResult, setSimResult] = useState(null);
  const [promoting, setPromoting] = useState(false);
  const [promoteResult, setPromoteResult] = useState(null);
  const [confirming, setConfirming] = useState(false);

  const load = useCallback(async () => {
    if (!sustainId) return;
    try {
      const resp = await api.get(`/devui/sustain/${encodeURIComponent(sustainId)}/operators`);
      const ops = resp?.data?.operators || [];
      setOperators(ops);
      if (ops.length > 0) setChosenOp(ops[0].name);
    } catch { setOperators([]); }
  }, [sustainId]);

  useEffect(() => {
    load();
    setSteps([]); setSimResult(null); setPromoteResult(null); setConfirming(false);
  }, [load]);

  const addStep = () => {
    if (!chosenOp) return;
    const params = {};
    for (const [k, v] of Object.entries(paramValues)) if (v !== '') params[k] = coerce(v);
    setSteps(s => [...s, { operator: chosenOp, params }]);
    setParamValues({});
  };

  const removeStep = (i) => setSteps(s => s.filter((_, idx) => idx !== i));

  const runSim = async () => {
    if (steps.length === 0) return;
    setRunning(true);
    setSimResult(null);
    setPromoteResult(null);
    try {
      const resp = await api.post('/devui/simulate', { sustain_id: sustainId, proposal: steps });
      setSimResult(resp?.data || null);
    } catch (e) {
      setSimResult({ error: e.message || 'simulation unreachable' });
    } finally {
      setRunning(false);
    }
  };

  const promote = async () => {
    setPromoting(true);
    try {
      const resp = await api.post(`/devui/sustain/${encodeURIComponent(sustainId)}/promote-simulation`, { steps });
      setPromoteResult(resp?.data || null);
    } catch (e) {
      setPromoteResult({ error: e.message || 'promotion unreachable' });
    } finally {
      setPromoting(false);
      setConfirming(false);
    }
  };

  if (!sustainId) return <Empty text="select a sustain to simulate" />;
  if (operators.length === 0) return <Empty text="no operators declared for this sustain" />;

  const op = operators.find(o => o.name === chosenOp);

  return (
    <div>
      <div style={sectionLabel}>branch — build a hypothetical sequence</div>
      <div style={{ display: 'flex', gap: 10, alignItems: 'flex-end', marginBottom: 12, flexWrap: 'wrap' }}>
        <label>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 9.5, color: 'var(--text-dim)', marginBottom: 3 }}>operator</div>
          <select
            value={chosenOp || ''}
            onChange={e => { setChosenOp(e.target.value); setParamValues({}); }}
            style={{
              fontFamily: 'var(--mono)', fontSize: 11.5, color: 'var(--text-primary)',
              background: 'var(--bg-raised)', border: '1px solid var(--border-mid)',
              borderRadius: 'var(--radius-sm)', padding: '7px 10px', minWidth: 180,
            }}
          >
            {operators.map(o => <option key={o.name} value={o.name}>{o.name}</option>)}
          </select>
        </label>
        {op?.params.map(p => (
          <label key={p}>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 9.5, color: 'var(--text-dim)', marginBottom: 3 }}>{p}</div>
            <input
              value={paramValues[p] ?? ''}
              onChange={e => setParamValues(v => ({ ...v, [p]: e.target.value }))}
              style={{
                fontFamily: 'var(--mono)', fontSize: 11.5, color: 'var(--text-primary)',
                background: 'var(--bg-raised)', border: '1px solid var(--border-mid)',
                borderRadius: 'var(--radius-sm)', padding: '7px 10px', width: 110,
              }}
            />
          </label>
        ))}
        <button onClick={addStep} style={ghostButton}>+ ADD STEP</button>
      </div>

      {steps.length > 0 && (
        <div style={{ marginBottom: 14 }}>
          {steps.map((s, i) => (
            <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 0' }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)' }}>{i + 1}.</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)' }}>{s.operator}</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)' }}>{JSON.stringify(s.params)}</span>
              <button onClick={() => removeStep(i)} style={{ ...ghostButton, padding: '2px 8px', fontSize: 9 }}>×</button>
            </div>
          ))}
        </div>
      )}

      <button onClick={runSim} disabled={steps.length === 0 || running} style={{ ...primaryButton, opacity: (steps.length === 0 || running) ? 0.5 : 1 }}>
        {running ? 'SIMULATING…' : 'RUN BRANCH'}
      </button>

      {simResult && !simResult.error && (
        <div style={{ marginTop: 18 }}>
          <div style={{
            display: 'inline-block', fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 700, letterSpacing: '0.06em',
            color: 'var(--amber)', border: '1px solid var(--amber-border)', background: 'var(--amber-glow)',
            borderRadius: 'var(--radius-sm)', padding: '3px 8px', marginBottom: 10,
          }}>
            ◆ HYPOTHETICAL — SIMULATED · nothing written to live state
          </div>

          {(simResult.steps || []).map((s, i) => (
            <div key={i} style={{ marginBottom: 10, padding: '10px 12px', background: 'var(--bg-raised)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)' }}>
              <div style={{ fontFamily: 'var(--mono)', fontSize: 10.5, color: s.result?.status === 'ok' ? 'var(--teal)' : 'var(--danger)' }}>
                step {i + 1} · {s.operator} · {s.result?.status === 'ok' ? '✓ would succeed' : `✗ ${s.result?.reason || 'would be refused'}`}
              </div>
            </div>
          ))}

          <div style={{ ...mutedText, marginTop: 4, marginBottom: 10 }}>
            projected final liquid balance: <b style={{ color: 'var(--text-primary)' }}>
              {simResult.final_state?.finances?.liquid?.balance ?? '—'}
            </b>
            {simResult.final_parent_rollup?.aggregates && Object.entries(simResult.final_parent_rollup.aggregates).map(([id, agg]) => (
              <span key={id}> · {id.replace(/_/g, ' ')}: <b style={{ color: 'var(--amber)' }}>{agg.value}</b></span>
            ))}
          </div>

          {!confirming ? (
            <button onClick={() => setConfirming(true)} style={ghostButton}>PROMOTE THIS BRANCH TO REALITY</button>
          ) : (
            <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
              <span style={{ ...mutedText, color: 'var(--danger)' }}>
                this re-runs every step for real, against current live state — confirm?
              </span>
              <button onClick={promote} disabled={promoting} style={dangerButton}>{promoting ? 'PROMOTING…' : 'YES, PROMOTE'}</button>
              <button onClick={() => setConfirming(false)} style={ghostButton}>CANCEL</button>
            </div>
          )}
        </div>
      )}

      {simResult?.error && <div style={{ ...mutedText, marginTop: 12, color: 'var(--danger)' }}>{simResult.error}</div>}

      {promoteResult && !promoteResult.error && (
        <div style={{ marginTop: 14, padding: '10px 12px', borderRadius: 'var(--radius-sm)', background: 'var(--bg-raised)', border: '1px solid var(--border-mid)' }}>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 11, color: promoteResult.all_succeeded ? 'var(--teal)' : 'var(--warn)' }}>
            {promoteResult.all_succeeded ? '✓ promoted — every step committed for real' : `⚠ ${promoteResult.steps_attempted}/${promoteResult.steps_requested} steps committed before a real refusal`}
          </div>
          {(promoteResult.steps || []).filter(s => s.result?.status !== 'ok').map((s, i) => (
            <div key={i} style={{ ...mutedText, marginTop: 4, color: 'var(--danger)' }}>{s.operator}: {s.result?.reason}</div>
          ))}
        </div>
      )}
    </div>
  );
}
