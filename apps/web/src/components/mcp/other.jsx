import React from 'react';
import { api } from '../../lib/api.js';
/* Editor + Controller + Library panels (consolidated) */

const { useState: dUseState, useEffect: dUseEffect, useMemo: dUseMemo, useRef: dUseRef } = React;

/* ───────────────────────────────────────────────────────────
   EDITOR PANEL — Sustena primitives editor
   Sections: SUSTAIN · OPERATORS · OPERATIVES · CONSTRAINTS
   ─────────────────────────────────────────────────────────── */

function EditorPanel({ sustain }) {
  const [section, setSection] = dUseState('sustain');
  const [liveState, setLiveState] = dUseState(null);
  const [stateLoading, setStateLoading] = dUseState(false);
  const [operators, setOperators] = dUseState([]);
  const [graphData, setGraphData] = dUseState({ nodes: [], edges: [] });
  const [constraints, setConstraints] = dUseState([]);

  const sustainId = sustain?.id;

  const loadAll = dUseRef(async (sid) => {
    if (!sid) return;
    setStateLoading(true);
    api.get(`/devui/state?sustain_id=${encodeURIComponent(sid)}`)
      .then(d => { setLiveState(d?.data?.state || {}); setConstraints(d?.data?.constraints || []); })
      .catch(() => { setLiveState({}); setConstraints([]); })
      .finally(() => setStateLoading(false));
    api.get(`/devui/sustain/${encodeURIComponent(sid)}/operators`)
      .then(d => setOperators(d?.data?.operators || []))
      .catch(() => {});
    api.get(`/devui/sustain/${encodeURIComponent(sid)}/graph`)
      .then(d => setGraphData({ nodes: d?.data?.nodes || [], edges: d?.data?.edges || [] }))
      .catch(() => {});
  });

  dUseEffect(() => { loadAll.current(sustainId); }, [sustainId]);

  const refreshState = async () => {
    if (!sustainId) return;
    try {
      const d = await api.get(`/devui/state?sustain_id=${encodeURIComponent(sustainId)}`);
      setLiveState(d?.data?.state || {});
      setConstraints(d?.data?.constraints || []);
    } catch {}
  };

  const SECTIONS = [
    { id: 'sustain',     label: 'SUSTAIN',     icon: '▭',  sub: 'state · patches' },
    { id: 'operators',   label: 'OPERATORS',   icon: '◖◗', sub: 'spec · cost · params' },
    { id: 'operatives',  label: 'OPERATIVES',  icon: '⬢',  sub: 'network · delegation' },
    { id: 'constraints', label: 'CONSTRAINTS', icon: '◆',  sub: 'rules · pass / fail' },
  ];

  return (
    <div className="panel-enter" style={{ display: 'grid', gridTemplateColumns: '196px 1fr', height: '100%' }}>

      {/* Left nav rail */}
      <div style={{ borderRight: '1px solid var(--border)', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
        <div style={{ padding: '18px 14px 14px', borderBottom: '1px solid var(--border)', flexShrink: 0 }}>
          <span className="label-10" style={{ color: 'var(--text-muted)', display: 'block', marginBottom: 6 }}>EDITOR</span>
          <div style={{ fontFamily: 'var(--ui)', fontSize: 14, fontWeight: 500, color: 'var(--text-primary)', lineHeight: 1.3 }}>
            {sustain?.label || '—'}
          </div>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', marginTop: 3, display: 'block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {sustainId}
          </span>
        </div>

        <nav style={{ display: 'flex', flexDirection: 'column', gap: 2, padding: '8px 6px', flex: 1 }}>
          {SECTIONS.map(s => {
            const active = section === s.id;
            return (
              <button key={s.id} onClick={() => setSection(s.id)} style={{
                display: 'flex', alignItems: 'center', gap: 10,
                padding: '9px 10px', borderRadius: 'var(--radius-sm)',
                background: active ? 'var(--amber-glow)' : 'transparent',
                border: `1px solid ${active ? 'var(--amber-border)' : 'transparent'}`,
                transition: 'all var(--t-fast)', textAlign: 'left', cursor: 'pointer',
              }}
              onMouseEnter={e => { if (!active) e.currentTarget.style.background = 'var(--bg-raised)'; }}
              onMouseLeave={e => { if (!active) e.currentTarget.style.background = 'transparent'; }}
              >
                <span style={{ fontFamily: 'var(--mono)', fontSize: 13, color: active ? 'var(--amber)' : 'var(--text-muted)', width: 16, textAlign: 'center', flexShrink: 0 }}>{s.icon}</span>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 1, minWidth: 0 }}>
                  <span style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.06em', color: active ? 'var(--amber)' : 'var(--text-secondary)' }}>{s.label}</span>
                  <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)', letterSpacing: '0.04em' }}>{s.sub}</span>
                </div>
              </button>
            );
          })}
        </nav>

        <div style={{ padding: '10px 10px', borderTop: '1px solid var(--border)', flexShrink: 0 }}>
          <button onClick={() => loadAll.current(sustainId)} style={{
            width: '100%', padding: '6px 0', fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.08em',
            color: 'var(--text-muted)', background: 'transparent', border: '1px solid var(--border)',
            borderRadius: 'var(--radius-sm)', cursor: 'pointer', transition: 'all var(--t-fast)',
          }}
          onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
          onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border)'; e.currentTarget.style.color = 'var(--text-muted)'; }}
          >↻ REFRESH</button>
        </div>
      </div>

      {/* Main area */}
      <div style={{ overflow: 'hidden', display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        {section === 'sustain'     && <SustainEditorSection sustain={sustain} liveState={liveState} stateLoading={stateLoading} onRefresh={refreshState} />}
        {section === 'operators'   && <OperatorsEditorSection sustain={sustain} operators={operators} />}
        {section === 'operatives'  && <OperativesEditorSection graphData={graphData} />}
        {section === 'constraints' && <ConstraintsEditorSection constraints={constraints} />}
      </div>
    </div>
  );
}

/* ── SUSTAIN section — state tree + patch editor ──────────────────────────── */
function SustainEditorSection({ sustain, liveState, stateLoading, onRefresh }) {
  const [patchPath, setPatchPath] = dUseState('');
  const [patchValue, setPatchValue] = dUseState('');
  const [patchOp, setPatchOp] = dUseState('replace');
  const [applying, setApplying] = dUseState(false);
  const [result, setResult] = dUseState(null);
  const [error, setError] = dUseState(null);

  const onSelectPath = (path, currentVal) => {
    setPatchPath(path);
    setResult(null); setError(null);
    if (currentVal !== undefined && currentVal !== null) {
      setPatchValue(typeof currentVal === 'object' ? JSON.stringify(currentVal, null, 2) : String(currentVal));
    }
  };

  const applyPatch = async () => {
    if (!patchPath.trim()) return;
    let value;
    if (patchOp !== 'remove') {
      try { value = JSON.parse(patchValue); } catch { value = patchValue; }
    }
    const patch = patchOp === 'remove'
      ? [{ op: 'remove', path: patchPath.trim() }]
      : [{ op: patchOp, path: patchPath.trim(), value }];

    setApplying(true); setError(null); setResult(null);
    try {
      const res = await api.post('/devui/console/execute', {
        sustain_id: sustain?.id || '',
        operator_id: 'edit.state_patch',
        params: { patch },
      });
      const data = res?.data?.result?.data || {};
      if (data.error_count > 0 && !data.applied_count) {
        setError(data.errors?.[0] || 'Patch failed');
      } else {
        setResult(data);
        await onRefresh();
        window.flash?.(`State patched · ${data.applied_count ?? 0} ops`, 'ok');
      }
    } catch (err) {
      setError(err.message || 'API error');
    } finally {
      setApplying(false);
    }
  };

  const inputBase = { background: 'var(--bg-base)', border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)', padding: '7px 10px', fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)', outline: 'none', width: '100%', boxSizing: 'border-box' };

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 300px', height: '100%', minHeight: 0 }}>
      {/* State tree */}
      <div style={{ display: 'flex', flexDirection: 'column', borderRight: '1px solid var(--border)', overflow: 'hidden' }}>
        <div style={{ padding: '10px 16px', borderBottom: '1px solid var(--border)', flexShrink: 0, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span className="label-10">STATE TREE</span>
          <span className="meta-10" style={{ color: 'var(--text-dim)' }}>click any value to select path</span>
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: '12px 16px' }}>
          {stateLoading
            ? <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)' }}>loading…</span>
            : liveState && Object.keys(liveState).length > 0
              ? <StateTreeNode data={liveState} path="" selectedPath={patchPath} onSelect={onSelectPath} depth={0} />
              : <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>no state recorded yet · seed via SEED panel</span>
          }
        </div>
      </div>

      {/* Patch form */}
      <div style={{ display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
        <div style={{ padding: '10px 16px', borderBottom: '1px solid var(--border)', flexShrink: 0 }}>
          <span className="label-10">PATCH STATE · edit.state_patch</span>
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: '14px 16px', display: 'flex', flexDirection: 'column', gap: 12 }}>
          <div>
            <span className="label-10" style={{ display: 'block', marginBottom: 5 }}>OPERATION</span>
            <div style={{ display: 'flex', gap: 4 }}>
              {['replace', 'add', 'remove'].map(op => (
                <button key={op} onClick={() => setPatchOp(op)} style={{
                  padding: '4px 10px', fontFamily: 'var(--mono)', fontSize: 9, cursor: 'pointer',
                  color: patchOp === op ? 'var(--amber)' : 'var(--text-muted)',
                  background: patchOp === op ? 'var(--amber-glow)' : 'transparent',
                  border: `1px solid ${patchOp === op ? 'var(--amber-border)' : 'var(--border)'}`,
                  borderRadius: 3, transition: 'all var(--t-fast)',
                }}>{op}</button>
              ))}
            </div>
          </div>
          <div>
            <span className="label-10" style={{ display: 'block', marginBottom: 5 }}>PATH</span>
            <input value={patchPath} onChange={e => setPatchPath(e.target.value)} placeholder="e.g. finances.liquid.balance" style={inputBase} />
          </div>
          {patchOp !== 'remove' && (
            <div>
              <span className="label-10" style={{ display: 'block', marginBottom: 5 }}>VALUE</span>
              <textarea value={patchValue} onChange={e => setPatchValue(e.target.value)} rows={4} placeholder={'5000\n"string"\n{"key":"val"}'} style={{ ...inputBase, resize: 'vertical', lineHeight: 1.5 }} />
            </div>
          )}
          {error && <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--danger)' }}>{error}</span>}
          {result && (
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--teal)' }}>
              ✓ {result.applied_count} applied{result.error_count > 0 ? ` · ${result.error_count} errors` : ''}
            </span>
          )}
          <button onClick={applyPatch} disabled={!patchPath.trim() || applying} style={{
            padding: '8px 0', fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
            background: (!patchPath.trim() || applying) ? 'var(--bg-overlay)' : 'var(--amber)',
            color: (!patchPath.trim() || applying) ? 'var(--text-dim)' : 'var(--bg-base)',
            border: 'none', borderRadius: 'var(--radius-sm)',
            cursor: (!patchPath.trim() || applying) ? 'not-allowed' : 'pointer', transition: 'all var(--t-fast)',
          }}>{applying ? '⟳ APPLYING…' : 'APPLY PATCH'}</button>
        </div>
      </div>
    </div>
  );
}

function StateTreeNode({ data, path, selectedPath, onSelect, depth }) {
  const [expanded, setExpanded] = dUseState(depth < 2);
  if (data === null || data === undefined) {
    return <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>null</span>;
  }
  if (typeof data !== 'object') {
    const isSelected = path === selectedPath;
    return (
      <button onClick={() => onSelect(path, data)} style={{
        fontFamily: 'var(--mono)', fontSize: 10,
        color: isSelected ? 'var(--amber)' : typeof data === 'number' ? 'var(--node-state)' : 'var(--text-secondary)',
        background: isSelected ? 'var(--amber-glow)' : 'transparent',
        border: `1px solid ${isSelected ? 'var(--amber-border)' : 'transparent'}`,
        borderRadius: 2, padding: '1px 5px', cursor: 'pointer', transition: 'all var(--t-fast)',
      }}>{typeof data === 'string' ? `"${data}"` : String(data)}</button>
    );
  }
  const entries = Object.entries(data);
  if (entries.length === 0) return <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>{Array.isArray(data) ? '[]' : '{}'}</span>;
  return (
    <div style={{ display: 'flex', flexDirection: 'column' }}>
      {entries.map(([key, val]) => {
        const childPath = path ? `${path}.${key}` : key;
        const isObj = typeof val === 'object' && val !== null;
        const isSelected = childPath === selectedPath;
        return (
          <div key={key} style={{ display: 'flex', flexDirection: 'column', paddingLeft: depth > 0 ? 14 : 0, marginBottom: 1 }}>
            <div style={{ display: 'flex', alignItems: 'baseline', gap: 3 }}>
              {isObj
                ? <button onClick={() => setExpanded(e => !e)} style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', background: 'none', border: 'none', cursor: 'pointer', padding: '1px 2px', minWidth: 10, lineHeight: 1 }}>{expanded ? '▾' : '▸'}</button>
                : <span style={{ minWidth: 14 }} />
              }
              <button onClick={() => onSelect(childPath, val)} style={{
                fontFamily: 'var(--mono)', fontSize: 10, color: isSelected ? 'var(--amber)' : 'var(--node-operator)',
                background: isSelected ? 'var(--amber-glow)' : 'transparent',
                border: `1px solid ${isSelected ? 'var(--amber-border)' : 'transparent'}`,
                borderRadius: 2, padding: '1px 4px', cursor: 'pointer', transition: 'all var(--t-fast)',
              }}>{key}</button>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--border-light)' }}>:</span>
              {isObj
                ? <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>{Array.isArray(val) ? `[${val.length}]` : `{${Object.keys(val).length}}`}</span>
                : <StateTreeNode data={val} path={childPath} selectedPath={selectedPath} onSelect={onSelect} depth={depth + 1} />
              }
            </div>
            {isObj && expanded && (
              <div>
                <StateTreeNode data={val} path={childPath} selectedPath={selectedPath} onSelect={onSelect} depth={depth + 1} />
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/* ── OPERATORS section — list + spec editor ───────────────────────────────── */
function OperatorsEditorSection({ sustain, operators }) {
  const [selectedOp, setSelectedOp] = dUseState(null);
  const [editDesc, setEditDesc] = dUseState('');
  const [editPawa, setEditPawa] = dUseState('');
  const [saving, setSaving] = dUseState(false);
  const [saveResult, setSaveResult] = dUseState(null);

  dUseEffect(() => {
    if (selectedOp) {
      setEditDesc(selectedOp.description || '');
      setEditPawa(String(selectedOp.pawa_cost ?? 0));
      setSaveResult(null);
    }
  }, [selectedOp?.name]);

  const saveField = async (field, value) => {
    if (!selectedOp) return;
    setSaving(true); setSaveResult(null);
    try {
      await api.post('/devui/console/execute', {
        sustain_id: sustain?.id || '',
        operator_id: 'edit.operator_spec',
        params: { operator_name: selectedOp.name, field, value },
      });
      setSaveResult({ field, ok: true });
      window.flash?.(`${selectedOp.name} · ${field} saved`, 'ok');
    } catch (err) {
      setSaveResult({ field, ok: false, msg: err.message });
    } finally {
      setSaving(false);
    }
  };

  const PROTOCOL_COLOR = { rpc: 'var(--teal)', event_driven: 'var(--node-event)', polling: 'var(--text-muted)', streaming: 'var(--amber)' };
  const inputBase = { background: 'var(--bg-base)', border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)', padding: '7px 10px', fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)', outline: 'none', width: '100%', boxSizing: 'border-box' };

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '260px 1fr', height: '100%', minHeight: 0 }}>
      {/* Operator list */}
      <div style={{ borderRight: '1px solid var(--border)', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
        <div style={{ padding: '10px 16px', borderBottom: '1px solid var(--border)', flexShrink: 0 }}>
          <span className="label-10">OPERATORS · {operators.length}</span>
        </div>
        <div style={{ flex: 1, overflow: 'auto' }}>
          {operators.length === 0
            ? <div style={{ padding: '16px', fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>no operators · sustain is clear</div>
            : operators.map(op => {
              const active = selectedOp?.name === op.name;
              const pc = PROTOCOL_COLOR[op.protocol] || 'var(--text-muted)';
              return (
                <button key={op.name} onClick={() => setSelectedOp(op)} style={{
                  width: '100%', textAlign: 'left', display: 'flex', flexDirection: 'column', gap: 3,
                  padding: '10px 16px', borderBottom: '1px solid var(--border)',
                  background: active ? 'var(--amber-glow)' : 'transparent',
                  cursor: 'pointer', transition: 'background var(--t-fast)',
                }}
                onMouseEnter={e => { if (!active) e.currentTarget.style.background = 'var(--bg-raised)'; }}
                onMouseLeave={e => { if (!active) e.currentTarget.style.background = 'transparent'; }}
                >
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 6 }}>
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, color: active ? 'var(--amber)' : 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', flex: 1 }}>{op.name}</span>
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: pc, flexShrink: 0, letterSpacing: '0.06em' }}>{(op.protocol || 'rpc').toUpperCase()}</span>
                  </div>
                  <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{op.description || '—'}</span>
                  <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)' }}>{op.pawa_cost ?? 0} pwa</span>
                </button>
              );
            })
          }
        </div>
      </div>

      {/* Operator editor */}
      {!selectedOp
        ? <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>select an operator to edit</span>
          </div>
        : <div style={{ display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
            <div style={{ padding: '10px 20px', borderBottom: '1px solid var(--border)', flexShrink: 0, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 12, fontWeight: 500, color: 'var(--text-primary)' }}>{selectedOp.name}</span>
              <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: PROTOCOL_COLOR[selectedOp.protocol] || 'var(--text-muted)', letterSpacing: '0.06em' }}>{(selectedOp.protocol || 'rpc').toUpperCase()}</span>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--amber)' }}>{selectedOp.pawa_cost ?? 0} pwa</span>
              </div>
            </div>
            <div style={{ flex: 1, overflow: 'auto', padding: '18px 20px', display: 'flex', flexDirection: 'column', gap: 16, maxWidth: 520 }}>

              {/* Description */}
              <div>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
                  <span className="label-10">DESCRIPTION</span>
                  <button onClick={() => saveField('description', editDesc)} disabled={saving || editDesc === (selectedOp.description || '')} style={{
                    padding: '3px 10px', fontFamily: 'var(--mono)', fontSize: 9, cursor: 'pointer',
                    color: (saving || editDesc === (selectedOp.description || '')) ? 'var(--text-dim)' : 'var(--teal)',
                    background: 'transparent',
                    border: `1px solid ${(saving || editDesc === (selectedOp.description || '')) ? 'var(--border)' : 'var(--teal-border)'}`,
                    borderRadius: 3, transition: 'all var(--t-fast)',
                  }}>SAVE</button>
                </div>
                <textarea value={editDesc} onChange={e => setEditDesc(e.target.value)} rows={3} style={{ ...inputBase, resize: 'vertical', lineHeight: 1.5, fontFamily: 'var(--ui)', fontSize: 12 }} />
              </div>

              {/* Pawa cost */}
              <div>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
                  <span className="label-10">PAWA COST</span>
                  <button onClick={() => saveField('pawa_cost', parseInt(editPawa, 10) || 0)} disabled={saving || editPawa === String(selectedOp.pawa_cost ?? 0)} style={{
                    padding: '3px 10px', fontFamily: 'var(--mono)', fontSize: 9, cursor: 'pointer',
                    color: (saving || editPawa === String(selectedOp.pawa_cost ?? 0)) ? 'var(--text-dim)' : 'var(--teal)',
                    background: 'transparent',
                    border: `1px solid ${(saving || editPawa === String(selectedOp.pawa_cost ?? 0)) ? 'var(--border)' : 'var(--teal-border)'}`,
                    borderRadius: 3, transition: 'all var(--t-fast)',
                  }}>SAVE</button>
                </div>
                <input type="number" value={editPawa} onChange={e => setEditPawa(e.target.value)} min={0} style={inputBase} />
              </div>

              {/* Params — read-only */}
              {selectedOp.params && selectedOp.params.length > 0 && (
                <div>
                  <span className="label-10" style={{ display: 'block', marginBottom: 8 }}>PARAMS</span>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                    {selectedOp.params.map((p, i) => (
                      <div key={i} style={{ display: 'flex', justifyContent: 'space-between', padding: '6px 10px', background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)' }}>
                        <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--node-operator)' }}>{p.name ?? p}</span>
                        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>{p.type ?? 'any'}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {saveResult && (
                <div style={{ padding: '8px 12px', borderRadius: 'var(--radius-sm)', background: saveResult.ok ? 'rgba(42,184,160,0.08)' : 'rgba(224,80,80,0.08)', border: `1px solid ${saveResult.ok ? 'var(--teal-border)' : 'rgba(224,80,80,0.3)'}`, fontFamily: 'var(--mono)', fontSize: 10, color: saveResult.ok ? 'var(--teal)' : 'var(--danger)' }}>
                  {saveResult.ok ? `✓ ${saveResult.field} saved` : `✗ ${saveResult.msg}`}
                </div>
              )}
            </div>
          </div>
      }
    </div>
  );
}

/* ── OPERATIVES section — network tree + detail ────────────────────────────── */
function OperativesEditorSection({ graphData }) {
  const [selected, setSelected] = dUseState(null);
  const { nodes, edges } = graphData;

  const orchie = nodes.find(n => n.type === 'orchie');
  const operatives = nodes.filter(n => n.type === 'operative');
  const subOps = nodes.filter(n => n.type === 'sub_operative');
  const getSubOps = (parentId) => subOps.filter(s => s.parent === parentId);

  const delegatesTo = selected ? edges.filter(e => e.from === selected.id) : [];
  const delegatesFrom = selected ? edges.filter(e => e.to === selected.id) : [];

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '260px 1fr', height: '100%', minHeight: 0 }}>
      {/* Network tree */}
      <div style={{ borderRight: '1px solid var(--border)', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
        <div style={{ padding: '10px 16px', borderBottom: '1px solid var(--border)', flexShrink: 0 }}>
          <span className="label-10">NETWORK · {nodes.length} NODES</span>
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: '10px 8px' }}>
          {nodes.length === 0
            ? <div style={{ padding: '8px', fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>no operatives reporting · all thresholds nominal</div>
            : <div style={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
                {orchie && <OpNetworkNode node={orchie} selected={selected?.id === orchie.id} onSelect={setSelected} />}
                {operatives.map(op => (
                  <div key={op.id}>
                    <div style={{ paddingLeft: 14 }}>
                      <OpNetworkNode node={op} selected={selected?.id === op.id} onSelect={setSelected} />
                    </div>
                    {getSubOps(op.id).map(sub => (
                      <div key={sub.id} style={{ paddingLeft: 28 }}>
                        <OpNetworkNode node={sub} selected={selected?.id === sub.id} onSelect={setSelected} isSub />
                      </div>
                    ))}
                  </div>
                ))}
              </div>
          }
        </div>
      </div>

      {/* Detail */}
      {!selected
        ? <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>select an operative to inspect</span>
          </div>
        : <div style={{ display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
            <div style={{ padding: '10px 20px', borderBottom: '1px solid var(--border)', flexShrink: 0, display: 'flex', alignItems: 'center', gap: 10 }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 12, fontWeight: 500, color: 'var(--text-primary)' }}>{selected.label}</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)', padding: '2px 8px', border: '1px solid var(--border)', borderRadius: 10 }}>{selected.type.replace('_', ' ')}</span>
            </div>
            <div style={{ flex: 1, overflow: 'auto', padding: '18px 20px', display: 'flex', flexDirection: 'column', gap: 16, maxWidth: 520 }}>
              <div>
                <span className="label-10" style={{ display: 'block', marginBottom: 5 }}>ROLE</span>
                <span style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-secondary)' }}>{selected.role}</span>
              </div>
              {selected.domain && selected.domain.length > 0 && (
                <div>
                  <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>DOMAINS</span>
                  <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
                    {selected.domain.map(d => (
                      <span key={d} style={{ padding: '3px 10px', fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--node-operative)', border: '1px solid rgba(42,184,160,0.3)', borderRadius: 10 }}>{d}</span>
                    ))}
                  </div>
                </div>
              )}
              {delegatesFrom.length > 0 && (
                <div>
                  <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>RECEIVES FROM</span>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                    {delegatesFrom.map((e, i) => <EdgeRow key={i} label={e.from} kind={e.type} direction="←" />)}
                  </div>
                </div>
              )}
              {delegatesTo.length > 0 && (
                <div>
                  <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>DELEGATES TO</span>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                    {delegatesTo.map((e, i) => <EdgeRow key={i} label={e.to} kind={e.type} direction="→" />)}
                  </div>
                </div>
              )}
              <div style={{ padding: '10px 12px', background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)' }}>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', lineHeight: 1.6 }}>
                  Operative structure (domain routing, sub-operative graphs, evaluation/deliberation specs) is defined in the sustain JSON. Edit the spec to restructure the network — the graph here reflects live config.
                </span>
              </div>
            </div>
          </div>
      }
    </div>
  );
}

function OpNetworkNode({ node, selected, onSelect, isSub }) {
  const typeColor = { orchie: 'var(--amber)', operative: 'var(--node-operative)', sub_operative: 'var(--text-secondary)' }[node.type] || 'var(--text-muted)';
  const icon = { orchie: '◉', operative: '⬢', sub_operative: '⬡' }[node.type] || '·';
  return (
    <button onClick={() => onSelect(node)} style={{
      width: '100%', textAlign: 'left', display: 'flex', alignItems: 'center', gap: 8,
      padding: isSub ? '5px 8px' : '7px 10px',
      borderRadius: 'var(--radius-sm)',
      background: selected ? `${typeColor}18` : 'transparent',
      border: `1px solid ${selected ? `${typeColor}40` : 'transparent'}`,
      cursor: 'pointer', transition: 'all var(--t-fast)', marginBottom: 1,
    }}
    onMouseEnter={e => { if (!selected) e.currentTarget.style.background = 'var(--bg-raised)'; }}
    onMouseLeave={e => { if (!selected) e.currentTarget.style.background = 'transparent'; }}
    >
      <span style={{ fontFamily: 'var(--mono)', fontSize: isSub ? 9 : 12, color: typeColor, flexShrink: 0 }}>{icon}</span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontFamily: 'var(--mono)', fontSize: isSub ? 9 : 10, fontWeight: 500, color: selected ? typeColor : 'var(--text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{node.label}</div>
        <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{node.role}</div>
      </div>
    </button>
  );
}

function EdgeRow({ label, kind, direction }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '5px 10px', background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)' }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>{direction}</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)', flex: 1 }}>{label}</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.06em' }}>{kind}</span>
    </div>
  );
}

/* ── CONSTRAINTS section ─────────────────────────────────────────────────────── */
function ConstraintsEditorSection({ constraints }) {
  const passing = constraints.filter(c => c.passed !== false);
  const failing = constraints.filter(c => c.passed === false);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      <div style={{ padding: '10px 20px', borderBottom: '1px solid var(--border)', flexShrink: 0, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <span className="label-10">CONSTRAINTS · {constraints.length}</span>
        <div style={{ display: 'flex', gap: 12 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--teal)' }}>● {passing.length} pass</span>
          {failing.length > 0 && <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--danger)' }}>● {failing.length} fail</span>}
        </div>
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: '16px 20px', display: 'flex', flexDirection: 'column', gap: 8 }}>
        {constraints.length === 0
          ? <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>no constraints · all within bounds</span>
          : constraints.map((c, i) => {
              const passed = c.passed !== false;
              const statusColor = passed ? 'var(--teal)' : 'var(--danger)';
              return (
                <div key={i} style={{ background: 'var(--bg-base)', border: `1px solid ${passed ? 'var(--border)' : 'rgba(224,80,80,0.3)'}`, borderRadius: 'var(--radius-sm)', padding: '12px 14px', display: 'flex', flexDirection: 'column', gap: 6 }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, color: 'var(--text-primary)' }}>{c.name || c.id || `constraint_${i}`}</span>
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: statusColor, letterSpacing: '0.08em' }}>{passed ? '● PASS' : '● FAIL'}</span>
                  </div>
                  {c.expr && (
                    <code style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)', background: 'var(--bg-surface)', padding: '4px 8px', borderRadius: 3, display: 'block', whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>{c.expr}</code>
                  )}
                  {c.message && (
                    <span style={{ fontFamily: 'var(--ui)', fontSize: 11, color: passed ? 'var(--text-muted)' : 'var(--danger)', lineHeight: 1.5 }}>{c.message}</span>
                  )}
                </div>
              );
            })
        }
        {constraints.length > 0 && (
          <div style={{ padding: '10px 12px', background: 'var(--bg-base)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)', marginTop: 4 }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', lineHeight: 1.6 }}>
              Constraint expressions are defined in the sustain JSON spec. To modify them, edit the spec and restart the engine. Use SUSTAIN → PATCH STATE to modify the state values that constraint expressions evaluate against.
            </span>
          </div>
        )}
      </div>
    </div>
  );
}

/* ── legacy stubs (kept so window.assign doesn't break; editor replaced above) ── */
function StateEditorTab() { return null; }
function OpSpecEditorTab() { return null; }
function PaletteItem() { return null; }
function EditorCanvas() { return null; }
function NodeInspector() { return null; }
/* ── legacy stubs deleted — section components above replace them ─── */
function _unused_removed_DELETED() {
  return (
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', height: '100%', minHeight: 0 }}>
      {/* Left — live state JSON */}
      <div style={{ display: 'flex', flexDirection: 'column', borderRight: '1px solid var(--border)', overflow: 'hidden' }}>
        <div style={{ padding: '6px 12px', borderBottom: '1px solid var(--border)', flexShrink: 0, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span className="label-10" style={{ fontSize: 9 }}>LIVE STATE · {sustain?.id}</span>
          <button onClick={onRefresh} style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', background: 'transparent', border: 'none', cursor: 'pointer', padding: 0, transition: 'color var(--t-fast)' }}
            onMouseEnter={e => e.currentTarget.style.color = 'var(--amber)'}
            onMouseLeave={e => e.currentTarget.style.color = 'var(--text-muted)'}
          >↻ refresh</button>
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: '8px 12px' }}>
          {stateLoading ? (
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)' }}>loading…</span>
          ) : liveState && Object.keys(liveState).length > 0 ? (
            <pre style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)', lineHeight: 1.5, whiteSpace: 'pre-wrap', wordBreak: 'break-word', margin: 0 }}>
              {JSON.stringify(liveState, null, 2)}
            </pre>
          ) : (
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>
              no state recorded yet
            </span>
          )}
        </div>
      </div>

      {/* Right — patch editor */}
      <div style={{ display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
        <div style={{ padding: '6px 12px', borderBottom: '1px solid var(--border)', flexShrink: 0 }}>
          <span className="label-10" style={{ fontSize: 9 }}>PATCH OPS · edit.state_patch</span>
        </div>
        <div style={{ padding: '5px 12px', flexShrink: 0 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)' }}>
            [&#123;"op":"replace|add|remove","path":"dot.path","value":...&#125;]
          </span>
        </div>
        <textarea
          value={patchText}
          onChange={e => onPatchChange(e.target.value)}
          spellCheck={false}
          style={{
            flex: 1, resize: 'none',
            fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)',
            background: 'var(--bg-base)', border: 'none', outline: 'none',
            padding: '8px 12px', lineHeight: 1.5,
          }}
        />
        <div style={{ padding: '8px 12px', borderTop: '1px solid var(--border)', flexShrink: 0, display: 'flex', flexDirection: 'column', gap: 6 }}>
          {patchError && (
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--danger)' }}>{patchError}</span>
          )}
          {patchResult && (
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--teal)' }}>
              ✓ {patchResult.applied_count} ops applied
              {patchResult.error_count > 0 && <span style={{ color: 'var(--amber)' }}> · {patchResult.error_count} errors</span>}
            </span>
          )}
          <button
            onClick={onApplyPatch}
            disabled={patchApplying}
            style={{
              padding: '7px 14px', alignSelf: 'flex-start',
              fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
              background: patchApplying ? 'var(--bg-overlay)' : 'var(--amber)',
              color: patchApplying ? 'var(--text-dim)' : 'var(--bg-base)',
              border: `1px solid ${patchApplying ? 'var(--border)' : 'var(--amber)'}`,
              borderRadius: 'var(--radius-sm)', cursor: patchApplying ? 'not-allowed' : 'pointer',
              transition: 'all var(--t-fast)',
            }}
          >
            {patchApplying ? '⟳ APPLYING…' : 'APPLY PATCH'}
          </button>
        </div>
      </div>
    </div>
  );
}

function _unused_OpSpecEditorTab({ opName, onOpNameChange, opField, onOpFieldChange, opValue, onOpValueChange, opResult, opApplying, onApply, realOps }) {
  const EDITABLE_FIELDS = ['description', 'pawa_cost', 'license_tier', 'author', 'ui_schema'];
  return (
    <div style={{ padding: 20, display: 'flex', flexDirection: 'column', gap: 16, overflow: 'auto', height: '100%', boxSizing: 'border-box' }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', lineHeight: 1.6 }}>
        Modify operator metadata in the in-memory OPERATOR_REGISTRY. Changes take effect immediately but are not persisted across server restarts.
      </span>
      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>OPERATOR</span>
        {realOps.length > 0 && (
          <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginBottom: 6 }}>
            {realOps.slice(0, 6).map(o => (
              <button key={o.name} onClick={() => onOpNameChange(o.name)} style={{
                padding: '3px 8px', fontFamily: 'var(--mono)', fontSize: 9,
                color: opName === o.name ? 'var(--amber)' : 'var(--text-muted)',
                background: opName === o.name ? 'var(--amber-glow)' : 'transparent',
                border: `1px solid ${opName === o.name ? 'var(--amber-border)' : 'var(--border)'}`,
                borderRadius: 3, cursor: 'pointer', transition: 'all var(--t-fast)',
              }}>{o.name}</button>
            ))}
          </div>
        )}
        <input
          value={opName}
          onChange={e => onOpNameChange(e.target.value)}
          placeholder="e.g. budget.allocate"
          style={{
            width: '100%', boxSizing: 'border-box',
            background: 'var(--bg-base)', border: '1px solid var(--border-mid)',
            borderRadius: 'var(--radius-sm)', padding: '7px 10px',
            fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)', outline: 'none',
          }}
        />
      </div>
      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>FIELD</span>
        <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
          {EDITABLE_FIELDS.map(f => (
            <button key={f} onClick={() => onOpFieldChange(f)} style={{
              padding: '4px 10px', fontFamily: 'var(--mono)', fontSize: 9,
              color: opField === f ? 'var(--teal)' : 'var(--text-muted)',
              background: opField === f ? 'rgba(42,184,160,0.1)' : 'transparent',
              border: `1px solid ${opField === f ? 'var(--teal-border)' : 'var(--border)'}`,
              borderRadius: 3, cursor: 'pointer', transition: 'all var(--t-fast)',
            }}>{f}</button>
          ))}
        </div>
      </div>
      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>NEW VALUE</span>
        <input
          value={opValue}
          onChange={e => onOpValueChange(e.target.value)}
          placeholder="new value…"
          style={{
            width: '100%', boxSizing: 'border-box',
            background: 'var(--bg-base)', border: '1px solid var(--border-mid)',
            borderRadius: 'var(--radius-sm)', padding: '7px 10px',
            fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)', outline: 'none',
          }}
        />
      </div>
      {opResult && (
        <div style={{
          background: 'var(--bg-base)', border: '1px solid var(--border)',
          borderRadius: 'var(--radius-sm)', padding: '8px 12px',
          fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--teal)',
        }}>
          ✓ {opResult.operator_name} · {opResult.field}: {String(opResult.old_value)} → {String(opResult.new_value)}
        </div>
      )}
      <button
        onClick={onApply}
        disabled={opApplying}
        style={{
          padding: '7px 14px', alignSelf: 'flex-start',
          fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
          background: opApplying ? 'var(--bg-overlay)' : 'var(--teal)',
          color: opApplying ? 'var(--text-dim)' : 'var(--bg-base)',
          border: `1px solid ${opApplying ? 'var(--border)' : 'var(--teal)'}`,
          borderRadius: 'var(--radius-sm)', cursor: opApplying ? 'not-allowed' : 'pointer',
          transition: 'all var(--t-fast)',
        }}
      >
        {opApplying ? '⟳ UPDATING…' : 'UPDATE SPEC'}
      </button>
    </div>
  );
}

function _unused_PaletteItem({ type, label, shape }) {
  const color = {
    state: 'var(--node-state)', operator: 'var(--node-operator)',
    constraint: 'var(--node-constraint)', event: 'var(--node-event)',
    operative: 'var(--node-operative)', time: 'var(--node-time)',
  }[type];
  return (
    <div className="lift" style={{
      display: 'flex', alignItems: 'center', gap: 8,
      padding: '8px 10px',
      background: 'var(--bg-base)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius-sm)',
      cursor: 'grab',
    }}>
      <span style={{ color, fontFamily: 'var(--mono)', fontSize: 12, width: 18, textAlign: 'center' }}>{shape}</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)', letterSpacing: '0.06em' }}>{label}</span>
    </div>
  );
}

function _unused_EditorCanvas({ selectedId, onSelect }) {
  return (
    <div style={{
      position: 'relative', width: '100%', height: '100%',
      overflow: 'hidden',
      background: `linear-gradient(0deg, var(--bg-base) 0%, var(--bg-surface) 100%)`,
    }}>
      <svg width="100%" height="100%" style={{ position: 'absolute', inset: 0, pointerEvents: 'none' }}>
        <defs>
          <pattern id="editor-grid" width="20" height="20" patternUnits="userSpaceOnUse">
            <circle cx="1" cy="1" r="0.6" fill="var(--border-mid)" opacity="0.5" />
          </pattern>
        </defs>
        <rect width="100%" height="100%" fill="url(#editor-grid)" />
      </svg>

      <div style={{ position: 'absolute', inset: 0 }}>
        <div style={{ position: 'relative', width: 920, height: 400 }}>
          <DagEdges nodes={EDITOR_NODES} edges={EDITOR_EDGES} activePath={[]} width={920} height={400} />
          {EDITOR_NODES.map(n => (
            <DagNode
              key={n.id}
              node={n}
              active={n.id === selectedId}
              onClick={() => onSelect(n.id)}
            />
          ))}
        </div>
      </div>

      <div style={{ position: 'absolute', bottom: 12, left: 14, display: 'flex', alignItems: 'center', gap: 12 }}>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>SCALE · 100%</span>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>·</span>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>SUGIYAMA LAYOUT</span>
      </div>
      <div style={{ position: 'absolute', top: 12, right: 14 }}>
        <span className="meta-10" style={{ color: 'var(--teal)' }}>● SAVED · 14:32</span>
      </div>
    </div>
  );
}

function _unused_NodeInspector({ node }) {
  const fields = {
    state:      [['type', 'number'], ['scope', 'sustain'], ['snapshot', 'enabled']],
    operator:   [['signature', '(State, Inputs) → Δ'], ['cost', '0.02 pwa'], ['constraints', '2 attached']],
    constraint: [['kind', 'predicate'], ['scope', 'pre + post'], ['failure_mode', 'block']],
    event:      [['immutable', 'true'], ['ledger', 'append'], ['ui_schema', 'default']],
    operative:  [['agent_type', 'LLM-bounded'], ['model', 'haiku-4.5'], ['autonomy', 'medium']],
    time:       [['cron', '0 8 * * *'], ['timezone', 'EAT'], ['skew', '<300ms']],
  }[node.type] || [];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 4 }}>LABEL</span>
        <input value={node.label} readOnly style={{
          width: '100%',
          background: 'var(--bg-base)',
          border: '1px solid var(--border-mid)',
          borderRadius: 'var(--radius-sm)',
          padding: '6px 10px',
          fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-primary)',
          outline: 'none',
        }} />
      </div>

      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>PROPERTIES</span>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          {fields.map(([k, v]) => (
            <div key={k} style={{ display: 'flex', justifyContent: 'space-between', padding: '5px 0', borderBottom: '1px solid var(--border)' }}>
              <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{k}</span>
              <span className="val-12" style={{ fontSize: 11, color: 'var(--text-primary)' }}>{v}</span>
            </div>
          ))}
        </div>
      </div>

      {node.type === 'operator' && (
        <div>
          <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>UI_SCHEMA · PREVIEW</span>
          <div style={{
            background: 'var(--bg-base)', border: '1px solid var(--border)',
            borderRadius: 'var(--radius-sm)', padding: 10, display: 'flex', flexDirection: 'column', gap: 6,
          }}>
            <span className="meta-10" style={{ color: 'var(--text-muted)' }}>budget_allocation_card</span>
            <div style={{ display: 'flex', justifyContent: 'space-between' }}>
              <span style={{ fontFamily: 'var(--ui)', fontSize: 12 }}>Food</span>
              <span className="val-12" style={{ fontSize: 11 }}>5,000 KSH</span>
            </div>
            <div style={{ height: 3, background: 'var(--bg-overlay)', borderRadius: 1 }}>
              <div style={{ width: '70%', height: '100%', background: 'var(--teal)' }} />
            </div>
            <span className="meta-10" style={{ color: 'var(--text-muted)' }}>1,500 / 5,000 remaining</span>
          </div>
        </div>
      )}

      <div style={{ display: 'flex', gap: 6, marginTop: 4 }}>
        <PBtn variant="ghost" onClick={() => window.confirmAction({
          title: `Fork simulation from ${node.label}?`,
          body: 'Creates a forked branch in the simulator with current state snapshotted at this node.',
          ctaLabel: 'FORK',
          tone: 'amber',
          onConfirm: () => window.flash(`Forked · branch E created from ${node.label}`, 'ok'),
        })}>FORK FROM HERE</PBtn>
      </div>
    </div>
  );
}

/* ── Helper: call console execute for control operators ─────────────────── */
async function callControlOperator(sustainId, operatorId, params) {
  return api.post('/devui/console/execute', {
    sustain_id: sustainId,
    operator_id: operatorId,
    params,
  });
}

/* ───────────────────────────────────────────────────────────
   CONTROLLER PANEL — proposal queue + terminal
   ─────────────────────────────────────────────────────────── */

function _transformProposal(raw, fallbackSustainId) {
  const sim = raw.simulation || {};
  const votes = raw.votes || { for: 0, against: 0, abstain: 0, total: 0, breakdown: [] };
  const score = typeof sim.outcome_score === 'number' ? sim.outcome_score : null;
  return {
    id:       raw.id,
    sustain:  raw.sustain_id || fallbackSustainId,
    title:    raw.operator_name,
    summary:  `Proposed by ${raw.proposed_by}`,
    autonomy: score != null ? (score > 0.8 ? 'HIGH' : score > 0.5 ? 'MED' : 'LOW') : 'MED',
    cta:      'EXECUTE',
    cost:     sim.pawa_cost ?? '—',
    sim:      score != null ? {
      outcomeScore:       score,
      constraintPassRate: sim.constraint_pass_rate ?? 1,
      projection:         sim.projection || `Execute ${raw.operator_name}`,
      runs:               sim.runs ?? 1,
    } : null,
    council:  votes,
  };
}

function ControllerPanel({ tick, sustain, openModal }) {
  // Universal control state
  const [proposals, setProposals] = dUseState([]);
  const [autonomy, setAutonomy] = dUseState(0);       // 0-100, threshold for auto-execute
  const [pawaCeiling, setPawaCeiling] = dUseState(8500);
  const [timelock, setTimelock] = dUseState(0);       // hours
  const [execMode, setExecMode] = dUseState('GUARDED'); // GUARDED | LIVE | HOLD
  const [throttle, setThrottle] = dUseState(0);      // 0-1
  const [opsEnabled, setOpsEnabled] = dUseState({ mentor: true, protege: true, curator: true, navigator: false });
  const [emergency, setEmergency] = dUseState(false);

  const sustainId = sustain?.id;

  const fetchProposals = async (sid) => {
    if (!sid) return;
    try {
      const d = await api.get(`/api/v1/council/${encodeURIComponent(sid)}/proposals?status=PASSED`);
      const raw = d?.data?.proposals || [];
      setProposals(raw.map(r => _transformProposal(r, sid)));
    } catch {}
  };

  dUseEffect(() => { fetchProposals(sustainId); }, [sustainId]);

  dUseEffect(() => {
    if (!sustainId) return;
    const id = setInterval(() => fetchProposals(sustainId), 15000);
    return () => clearInterval(id);
  }, [sustainId]);

  return (
    <div className="panel-enter" style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: 14, padding: '24px 28px', overflowY: 'auto' }}>
      {/* Proposal queue */}
      <div>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 10 }}>
          <span className="label-11">PROPOSAL QUEUE · PASSED COUNCIL</span>
          <span className="meta-10">{proposals.length} AWAITING EXECUTION</span>
        </div>
        {proposals.length === 0
          ? <div style={{ padding: '16px 0', fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)', textAlign: 'center' }}>council is quiet · no proposals in motion</div>
          : <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 12 }}>
              {proposals.map((p, i) => (
                <ProposalCard
                  key={p.id}
                  p={p}
                  delay={i * 80}
                  sustainId={sustainId || 'homestead.bonnie'}
                  onClick={() => openModal(p)}
                />
              ))}
            </div>
        }
      </div>

      {/* Universal controls bar */}
      <Card title="UNIVERSAL CONTROLS" sub="[CTL-0148] · EXECUTION AUTHORITY · SCADA" padded
        style={{ flexShrink: 0 }}
        actions={
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <Badge tone={emergency ? 'danger' : execMode === 'LIVE' ? 'amber' : 'ok'} dot>{emergency ? 'EMERGENCY HOLD' : execMode}</Badge>
            <span className="meta-10" style={{ color: 'var(--text-muted)' }}>SYNC · {(28 + tick % 7).toString().padStart(2, '0')}ms</span>
          </div>
        }
      >
        {/* Top row: 5 instrument frames */}
        <div style={{
          display: 'grid',
          gridTemplateColumns: '170px 1fr 200px 1fr 165px',
          gap: 18, alignItems: 'stretch',
        }}>
          {/* Knob — autonomy threshold */}
          <Instrument id="X-T90" label="AUTONOMY">
            <Knob
              value={autonomy} min={0} max={100} step={1} unit="%"
              color="var(--amber)"
              onChange={setAutonomy}
              note={autonomy < 35 ? 'CONSERVATIVE' : autonomy > 70 ? 'PERMISSIVE' : 'BALANCED'}
            />
          </Instrument>

          {/* Sliders + readouts */}
          <Instrument id="P-LIM" label="CEILINGS · LIMITS">
            <div style={{ display: 'flex', flexDirection: 'column', gap: 18, padding: '8px 4px' }}>
              <Slider
                label="PAWA CEILING" param="ceiling-A"
                value={pawaCeiling} min={1000} max={20000} step={250} unit="pwa"
                color="var(--amber)"
                onChange={setPawaCeiling}
                fmt={v => v.toLocaleString()}
                limit={{ value: 17500, label: 'LIMIT' }}
              />
              <Slider
                label="OPERATIVE THROTTLE" param="throttle-B"
                value={throttle * 100} min={0} max={100} step={5} unit="%"
                color="var(--teal)"
                onChange={v => setThrottle(v / 100)}
                fmt={v => `${v}`}
                limit={{ value: 92, label: 'LIMIT' }}
              />
            </div>
          </Instrument>

          {/* Big alert dial — centerpiece */}
          <Instrument id="ALRT-78" label="EXEC AUTHORITY">
            <BigDial
              value={Math.round(autonomy * throttle)}
              level={autonomy * throttle > 60 ? 'HIGH' : autonomy * throttle > 35 ? 'MED' : 'LOW'}
              tick={tick}
            />
          </Instrument>

          {/* Stepper + vertical scale combo */}
          <Instrument id="T-LCK" label="GUARDS">
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 14, padding: '6px 4px', alignItems: 'stretch' }}>
              <Stepper
                label="TIMELOCK" param="hold-Δ"
                value={timelock} min={0} max={168} step={12} unit="HR"
                onChange={setTimelock}
              />
              <VerticalScale
                label="BURN" param="rate-Σ"
                value={0} min={0} max={6000} ceiling={0}
                unit="KSH/D"
                tick={tick}
              />
            </div>
          </Instrument>

          {/* Mode + emergency */}
          <Instrument id="E-HLD" label="EXEC MODE">
            <div style={{ display: 'flex', flexDirection: 'column', gap: 12, padding: '6px 0' }}>
              <Segmented
                value={execMode} options={['HOLD', 'GUARDED', 'LIVE']}
                onChange={setExecMode}
              />
              <BigToggle
                label="EMERGENCY"
                value={emergency} onChange={setEmergency}
                danger
              />
            </div>
          </Instrument>
        </div>

        {/* Operative enable row */}
        <div style={{ marginTop: 18, paddingTop: 14, borderTop: '1px solid var(--border)', display: 'grid', gridTemplateColumns: '1fr auto', gap: 24, alignItems: 'center' }}>
          <div>
            <span className="label-10" style={{ display: 'block', marginBottom: 10 }}>OPERATIVE COUNCIL · BUS ENABLE</span>
            <div style={{ display: 'flex', gap: 22, flexWrap: 'wrap' }}>
              {[
                { id: 'mentor', label: 'MENTOR' },
                { id: 'protege', label: 'PROTÉGÉ' },
                { id: 'curator', label: 'CURATOR' },
                { id: 'navigator', label: 'NAVIGATOR' },
              ].map(o => (
                <RockerSwitch key={o.id}
                  label={o.label}
                  value={opsEnabled[o.id]}
                  onChange={(v) => setOpsEnabled(s => ({ ...s, [o.id]: v }))}
                />
              ))}
            </div>
          </div>
          <div style={{ display: 'flex', gap: 20, alignItems: 'center' }}>
            <ReadoutBlock label="VALUE" big={`${Object.values(opsEnabled).filter(Boolean).length}/4`} sub="active" />
            <ReadoutBlock label="P.T.C" big={`${(autonomy / 100 * throttle * 100).toFixed(2)}%`} sub="weighted" tone="amber" />
            <LedGrid count={8} active={tick % 8} />
          </div>
        </div>
      </Card>

      {/* Terminal + IoT + Rollback */}
      <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1.5fr) minmax(0, 1fr)', gap: 14, minHeight: 440 }}>
        <OperatorConsoleCard tick={tick} sustain={sustain} />

        <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          <Card title="IOT · CONNECTED DEVICES" sub="5 ONLINE" padded scroll>
            <IotList />
          </Card>
          <Card title="ROLLBACK" sub="control.rollback" padded>
            <RollbackPanel sustain={sustain} />
          </Card>
        </div>
      </div>
    </div>
  );
}

/* ───────────────────────────────────────────────────────────
   UNIVERSAL CONTROL WIDGETS
   ─────────────────────────────────────────────────────────── */

/* Instrument — bracket-frame wrapper with ID + label, corner crops */
function Instrument({ id, label, children }) {
  return (
    <div style={{
      position: 'relative',
      background: 'transparent',
      border: '1px solid var(--border)',
      borderRadius: 2,
      padding: '20px 10px 10px',
      display: 'flex', flexDirection: 'column',
    }}>
      {/* Top-left ID label, sci-fi style */}
      <span style={{
        position: 'absolute', top: -7, left: 12,
        padding: '0 6px',
        background: 'var(--bg-surface)',
        fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.12em',
        color: 'var(--amber)',
      }}>[{id}]</span>
      {/* Top-right label */}
      <span style={{
        position: 'absolute', top: -7, right: 12,
        padding: '0 6px',
        background: 'var(--bg-surface)',
        fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500, letterSpacing: '0.12em',
        color: 'var(--text-muted)',
      }}>{label}</span>
      {/* Corner crops */}
      <CornerCrop pos="tl" />
      <CornerCrop pos="tr" />
      <CornerCrop pos="bl" />
      <CornerCrop pos="br" />
      {children}
    </div>
  );
}

function CornerCrop({ pos }) {
  const styles = {
    tl: { top: -1, left: -1, borderTopWidth: 1, borderLeftWidth: 1 },
    tr: { top: -1, right: -1, borderTopWidth: 1, borderRightWidth: 1 },
    bl: { bottom: -1, left: -1, borderBottomWidth: 1, borderLeftWidth: 1 },
    br: { bottom: -1, right: -1, borderBottomWidth: 1, borderRightWidth: 1 },
  }[pos];
  return <span style={{
    position: 'absolute', width: 7, height: 7,
    borderStyle: 'solid', borderColor: 'var(--amber)', borderWidth: 0,
    ...styles,
    pointerEvents: 'none',
  }} />;
}

/* Knob — rotational dial. Drag vertically to change. */
function Knob({ value, min, max, step = 1, unit, color = 'var(--amber)', onChange, note }) {
  const dragRef = dUseRef(null);
  const startRef = dUseRef({ y: 0, v: value });

  const pct = (value - min) / (max - min);
  const angle = -135 + pct * 270;

  const onDown = (e) => {
    startRef.current = { y: e.clientY, v: value };
    const onMove = (ev) => {
      const dy = startRef.current.y - ev.clientY;
      const range = max - min;
      const newVal = Math.max(min, Math.min(max, startRef.current.v + Math.round((dy / 120) * range / step) * step));
      onChange?.(newVal);
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  };

  const size = 110;
  const ringR = size / 2 - 8;
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4 }}>
      <div ref={dragRef} onMouseDown={onDown} style={{
        position: 'relative', width: size, height: size, cursor: 'ns-resize', userSelect: 'none',
      }}>
        <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} style={{ overflow: 'visible' }}>
          {/* Dim track arc */}
          <circle cx={size/2} cy={size/2} r={ringR}
            fill="none" stroke="var(--border)" strokeWidth="2"
            strokeDasharray={`${2 * Math.PI * ringR * 0.75} 999`}
            transform={`rotate(135 ${size/2} ${size/2})`}
            strokeLinecap="round"
          />
          {/* Active arc */}
          <circle cx={size/2} cy={size/2} r={ringR}
            fill="none" stroke={color} strokeWidth="2.4"
            strokeDasharray={`${2 * Math.PI * ringR * 0.75 * pct} 999`}
            transform={`rotate(135 ${size/2} ${size/2})`}
            strokeLinecap="round"
            style={{ filter: `drop-shadow(0 0 4px ${color})` }}
          />
          {/* Tick marks */}
          {Array.from({ length: 11 }).map((_, i) => {
            const a = -135 + (i / 10) * 270;
            const rad = (a * Math.PI) / 180;
            const ri = ringR - 6;
            const ro = ringR - 2;
            return <line key={i}
              x1={size/2 + ri * Math.cos(rad)} y1={size/2 + ri * Math.sin(rad)}
              x2={size/2 + ro * Math.cos(rad)} y2={size/2 + ro * Math.sin(rad)}
              stroke="var(--border-mid)"
              strokeWidth={i % 5 === 0 ? 1.2 : 0.8}
            />;
          })}
          {/* Indicator tab on the active arc */}
          <circle
            cx={size/2 + ringR * Math.cos((angle * Math.PI) / 180)}
            cy={size/2 + ringR * Math.sin((angle * Math.PI) / 180)}
            r="3.5" fill={color}
            style={{ filter: `drop-shadow(0 0 4px ${color})` }}
          />
        </svg>
        {/* Center stack */}
        <div style={{
          position: 'absolute', inset: 0, display: 'flex',
          flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
          pointerEvents: 'none',
        }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 28, fontWeight: 500, color: 'var(--text-primary)', letterSpacing: '-0.02em', lineHeight: 1 }}>
            {value}<span style={{ fontSize: 14, color: 'var(--text-secondary)', marginLeft: 2 }}>{unit}</span>
          </span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)', letterSpacing: '0.1em', marginTop: 3 }}>
            Threshold-α
          </span>
        </div>
      </div>
      {note && <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color, letterSpacing: '0.12em' }}>{note}</span>}
    </div>
  );
}

/* Horizontal slider with LIMIT marker + parameter subscript */
function Slider({ label, param, value, min, max, step = 1, unit, color = 'var(--amber)', onChange, fmt, limit }) {
  const pct = (value - min) / (max - min);
  const limitPct = limit ? (limit.value - min) / (max - min) : null;
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
        <div style={{ display: 'flex', flexDirection: 'column' }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', letterSpacing: '0.12em' }}>{label}</span>
          {param && <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.06em' }}>{param}</span>}
        </div>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 4 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 18, fontWeight: 500, color: 'var(--text-primary)', letterSpacing: '-0.01em' }}>{fmt ? fmt(value) : value}</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)' }}>{unit}</span>
        </div>
      </div>
      <div style={{ position: 'relative', height: 18 }}>
        {/* Track */}
        <div style={{
          position: 'absolute', left: 0, right: 0, top: '50%', transform: 'translateY(-50%)',
          height: 3, background: 'var(--bg-base)', borderRadius: 1.5, border: '1px solid var(--border)',
        }}>
          <div style={{ width: `${pct * 100}%`, height: '100%', background: color, borderRadius: 1.5, boxShadow: `0 0 4px ${color}` }} />
        </div>
        {/* Tick marks */}
        {[0, 0.25, 0.5, 0.75, 1].map(p => (
          <span key={p} style={{
            position: 'absolute', left: `${p * 100}%`, top: '50%',
            width: 1, height: p % 0.5 === 0 ? 8 : 5, background: 'var(--border-mid)',
            transform: 'translate(-0.5px, -50%)',
          }} />
        ))}
        {/* Limit marker */}
        {limitPct != null && (
          <>
            <span style={{
              position: 'absolute', left: `${limitPct * 100}%`, top: '50%',
              width: 1, height: 12, background: 'var(--danger)',
              transform: 'translate(-0.5px, -50%)',
            }} />
            <span style={{
              position: 'absolute', left: `${limitPct * 100}%`, top: -2,
              transform: 'translateX(-50%)',
              fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--danger)', letterSpacing: '0.08em',
              whiteSpace: 'nowrap',
            }}>▼{limit.label}</span>
          </>
        )}
        {/* Native input (overlaid) */}
        <input type="range"
          value={value} min={min} max={max} step={step}
          onChange={e => onChange?.(Number(e.target.value))}
          style={{
            position: 'absolute', inset: 0, opacity: 0, cursor: 'ew-resize',
            width: '100%', height: '100%', margin: 0,
          }}
        />
        {/* Thumb */}
        <div style={{
          position: 'absolute', left: `${pct * 100}%`, top: '50%',
          transform: 'translate(-50%, -50%)',
          width: 12, height: 12, borderRadius: '50%',
          background: 'var(--bg-base)', border: `2px solid ${color}`,
          boxShadow: `0 0 6px ${color}`,
          pointerEvents: 'none',
        }} />
      </div>
    </div>
  );
}

/* Stepper — discrete value with − / N / + */
function Stepper({ label, param, value, min, max, step, unit, onChange }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 5 }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', letterSpacing: '0.12em' }}>{label}</span>
      <div style={{
        display: 'inline-flex', alignItems: 'center',
        background: 'var(--bg-base)', border: '1px solid var(--border-mid)',
        borderRadius: 'var(--radius-sm)', overflow: 'hidden',
      }}>
        <button onClick={() => onChange?.(Math.max(min, value - step))} style={stepBtnStyle}>−</button>
        <div style={{
          minWidth: 44, padding: '6px 4px', textAlign: 'center',
          borderLeft: '1px solid var(--border)', borderRight: '1px solid var(--border)',
        }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 18, fontWeight: 500, color: 'var(--text-primary)', display: 'block', lineHeight: 1 }}>{value}</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)', letterSpacing: '0.1em' }}>{unit}</span>
        </div>
        <button onClick={() => onChange?.(Math.min(max, value + step))} style={stepBtnStyle}>+</button>
      </div>
      {param && <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.06em' }}>{param}</span>}
    </div>
  );
}
const stepBtnStyle = {
  width: 26, height: 32, fontFamily: 'var(--mono)', fontSize: 14, color: 'var(--text-primary)',
  background: 'transparent', transition: 'background var(--t-fast)',
};

/* Vertical scale — meter with ceiling marker */
function VerticalScale({ label, param, value, min, max, ceiling, unit, tick }) {
  const pct = Math.min(1, (value - min) / (max - min));
  const ceilPct = ceiling ? Math.min(1, (ceiling - min) / (max - min)) : null;
  const over = ceiling && value > ceiling;
  const color = over ? 'var(--danger)' : value > (ceiling || max) * 0.85 ? 'var(--amber)' : 'var(--teal)';
  const wobble = Math.sin(tick * 0.4) * 0.01;
  const livePct = Math.max(0, Math.min(1, pct + wobble));

  const H = 70;
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4 }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', letterSpacing: '0.12em' }}>{label}</span>
      <div style={{ display: 'flex', alignItems: 'flex-end', gap: 5 }}>
        <div style={{ display: 'flex', flexDirection: 'column-reverse', height: H, justifyContent: 'space-between', alignItems: 'flex-end' }}>
          {[0, 50, 100].map(p => (
            <span key={p} style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)' }}>{p}</span>
          ))}
        </div>
        <div style={{
          position: 'relative',
          width: 16, height: H,
          background: 'var(--bg-base)',
          border: '1px solid var(--border-mid)',
          borderRadius: 1,
          overflow: 'hidden',
        }}>
          <div style={{
            position: 'absolute', bottom: 0, left: 0, right: 0,
            height: `${livePct * 100}%`, background: color,
            transition: 'height 0.4s ease',
            boxShadow: `0 0 6px ${color}`,
          }} />
          {ceilPct != null && (
            <span style={{
              position: 'absolute', left: -2, right: -2, bottom: `${ceilPct * 100}%`,
              height: 1, background: 'var(--danger)',
            }} />
          )}
        </div>
      </div>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 500, color }}>{value.toLocaleString()}</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)' }}>{unit}</span>
      {param && <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.06em' }}>{param}</span>}
    </div>
  );
}

/* Segmented control */
function Segmented({ value, options, onChange }) {
  return (
    <div style={{
      display: 'flex',
      background: 'var(--bg-base)',
      border: '1px solid var(--border-mid)',
      borderRadius: 'var(--radius-sm)',
      padding: 2,
    }}>
      {options.map(o => (
        <button key={o} onClick={() => onChange?.(o)} style={{
          flex: 1, padding: '5px 6px',
          fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
          color: value === o ? 'var(--amber)' : 'var(--text-muted)',
          background: value === o ? 'var(--amber-glow)' : 'transparent',
          borderRadius: 2,
          transition: 'all var(--t-fast)',
        }}>{o}</button>
      ))}
    </div>
  );
}

/* Big toggle with industrial look */
function BigToggle({ label, value, onChange, danger }) {
  const c = danger ? 'var(--danger)' : 'var(--teal)';
  return (
    <button onClick={() => onChange?.(!value)} style={{
      display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8,
      padding: '8px 10px',
      background: value ? `color-mix(in oklab, ${c} 18%, var(--bg-base))` : 'var(--bg-base)',
      border: `1px solid ${value ? c : 'var(--border-mid)'}`,
      borderRadius: 'var(--radius-sm)',
      transition: 'all var(--t-fast)',
    }}>
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-start' }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', letterSpacing: '0.12em' }}>{label}</span>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, color: value ? c : 'var(--text-muted)', letterSpacing: '0.08em' }}>
          {value ? 'ARMED' : 'OFF'}
        </span>
      </div>
      <span style={{
        width: 26, height: 14, borderRadius: 7,
        background: value ? c : 'var(--bg-overlay)',
        position: 'relative', transition: 'background var(--t-fast)',
        flexShrink: 0,
      }}>
        <span style={{
          position: 'absolute', top: 1, left: value ? 13 : 1,
          width: 12, height: 12, borderRadius: '50%',
          background: 'var(--text-primary)',
          transition: 'left var(--t-fast)',
          boxShadow: value ? `0 0 6px ${c}` : 'none',
        }} />
      </span>
    </button>
  );
}

/* Big alert dial — centerpiece. Concentric ring with LEVEL readout. */
function BigDial({ value, level, tick }) {
  const size = 130;
  const cx = size / 2, cy = size / 2;
  const r = size / 2 - 10;
  // Map 0-100 to -225° to +45° (3/4 turn) starting from lower-left
  const startAngle = 135; // degrees, CSS / SVG convention (3 o'clock = 0)
  const sweep = 270;
  const pct = Math.min(1, Math.max(0, value / 100));
  const endAngle = startAngle + sweep * pct;
  // Arc path
  const polar = (angDeg, radius) => {
    const a = (angDeg - 90) * Math.PI / 180;
    return [cx + radius * Math.cos(a), cy + radius * Math.sin(a)];
  };
  const [sx, sy] = polar(startAngle, r);
  const [ex, ey] = polar(endAngle, r);
  const [trackEx, trackEy] = polar(startAngle + sweep, r);
  const largeArc = sweep * pct > 180 ? 1 : 0;
  const trackPath = `M ${sx} ${sy} A ${r} ${r} 0 1 1 ${trackEx} ${trackEy}`;
  const activePath = pct > 0 ? `M ${sx} ${sy} A ${r} ${r} 0 ${largeArc} 1 ${ex} ${ey}` : '';

  const c = level === 'HIGH' ? 'var(--amber)' : level === 'MED' ? 'var(--teal)' : 'var(--info)';
  // Live wobble on tick marks
  const tickActive = (tick % 8);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4 }}>
      <div style={{ position: 'relative', width: size, height: size }}>
        <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} style={{ overflow: 'visible' }}>
          {/* Outer track */}
          <path d={trackPath} fill="none" stroke="var(--border)" strokeWidth="2.5" strokeLinecap="round" />
          {/* Active arc */}
          {activePath && <path d={activePath} fill="none" stroke={c} strokeWidth="3"
            strokeLinecap="round" style={{ filter: `drop-shadow(0 0 6px ${c})` }}
          />}
          {/* Inner ring tick marks */}
          {Array.from({ length: 18 }).map((_, i) => {
            const ang = startAngle + (sweep / 18) * i;
            const [x1, y1] = polar(ang, r - 12);
            const [x2, y2] = polar(ang, r - 8);
            const isActive = i === tickActive;
            return <line key={i}
              x1={x1} y1={y1} x2={x2} y2={y2}
              stroke={isActive ? c : 'var(--border-mid)'}
              strokeWidth={isActive ? 2 : 1}
              opacity={isActive ? 1 : 0.5}
            />;
          })}
          {/* Center backing circle */}
          <circle cx={cx} cy={cy} r={r - 18} fill="var(--bg-base)" stroke="var(--border)" />
        </svg>
        {/* Center stack: ALERT / value / LEVEL / level */}
        <div style={{
          position: 'absolute', inset: 0, display: 'flex',
          flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
          pointerEvents: 'none', gap: 0,
        }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: c, letterSpacing: '0.12em' }}>EXEC</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 30, fontWeight: 500, color: 'var(--text-primary)', letterSpacing: '-0.02em', lineHeight: 1.1 }}>
            {value}<span style={{ fontSize: 14, color: 'var(--text-secondary)' }}>%</span>
          </span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', letterSpacing: '0.12em', marginTop: 2 }}>LEVEL</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, color: c, letterSpacing: '0.12em' }}>{level}</span>
        </div>
      </div>
    </div>
  );
}

/* Readout block — tiny label / big value / sub */
function ReadoutBlock({ label, big, sub, tone }) {
  const c = tone === 'amber' ? 'var(--amber)' : 'var(--text-primary)';
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 1, minWidth: 60 }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', letterSpacing: '0.12em' }}>{label}</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 18, fontWeight: 500, color: c, letterSpacing: '-0.01em', lineHeight: 1.1 }}>{big}</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', letterSpacing: '0.06em' }}>{sub}</span>
    </div>
  );
}

/* Rocker switch for operative enable */
function RockerSwitch({ label, value, onChange }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 5 }}>
      <span className="label-10" style={{ fontSize: 9 }}>{label}</span>
      <button onClick={() => onChange?.(!value)} style={{
        width: 38, height: 22,
        display: 'flex', alignItems: 'center', justifyContent: value ? 'flex-end' : 'flex-start',
        padding: 2,
        background: value ? 'var(--teal-glow)' : 'var(--bg-base)',
        border: `1px solid ${value ? 'var(--teal-border)' : 'var(--border-mid)'}`,
        borderRadius: 11,
        transition: 'all var(--t-fast)',
      }}>
        <span style={{
          width: 16, height: 16, borderRadius: '50%',
          background: value ? 'var(--teal)' : 'var(--text-muted)',
          boxShadow: value ? '0 0 6px var(--teal)' : 'none',
          transition: 'all var(--t-fast)',
        }} />
      </button>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: value ? 'var(--teal)' : 'var(--text-muted)', letterSpacing: '0.08em' }}>
        {value ? 'ON' : 'OFF'}
      </span>
    </div>
  );
}

/* Status LED grid — animated */
function LedGrid({ count = 6, active }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4, alignItems: 'center' }}>
      <span className="label-10" style={{ fontSize: 9 }}>BUS</span>
      <div style={{ display: 'flex', gap: 4 }}>
        {Array.from({ length: count }).map((_, i) => {
          const isActive = i === active % count;
          return <span key={i} style={{
            width: 8, height: 8, borderRadius: '50%',
            background: isActive ? 'var(--amber)' : 'var(--bg-overlay)',
            boxShadow: isActive ? '0 0 6px var(--amber)' : 'none',
            border: '1px solid ' + (isActive ? 'var(--amber)' : 'var(--border-mid)'),
            transition: 'all 200ms',
          }} />;
        })}
      </div>
    </div>
  );
}

function ProposalCard({ p, delay, sustainId, onClick }) {
  const [executing, setExecuting] = dUseState(false);
  const [expanded, setExpanded] = dUseState(false);
  const autonomyColor = p.autonomy === 'HIGH' ? 'var(--amber)' : p.autonomy === 'LOW' ? 'var(--teal)' : 'var(--text-secondary)';
  const ctaTone = p.cta === 'AUTO-EXECUTE' ? 'teal' : 'amber';

  const doExecute = async () => {
    setExecuting(true);
    try {
      await callControlOperator(sustainId, 'control.execute_approved', { proposal_id: p.id });
      window.flash?.(`${p.id} executed via control.execute_approved`, 'ok');
    } catch (err) {
      window.flash?.(`Execute failed: ${err.message}`, 'danger');
    } finally {
      setExecuting(false);
    }
  };

  const handleExecute = (e) => {
    e.stopPropagation();
    if (p.autonomy === 'HIGH') {
      window.openPinPad?.({
        title: `EXECUTE · ${p.id}`,
        body: `${p.title}. This will call control.execute_approved.`,
        onConfirm: doExecute,
      });
    } else {
      window.confirmAction?.({
        title: `Execute ${p.id}?`,
        body: `Runs control.execute_approved. ${p.sim?.projection || ''}`,
        ctaLabel: p.cta || 'EXECUTE',
        tone: 'amber',
        onConfirm: doExecute,
      });
    }
  };

  return (
    <div className="fade-up lift" style={{
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)',
      padding: 16,
      animationDelay: `${delay}ms`,
      display: 'flex', flexDirection: 'column', gap: 10,
      cursor: 'pointer',
    }}
    onClick={onClick}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
          <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{p.id} · {p.sustain}</span>
          <span style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)', fontWeight: 500, lineHeight: 1.3 }}>{p.title}</span>
        </div>
        <Badge tone={ctaTone}>{p.autonomy}</Badge>
      </div>
      <span style={{ fontSize: 11, color: 'var(--text-secondary)', lineHeight: 1.55 }}>{p.summary}</span>
      {p.sim && (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 8, paddingTop: 8, borderTop: '1px solid var(--border)' }}>
          <SmallMetric label="SCORE" value={p.sim.outcomeScore?.toFixed(2) ?? '—'} tone={p.sim.outcomeScore > 0.8 ? 'teal' : 'amber'} />
          <SmallMetric label="CSTR PASS" value={p.sim.constraintPassRate != null ? `${(p.sim.constraintPassRate * 100).toFixed(0)}%` : '—'} />
          <SmallMetric label="PAWA" value={p.cost ?? '—'} />
        </div>
      )}
      {/* Per-councillor vote bars */}
      {(() => {
        const { for: f = 0, against: a = 0, abstain: ab = 0, total: t = 0, breakdown = [] } = p.council || {};
        if (t === 0) return null;
        return (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 5, paddingTop: 8, borderTop: '1px solid var(--border)' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div style={{ display: 'flex', gap: 10 }}>
                <span className="meta-10" style={{ color: 'var(--teal)' }}>FOR {f}</span>
                <span className="meta-10" style={{ color: 'var(--danger)' }}>AGAINST {a}</span>
                <span className="meta-10" style={{ color: 'var(--text-muted)' }}>ABSTAIN {ab}</span>
              </div>
              {breakdown.length > 0 && (
                <button onClick={e => { e.stopPropagation(); setExpanded(v => !v); }} style={{
                  fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)',
                  background: 'transparent', border: 'none', cursor: 'pointer', padding: 0,
                }}>{expanded ? '▲' : '▼'}</button>
              )}
            </div>
            <div style={{ display: 'flex', height: 3, borderRadius: 2, overflow: 'hidden', background: 'var(--bg-base)' }}>
              <div style={{ width: `${(f / t) * 100}%`, background: 'var(--teal)' }} />
              <div style={{ width: `${(a / t) * 100}%`, background: 'var(--danger)' }} />
              <div style={{ width: `${(ab / t) * 100}%`, background: 'var(--border)' }} />
            </div>
            {expanded && breakdown.map(v => (
              <div key={v.operative_id} style={{
                display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 8,
                padding: '5px 8px', background: 'var(--bg-base)', borderRadius: 'var(--radius-sm)',
              }}>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-secondary)', flex: 1 }}>
                  {v.operative_id}
                </span>
                <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-end', gap: 2, flexShrink: 0 }}>
                  <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: v.vote === 'YES' ? 'var(--teal)' : v.vote === 'NO' ? 'var(--danger)' : 'var(--text-muted)' }}>
                    {v.vote}
                  </span>
                  {v.reasoning && (
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)', maxWidth: 140, textAlign: 'right', lineHeight: 1.4 }}>
                      {v.reasoning.length > 60 ? v.reasoning.slice(0, 60) + '…' : v.reasoning}
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>
        );
      })()}
      <div style={{ display: 'flex', gap: 6, marginTop: 4 }}>
        <button
          onClick={handleExecute}
          disabled={executing}
          style={{
            flex: 1, padding: '6px 10px',
            fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
            background: executing ? 'var(--bg-overlay)' : 'var(--amber)',
            color: executing ? 'var(--text-dim)' : 'var(--bg-base)',
            border: `1px solid ${executing ? 'var(--border)' : 'var(--amber)'}`,
            borderRadius: 'var(--radius-sm)',
            cursor: executing ? 'not-allowed' : 'pointer',
            transition: 'all var(--t-fast)',
          }}
        >
          {executing ? '⟳' : (p.cta || 'EXECUTE')}
        </button>
        <button onClick={e => {
          e.stopPropagation();
          window.confirmAction?.({
            title: `Reject ${p.id}?`,
            body: `Returns to draft. Authors notified.`,
            ctaLabel: 'REJECT',
            tone: 'danger',
            onConfirm: () => window.flash?.(`${p.id} rejected`, 'danger'),
          });
        }} style={{
          padding: '6px 10px',
          fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
          background: 'transparent', color: 'var(--text-secondary)',
          border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
        }}>REJECT</button>
      </div>
    </div>
  );
}

function SmallMetric({ label, value, tone }) {
  const c = tone === 'teal' ? 'var(--teal)' : tone === 'amber' ? 'var(--amber)' : 'var(--text-primary)';
  return (
    <div style={{ display: 'flex', flexDirection: 'column' }}>
      <span className="meta-10" style={{ fontSize: 9 }}>{label}</span>
      <span className="val-12" style={{ color: c }}>{value}</span>
    </div>
  );
}

/* Parse a shell-style command string into { operator, params }
   e.g. "budget.allocate pocket=food amount=5000" */
function parseCommand(raw) {
  const parts = raw.trim().split(/\s+/);
  const operator = parts[0];
  const params = {};
  parts.slice(1).forEach(p => {
    const [k, ...vs] = p.split('=');
    const v = vs.join('=');
    const num = Number(v);
    params[k] = isNaN(num) ? v : num;
  });
  return { operator, params };
}

/* ── OperatorConsoleCard — tab toggle between CONSOLE and UI PREVIEW ─────── */
function OperatorConsoleCard({ tick, sustain }) {
  const [tab, setTab] = dUseState('console');
  const tabStyle = (id) => ({
    padding: '4px 10px',
    fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500, letterSpacing: '0.08em',
    color: tab === id ? 'var(--amber)' : 'var(--text-muted)',
    background: tab === id ? 'var(--amber-glow)' : 'transparent',
    border: `1px solid ${tab === id ? 'var(--amber-border)' : 'var(--border)'}`,
    borderRadius: 'var(--radius-sm)',
    transition: 'all var(--t-fast)',
  });
  return (
    <Card
      title={tab === 'console' ? 'OPERATOR CONSOLE' : 'UI PREVIEW'}
      sub={tab === 'console' ? 'DIRECT INVOCATION' : 'SPEC → WIDGET'}
      padded={false}
      actions={
        <div style={{ display: 'flex', gap: 4 }}>
          <button style={tabStyle('console')} onClick={() => setTab('console')}>CONSOLE</button>
          <button style={tabStyle('preview')} onClick={() => setTab('preview')}>UI PREVIEW</button>
        </div>
      }
    >
      {tab === 'console'
        ? <ControllerTerminal tick={tick} sustain={sustain} />
        : <UIPreviewPanel sustain={sustain} />}
    </Card>
  );
}

/* ── UIPreviewPanel — paste spec JSON, see rendered widget ───────────────── */
function UIPreviewPanel({ sustain }) {
  const DEFAULT_SPEC = JSON.stringify({
    widget_type: 'budget_allocation_card',
    fields: [
      { label: 'Pocket', source: 'inputs.pocket_name', display: 'text' },
      { label: 'Amount', source: 'inputs.amount', display: 'currency' },
      { label: 'Remaining', source: 'state.finances.liquid.balance', display: 'currency' },
    ],
    ctas: ['View Budget', 'Allocate Another'],
  }, null, 2);

  const [specText, setSpecText] = dUseState(DEFAULT_SPEC);
  const [mockText, setMockText] = dUseState(JSON.stringify({ inputs: { pocket_name: 'food', amount: 3000 } }, null, 2));
  const [result, setResult] = dUseState(null);
  const [error, setError] = dUseState(null);
  const [loading, setLoading] = dUseState(false);
  const debounceRef = dUseRef(null);

  const runPreview = dUseRef(async (spec, mock) => {
    let specJson, mockState;
    try { specJson = JSON.parse(spec); } catch { setError('Invalid spec JSON'); return; }
    try { mockState = JSON.parse(mock || '{}'); } catch { setError('Invalid mock state JSON'); return; }
    setLoading(true);
    setError(null);
    try {
      const res = await api.post('/devui/preview-widget', {
        spec_json: specJson,
        mock_state: mockState,
      });
      setResult(res?.data?.widget || null);
    } catch (err) {
      setError(err.message || 'API error');
    } finally {
      setLoading(false);
    }
  });

  const schedulePreview = (spec, mock) => {
    clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => runPreview.current(spec, mock), 500);
  };

  dUseEffect(() => { schedulePreview(specText, mockText); }, [specText, mockText]);

  const fields = result?.data?.fields || [];
  const ctas   = result?.data?.ctas || [];

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', height: '100%', minHeight: 0 }}>
      {/* Left — editors */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 0, borderRight: '1px solid var(--border)', overflow: 'hidden' }}>
        <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', borderBottom: '1px solid var(--border)' }}>
          <div style={{ padding: '5px 10px', borderBottom: '1px solid var(--border)', flexShrink: 0 }}>
            <span className="label-10" style={{ fontSize: 9 }}>SPEC JSON</span>
          </div>
          <textarea
            value={specText}
            onChange={e => { setSpecText(e.target.value); schedulePreview(e.target.value, mockText); }}
            spellCheck={false}
            style={{
              flex: 1, resize: 'none',
              fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)',
              background: 'var(--bg-base)', border: 'none', outline: 'none',
              padding: '8px 10px', lineHeight: 1.5,
            }}
          />
        </div>
        <div style={{ flex: '0 0 100px', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
          <div style={{ padding: '5px 10px', borderBottom: '1px solid var(--border)', flexShrink: 0 }}>
            <span className="label-10" style={{ fontSize: 9 }}>MOCK STATE</span>
          </div>
          <textarea
            value={mockText}
            onChange={e => { setMockText(e.target.value); schedulePreview(specText, e.target.value); }}
            spellCheck={false}
            style={{
              flex: 1, resize: 'none',
              fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)',
              background: 'var(--bg-base)', border: 'none', outline: 'none',
              padding: '8px 10px', lineHeight: 1.5,
            }}
          />
        </div>
      </div>

      {/* Right — rendered widget preview */}
      <div style={{ padding: 14, overflow: 'auto', display: 'flex', flexDirection: 'column', gap: 10 }}>
        {loading && (
          <span className="t-meta" style={{ animation: 'pulse 0.8s ease-in-out infinite', fontSize: 10 }}>⟳ rendering…</span>
        )}
        {error && (
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--danger)' }}>{error}</span>
        )}
        {result && !loading && (
          <>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <span className="label-10" style={{ color: 'var(--teal)', fontSize: 9 }}>● LIVE</span>
              <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{result.type}</span>
            </div>
            <div style={{
              background: 'var(--bg-base)', border: '1px solid var(--border)',
              borderRadius: 'var(--radius-sm)', padding: 12,
              display: 'flex', flexDirection: 'column', gap: 8,
            }}>
              {fields.map((f, i) => (
                <div key={i} style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', borderBottom: i < fields.length - 1 ? '1px solid var(--border)' : 'none', paddingBottom: 6 }}>
                  <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{f.label}</span>
                  <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: f.value == null ? 'var(--text-dim)' : 'var(--text-primary)' }}>
                    {f.value == null ? '—' : String(f.value)}
                  </span>
                </div>
              ))}
              {ctas.length > 0 && (
                <div style={{ display: 'flex', gap: 6, marginTop: 4 }}>
                  {ctas.map((c, i) => (
                    <span key={i} style={{
                      padding: '3px 8px',
                      fontFamily: 'var(--mono)', fontSize: 9,
                      color: 'var(--amber)', border: '1px solid var(--amber-border)',
                      borderRadius: 'var(--radius-sm)', background: 'var(--amber-glow)',
                    }}>{c}</span>
                  ))}
                </div>
              )}
              {result.summary && (
                <span className="meta-10" style={{ color: 'var(--text-dim)', fontSize: 9, borderTop: '1px solid var(--border)', paddingTop: 6 }}>{result.summary}</span>
              )}
            </div>
          </>
        )}
        {!result && !loading && !error && (
          <span className="meta-10" style={{ color: 'var(--text-dim)' }}>Edit spec to render preview</span>
        )}
      </div>
    </div>
  );
}

/* Protocol badge colour + label */
const PROTOCOL_STYLE = {
  rpc:          { color: 'var(--teal)',          label: 'RPC' },
  event_driven: { color: 'var(--node-event)',    label: 'EVENT_DRIVEN' },
  polling:      { color: 'var(--text-secondary)','label': 'POLLING' },
  streaming:    { color: 'var(--amber)',         label: 'STREAMING' },
};

function ControllerTerminal({ tick, sustain }) {
  const BOOT_LINES = [
    { t: 'meta', text: 'sustena.controller · interactive shell · type "help"' },
    { t: 'meta', text: `sustain · ${sustain?.id || 'homestead.bonnie'} · privilege L0` },
    { t: 'meta', text: 'connected to /devui/console/execute' },
  ];
  const [lines, setLines] = dUseState(BOOT_LINES);
  const [input, setInput] = dUseState('');
  const [busy, setBusy] = dUseState(false);
  const [history, setHistory] = dUseState([]);
  const [histIdx, setHistIdx] = dUseState(-1);
  /* operator registry → protocol lookup */
  const [opRegistry, setOpRegistry] = dUseState({});
  /* streaming state: when a streaming operator result arrives, store it */
  const [streamLines, setStreamLines] = dUseState([]);
  const [streamOp, setStreamOp] = dUseState(null);
  const inputRef = dUseRef(null);
  const scrollRef = dUseRef(null);

  /* load registry once on mount */
  dUseEffect(() => {
    api.get('/devui/registry/operators')
      .then(d => setOpRegistry(d?.data?.operators || {}))
      .catch(() => {});
  }, []);

  const addLine = (t, text) => setLines(prev => [...prev, { t, text }]);

  const scrollToBottom = () => {
    setTimeout(() => {
      if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }, 20);
  };

  const runCommand = async (raw) => {
    if (!raw.trim()) return;
    const cmd = raw.trim();

    // Help command
    if (cmd === 'help') {
      addLine('meta', 'usage: <operator> [key=value ...]');
      addLine('meta', 'e.g.:  budget.allocate pocket=food amount=5000');
      addLine('meta', 'e.g.:  pantry.consume item=oil quantity=0.4');
      scrollToBottom();
      return;
    }

    setHistory(prev => [cmd, ...prev.slice(0, 49)]);
    setHistIdx(-1);
    addLine('cmd', cmd);
    setBusy(true);

    const { operator, params } = parseCommand(cmd);

    try {
      const resp = await api.post('/devui/console/execute', {
        sustain_id: sustain?.id || 'homestead.bonnie',
        operator_id: operator,
        params,
      });

      // resp shape: { status, data: { result: { status, data: { delta, ... } }, events, ... }, timestamp }
      const payload   = resp?.data || {};
      const opResult  = payload?.result?.data || {};
      const opStatus  = payload?.result?.status || 'ok';
      const reason    = payload?.result?.reason || payload?.result?.constraint_violated;
      const delta     = opResult?.delta || {};
      const events    = payload?.events || [];

      // Outcome line — surface failures/deferrals instead of silently saying "ok"
      if (opStatus === 'failed') {
        addLine('danger', `[FAIL] ${operator}${reason ? ' · ' + reason : ''}`);
      } else if (opStatus === 'deferred') {
        addLine('meta', `[DEFERRED] ${operator} → council${reason ? ' · ' + reason : ''}`);
      }

      // Show delta fields (operators that return a delta map)
      Object.entries(delta).forEach(([field, val]) => {
        addLine('exec', `[ΔSTATE] ${field}: ${val}`);
      });

      // Show the operator's committed result fields (e.g. liquid_balance, amount_credited)
      const resultFields = opStatus === 'ok'
        ? Object.entries(opResult).filter(([k, v]) => k !== 'delta' && v !== null && typeof v !== 'object')
        : [];
      resultFields.forEach(([k, v]) => addLine('exec', `[RESULT] ${k}: ${v}`));

      // Show emitted events (event_name/payload from the events table, or type/data)
      events.forEach(e => {
        const ts = e.timestamp ? new Date(e.timestamp).toISOString().slice(11, 19) + 'Z' : formatClock(new Date());
        addLine('event', `[EVENT] ${e.event_name || e.type || e.name || 'UNKNOWN'} · ${JSON.stringify(e.payload || e.data || {})} · t=${ts}`);
      });

      if (opStatus === 'ok' && !Object.keys(delta).length && !resultFields.length && !events.length) {
        addLine('check', '[OK] executed · no state change');
      }

      // Toast notification mirroring the outcome
      if (opStatus === 'failed') window.flash?.(`✗ ${operator}${reason ? ' · ' + reason : ' failed'}`, 'danger');
      else if (opStatus === 'deferred') window.flash?.(`${operator} → council review`, 'amber');
      else window.flash?.(`✓ ${operator} committed`, 'ok');

      const ms   = opResult?.duration_ms ?? '—';
      const pawa = opResult?.pawa_cost ?? opResult?.pawa ?? '—';
      addLine('meta', `∴ committed in ${ms}ms · pawa −${pawa}`);

      /* ── protocol badge ── */
      const opMeta = opRegistry[operator] || {};
      const protocol = opMeta.protocol || 'rpc';
      const ps = PROTOCOL_STYLE[protocol] || PROTOCOL_STYLE.rpc;
      /* proto line format: "LABEL·operatorName" */
      addLine('proto', `${ps.label}·${operator}`);

      if (protocol === 'streaming') {
        setStreamOp(operator);
        const events = opResult?.data?.events ?? opResult?.events ?? [];
        setStreamLines(
          events.length
            ? events.map(e => `${e.event_name || e.type || '?'} · ${JSON.stringify(e.payload || e.data || {})}`)
            : ['[STREAM] operator returned snapshot · no live events yet']
        );
        addLine('stream', '[STREAM OPEN] · live event feed active below');
      } else if (protocol === 'event_driven') {
        addLine('event', '[EVENT_DRIVEN] operator registered · waiting for trigger event');
      } else if (protocol === 'polling') {
        addLine('meta', '[POLLING] operator will re-run on its configured interval');
      }
    } catch (err) {
      addLine('danger', `[ERROR] ${err.message || 'API unreachable'}`);
      addLine('check', '[FALLBACK] constraint checks skipped (offline)');
      addLine('exec', `[MOCK] ${operator} · params: ${JSON.stringify(params)}`);
    } finally {
      setBusy(false);
      scrollToBottom();
    }
  };

  const handleKeyDown = (e) => {
    if (e.key === 'Enter') {
      const cmd = input;
      setInput('');
      runCommand(cmd);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      const idx = Math.min(histIdx + 1, history.length - 1);
      setHistIdx(idx);
      setInput(history[idx] || '');
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      const idx = Math.max(histIdx - 1, -1);
      setHistIdx(idx);
      setInput(idx === -1 ? '' : history[idx] || '');
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Scrollable output */}
      <div ref={scrollRef} className="terminal" style={{ flex: 1, overflowY: 'auto' }}>
        {lines.map((l, i) => (
          <div key={i} className="t-line">
            {l.t === 'cmd'    && <><span className="t-prompt">›</span> <span style={{ color: 'var(--text-primary)' }}>{l.text}</span></>}
            {l.t === 'check'  && <span className="t-ok">{l.text}</span>}
            {l.t === 'exec'   && <span style={{ color: 'var(--amber)' }}>{l.text}</span>}
            {l.t === 'event'  && <span style={{ color: 'var(--node-event)' }}>{l.text}</span>}
            {l.t === 'meta'   && <span className="t-meta">{l.text}</span>}
            {l.t === 'danger' && <span style={{ color: 'var(--danger)' }}>{l.text}</span>}
            {l.t === 'proto'  && (() => {
              const sepIdx = l.text.indexOf('·');
              const pLabel = sepIdx > -1 ? l.text.slice(0, sepIdx).trim() : l.text;
              const opName = sepIdx > -1 ? l.text.slice(sepIdx + 1).trim() : '';
              const ps = Object.values(PROTOCOL_STYLE).find(p => p.label === pLabel) || PROTOCOL_STYLE.rpc;
              return (
                <span style={{ color: 'var(--text-muted)', fontSize: 9 }}>
                  ∴ protocol{' '}
                  <span style={{
                    color: ps.color, background: `${ps.color}18`,
                    padding: '1px 5px', borderRadius: 3,
                    fontWeight: 600, letterSpacing: '0.08em',
                  }}>{pLabel}</span>
                  {opName ? <span style={{ marginLeft: 4 }}>{opName}</span> : null}
                </span>
              );
            })()}
            {l.t === 'stream' && <span style={{ color: 'var(--amber)' }}>{l.text}</span>}
          </div>
        ))}
        {busy && (
          <div className="t-line">
            <span className="t-meta" style={{ animation: 'pulse 0.8s ease-in-out infinite' }}>⟳ executing…</span>
          </div>
        )}
      </div>

      {/* Stream panel — shown when a streaming operator result is present */}
      {streamOp && streamLines.length > 0 && (
        <div style={{
          flexShrink: 0,
          borderTop: '1px solid var(--amber-border)',
          background: 'var(--bg-base)',
          maxHeight: 120, overflowY: 'auto',
        }}>
          <div style={{
            display: 'flex', alignItems: 'center', justifyContent: 'space-between',
            padding: '4px 14px',
            borderBottom: '1px solid var(--border)',
          }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--amber)', letterSpacing: '0.1em' }}>
              ▶ STREAM · {streamOp}
            </span>
            <button onClick={() => { setStreamOp(null); setStreamLines([]); }} style={{
              fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', background: 'transparent',
            }}>✕</button>
          </div>
          {streamLines.map((l, i) => (
            <div key={i} style={{ padding: '3px 14px', fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)' }}>
              {l}
            </div>
          ))}
        </div>
      )}

      {/* Input row */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 6,
        padding: '6px 14px',
        borderTop: '1px solid var(--border)',
        background: 'var(--bg-surface)',
        flexShrink: 0,
      }}>
        <span className="t-prompt" style={{ flexShrink: 0 }}>›</span>
        <input
          ref={inputRef}
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={busy}
          placeholder={busy ? 'executing…' : 'operator key=val …'}
          spellCheck={false}
          style={{
            flex: 1,
            fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-primary)',
            background: 'transparent', border: 'none', outline: 'none',
            caretColor: 'var(--amber)',
          }}
        />
      </div>
    </div>
  );
}

function IotList() {
  const devices = [];  // populated from MQTT / device API
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      {devices.length === 0 && (
        <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)', padding: '8px 0' }}>
          No devices online — seed via SEED panel
        </span>
      )}
      {devices.map((d, i) => (
        <button key={i} onClick={() => window.confirmAction?.({
          title: `${d.topic}`,
          body: `Last event: ${d.value} · ${d.last}. Send a test pulse to this MQTT topic?`,
          ctaLabel: 'PING',
          tone: 'amber',
          onConfirm: () => window.flash?.(`Ping sent · ${d.topic}`, 'info'),
        })} className="fade-up" style={{
          textAlign: 'left',
          display: 'flex', justifyContent: 'space-between', alignItems: 'center',
          padding: '8px 0', borderBottom: i < devices.length - 1 ? '1px solid var(--border)' : 'none',
          animationDelay: `${i * 50}ms`,
          background: 'transparent',
          transition: 'background var(--t-fast)',
        }}
        onMouseEnter={e => e.currentTarget.style.background = 'var(--bg-raised)'}
        onMouseLeave={e => e.currentTarget.style.background = 'transparent'}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0 }}>
            <span className="pulse" style={{ width: 4, height: 4, borderRadius: '50%', background: d.status === 'ok' ? 'var(--teal)' : 'var(--amber)', flexShrink: 0 }} />
            <div style={{ display: 'flex', flexDirection: 'column', minWidth: 0 }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{d.topic}</span>
              <span className="meta-10" style={{ fontSize: 9, color: 'var(--text-muted)' }}>{d.last}</span>
            </div>
          </div>
          <span className="val-12" style={{ fontSize: 11, color: d.status === 'ok' ? 'var(--text-primary)' : 'var(--amber)' }}>{d.value}</span>
        </button>
      ))}
    </div>
  );
}

function RollbackPanel({ sustain }) {
  const [version, setVersion] = dUseState('');
  const [rolling, setRolling] = dUseState(false);
  const [lastResult, setLastResult] = dUseState(null);

  const doRollback = async () => {
    setRolling(true);
    const sustainId = sustain?.id || 'homestead.bonnie';
    const params = version.trim()
      ? { sustain_id: sustainId, to_version: parseInt(version, 10) }
      : { sustain_id: sustainId };
    try {
      const res = await callControlOperator(sustainId, 'control.rollback', params);
      const data = res?.data?.result?.data || {};
      setLastResult(data);
      window.flash?.(
        `Rolled back to v${data.restored_version ?? '?'} · ${data.snapshot_timestamp?.slice(0, 19) ?? ''}`,
        'amber',
      );
    } catch (err) {
      window.flash?.(`Rollback failed: ${err.message}`, 'danger');
    } finally {
      setRolling(false);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      <span style={{ fontSize: 11, color: 'var(--text-secondary)', lineHeight: 1.5 }}>
        Restore state to a prior snapshot. Snapshots are created automatically by{' '}
        <span style={{ color: 'var(--amber)', fontFamily: 'var(--mono)', fontSize: 10 }}>control.execute_approved</span>.
      </span>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
        <span className="label-10">SNAPSHOT VERSION (leave blank for latest)</span>
        <input
          value={version}
          onChange={e => setVersion(e.target.value)}
          placeholder="e.g. 3  (omit = most recent)"
          style={{
            width: '100%',
            background: 'var(--bg-base)', border: '1px solid var(--border-mid)',
            borderRadius: 'var(--radius-sm)', padding: '7px 10px',
            fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)',
            outline: 'none',
          }}
        />
      </div>
      {lastResult && (
        <div style={{
          background: 'var(--bg-base)', border: '1px solid var(--amber-border)',
          borderRadius: 'var(--radius-sm)', padding: '6px 10px',
          fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--amber)',
        }}>
          Restored v{lastResult.restored_version} · {lastResult.snapshot_timestamp?.slice(0, 19) ?? '—'}
        </div>
      )}
      <div style={{ display: 'flex', gap: 6, marginTop: 2 }}>
        <PBtn
          onClick={() => window.confirmAction?.({
            title: `Rollback ${sustain?.id || 'sustain'}?`,
            body: `Restores state to snapshot ${version.trim() || 'latest'}. Only in-process snapshots are available.`,
            ctaLabel: 'ROLLBACK',
            tone: 'danger',
            onConfirm: doRollback,
          })}
          disabled={rolling}
        >
          {rolling ? '⟳' : 'ROLLBACK'}
        </PBtn>
      </div>
    </div>
  );
}

/* ───────────────────────────────────────────────────────────
   LIBRARY PANEL — Mycelium marketplace
   ─────────────────────────────────────────────────────────── */
function LibraryPanel({ openModal }) {
  const [tab, setTab] = dUseState('operatives');
  const [search, setSearch] = dUseState('');
  const [sortBy, setSortBy] = dUseState('trust'); // trust | downloads | name
  const [freeOnly, setFreeOnly] = dUseState(false);
  const [author, setAuthor] = dUseState('all');
  const [libraryData, setLibraryData] = dUseState({ operatives: [], operators: [], spores: [], widgets: [] });

  // Publish form state
  const [publishOpen, setPublishOpen] = dUseState(false);
  const [pubName,     setPubName]     = dUseState('');
  const [pubKind,     setPubKind]     = dUseState('operative');
  const [pubDesc,     setPubDesc]     = dUseState('');
  const [pubTags,     setPubTags]     = dUseState('');
  const [publishing,  setPublishing]  = dUseState(false);

  const fetchLibrary = () => {
    api.get('/devui/library')
      .then(d => {
        const data = d?.data;
        if (!data) return;
        setLibraryData({
          operatives: data.operatives || [],
          operators:  data.operators  || [],
          spores:     data.spores     || [],
          widgets:    data.widgets    || [],
        });
      })
      .catch(() => {});
  };

  dUseEffect(() => { fetchLibrary(); }, []);

  const doPublish = async () => {
    if (!pubName.trim()) { window.flash?.('Package name required', 'amber'); return; }
    const token = localStorage.getItem('sustena_token');
    if (!token) { window.flash?.('Sign in to publish packages', 'amber'); return; }
    setPublishing(true);
    try {
      const base = import.meta.env.VITE_API_BASE ?? '';
      const res = await fetch(`${base}/api/v1/arena/packages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
        body: JSON.stringify({
          name: pubName.trim(),
          kind: pubKind,
          description: pubDesc.trim() || null,
          tags: pubTags.split(',').map(t => t.trim()).filter(Boolean),
        }),
      });
      if (!res.ok) { const d = await res.json(); window.flash?.(d.detail ?? 'Publish failed', 'danger'); return; }
      window.flash?.(`${pubName} published to Mycelium`, 'ok');
      setPublishOpen(false);
      setPubName(''); setPubDesc(''); setPubTags('');
      fetchLibrary();
    } catch (err) {
      window.flash?.(`Publish failed: ${err.message}`, 'danger');
    } finally {
      setPublishing(false);
    }
  };

  const items = libraryData[tab] || [];

  // Unique authors per tab
  const authors = dUseMemo(() => {
    const set = new Set(items.map(it => it.author));
    return ['all', ...[...set]];
  }, [tab]);

  // Reset author when tab changes if invalid
  dUseEffect(() => { setAuthor('all'); }, [tab]);

  let filtered = items.filter(it => it.name.toLowerCase().includes(search.toLowerCase()));
  if (freeOnly) filtered = filtered.filter(it => (typeof it.pawa === 'string' && it.pawa === 'free') || it.pawa === 0);
  if (author !== 'all') filtered = filtered.filter(it => it.author === author);
  filtered = [...filtered].sort((a, b) => {
    if (sortBy === 'trust') return b.trust - a.trust;
    if (sortBy === 'downloads') return b.downloads - a.downloads;
    return a.name.localeCompare(b.name);
  });

  const TAB_META = {
    operatives: { label: 'OPERATIVES',     sub: 'Autonomous LLM-bounded agents',           accent: 'var(--node-operative)' },
    operators:  { label: 'OPERATORS',      sub: 'State transformations · pure functions',  accent: 'var(--node-operator)' },
    spores:     { label: 'SUSTAIN SPORES', sub: 'Forkable system templates',               accent: 'var(--node-state)' },
    widgets:    { label: 'WIDGETS',        sub: 'ResponseWidget renderers · Orchie UI',    accent: 'var(--node-event)' },
  };
  const meta = TAB_META[tab];

  return (
    <div className="panel-enter" style={{ display: 'flex', flexDirection: 'column', height: '100%', padding: '24px 28px', gap: 16 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-end' }}>
        <div>
          <span className="label-10" style={{ display: 'block', marginBottom: 4 }}>MYCELIUM · NETWORK LIBRARY</span>
          <h2 style={{ fontFamily: 'var(--ui)', fontSize: 22, fontWeight: 500, letterSpacing: '-0.01em' }}>
            <span style={{ color: meta.accent }}>{filtered.length}</span> <span style={{ color: 'var(--text-secondary)' }}>{tab} · {meta.sub.toLowerCase()}</span>
          </h2>
        </div>
        <PBtn onClick={() => setPublishOpen(p => !p)}><Icon name="plus" size={11} /> PUBLISH</PBtn>
      </div>

      {/* Inline publish form */}
      {publishOpen && (
        <div style={{
          background: 'var(--bg-base)', border: '1px solid var(--amber-border)',
          borderRadius: 'var(--radius-md)', padding: '16px 18px',
          display: 'flex', flexDirection: 'column', gap: 10,
          animation: 'modalSlideIn 0.2s cubic-bezier(0.22,0.61,0.36,1)',
        }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <span className="label-10" style={{ color: 'var(--amber)' }}>PUBLISH PACKAGE</span>
            <button onClick={() => setPublishOpen(false)} style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)', background: 'none', border: 'none', cursor: 'pointer' }}>✕</button>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 160px', gap: 8 }}>
            <input value={pubName} onChange={e => setPubName(e.target.value)} placeholder="Package name" style={{
              background: 'var(--bg-surface)', border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
              padding: '7px 10px', fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)', outline: 'none',
            }} />
            <select value={pubKind} onChange={e => setPubKind(e.target.value)} style={{
              background: 'var(--bg-surface)', border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
              padding: '7px 10px', fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)', outline: 'none',
            }}>
              {['operative', 'operator', 'spore', 'widget'].map(k => (
                <option key={k} value={k}>{k}</option>
              ))}
            </select>
          </div>
          <input value={pubDesc} onChange={e => setPubDesc(e.target.value)} placeholder="Description (optional)" style={{
            background: 'var(--bg-surface)', border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
            padding: '7px 10px', fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-primary)', outline: 'none',
          }} />
          <input value={pubTags} onChange={e => setPubTags(e.target.value)} placeholder="Tags: finance, budget, household" style={{
            background: 'var(--bg-surface)', border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
            padding: '7px 10px', fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)', outline: 'none',
          }} />
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 6 }}>
            <PBtn variant="ghost" onClick={() => setPublishOpen(false)}>CANCEL</PBtn>
            <PBtn onClick={doPublish} disabled={!pubName.trim() || publishing}>
              {publishing ? '⟳ PUBLISHING…' : 'PUBLISH TO MYCELIUM'}
            </PBtn>
          </div>
        </div>
      )}

      {/* Tabs */}
      <div style={{ display: 'flex', gap: 4, borderBottom: '1px solid var(--border)' }}>
        {Object.entries(TAB_META).map(([id, m]) => (
          <button key={id} onClick={() => setTab(id)} style={{
            position: 'relative',
            padding: '10px 14px',
            fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500,
            letterSpacing: '0.06em', textTransform: 'uppercase',
            color: tab === id ? m.accent : 'var(--text-secondary)',
            background: 'transparent',
            transition: 'color var(--t-fast)',
            display: 'inline-flex', alignItems: 'center', gap: 6,
          }}>
            <LibraryMark kind={id} size={14} muted={tab !== id} />
            {m.label} <span style={{ color: 'var(--text-muted)', marginLeft: 2 }}>{(libraryData[id] || []).length}</span>
            {tab === id && <span style={{ position: 'absolute', left: 14, right: 14, bottom: -1, height: 2, background: m.accent }} />}
          </button>
        ))}
      </div>

      {/* Filter bar */}
      <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
        <input
          value={search}
          onChange={e => setSearch(e.target.value)}
          placeholder={`Search ${tab}...`}
          style={{
            flex: '1 1 240px', minWidth: 240,
            background: 'var(--bg-surface)', border: '1px solid var(--border)',
            borderRadius: 'var(--radius-sm)', padding: '8px 12px',
            fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-primary)',
            outline: 'none',
          }}
          onFocus={e => e.target.style.borderColor = 'var(--amber-border)'}
          onBlur={e => e.target.style.borderColor = 'var(--border)'}
        />
        <FilterGroup label="SORT">
          {[
            { v: 'trust', l: 'TRUST' },
            { v: 'downloads', l: 'INSTALLS' },
            { v: 'name', l: 'A–Z' },
          ].map(o => <TBtn key={o.v} active={sortBy === o.v} onClick={() => setSortBy(o.v)}>{o.l}</TBtn>)}
        </FilterGroup>
        <FilterGroup label="PRICE">
          <TBtn active={!freeOnly} onClick={() => setFreeOnly(false)}>ALL</TBtn>
          <TBtn active={freeOnly} onClick={() => setFreeOnly(true)}>FREE</TBtn>
        </FilterGroup>
        <FilterGroup label="AUTHOR">
          <select value={author} onChange={e => setAuthor(e.target.value)} style={{
            background: 'var(--bg-surface)', border: '1px solid var(--border)',
            borderRadius: 'var(--radius-sm)', padding: '4px 8px',
            fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.06em', textTransform: 'uppercase',
            color: author === 'all' ? 'var(--text-muted)' : 'var(--amber)',
            outline: 'none',
          }}>
            {authors.map(a => <option key={a} value={a}>{a === 'all' ? 'ANY AUTHOR' : a}</option>)}
          </select>
        </FilterGroup>
        {(search || freeOnly || author !== 'all' || sortBy !== 'trust') && (
          <button onClick={() => { setSearch(''); setSortBy('trust'); setFreeOnly(false); setAuthor('all'); }} style={{
            fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.08em', color: 'var(--text-muted)',
            padding: '4px 8px', background: 'transparent', border: '1px dashed var(--border-mid)',
            borderRadius: 'var(--radius-sm)',
          }}>CLEAR</button>
        )}
      </div>

      {/* Grid — layout varies per tab */}
      <div style={{
        display: 'grid',
        gridTemplateColumns: tab === 'operators'
          ? '1fr'
          : 'repeat(auto-fill, minmax(280px, 1fr))',
        gap: tab === 'operators' ? 0 : 12,
        flex: 1, minHeight: 0, overflow: 'auto', alignContent: 'start',
      }}>
        {filtered.length === 0
          ? <Empty
              label={items.length === 0 ? 'mycelium is quiet · no packages yet' : 'NO MATCH'}
              sub={items.length === 0 ? 'publish the first package above' : 'try clearing some filters'}
            />
          : filtered.map((it, i) => {
              if (tab === 'operatives') return <OperativeLibCard key={it.id || it.name} it={it} delay={i * 40} onClick={() => openModal(it, tab)} />;
              if (tab === 'operators')  return <OperatorLibRow  key={it.id || it.name} it={it} delay={i * 30} onClick={() => openModal(it, tab)} />;
              if (tab === 'spores')     return <SporeLibCard    key={it.id || it.name} it={it} delay={i * 40} onClick={() => openModal(it, tab)} />;
              if (tab === 'widgets')    return <WidgetLibCard   key={it.id || it.name} it={it} delay={i * 40} onClick={() => openModal(it, tab)} />;
            })}
      </div>
    </div>
  );
}

function FilterGroup({ label, children }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
      <span className="label-10" style={{ fontSize: 9 }}>{label}</span>
      <div style={{ display: 'flex', gap: 4 }}>{children}</div>
    </div>
  );
}

/* ─── Library mark — typed icon per kind ──────────────────── */
function LibraryMark({ kind, size = 22, muted = false }) {
  const accent = muted ? 'var(--text-muted)' : {
    operatives: 'var(--node-operative)',
    operators:  'var(--node-operator)',
    spores:     'var(--node-state)',
    widgets:    'var(--node-event)',
  }[kind];
  const s = { width: size, height: size, fill: 'none', stroke: accent, strokeWidth: 1.4, strokeLinecap: 'round', strokeLinejoin: 'round' };
  switch (kind) {
    case 'operatives':
      // Octagon with central dot (agent)
      return (
        <svg {...s} viewBox="0 0 24 24">
          <polygon points="8,3 16,3 21,8 21,16 16,21 8,21 3,16 3,8" />
          <circle cx="12" cy="12" r="2.5" fill={accent} stroke="none" />
          <circle cx="9" cy="9" r="0.7" fill={accent} stroke="none" />
          <circle cx="15" cy="9" r="0.7" fill={accent} stroke="none" />
        </svg>
      );
    case 'operators':
      // Pill with port markers
      return (
        <svg {...s} viewBox="0 0 24 24">
          <rect x="3" y="8" width="18" height="8" rx="4" />
          <line x1="1" y1="12" x2="3" y2="12" />
          <line x1="21" y1="12" x2="23" y2="12" />
          <circle cx="12" cy="12" r="1.5" fill={accent} stroke="none" />
        </svg>
      );
    case 'spores':
      // Spore: ring with budding nodes
      return (
        <svg {...s} viewBox="0 0 24 24">
          <circle cx="12" cy="12" r="5" />
          <circle cx="12" cy="4" r="1.8" fill={accent} stroke="none" />
          <circle cx="20" cy="14" r="1.8" fill={accent} stroke="none" />
          <circle cx="6" cy="18" r="1.8" fill={accent} stroke="none" />
          <line x1="12" y1="7" x2="12" y2="5.8" />
          <line x1="16.5" y1="13" x2="18.2" y2="13.7" />
          <line x1="8.5" y1="14" x2="7" y2="16.5" />
        </svg>
      );
    case 'widgets':
      // Layout grid
      return (
        <svg {...s} viewBox="0 0 24 24">
          <rect x="3" y="3" width="8" height="8" rx="1" />
          <rect x="13" y="3" width="8" height="4" rx="1" fill={accent} stroke="none" />
          <rect x="13" y="9" width="8" height="12" rx="1" />
          <rect x="3" y="13" width="8" height="8" rx="1" fill={accent} stroke="none" opacity="0.35" />
        </svg>
      );
    default: return null;
  }
}

/* ─── OPERATIVES: rich agent cards with avatar + capabilities ───── */
function OperativeLibCard({ it, delay, onClick }) {
  return (
    <button onClick={onClick} className="fade-up lift" style={{
      textAlign: 'left',
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)',
      padding: 14,
      display: 'flex', flexDirection: 'column', gap: 10,
      animationDelay: `${delay}ms`,
      position: 'relative', overflow: 'hidden',
    }}>
      <span style={{ position: 'absolute', top: 0, left: 0, right: 0, height: 2, background: 'var(--node-operative)' }} />
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12 }}>
        <div style={{
          width: 38, height: 38, borderRadius: '50%',
          background: 'var(--bg-base)', border: `1px solid ${'var(--node-operative)'}`,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          boxShadow: '0 0 12px rgba(42,184,160,0.25)',
          flexShrink: 0,
        }}>
          <LibraryMark kind="operatives" size={22} />
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
            <span style={{ fontFamily: 'var(--ui)', fontSize: 15, fontWeight: 500 }}>{it.name}</span>
            <div style={{ display: 'flex', alignItems: 'center', gap: 3 }}>
              <span style={{ color: 'var(--amber)', fontSize: 10 }}>★</span>
              <span className="val-12" style={{ fontSize: 11 }}>{it.trust}</span>
            </div>
          </div>
          <div className="meta-10" style={{ color: 'var(--text-muted)', marginTop: 2 }}>
            {it.author} · v{it.version}
          </div>
        </div>
      </div>
      <span style={{ fontSize: 11, color: 'var(--text-secondary)', lineHeight: 1.5, minHeight: 32 }}>{it.desc}</span>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', paddingTop: 8, borderTop: '1px solid var(--border)' }}>
        <div style={{ display: 'flex', gap: 12 }}>
          <Stat label="PAWA" value={it.pawa} />
          <Stat label="INSTALLS" value={it.downloads.toLocaleString()} />
        </div>
        <span style={{
          padding: '4px 10px', fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
          color: 'var(--node-operative)', border: `1px solid var(--node-operative)`,
          borderRadius: 'var(--radius-sm)',
        }}>INSTALL</span>
      </div>
    </button>
  );
}

/* ─── OPERATORS: list-style rows with signature ─────────────── */
function OperatorLibRow({ it, delay, onClick }) {
  return (
    <button onClick={onClick} className="fade-up" style={{
      textAlign: 'left',
      display: 'grid', gridTemplateColumns: '36px 1.6fr 1.4fr 1fr 70px 80px',
      gap: 14, alignItems: 'center',
      padding: '12px 16px',
      borderBottom: '1px solid var(--border)',
      background: 'transparent',
      animationDelay: `${delay}ms`,
      transition: 'background var(--t-fast)',
    }}
    onMouseEnter={e => e.currentTarget.style.background = 'var(--bg-raised)'}
    onMouseLeave={e => e.currentTarget.style.background = 'transparent'}
    >
      <LibraryMark kind="operators" size={20} />
      <div style={{ display: 'flex', flexDirection: 'column', minWidth: 0 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 13, color: 'var(--text-primary)', fontWeight: 500 }}>{it.name}</span>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>{it.author} · v{it.version}</span>
      </div>
      <span style={{ fontSize: 11, color: 'var(--text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{it.desc}</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)' }}>
        (State, Inputs) → <span style={{ color: 'var(--node-operator)' }}>Δ</span>
      </span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--amber)', textAlign: 'right' }}>{it.pawa} pwa</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-secondary)', textAlign: 'right' }}>{it.downloads.toLocaleString()}</span>
    </button>
  );
}

/* ─── SPORES: template cards with state-tree preview ──────── */
function SporeLibCard({ it, delay, onClick }) {
  return (
    <button onClick={onClick} className="fade-up lift" style={{
      textAlign: 'left',
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)',
      padding: 14,
      display: 'flex', flexDirection: 'column', gap: 10,
      animationDelay: `${delay}ms`,
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <LibraryMark kind="spores" size={20} />
          <div>
            <div style={{ fontFamily: 'var(--ui)', fontSize: 14, fontWeight: 500 }}>{it.name}</div>
            <div className="meta-10" style={{ color: 'var(--text-muted)', marginTop: 2 }}>{it.author} · v{it.version}</div>
          </div>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 3 }}>
          <span style={{ color: 'var(--amber)', fontSize: 10 }}>★</span>
          <span className="val-12" style={{ fontSize: 11 }}>{it.trust}</span>
        </div>
      </div>
      {/* mini state tree preview */}
      <div style={{ background: 'var(--bg-base)', borderRadius: 'var(--radius-sm)', padding: '8px 10px', display: 'flex', flexDirection: 'column', gap: 2, fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)' }}>
        <span><span style={{ color: 'var(--node-state)' }}>▸</span> state.<span style={{ color: 'var(--text-secondary)' }}>finances</span></span>
        <span style={{ paddingLeft: 12 }}><span style={{ color: 'var(--node-state)' }}>▸</span> pockets · burn_rate · cash</span>
        <span><span style={{ color: 'var(--node-state)' }}>▸</span> state.<span style={{ color: 'var(--text-secondary)' }}>{it.name.toLowerCase()}_specific</span></span>
      </div>
      <span style={{ fontSize: 11, color: 'var(--text-secondary)', lineHeight: 1.5, minHeight: 32 }}>{it.desc}</span>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', paddingTop: 8, borderTop: '1px solid var(--border)' }}>
        <div style={{ display: 'flex', gap: 12 }}>
          <Stat label="LIC" value={it.pawa} />
          <Stat label="FORKS" value={it.downloads.toLocaleString()} />
        </div>
        <span style={{
          padding: '4px 10px', fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500, letterSpacing: '0.08em',
          color: 'var(--node-state)', border: `1px solid var(--node-state)`,
          borderRadius: 'var(--radius-sm)',
        }}>FORK SPORE</span>
      </div>
    </button>
  );
}

/* ─── WIDGETS: preview thumbnails ─────────────────────────── */
function WidgetLibCard({ it, delay, onClick }) {
  return (
    <button onClick={onClick} className="fade-up lift" style={{
      textAlign: 'left',
      background: 'var(--bg-surface)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius-md)',
      padding: 0,
      display: 'flex', flexDirection: 'column',
      animationDelay: `${delay}ms`,
      overflow: 'hidden',
    }}>
      {/* Preview */}
      <div style={{
        height: 100,
        background: 'linear-gradient(135deg, var(--bg-raised), var(--bg-base))',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        borderBottom: '1px solid var(--border)',
        position: 'relative', overflow: 'hidden',
      }}>
        <WidgetPreview name={it.name} />
      </div>
      <div style={{ padding: 12, display: 'flex', flexDirection: 'column', gap: 6 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
          <div>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-primary)', fontWeight: 500 }}>{it.name}</div>
            <div className="meta-10" style={{ color: 'var(--text-muted)', marginTop: 2 }}>{it.author} · v{it.version}</div>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 3 }}>
            <span style={{ color: 'var(--amber)', fontSize: 10 }}>★</span>
            <span className="val-12" style={{ fontSize: 10 }}>{it.trust}</span>
          </div>
        </div>
        <span className="meta-10" style={{ color: 'var(--text-muted)', minHeight: 28, lineHeight: 1.4 }}>{it.desc}</span>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', paddingTop: 6, borderTop: '1px solid var(--border)' }}>
          <span className="meta-10" style={{ fontSize: 9 }}>{it.downloads.toLocaleString()} uses</span>
          <span style={{
            padding: '3px 8px', fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500, letterSpacing: '0.08em',
            color: 'var(--node-event)', border: `1px solid var(--node-event)`,
            borderRadius: 'var(--radius-sm)',
          }}>INSTALL</span>
        </div>
      </div>
    </button>
  );
}

/* Mini widget previews — abstract geometric representations */
function WidgetPreview({ name }) {
  if (name.includes('proposal')) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 3, width: '80%' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between' }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)' }}>SUS-0148</span>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--amber)' }}>● VOTING</span>
        </div>
        <div style={{ height: 3, display: 'flex', borderRadius: 2, overflow: 'hidden', background: 'var(--bg-overlay)' }}>
          <div style={{ width: '62%', background: 'var(--teal)' }} />
          <div style={{ width: '18%', background: 'var(--danger)' }} />
        </div>
      </div>
    );
  }
  if (name.includes('mpesa')) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4 }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--amber)', letterSpacing: '0.1em' }}>M-PESA · STK</span>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 16, fontWeight: 500, color: 'var(--text-primary)' }}>1,120</span>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)' }}>KSH</span>
      </div>
    );
  }
  if (name.includes('budget') || name.includes('allocation')) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 4, width: '80%' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', fontFamily: 'var(--mono)', fontSize: 8 }}>
          <span style={{ color: 'var(--text-muted)' }}>FOOD</span>
          <span style={{ color: 'var(--text-primary)' }}>5,000</span>
        </div>
        <div style={{ height: 4, background: 'var(--bg-overlay)', borderRadius: 2, overflow: 'hidden' }}>
          <div style={{ width: '70%', height: '100%', background: 'var(--teal)' }} />
        </div>
      </div>
    );
  }
  if (name.includes('burn') || name.includes('sparkline')) {
    return (
      <svg width="120" height="44" viewBox="0 0 120 44" style={{ overflow: 'visible' }}>
        <path d="M 4 36 L 20 30 L 36 32 L 52 22 L 68 24 L 84 16 L 100 18 L 116 8"
          fill="none" stroke="var(--amber)" strokeWidth="1.4" />
        <circle cx="116" cy="8" r="2.5" fill="var(--amber)" />
      </svg>
    );
  }
  if (name.includes('pantry')) {
    return (
      <div style={{ display: 'flex', gap: 4 }}>
        {[0.4, 0.8, 0.2, 0.6].map((p, i) => (
          <div key={i} style={{
            width: 16, height: 36 * (0.2 + p * 0.8),
            background: p < 0.3 ? 'var(--danger)' : p < 0.5 ? 'var(--amber)' : 'var(--teal)',
            borderRadius: 2, alignSelf: 'flex-end',
          }} />
        ))}
      </div>
    );
  }
  if (name.includes('alert')) {
    return (
      <div style={{
        padding: '6px 10px', borderLeft: '2px solid var(--amber)',
        background: 'var(--amber-glow)', borderRadius: 'var(--radius-sm)',
      }}>
        <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--amber)', letterSpacing: '0.1em' }}>⚠ THRESHOLD</span>
      </div>
    );
  }
  return <LibraryMark kind="widgets" size={36} />;
}

function Stat({ label, value }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column' }}>
      <span className="meta-10" style={{ fontSize: 9 }}>{label}</span>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, color: 'var(--text-primary)' }}>{value}</span>
    </div>
  );
}

/* ───────────────────────────────────────────────────────────
   SEED PANEL — inject real sustain data through the UI
   ─────────────────────────────────────────────────────────── */
function SeedPanel() {
  /* ── local state ── */
  const [sustainId,   setSustainId]   = dUseState('homestead.bonnie');
  const [label,       setLabel]       = dUseState('Bonnie\'s Homestead');
  const [type,        setType]        = dUseState('household');
  const [description, setDescription] = dUseState('');

  const [pocketSid,   setPocketSid]   = dUseState('homestead.bonnie');
  const [pocketName,  setPocketName]  = dUseState('');
  const [allocation,  setAllocation]  = dUseState('');
  const [ceiling,     setCeiling]     = dUseState('');
  const [sessionPockets, setSessionPockets] = dUseState([]);

  const [operativeSid, setOperativeSid] = dUseState('homestead.bonnie');
  const [operatives,   setOperatives]   = dUseState({
    mentor: false, protege: false, curator: false,
    attache: false, navigator: false, clerk: false,
  });

  const [evSid,   setEvSid]   = dUseState('homestead.bonnie');
  const [evType,  setEvType]  = dUseState('received');
  const [evAmt,   setEvAmt]   = dUseState('');
  const [evDesc,  setEvDesc]  = dUseState('');

  const [status,        setStatus]        = dUseState(null);
  const [statusLoading, setStatusLoading] = dUseState(false);

  /* ── helpers ── */
  const flash = (msg, tone = 'ok') => window.flash?.(msg, tone);

  const inputStyle = {
    width: '100%', boxSizing: 'border-box',
    background: 'var(--bg-base)',
    border: '1px solid var(--border-mid)',
    borderRadius: 'var(--radius-sm)',
    padding: '7px 10px',
    fontFamily: 'var(--mono)', fontSize: 11,
    color: 'var(--text-primary)',
    outline: 'none',
  };

  const selectStyle = { ...inputStyle, cursor: 'pointer' };

  const labelStyle = {
    display: 'block', marginBottom: 5,
    fontFamily: 'var(--mono)', fontSize: 9,
    color: 'var(--text-muted)', letterSpacing: '0.08em', textTransform: 'uppercase',
  };

  const fieldStyle = { display: 'flex', flexDirection: 'column', gap: 0 };

  const sectionHead = (title, sub) => (
    <div style={{ marginBottom: 14 }}>
      <span style={{ fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 600,
        letterSpacing: '0.1em', color: 'var(--text-primary)', textTransform: 'uppercase' }}>{title}</span>
      {sub && <span style={{ fontFamily: 'var(--mono)', fontSize: 9,
        color: 'var(--text-muted)', marginLeft: 8 }}>{sub}</span>}
    </div>
  );

  /* ── actions ── */
  async function submitSustain() {
    try {
      await api.post('/seed/sustain', {
        id: sustainId.trim(), label: label.trim(),
        type, description: description.trim(),
      });
      flash(`Sustain "${sustainId}" seeded`, 'ok');
      setPocketSid(sustainId.trim());
      setOperativeSid(sustainId.trim());
      setEvSid(sustainId.trim());
      loadStatus();
    } catch (e) {
      flash(e.message, 'danger');
    }
  }

  async function addPocket() {
    if (!pocketName.trim()) { flash('Pocket name required', 'amber'); return; }
    try {
      await api.post('/seed/pocket', {
        sustain_id: pocketSid.trim(), name: pocketName.trim(),
        allocation: parseFloat(allocation) || 0,
        ceiling: parseFloat(ceiling) || 0,
        currency: 'KES',
      });
      flash(`Pocket "${pocketName}" added`, 'ok');
      setSessionPockets(p => [...p, {
        sustain_id: pocketSid, name: pocketName,
        allocation: parseFloat(allocation) || 0,
        ceiling: parseFloat(ceiling) || 0,
      }]);
      setPocketName(''); setAllocation(''); setCeiling('');
      loadStatus();
    } catch (e) {
      flash(e.message, 'danger');
    }
  }

  async function toggleOperative(opId, enabled) {
    setOperatives(o => ({ ...o, [opId]: enabled }));
    try {
      await api.post('/seed/operative', {
        sustain_id: operativeSid.trim(),
        operative_id: opId,
        config: {},
        enabled,
      });
      flash(`${opId} ${enabled ? 'enabled' : 'disabled'}`, 'ok');
    } catch (e) {
      setOperatives(o => ({ ...o, [opId]: !enabled }));
      flash(e.message, 'danger');
    }
  }

  async function injectEvent() {
    if (!evAmt || isNaN(parseFloat(evAmt))) { flash('Valid amount required', 'amber'); return; }
    try {
      await api.post('/seed/event', {
        sustain_id: evSid.trim(), type: evType,
        amount: parseFloat(evAmt),
        description: evDesc.trim(), metadata: {},
      });
      flash(`Event injected · KES ${parseFloat(evAmt).toLocaleString()}`, 'ok');
      setEvAmt(''); setEvDesc('');
      loadStatus();
    } catch (e) {
      flash(e.message, 'danger');
    }
  }

  async function loadStatus() {
    setStatusLoading(true);
    try {
      const res = await api.get('/seed/status');
      setStatus(res.data);
    } catch (e) {
      flash('Status fetch failed', 'amber');
    } finally {
      setStatusLoading(false);
    }
  }

  dUseEffect(() => { loadStatus(); }, []);

  /* ── operative config ── */
  const OPERATIVES = [
    { id: 'mentor',    label: 'Mentor',    color: 'var(--node-operative)' },
    { id: 'protege',   label: 'Protégé',   color: 'var(--node-operative)' },
    { id: 'curator',   label: 'Curator',   color: 'var(--node-operative)' },
    { id: 'attache',   label: 'Attaché',   color: 'var(--node-operative)' },
    { id: 'navigator', label: 'Navigator', color: 'var(--node-operative)' },
    { id: 'clerk',     label: 'Clerk',     color: 'var(--node-operative)' },
  ];

  /* ── render ── */
  return (
    <div className="panel-enter" style={{
      height: '100%', overflow: 'auto',
      padding: '28px 32px',
      display: 'flex', flexDirection: 'column', gap: 0,
    }}>
      {/* Header */}
      <div style={{ marginBottom: 28 }}>
        <span style={{
          fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 600,
          letterSpacing: '0.12em', color: 'var(--amber)', textTransform: 'uppercase',
        }}>SEED</span>
        <span style={{
          fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)',
          marginLeft: 10,
        }}>inject real sustain data · no SQL required</span>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 20, alignItems: 'start' }}>

        {/* ── LEFT COLUMN ── */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>

          {/* 1. Sustain form */}
          <div style={{
            background: 'var(--bg-surface)', border: '1px solid var(--border)',
            borderRadius: 'var(--radius-md)', padding: 18,
          }}>
            {sectionHead('SUSTAIN', 'create or update')}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              <div style={fieldStyle}>
                <label style={labelStyle}>ID</label>
                <input style={inputStyle} value={sustainId} placeholder="homestead.bonnie"
                  onChange={e => setSustainId(e.target.value)} />
              </div>
              <div style={fieldStyle}>
                <label style={labelStyle}>LABEL</label>
                <input style={inputStyle} value={label} placeholder="Bonnie's Homestead"
                  onChange={e => setLabel(e.target.value)} />
              </div>
              <div style={fieldStyle}>
                <label style={labelStyle}>TYPE</label>
                <select style={selectStyle} value={type} onChange={e => setType(e.target.value)}>
                  <option value="household">Household</option>
                  <option value="business">Business</option>
                  <option value="chama">Chama</option>
                  <option value="farm">Farm</option>
                </select>
              </div>
              <div style={fieldStyle}>
                <label style={labelStyle}>DESCRIPTION</label>
                <input style={inputStyle} value={description} placeholder="Optional description"
                  onChange={e => setDescription(e.target.value)} />
              </div>
              <button onClick={submitSustain} style={{
                marginTop: 4, padding: '7px 0',
                fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 600,
                letterSpacing: '0.1em', textTransform: 'uppercase',
                background: 'var(--amber)', color: 'var(--bg-base)',
                border: 'none', borderRadius: 'var(--radius-sm)', cursor: 'pointer',
                transition: 'opacity var(--t-fast)',
              }}
              onMouseEnter={e => e.currentTarget.style.opacity = '0.85'}
              onMouseLeave={e => e.currentTarget.style.opacity = '1'}
              >SEED SUSTAIN</button>
            </div>
          </div>

          {/* 2. Pocket form */}
          <div style={{
            background: 'var(--bg-surface)', border: '1px solid var(--border)',
            borderRadius: 'var(--radius-md)', padding: 18,
          }}>
            {sectionHead('POCKET', 'allocation bucket')}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              <div style={fieldStyle}>
                <label style={labelStyle}>SUSTAIN ID</label>
                <input style={inputStyle} value={pocketSid} onChange={e => setPocketSid(e.target.value)} />
              </div>
              <div style={fieldStyle}>
                <label style={labelStyle}>POCKET NAME</label>
                <input style={inputStyle} value={pocketName} placeholder="e.g. groceries"
                  onChange={e => setPocketName(e.target.value)} />
              </div>
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
                <div style={fieldStyle}>
                  <label style={labelStyle}>ALLOCATION (KES)</label>
                  <input style={inputStyle} type="number" value={allocation} placeholder="5000"
                    onChange={e => setAllocation(e.target.value)} />
                </div>
                <div style={fieldStyle}>
                  <label style={labelStyle}>CEILING (KES)</label>
                  <input style={inputStyle} type="number" value={ceiling} placeholder="8000"
                    onChange={e => setCeiling(e.target.value)} />
                </div>
              </div>
              <button onClick={addPocket} style={{
                marginTop: 4, padding: '7px 0',
                fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 600,
                letterSpacing: '0.1em', textTransform: 'uppercase',
                background: 'transparent', color: 'var(--teal)',
                border: '1px solid var(--teal)', borderRadius: 'var(--radius-sm)', cursor: 'pointer',
                transition: 'all var(--t-fast)',
              }}
              onMouseEnter={e => { e.currentTarget.style.background = 'rgba(0,200,170,0.08)'; }}
              onMouseLeave={e => { e.currentTarget.style.background = 'transparent'; }}
              >+ ADD POCKET</button>

              {/* Session pocket list */}
              {sessionPockets.length > 0 && (
                <div style={{ marginTop: 6, display: 'flex', flexDirection: 'column', gap: 4 }}>
                  <span style={{ ...labelStyle, marginBottom: 4 }}>ADDED THIS SESSION</span>
                  {sessionPockets.map((p, i) => (
                    <div key={i} style={{
                      display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                      padding: '5px 8px',
                      background: 'var(--bg-base)', borderRadius: 'var(--radius-sm)',
                      border: '1px solid var(--border)',
                    }}>
                      <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)' }}>{p.name}</span>
                      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>
                        {p.allocation.toLocaleString()} / {p.ceiling.toLocaleString()} KES
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>

        {/* ── RIGHT COLUMN ── */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>

          {/* 3. Operatives */}
          <div style={{
            background: 'var(--bg-surface)', border: '1px solid var(--border)',
            borderRadius: 'var(--radius-md)', padding: 18,
          }}>
            {sectionHead('OPERATIVES', 'enable for sustain')}
            <div style={{ marginBottom: 10 }}>
              <label style={labelStyle}>SUSTAIN ID</label>
              <input style={inputStyle} value={operativeSid} onChange={e => setOperativeSid(e.target.value)} />
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {OPERATIVES.map(op => (
                <label key={op.id} style={{
                  display: 'flex', alignItems: 'center', gap: 10,
                  padding: '7px 10px',
                  background: operatives[op.id] ? 'rgba(255,186,60,0.06)' : 'var(--bg-base)',
                  border: `1px solid ${operatives[op.id] ? 'var(--amber-border)' : 'var(--border)'}`,
                  borderRadius: 'var(--radius-sm)', cursor: 'pointer',
                  transition: 'all var(--t-fast)',
                }}>
                  <input
                    type="checkbox"
                    checked={operatives[op.id]}
                    onChange={e => toggleOperative(op.id, e.target.checked)}
                    style={{ accentColor: 'var(--amber)', width: 13, height: 13, cursor: 'pointer' }}
                  />
                  <span style={{
                    fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 500,
                    color: operatives[op.id] ? 'var(--amber)' : 'var(--text-secondary)',
                    letterSpacing: '0.06em', textTransform: 'uppercase',
                    transition: 'color var(--t-fast)',
                  }}>{op.label}</span>
                </label>
              ))}
            </div>
          </div>

          {/* 4. Event injector */}
          <div style={{
            background: 'var(--bg-surface)', border: '1px solid var(--border)',
            borderRadius: 'var(--radius-md)', padding: 18,
          }}>
            {sectionHead('EVENT INJECTOR', 'manual M-Pesa entry')}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              <div style={fieldStyle}>
                <label style={labelStyle}>SUSTAIN ID</label>
                <input style={inputStyle} value={evSid} onChange={e => setEvSid(e.target.value)} />
              </div>
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
                <div style={fieldStyle}>
                  <label style={labelStyle}>TYPE</label>
                  <select style={selectStyle} value={evType} onChange={e => setEvType(e.target.value)}>
                    <option value="received">Received</option>
                    <option value="sent">Sent</option>
                    <option value="purchase">Purchase</option>
                    <option value="sale">Sale</option>
                  </select>
                </div>
                <div style={fieldStyle}>
                  <label style={labelStyle}>AMOUNT (KES)</label>
                  <input style={inputStyle} type="number" value={evAmt} placeholder="1200"
                    onChange={e => setEvAmt(e.target.value)} />
                </div>
              </div>
              <div style={fieldStyle}>
                <label style={labelStyle}>DESCRIPTION</label>
                <input style={inputStyle} value={evDesc} placeholder="e.g. Naivas groceries"
                  onChange={e => setEvDesc(e.target.value)} />
              </div>
              <button onClick={injectEvent} style={{
                marginTop: 4, padding: '7px 0',
                fontFamily: 'var(--mono)', fontSize: 10, fontWeight: 600,
                letterSpacing: '0.1em', textTransform: 'uppercase',
                background: 'transparent', color: 'var(--node-event)',
                border: '1px solid var(--node-event)', borderRadius: 'var(--radius-sm)', cursor: 'pointer',
                transition: 'all var(--t-fast)',
              }}
              onMouseEnter={e => { e.currentTarget.style.background = 'rgba(120,180,255,0.08)'; }}
              onMouseLeave={e => { e.currentTarget.style.background = 'transparent'; }}
              >INJECT EVENT</button>
            </div>
          </div>
        </div>
      </div>

      {/* 5. Seed status */}
      <div style={{
        marginTop: 20,
        background: 'var(--bg-surface)', border: '1px solid var(--border)',
        borderRadius: 'var(--radius-md)', padding: 18,
      }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 14 }}>
          {sectionHead('SEED STATUS', 'currently in DB')}
          <button onClick={loadStatus} style={{
            fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.08em',
            color: 'var(--text-muted)', background: 'transparent',
            border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)',
            padding: '3px 8px', cursor: 'pointer', textTransform: 'uppercase',
          }}>{statusLoading ? '...' : 'REFRESH'}</button>
        </div>

        {!status && !statusLoading && (
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>
            No data yet — seed a sustain above.
          </span>
        )}
        {statusLoading && (
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-muted)' }}>Loading…</span>
        )}
        {status && status.sustains && status.sustains.length === 0 && (
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>
            No seeded sustains found.
          </span>
        )}
        {status && status.sustains && status.sustains.length > 0 && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {/* Column headers */}
            <div style={{
              display: 'grid', gridTemplateColumns: '1fr 1fr 80px 80px 80px',
              gap: 8, padding: '0 8px 6px',
              borderBottom: '1px solid var(--border)',
            }}>
              {['ID', 'NAME', 'POCKETS', 'EVENTS', 'OPERATIVES'].map(h => (
                <span key={h} style={{ fontFamily: 'var(--mono)', fontSize: 8,
                  color: 'var(--text-dim)', letterSpacing: '0.08em' }}>{h}</span>
              ))}
            </div>
            {status.sustains.map(s => (
              <div key={s.id} style={{
                display: 'grid', gridTemplateColumns: '1fr 1fr 80px 80px 80px',
                gap: 8, padding: '6px 8px',
                background: 'var(--bg-base)', borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--border)',
              }}>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 10,
                  color: 'var(--amber)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {s.id}
                </span>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 10,
                  color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {s.name}
                </span>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 11,
                  color: 'var(--teal)', textAlign: 'center' }}>{s.pocket_count}</span>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 11,
                  color: 'var(--node-event)', textAlign: 'center' }}>{s.event_count}</span>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 11,
                  color: 'var(--node-operative)', textAlign: 'center' }}>{s.operative_count}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

Object.assign(window, {
  EditorPanel, ControllerPanel, LibraryPanel, SeedPanel,
  LibraryMark,
  Knob, Slider, Stepper, VerticalScale, Segmented, BigToggle, RockerSwitch, LedGrid,
  OperativeLibCard, OperatorLibRow, SporeLibCard, WidgetLibCard, WidgetPreview, Stat,
});
