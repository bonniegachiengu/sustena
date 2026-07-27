import React from 'react';
import { api } from '../../lib/api.js';
/* Simulator panel — real client-side scenario tree over /devui/simulate (Slice 9).
   Root = live state (fetched from /devui/state). Every other node is a
   hypothetical branch: a sequence of operators run through the sandboxed
   simulate() path (same gate, same roll-up math, zero DB writes). Nodes are
   never mistaken for real state — every simulated node carries a visible
   "SIM" badge and its own "hypothetical" caption; only the root is LIVE. */

const { useState: dUseState, useEffect: dUseEffect, useMemo: dUseMemo, useRef: dUseRef } = React;

function SimulatorPanel({ tick, sustain, leftOpen = true, rightOpen = true, onToggleLeft, onToggleRight }) {
  const sustainId = sustain?.id || 'homestead.bonnie';

  const [liveState, setLiveState] = dUseState(null);
  const [liveRollup, setLiveRollup] = dUseState(null);
  const [nodes, setNodes] = dUseState({});          // id -> node, excludes root
  const [selectedId, setSelectedId] = dUseState('root');
  const [pinnedIds, setPinnedIds] = dUseState(() => new Set());
  const [draftSteps, setDraftSteps] = dUseState([]);
  const [allowedOps, setAllowedOps] = dUseState([]);
  const [running, setRunning] = dUseState(false);
  const [promoting, setPromoting] = dUseState(false);
  const [rightTab, setRightTab] = dUseState('compare'); // 'compare' | 'score'
  const nodeCounter = dUseRef(0);

  const rootNode = dUseMemo(() => ({
    id: 'root', parentId: null, isRoot: true, label: 'NOW · LIVE',
    sequence: [], addedSteps: [], result: null, hasRefusal: false,
  }), []);

  function refreshLiveState() {
    return api.get(`/devui/state?sustain_id=${sustainId}`)
      .then(d => {
        setLiveState(d?.data?.state || null);
        setLiveRollup(d?.data?.rollup || null);
      })
      .catch(() => { setLiveState(null); setLiveRollup(null); });
  }

  // New sustain selected → the whole hypothetical tree is scoped to it; start clean.
  dUseEffect(() => {
    setNodes({});
    setSelectedId('root');
    setPinnedIds(new Set());
    setDraftSteps([]);
    refreshLiveState();
    api.get(`/devui/sustain/${sustainId}/operators`)
      .then(d => setAllowedOps(d?.data?.operators || []))
      .catch(() => setAllowedOps([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sustainId]);

  function getNode(id) {
    return id === 'root' ? rootNode : nodes[id];
  }

  const childrenOf = dUseMemo(() => {
    const map = { root: [] };
    Object.values(nodes).forEach(n => {
      if (!map[n.parentId]) map[n.parentId] = [];
      map[n.parentId].push(n);
    });
    return map;
  }, [nodes]);

  const addDraftOperator = (opName) => {
    const op = allowedOps.find(o => o.name === opName);
    const params = {};
    paramNamesOf(op).forEach(n => { params[n] = ''; });
    setDraftSteps(prev => [...prev, { operator: opName, params }]);
  };

  const runBranch = () => {
    const base = getNode(selectedId);
    if (!base || draftSteps.length === 0 || running) return;
    const fullSeq = [...base.sequence, ...draftSteps];
    setRunning(true);
    api.post('/devui/simulate', { sustain_id: sustainId, proposal: fullSeq })
      .then(d => {
        const data = d?.data || {};
        nodeCounter.current += 1;
        const id = `sim-${nodeCounter.current}`;
        const lastOp = draftSteps[draftSteps.length - 1]?.operator || 'branch';
        const anyFailed = (data.steps || []).some(s => s.result?.status !== 'ok');
        setNodes(prev => ({
          ...prev,
          [id]: {
            id, parentId: selectedId,
            label: draftSteps.length > 1 ? `+${draftSteps.length} steps` : lastOp.split('.').slice(-1)[0],
            sequence: fullSeq, addedSteps: draftSteps,
            result: data, hasRefusal: anyFailed,
          },
        }));
        setSelectedId(id);
        setDraftSteps([]);
        setRunning(false);
      })
      .catch(() => { setRunning(false); window.flash?.('Simulation failed to run', 'error'); });
  };

  const promoteNode = (node) => {
    window.confirmAction?.({
      title: 'Promote this branch to reality?',
      body: `Replays ${node.sequence.length} step${node.sequence.length === 1 ? '' : 's'} for real against live state, through the same gate this sustain always uses. If live state changed since this branch was built, a step that passed here can honestly fail for real — that outcome is reported plainly.`,
      ctaLabel: 'PROMOTE',
      tone: 'amber',
      onConfirm: () => {
        setPromoting(true);
        api.post(`/devui/sustain/${sustainId}/promote-simulation`, { steps: node.sequence })
          .then(d => {
            const data = d?.data || {};
            setPromoting(false);
            refreshLiveState();
            window.flash?.(
              data.all_succeeded
                ? `Promoted · ${data.steps_attempted}/${data.steps_requested} steps applied for real`
                : `Promotion stopped · ${data.steps_attempted}/${data.steps_requested} steps applied before a real refusal`,
              data.all_succeeded ? 'ok' : 'warn',
            );
          })
          .catch(() => { setPromoting(false); window.flash?.('Promote failed', 'error'); });
      },
    });
  };

  const togglePin = (id) => {
    setPinnedIds(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  const selectedNode = getNode(selectedId);
  const branchCount = Object.keys(nodes).length;

  return (
    <div className="panel-enter" style={{ display: 'grid', gridTemplateRows: 'auto 1fr', height: '100%', gap: 14, padding: '24px 28px' }}>
      {/* Top toolbar */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
          <RailToggle open={leftOpen} onClick={onToggleLeft} side="left" label="TREE" />
          <div style={{ display: 'flex', flexDirection: 'column' }}>
            <span className="label-10">SCENARIO SIMULATOR</span>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 14, color: 'var(--text-primary)' }}>
              {sustainId} · {branchCount} hypothetical branch{branchCount === 1 ? '' : 'es'}
            </span>
          </div>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <span className="meta-10" style={{ color: 'var(--text-muted)' }}>
            never touches live state until promoted
          </span>
          <RailToggle open={rightOpen} onClick={onToggleRight} side="right" label="PANEL" />
        </div>
      </div>

      {/* Body: 3 zones (left/right collapsible) */}
      <div style={{
        display: 'grid',
        gridTemplateColumns: `${leftOpen ? '240px' : '0px'} minmax(0, 1fr) ${rightOpen ? '320px' : '0px'}`,
        gap: leftOpen || rightOpen ? 14 : 0,
        minHeight: 0,
        transition: 'grid-template-columns var(--t-mid), gap var(--t-mid)',
      }}>
        {/* Scenario tree */}
        {leftOpen && (
          <Card title="SCENARIO TREE" sub={`${branchCount} BRANCH${branchCount === 1 ? '' : 'ES'}`} padded={false} scroll>
            <div style={{ padding: '6px 0' }}>
              <ScenarioNodeReal
                node={rootNode} childrenOf={childrenOf} selectedId={selectedId} pinnedIds={pinnedIds}
                onSelect={setSelectedId} onTogglePin={togglePin} depth={0}
              />
            </div>
          </Card>
        )}

        {/* Node detail + branch builder */}
        <Card title="NODE DETAIL" sub={selectedNode?.isRoot ? 'LIVE STATE' : `HYPOTHETICAL · ${selectedNode?.label || ''}`} padded scroll>
          <NodeDetail
            node={selectedNode}
            getNode={getNode}
            liveState={liveState}
            liveRollup={liveRollup}
            allowedOps={allowedOps}
            draftSteps={draftSteps}
            onAddDraftOperator={addDraftOperator}
            onChangeDraft={setDraftSteps}
            onRunBranch={runBranch}
            running={running}
            onPromote={promoteNode}
            promoting={promoting}
          />
        </Card>

        {/* Compare / Score pipeline */}
        {rightOpen && (
          <Card
            title={rightTab === 'compare' ? 'COMPARE BRANCHES' : 'SCORE PIPELINE'}
            sub={rightTab === 'compare' ? `${pinnedIds.size} PINNED` : 'simulate.fork → run_path → score'}
            padded scroll
            actions={
              <div style={{ display: 'flex', gap: 4 }}>
                <TabBtn active={rightTab === 'compare'} onClick={() => setRightTab('compare')}>COMPARE</TabBtn>
                <TabBtn active={rightTab === 'score'} onClick={() => setRightTab('score')}>SCORE</TabBtn>
              </div>
            }
          >
            {rightTab === 'compare'
              ? <ComparePane pinnedIds={pinnedIds} getNode={getNode} liveState={liveState} onTogglePin={togglePin} />
              : <StateDiff tick={tick} sustain={sustain} selectedBranch={null} />
            }
          </Card>
        )}
      </div>
    </div>
  );
}

function TabBtn({ active, onClick, children }) {
  return (
    <button onClick={onClick} style={{
      padding: '3px 8px',
      fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.06em',
      color: active ? 'var(--amber)' : 'var(--text-muted)',
      background: active ? 'var(--amber-glow)' : 'transparent',
      border: `1px solid ${active ? 'var(--amber-border)' : 'var(--border)'}`,
      borderRadius: 'var(--radius-sm)', cursor: 'pointer',
    }}>{children}</button>
  );
}

/* ── Scenario tree node ─────────────────────────────────────────────────── */

function ScenarioNodeReal({ node, childrenOf, selectedId, pinnedIds, onSelect, onTogglePin, depth }) {
  const kids = childrenOf[node.id] || [];
  const isSelected = selectedId === node.id;
  const isPinned = pinnedIds.has(node.id);
  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center' }}>
        <button
          onClick={() => onSelect(node.id)}
          className="fade-up"
          style={{
            flex: 1, textAlign: 'left',
            display: 'flex', alignItems: 'center', gap: 6,
            padding: `7px ${12 + depth * 14}px`,
            background: isSelected ? 'var(--amber-glow)' : 'transparent',
            borderLeft: isSelected ? '2px solid var(--amber)' : '2px solid transparent',
            transition: 'background var(--t-fast)',
          }}
          onMouseEnter={e => !isSelected && (e.currentTarget.style.background = 'var(--bg-raised)')}
          onMouseLeave={e => !isSelected && (e.currentTarget.style.background = 'transparent')}
        >
          <span style={{ color: 'var(--text-dim)', fontFamily: 'var(--mono)', fontSize: 9 }}>{kids.length ? '▾' : '·'}</span>
          <span style={{
            fontFamily: 'var(--mono)', fontSize: 10, flex: 1,
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
            color: isSelected ? 'var(--amber)' : (node.isRoot ? 'var(--teal)' : 'var(--text-secondary)'),
          }}>{node.isRoot ? 'NOW · LIVE' : node.label}</span>
          {node.hasRefusal && <span title="a step in this branch was gate-refused" style={{ color: 'var(--danger)', fontSize: 10 }}>⚠</span>}
          {!node.isRoot && (
            <span style={{
              fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--amber)',
              background: 'var(--bg-base)', border: '1px solid var(--amber-border)',
              padding: '1px 4px', borderRadius: 6,
            }}>SIM</span>
          )}
        </button>
        {!node.isRoot && (
          <button
            onClick={() => onTogglePin(node.id)}
            title={isPinned ? 'unpin' : 'pin to compare'}
            style={{ width: 20, background: 'none', border: 'none', cursor: 'pointer', color: isPinned ? 'var(--amber)' : 'var(--text-dim)', fontSize: 10 }}
          >{isPinned ? '★' : '☆'}</button>
        )}
      </div>
      {kids.map(k => (
        <ScenarioNodeReal
          key={k.id} node={k} childrenOf={childrenOf} selectedId={selectedId} pinnedIds={pinnedIds}
          onSelect={onSelect} onTogglePin={onTogglePin} depth={depth + 1}
        />
      ))}
    </div>
  );
}

/* ── Node detail pane: sequence DAG, refusals, rollup, diff, branch builder ── */

function NodeDetail({
  node, getNode, liveState, liveRollup, allowedOps,
  draftSteps, onAddDraftOperator, onChangeDraft, onRunBranch, running,
  onPromote, promoting,
}) {
  if (!node) return <span className="meta-10" style={{ color: 'var(--text-dim)' }}>no node selected</span>;

  const isRoot = node.isRoot;
  const finalState = isRoot ? liveState : node.result?.final_state;
  const rollup = isRoot ? liveRollup : node.result?.final_parent_rollup;
  const parentNode = isRoot ? null : getNode(node.parentId);
  const parentState = isRoot ? null : (parentNode?.isRoot ? liveState : parentNode?.result?.final_state);
  const diffRows = (!isRoot && parentState && finalState) ? diffStates(parentState, finalState) : [];
  const failedSteps = !isRoot ? (node.result?.steps || []).filter(s => s.result?.status !== 'ok') : [];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* Honesty header — unmistakable live vs hypothetical */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 8, padding: '8px 10px',
        borderRadius: 'var(--radius-sm)',
        background: isRoot ? 'var(--teal-glow, rgba(45,212,191,0.08))' : 'var(--amber-glow)',
        border: `1px solid ${isRoot ? 'var(--teal)' : 'var(--amber-border)'}`,
      }}>
        <span style={{
          fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.08em', fontWeight: 600,
          color: isRoot ? 'var(--teal)' : 'var(--amber)',
        }}>{isRoot ? '● LIVE' : '◆ HYPOTHETICAL — SIMULATED'}</span>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>
          {isRoot ? 'this is the sustain\'s real, current state' : 'not applied to live state · nothing in this branch was written to the fold'}
        </span>
      </div>

      {/* Sequence DAG */}
      {!isRoot && node.sequence.length > 0 && (
        <div>
          <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>
            SEQUENCE FROM LIVE · {node.sequence.length} STEP{node.sequence.length !== 1 ? 'S' : ''}
          </span>
          <ProposalDag proposal={node.sequence} stepResults={(node.result?.steps || []).map(s => ({ status: s.result?.status }))} />
        </div>
      )}

      {/* Refusals */}
      {failedSteps.length > 0 && (
        <div>
          <span className="label-10" style={{ display: 'block', marginBottom: 6, color: 'var(--danger)' }}>GATE REFUSAL IN BRANCH</span>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {failedSteps.map((s, i) => (
              <div key={i} style={{ padding: '6px 8px', border: '1px solid var(--danger)', borderRadius: 'var(--radius-sm)' }}>
                <div style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)' }}>{s.operator}</div>
                <div style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--danger)', marginTop: 2 }}>{s.result?.reason || 'refused'}</div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Rollup */}
      {rollup && Object.keys(rollup.aggregates || {}).length > 0 && (
        <div>
          <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>
            {isRoot ? 'PARENT ROLL-UP (LIVE)' : 'PARENT ROLL-UP (HYPOTHETICAL)'}
          </span>
          <RollupMini rollup={rollup} />
        </div>
      )}

      {/* State diff vs parent */}
      {!isRoot && (
        <div>
          <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>
            STATE CHANGE VS {parentNode?.isRoot ? 'LIVE' : 'PARENT BRANCH'}
          </span>
          {diffRows.length === 0
            ? <span className="meta-10" style={{ color: 'var(--text-dim)' }}>no state change</span>
            : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 3 }}>
                {diffRows.map(r => (
                  <div key={r.path} style={{ display: 'flex', justifyContent: 'space-between', gap: 8, fontFamily: 'var(--mono)', fontSize: 10 }}>
                    <span style={{ color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{r.path}</span>
                    <span style={{ color: 'var(--text-secondary)', flexShrink: 0 }}>
                      {String(r.before ?? '—')} → <span style={{ color: 'var(--amber)' }}>{String(r.after ?? '—')}</span>
                    </span>
                  </div>
                ))}
              </div>
            )}
        </div>
      )}

      {/* Promote */}
      {!isRoot && (
        <button
          onClick={() => onPromote(node)}
          disabled={promoting}
          style={{
            padding: '8px 0',
            fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.07em',
            color: promoting ? 'var(--text-dim)' : 'var(--bg-base)',
            background: promoting ? 'var(--bg-overlay)' : 'var(--teal, #2dd4bf)',
            border: `1px solid ${promoting ? 'var(--border)' : 'var(--teal, #2dd4bf)'}`,
            borderRadius: 'var(--radius-sm)', cursor: promoting ? 'not-allowed' : 'pointer',
          }}
        >{promoting ? '⟳ PROMOTING…' : '⬆ PROMOTE THIS BRANCH TO REALITY'}</button>
      )}

      {/* Branch builder */}
      <div style={{ borderTop: '1px solid var(--border)', paddingTop: 14 }}>
        <span className="label-10" style={{ display: 'block', marginBottom: 4 }}>
          BRANCH FROM {isRoot ? 'LIVE' : 'THIS NODE'} · ADD STEPS
        </span>
        {allowedOps.length > 0 ? (
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, marginBottom: 8 }}>
            {allowedOps.map(op => (
              <button
                key={op.name}
                onClick={() => onAddDraftOperator(op.name)}
                title={op.description}
                style={{
                  padding: '3px 8px',
                  fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.06em',
                  color: 'var(--text-secondary)', background: 'var(--bg-base)',
                  border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)',
                  cursor: 'pointer', transition: 'all var(--t-fast)',
                }}
                onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
                onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border-mid)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
              >{op.name}</button>
            ))}
          </div>
        ) : (
          <span className="meta-10" style={{ color: 'var(--text-dim)', display: 'block', marginBottom: 8 }}>no operators available for this sustain</span>
        )}

        <OperatorSequenceCards proposal={draftSteps} onChange={onChangeDraft} />

        <button
          onClick={onRunBranch}
          disabled={running || draftSteps.length === 0}
          style={{
            width: '100%', marginTop: 10, padding: '8px 0',
            fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.07em',
            color: (running || draftSteps.length === 0) ? 'var(--text-dim)' : 'var(--bg-base)',
            background: (running || draftSteps.length === 0) ? 'var(--bg-overlay)' : 'var(--amber)',
            border: `1px solid ${(running || draftSteps.length === 0) ? 'var(--border)' : 'var(--amber)'}`,
            borderRadius: 'var(--radius-sm)',
            cursor: (running || draftSteps.length === 0) ? 'not-allowed' : 'pointer',
          }}
        >{running ? '⟳ SIMULATING…' : '▶ RUN BRANCH (SANDBOXED)'}</button>
      </div>
    </div>
  );
}

function RollupMini({ rollup }) {
  const aggs = Object.entries(rollup.aggregates || {});
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      {aggs.map(([id, agg]) => {
        const total = (agg.included?.length || 0) + (agg.excluded?.length || 0);
        return (
          <div key={id} style={{ display: 'flex', justifyContent: 'space-between', fontFamily: 'var(--mono)', fontSize: 10 }}>
            <span style={{ color: 'var(--text-muted)' }}>{id.replace(/_/g, ' ')} ({agg.op})</span>
            <span style={{ color: 'var(--text-primary)' }}>
              {typeof agg.value === 'number' ? agg.value.toLocaleString(undefined, { maximumFractionDigits: 2 }) : '—'}
              {' · '}{agg.included?.length || 0}/{total} included
            </span>
          </div>
        );
      })}
    </div>
  );
}

/* ── Compare pane — pinned branches side by side, diffed against live ────── */

function ComparePane({ pinnedIds, getNode, liveState, onTogglePin }) {
  const ids = [...pinnedIds];
  if (ids.length === 0) {
    return <span className="meta-10" style={{ color: 'var(--text-dim)' }}>pin branches (★) in the tree to compare their outcomes</span>;
  }
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
      {ids.map(id => {
        const node = getNode(id);
        if (!node) return null;
        const rows = diffStates(liveState, node.result?.final_state);
        const failed = (node.result?.steps || []).filter(s => s.result?.status !== 'ok');
        return (
          <div key={id} style={{ border: '1px solid var(--border-mid)', borderRadius: 'var(--radius-sm)', padding: 10 }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 6 }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--amber)' }}>{node.label}</span>
              <button onClick={() => onTogglePin(id)} style={{ background: 'none', border: 'none', color: 'var(--amber)', cursor: 'pointer', fontSize: 10 }}>★ unpin</button>
            </div>
            {failed.length > 0 && (
              <div className="meta-10" style={{ color: 'var(--danger)', marginBottom: 6 }}>⚠ {failed.length} step{failed.length !== 1 ? 's' : ''} refused</div>
            )}
            {rows.length === 0 ? (
              <span className="meta-10" style={{ color: 'var(--text-dim)' }}>no change vs live</span>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 3 }}>
                {rows.map(r => (
                  <div key={r.path} style={{ display: 'flex', justifyContent: 'space-between', gap: 8, fontFamily: 'var(--mono)', fontSize: 9 }}>
                    <span style={{ color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{r.path}</span>
                    <span style={{ color: 'var(--text-secondary)', flexShrink: 0 }}>{String(r.before ?? '—')} → {String(r.after ?? '—')}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/* ── Flatten/diff helpers for state comparison ────────────────────────────── */

function flattenState(obj, prefix = '') {
  const out = {};
  if (obj === null || obj === undefined) return out;
  if (typeof obj !== 'object' || Array.isArray(obj)) {
    out[prefix || '(root)'] = Array.isArray(obj) ? JSON.stringify(obj) : obj;
    return out;
  }
  const keys = Object.keys(obj);
  if (keys.length === 0) return out;
  keys.forEach(k => {
    const path = prefix ? `${prefix}.${k}` : k;
    const v = obj[k];
    if (v !== null && typeof v === 'object' && !Array.isArray(v)) {
      Object.assign(out, flattenState(v, path));
    } else {
      out[path] = Array.isArray(v) ? JSON.stringify(v) : v;
    }
  });
  return out;
}

function diffStates(before, after) {
  const a = flattenState(before || {});
  const b = flattenState(after || {});
  const paths = new Set([...Object.keys(a), ...Object.keys(b)]);
  const rows = [];
  paths.forEach(p => {
    if (a[p] !== b[p]) rows.push({ path: p, before: a[p], after: b[p] });
  });
  rows.sort((x, y) => x.path.localeCompare(y.path));
  return rows;
}

/* ── ProposalDag — renders proposal steps as a sequential DAG ───────────── */

function ProposalDag({ proposal, stepResults }) {
  if (!proposal || proposal.length === 0) return null;
  const NODE_W = 130, NODE_H = 46, GAP = 16;
  const CANVAS_W = proposal.length * (NODE_W + GAP) + GAP;
  const CANVAS_H = NODE_H + 20;

  const nodes = proposal.map((step, i) => ({
    id: `step${i}`,
    x: GAP + i * (NODE_W + GAP),
    y: 10,
    type: 'operator',
    label: (step.operator || '?').split('.').slice(-1)[0],
  }));

  const edges = nodes.slice(0, -1).map((n, i) => [n.id, nodes[i + 1].id]);

  const completedIds = new Set(
    (stepResults || []).map((s, i) => s.status === 'ok' ? `step${i}` : null).filter(Boolean)
  );

  return (
    <div style={{ overflowX: 'auto', overflowY: 'visible' }}>
      <div style={{ position: 'relative', width: CANVAS_W, height: CANVAS_H }}>
        <DagEdges
          nodes={nodes}
          edges={edges}
          activePath={[...completedIds]}
          width={CANVAS_W}
          height={CANVAS_H}
        />
        {nodes.map(node => (
          <DagNode key={node.id} node={node} active={completedIds.has(node.id)} pulse={false} />
        ))}
      </div>
    </div>
  );
}


/* ── StateDiff — uses simulate.fork → run_path → score pipeline ─────────── */

/* ── Param-name extraction (tolerant of list-of-strings, list-of-objects, dict) ── */
function paramNamesOf(op) {
  const p = op?.params;
  if (Array.isArray(p)) return p.map(x => (typeof x === 'string' ? x : x?.name)).filter(Boolean);
  if (p && typeof p === 'object') return Object.keys(p);
  return [];
}
function coerceVal(v) {
  if (v === '' || v == null) return '';
  const n = Number(v);
  return Number.isNaN(n) || v.trim?.() === '' ? v : n;
}

/* ── OperatorSequenceCards — draggable cards replacing the JSON sequence editor ── */
function OperatorSequenceCards({ proposal, onChange }) {
  const dragIdx = dUseRef(null);

  const reorder = (to) => {
    const from = dragIdx.current;
    if (from == null || from === to) return;
    const next = [...proposal];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    dragIdx.current = null;
    onChange(next);
  };
  const removeStep = (i) => onChange(proposal.filter((_, j) => j !== i));
  const setParamKey = (i, oldKey, newKey) => {
    const entries = Object.entries(proposal[i].params || {}).map(([k, v]) => (k === oldKey ? [newKey, v] : [k, v]));
    onChange(proposal.map((s, j) => (j === i ? { ...s, params: Object.fromEntries(entries) } : s)));
  };
  const setParamVal = (i, key, val) => {
    onChange(proposal.map((s, j) => (j === i ? { ...s, params: { ...s.params, [key]: coerceVal(val) } } : s)));
  };
  const removeParam = (i, key) => {
    const { [key]: _drop, ...rest } = proposal[i].params || {};
    onChange(proposal.map((s, j) => (j === i ? { ...s, params: rest } : s)));
  };
  const addParam = (i) => {
    const params = proposal[i].params || {};
    if ('' in params) return;
    onChange(proposal.map((s, j) => (j === i ? { ...s, params: { ...params, '': '' } } : s)));
  };

  if (!proposal.length) {
    return <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>no steps · add operators above</span>;
  }

  const inp = {
    background: 'var(--bg-surface)', border: '1px solid var(--border-mid)', borderRadius: 3,
    padding: '3px 6px', fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)', outline: 'none',
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {proposal.map((step, i) => (
        <div key={i}
          draggable
          onDragStart={() => { dragIdx.current = i; }}
          onDragOver={e => e.preventDefault()}
          onDrop={() => reorder(i)}
          style={{
            background: 'var(--bg-base)', border: '1px solid var(--border-mid)',
            borderRadius: 'var(--radius-sm)', padding: '8px 10px',
            display: 'flex', flexDirection: 'column', gap: 6,
          }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span title="drag to reorder" style={{ cursor: 'grab', color: 'var(--text-dim)', fontFamily: 'var(--mono)', fontSize: 12 }}>⠿</span>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)' }}>{i + 1}</span>
            <span style={{ flex: 1, fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--amber)' }}>{step.operator}</span>
            <button onClick={() => removeStep(i)} title="remove step"
              style={{ background: 'none', border: 'none', color: 'var(--danger)', cursor: 'pointer', fontFamily: 'var(--mono)', fontSize: 12 }}>×</button>
          </div>
          {Object.entries(step.params || {}).map(([k, v], pi) => (
            <div key={pi} style={{ display: 'flex', alignItems: 'center', gap: 6, paddingLeft: 20 }}>
              <input value={k} placeholder="param" onChange={e => setParamKey(i, k, e.target.value)} style={{ ...inp, width: 110 }} />
              <span style={{ color: 'var(--text-dim)' }}>=</span>
              <input value={String(v)} placeholder="value" onChange={e => setParamVal(i, k, e.target.value)} style={{ ...inp, flex: 1 }} />
              <button onClick={() => removeParam(i, k)} style={{ background: 'none', border: 'none', color: 'var(--text-dim)', cursor: 'pointer', fontFamily: 'var(--mono)', fontSize: 11 }}>×</button>
            </div>
          ))}
          <button onClick={() => addParam(i)} style={{ alignSelf: 'flex-start', marginLeft: 20, background: 'none', border: 'none', color: 'var(--text-muted)', cursor: 'pointer', fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.06em' }}>+ param</button>
        </div>
      ))}
    </div>
  );
}

function StateDiff({ tick, sustain, selectedBranch }) {
  const [result, setResult] = dUseState(null);
  const [loading, setLoading] = dUseState(false);
  const [ran, setRan] = dUseState(false);
  const [openStep, setOpenStep] = dUseState(null);
  const [goalMetric, setGoalMetric] = dUseState('minimize_budget_deviation');
  const [proposalText, setProposalText] = dUseState('[]');
  const [proposalError, setProposalError] = dUseState(null);
  const [allowedOps, setAllowedOps] = dUseState([]);

  const sustainId = sustain?.id || 'homestead.bonnie';

  dUseEffect(() => {
    api.get(`/devui/sustain/${sustainId}/operators`)
      .then(d => setAllowedOps(d?.data?.operators || []))
      .catch(() => setAllowedOps([]));
  }, [sustainId]);

  const addOperatorStep = (opName) => {
    const op = allowedOps.find(o => o.name === opName);
    const params = {};
    paramNamesOf(op).forEach(n => { params[n] = ''; });   // pre-fill known param fields
    let base = [];
    try { const c = JSON.parse(proposalText); base = Array.isArray(c) ? c : []; } catch {}
    setProposalText(JSON.stringify([...base, { operator: opName, params }], null, 2));
    setProposalError(null);
  };

  let parsedProposal = [];
  try { parsedProposal = JSON.parse(proposalText); } catch {}
  if (!Array.isArray(parsedProposal)) parsedProposal = [];

  const runPipeline = () => {
    let proposal;
    try {
      proposal = JSON.parse(proposalText);
      setProposalError(null);
    } catch {
      setProposalError('Invalid JSON');
      return;
    }
    setLoading(true);
    api.post('/devui/simulate-pipeline', {
      sustain_id: sustainId,
      proposal,
      goal_metric: goalMetric,
    })
      .then(d => {
        setResult(d?.data || null);
        setLoading(false);
        setRan(true);
        setOpenStep(null);
      })
      .catch(() => { setLoading(false); setRan(true); });
  };

  const score = result?.score ?? null;
  const interpretation = result?.interpretation ?? null;
  const steps = result?.steps ?? [];
  const stepsRun = result?.steps_run ?? 0;
  const stepsOk = result?.steps_succeeded ?? 0;
  const scoreColor = score === null ? 'var(--text-dim)'
    : score >= 0.8 ? 'var(--teal)'
    : score >= 0.5 ? 'var(--amber)'
    : 'var(--danger)';

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>

      {/* Goal metric selector */}
      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 4 }}>GOAL METRIC</span>
        <select
          value={goalMetric}
          onChange={e => setGoalMetric(e.target.value)}
          style={{
            width: '100%', background: 'var(--bg-base)', border: '1px solid var(--border-mid)',
            borderRadius: 'var(--radius-sm)', padding: '6px 8px',
            fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)', outline: 'none',
          }}
        >
          <option value="minimize_budget_deviation">minimize_budget_deviation</option>
          <option value="maximize_savings_rate">maximize_savings_rate</option>
          <option value="maximize_liquid_balance">maximize_liquid_balance</option>
        </select>
      </div>

      {/* Operator selector */}
      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 4 }}>ADD STEP · ALLOWED OPERATORS</span>
        {allowedOps.length > 0 ? (
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
            {allowedOps.map(op => (
              <button
                key={op.name}
                onClick={() => addOperatorStep(op.name)}
                title={op.description}
                style={{
                  padding: '3px 8px',
                  fontFamily: 'var(--mono)', fontSize: 9, letterSpacing: '0.06em',
                  color: 'var(--text-secondary)',
                  background: 'var(--bg-base)',
                  border: '1px solid var(--border-mid)',
                  borderRadius: 'var(--radius-sm)',
                  cursor: 'pointer', transition: 'all var(--t-fast)',
                }}
                onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--amber-border)'; e.currentTarget.style.color = 'var(--amber)'; }}
                onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border-mid)'; e.currentTarget.style.color = 'var(--text-secondary)'; }}
              >
                {op.name}
              </button>
            ))}
          </div>
        ) : (
          <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)' }}>
            no operators available · select a sustain to load options
          </span>
        )}
      </div>

      {/* Proposal editor */}
      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 4 }}>PROPOSAL · OPERATOR SEQUENCE</span>
        <OperatorSequenceCards
          proposal={parsedProposal}
          onChange={next => { setProposalText(JSON.stringify(next, null, 2)); setProposalError(null); }}
        />
        {proposalError && <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--danger)' }}>{proposalError}</span>}
      </div>

      {/* Run button */}
      <button
        onClick={runPipeline}
        disabled={loading}
        style={{
          width: '100%', padding: '8px 0',
          fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.07em',
          color: loading ? 'var(--text-dim)' : 'var(--bg-base)',
          background: loading ? 'var(--bg-overlay)' : 'var(--amber)',
          border: `1px solid ${loading ? 'var(--border)' : 'var(--amber)'}`,
          borderRadius: 'var(--radius-sm)',
          cursor: loading ? 'not-allowed' : 'pointer',
          transition: 'all var(--t-fast)',
        }}
      >
        {loading ? '⟳ SIMULATING…' : ran ? '↺ RE-SIMULATE' : '▶ RUN PIPELINE'}
      </button>

      {/* Proposal DAG — shows when there are steps in the textarea */}
      {parsedProposal.length > 0 && (
        <div>
          <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>PROPOSAL DAG · {parsedProposal.length} STEP{parsedProposal.length !== 1 ? 'S' : ''}</span>
          <ProposalDag proposal={parsedProposal} stepResults={result?.steps || []} />
        </div>
      )}

      {/* Score output */}
      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>
          SCORE{selectedBranch ? ` · BRANCH ${selectedBranch}` : ''}
        </span>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 36, fontWeight: 500, color: scoreColor, lineHeight: 1 }}>
            {score !== null ? score.toFixed(2) : '—'}
          </span>
          {score !== null && <span className="meta-10">/ 1.00</span>}
          {interpretation && (
            <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: scoreColor, letterSpacing: '0.08em' }}>
              {interpretation.toUpperCase()}
            </span>
          )}
        </div>
        {result && (
          <span className="meta-10" style={{ color: 'var(--text-muted)', marginTop: 4, display: 'block' }}>
            {stepsOk}/{stepsRun} steps succeeded · fork {result?.fork_id?.slice(0, 8) ?? '—'}
          </span>
        )}
        {!result && <span className="meta-10" style={{ color: 'var(--text-dim)' }}>no simulation run · add steps and click run pipeline</span>}
      </div>

      {/* Step accordion */}
      {steps.length > 0 && (
        <div>
          <span className="label-10" style={{ display: 'block', marginBottom: 6 }}>
            STEP DIFF · {stepsRun} STEP{stepsRun !== 1 ? 'S' : ''}
          </span>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
            {steps.map((step, i) => {
              const isOpen = openStep === i;
              const ok = step.status === 'ok';
              return (
                <div key={i} style={{
                  border: `1px solid ${ok ? 'var(--border)' : 'var(--danger)'}`,
                  borderRadius: 'var(--radius-sm)',
                  overflow: 'hidden',
                }}>
                  {/* Header row */}
                  <button onClick={() => setOpenStep(isOpen ? null : i)} style={{
                    width: '100%', textAlign: 'left',
                    display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                    padding: '7px 10px',
                    background: isOpen ? 'var(--bg-raised)' : 'transparent',
                    transition: 'background var(--t-fast)',
                  }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                      <span style={{ color: ok ? 'var(--teal)' : 'var(--danger)', fontSize: 11 }}>{ok ? '✓' : '✗'}</span>
                      <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)' }}>{step.operator}</span>
                    </div>
                    <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>{isOpen ? '▲' : '▼'}</span>
                  </button>
                  {/* Accordion body */}
                  {isOpen && (
                    <div style={{ padding: '8px 10px', background: 'var(--bg-base)', borderTop: '1px solid var(--border)' }}>
                      {step.reason && (
                        <div style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--danger)', marginBottom: 6 }}>
                          {step.reason}
                        </div>
                      )}
                      {Object.keys(step.params || {}).length > 0 && (
                        <div style={{ marginBottom: 6 }}>
                          <span className="meta-10" style={{ color: 'var(--text-muted)', display: 'block', marginBottom: 3 }}>PARAMS</span>
                          {Object.entries(step.params).map(([k, v]) => (
                            <div key={k} style={{ display: 'flex', justifyContent: 'space-between', padding: '2px 0' }}>
                              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)' }}>{k}</span>
                              <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-secondary)' }}>{String(v)}</span>
                            </div>
                          ))}
                        </div>
                      )}
                      {(step.data && Object.keys(step.data).length > 0) && (
                        <div>
                          <span className="meta-10" style={{ color: 'var(--text-muted)', display: 'block', marginBottom: 3 }}>RESULT DATA</span>
                          <pre style={{
                            fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-secondary)',
                            background: 'var(--bg-surface)', borderRadius: 2, padding: '4px 6px',
                            overflow: 'auto', maxHeight: 80, margin: 0,
                          }}>{JSON.stringify(step.data, null, 2)}</pre>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

Object.assign(window, { SimulatorPanel, RailToggle });

function RailToggle({ open, onClick, side, label }) {
  return (
    <button onClick={onClick} title={open ? `Hide ${label}` : `Show ${label}`} style={{
      display: 'inline-flex', alignItems: 'center', gap: 5,
      padding: '4px 8px',
      fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500, letterSpacing: '0.08em',
      color: open ? 'var(--text-primary)' : 'var(--text-muted)',
      background: open ? 'var(--bg-raised)' : 'transparent',
      border: '1px solid ' + (open ? 'var(--border-mid)' : 'var(--border)'),
      borderRadius: 'var(--radius-sm)',
      transition: 'all var(--t-fast)',
    }}
    onMouseEnter={e => e.currentTarget.style.borderColor = 'var(--amber-border)'}
    onMouseLeave={e => e.currentTarget.style.borderColor = open ? 'var(--border-mid)' : 'var(--border)'}
    >
      <svg width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round">
        {side === 'left'
          ? (open ? <path d="M5.5 2L3 4.5L5.5 7" /> : <path d="M3 2L5.5 4.5L3 7" />)
          : (open ? <path d="M3 2L5.5 4.5L3 7" /> : <path d="M5.5 2L3 4.5L5.5 7" />)}
      </svg>
      {label}
    </button>
  );
}
