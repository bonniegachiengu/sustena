import React from 'react';
import { api } from '../../lib/api.js';
/* Simulator panel — forked execution with scenario tree, animated DAG, state diff. */

const { useState: dUseState, useEffect: dUseEffect, useMemo: dUseMemo, useRef: dUseRef } = React;

function SimulatorPanel({ tick, sustain, leftOpen = true, rightOpen = true, onToggleLeft, onToggleRight }) {
  const [selectedBranch, setSelectedBranch] = dUseState('A2');
  const [playing, setPlaying] = dUseState(true);
  const [speed, setSpeed] = dUseState(10);
  const [activeNodes, setActiveNodes] = dUseState(new Set(['state0', 'gate0']));
  const [completedNodes, setCompletedNodes] = dUseState(new Set());

  // Step 4: Orchie expansion panel state
  const [orchieOpen, setOrchieOpen] = dUseState(false);
  const [selectedOrchieNode, setSelectedOrchieNode] = dUseState(null);

  // Step 6: Operator code editor state
  const [codeEditorOpen, setCodeEditorOpen] = dUseState(false);
  const [selectedOperator, setSelectedOperator] = dUseState(null);

  const handleOrchieExpand = (node) => {
    setSelectedOrchieNode(node);
    setOrchieOpen(true);
    setCodeEditorOpen(false); // close code editor if open
  };

  const handleOperatorClick = (node) => {
    setSelectedOperator(node);
    setCodeEditorOpen(true);
    setOrchieOpen(false); // close orchie panel if open
  };

  // Step through nodes when playing
  dUseEffect(() => {
    if (!playing) return;
    const t = setInterval(() => {
      setActiveNodes(prev => {
        const next = new Set(prev);
        const seq = SIM_NODES.map(n => n.id);
        // Find frontier — first un-activated node
        const idx = seq.findIndex(id => !next.has(id) && !completedNodes.has(id));
        if (idx === -1) {
          // Reset
          setCompletedNodes(new Set());
          return new Set(['state0', 'gate0']);
        }
        // Promote one active → completed, activate next
        const oldest = [...next][0];
        if (oldest && next.size >= 3) {
          setCompletedNodes(c => new Set([...c, oldest]));
          next.delete(oldest);
        }
        next.add(seq[idx]);
        return next;
      });
    }, 1400 / speed * 10);
    return () => clearInterval(t);
  }, [playing, speed, completedNodes]);

  const stepOnce = () => {
    setActiveNodes(prev => {
      const next = new Set(prev);
      const seq = SIM_NODES.map(n => n.id);
      const idx = seq.findIndex(id => !next.has(id) && !completedNodes.has(id));
      if (idx === -1) return prev;
      next.add(seq[idx]);
      return next;
    });
  };

  // Active edges = those connecting any active/completed node to active node
  const activePath = dUseMemo(() => [...activeNodes, ...completedNodes], [activeNodes, completedNodes]);

  return (
    <div className="panel-enter" style={{ display: 'grid', gridTemplateRows: 'auto 1fr', height: '100%', gap: 14, padding: '24px 28px' }}>
      {/* Top toolbar */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
          <RailToggle open={leftOpen} onClick={onToggleLeft} side="left" label="TREE" />
          <div style={{ display: 'flex', flexDirection: 'column' }}>
            <span className="label-10">SIMULATION RUN</span>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 14, color: 'var(--text-primary)' }}>SIM-{(2104 + tick % 100).toString().padStart(4, '0')} · monte_carlo · N=100</span>
          </div>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <div style={{ display: 'flex', gap: 4, padding: 2, background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)' }}>
            <PlayBtn onClick={() => setPlaying(true)}  active={playing} icon="play" />
            <PlayBtn onClick={() => setPlaying(false)} active={!playing} icon="pause" />
            <PlayBtn onClick={stepOnce} icon="step" />
          </div>
          <div style={{ display: 'flex', gap: 6 }}>
            {[1, 10, 100, 365].map(s => (
              <TBtn key={s} active={speed === s} onClick={() => setSpeed(s)}>{s}×</TBtn>
            ))}
          </div>
          <span className="meta-10" style={{ color: 'var(--text-muted)' }}>·</span>
          <PBtn variant="ghost" onClick={() => window.confirmAction?.({
            title: 'Fork new simulation branch?',
            body: 'Snapshot current branch state and create a new exploration branch.',
            ctaLabel: 'FORK',
            tone: 'amber',
            onConfirm: () => window.flash?.('Forked · branch E created · N=20', 'ok'),
          })}><Icon name="plus" size={11} /> FORK</PBtn>
          <RailToggle open={rightOpen} onClick={onToggleRight} side="right" label="DIFF" />
        </div>
      </div>

      {/* Body: 3 zones (left/right collapsible) */}
      <div style={{
        display: 'grid',
        gridTemplateColumns: `${leftOpen ? '240px' : '0px'} minmax(0, 1fr) ${rightOpen ? '280px' : '0px'}`,
        gap: leftOpen || rightOpen ? 14 : 0,
        minHeight: 0,
        transition: 'grid-template-columns var(--t-mid), gap var(--t-mid)',
      }}>
        {/* Scenario tree */}
        {leftOpen && (
          <Card title="SCENARIO TREE" sub={SCENARIO_TREE.length ? `${SCENARIO_TREE[0]?.children?.length ?? 0} BRANCHES` : 'NO DATA'} padded={false} scroll>
            {SCENARIO_TREE.length === 0
              ? <div style={{ padding: 16, fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-dim)', textAlign: 'center' }}>No data — seed via SEED panel</div>
              : <ScenarioTree tree={SCENARIO_TREE} selected={selectedBranch} onSelect={setSelectedBranch} />
            }
          </Card>
        )}

        {/* Execution canvas */}
        <Card title="EXECUTION CANVAS" sub={`BRANCH ${selectedBranch} · DAG`} padded={false}
          actions={
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <Legend swatch="var(--node-state)" label="STATE" small />
              <Legend swatch="var(--node-operator)" label="OP" small />
              <Legend swatch="var(--node-constraint)" label="CSTR" small />
              <Legend swatch="var(--node-operative)" label="AGENT" small />
              <Legend swatch="var(--node-event)" label="EVT" small />
            </div>
          }
        >
          <SimulatorCanvas
            activeNodes={activeNodes}
            completedNodes={completedNodes}
            activePath={activePath}
            onOrchieExpand={handleOrchieExpand}
            onOperatorClick={handleOperatorClick}
            orchieOpen={orchieOpen}
            codeEditorOpen={codeEditorOpen}
            selectedOperator={selectedOperator}
            onCloseOrchie={() => setOrchieOpen(false)}
            onCloseCodeEditor={() => setCodeEditorOpen(false)}
            tick={tick}
          />
        </Card>

        {/* State Diff */}
        {rightOpen && (
          <Card title="STATE DIFF" sub={`BRANCH ${selectedBranch}`} padded scroll>
            <StateDiff tick={tick} sustain={sustain} selectedBranch={selectedBranch} />
          </Card>
        )}
      </div>
    </div>
  );
}

function Legend({ swatch, label, small }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
      <span style={{ width: small ? 7 : 8, height: small ? 7 : 8, background: swatch, borderRadius: 1 }} />
      <span className="label-10" style={{ fontSize: 9 }}>{label}</span>
    </div>
  );
}

function PlayBtn({ icon, active, onClick }) {
  return (
    <button onClick={onClick} style={{
      width: 28, height: 24,
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      borderRadius: 'var(--radius-sm)',
      background: active ? 'var(--amber-glow)' : 'transparent',
      color: active ? 'var(--amber)' : 'var(--text-muted)',
      transition: 'all var(--t-fast)',
    }}>
      {icon === 'play' && <svg width="10" height="10" viewBox="0 0 10 10"><path d="M2 1 L9 5 L2 9 Z" fill="currentColor" /></svg>}
      {icon === 'pause' && <svg width="10" height="10" viewBox="0 0 10 10"><rect x="2" y="1" width="2.5" height="8" fill="currentColor" /><rect x="5.5" y="1" width="2.5" height="8" fill="currentColor" /></svg>}
      {icon === 'step' && <svg width="10" height="10" viewBox="0 0 10 10"><path d="M2 1 L7 5 L2 9 Z" fill="currentColor" /><rect x="7.5" y="1" width="1.5" height="8" fill="currentColor" /></svg>}
    </button>
  );
}

function ScenarioTree({ tree, selected, onSelect, depth = 0 }) {
  return (
    <div style={{ padding: '6px 0' }}>
      {tree.map(n => (
        <ScenarioNode key={n.id} n={n} selected={selected} onSelect={onSelect} depth={depth} />
      ))}
    </div>
  );
}

function ScenarioNode({ n, selected, onSelect, depth }) {
  const isLeaf = !n.children;
  const isActive = selected === n.id;
  const isHot = n.hot;
  return (
    <div>
      <button
        onClick={() => onSelect(n.id)}
        className="fade-up"
        style={{
          width: '100%', textAlign: 'left',
          display: 'flex', alignItems: 'center', gap: 6,
          padding: `7px ${12 + depth * 14}px 7px ${12 + depth * 14}px`,
          background: isActive ? 'var(--amber-glow)' : 'transparent',
          borderLeft: isActive ? '2px solid var(--amber)' : '2px solid transparent',
          transition: 'background var(--t-fast)',
        }}
        onMouseEnter={e => !isActive && (e.currentTarget.style.background = 'var(--bg-raised)')}
        onMouseLeave={e => !isActive && (e.currentTarget.style.background = 'transparent')}
      >
        <span style={{ color: 'var(--text-dim)', fontFamily: 'var(--mono)', fontSize: 9 }}>
          {n.children ? '▾' : '·'}
        </span>
        <span style={{
          fontFamily: 'var(--mono)', fontSize: 10,
          color: isActive ? 'var(--amber)' : (isHot ? 'var(--text-primary)' : 'var(--text-secondary)'),
          flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
        }}>{n.label}</span>
        {n.score != null && (
          <span style={{
            fontFamily: 'var(--mono)', fontSize: 9,
            color: n.score >= 0.8 ? 'var(--teal)' : n.score >= 0.6 ? 'var(--amber)' : 'var(--text-muted)',
            background: 'var(--bg-base)',
            padding: '1px 5px', borderRadius: 8,
          }}>{n.score.toFixed(2)}</span>
        )}
      </button>
      {n.children && <ScenarioTree tree={n.children} selected={selected} onSelect={onSelect} depth={depth + 1} />}
    </div>
  );
}

/* The animated DAG canvas */
function SimulatorCanvas({
  activeNodes, completedNodes, activePath,
  onOrchieExpand, onOperatorClick,
  orchieOpen, codeEditorOpen,
  selectedOperator, onCloseOrchie, onCloseCodeEditor,
  tick = 0,
}) {
  const W = 920, H = 400;
  const containerRef = dUseRef(null);
  return (
    <div ref={containerRef} style={{
      position: 'relative', width: '100%', height: '100%',
      overflow: 'hidden',
      background: `
        radial-gradient(circle at 50% 50%, transparent 60%, rgba(0,0,0,0.4)),
        linear-gradient(0deg, var(--bg-base) 0%, var(--bg-surface) 100%)
      `,
    }}>
      {/* Grid */}
      <svg width="100%" height="100%" style={{ position: 'absolute', inset: 0, pointerEvents: 'none' }}>
        <defs>
          <pattern id="grid" width="40" height="40" patternUnits="userSpaceOnUse">
            <path d="M 40 0 L 0 0 0 40" fill="none" stroke="var(--border)" strokeWidth="0.5" opacity="0.4" />
          </pattern>
        </defs>
        <rect width="100%" height="100%" fill="url(#grid)" />
      </svg>

      <div style={{ position: 'absolute', inset: 0, transform: 'translate(0, 0)' }}>
        <div style={{ position: 'relative', width: W, height: H }}>
          {/* Edges (SVG) */}
          <DagEdges nodes={SIM_NODES} edges={SIM_EDGES} activePath={activePath} width={W} height={H} />
          {/* Nodes */}
          {SIM_NODES.map(n => (
            <DagNode
              key={n.id}
              node={n}
              active={activeNodes.has(n.id) || completedNodes.has(n.id)}
              pulse={activeNodes.has(n.id) && !completedNodes.has(n.id)}
              onOrchieExpand={onOrchieExpand}
              onClick={onOperatorClick}
            />
          ))}
        </div>
      </div>

      {/* HUD overlays */}
      <div style={{ position: 'absolute', top: 12, left: 14, display: 'flex', flexDirection: 'column', gap: 4 }}>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>T+ {(activeNodes.size + completedNodes.size) * 1.4} s</span>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>OPS · {completedNodes.size} / {SIM_NODES.length}</span>
      </div>
      <div style={{ position: 'absolute', bottom: 12, right: 14, display: 'flex', flexDirection: 'column', alignItems: 'flex-end', gap: 4 }}>
        <span className="meta-10" style={{ color: 'var(--amber)' }}>ACTIVE · {activeNodes.size}</span>
        <span className="meta-10" style={{ color: 'var(--teal)' }}>COMPLETED · {completedNodes.size}</span>
      </div>

      {/* Step 4: Orchie expansion panel — slides in from right */}
      <OrchieExpansionPanel
        open={orchieOpen}
        onClose={onCloseOrchie}
        tick={tick}
      />

      {/* Step 6: Code editor — slides up from bottom */}
      {codeEditorOpen && selectedOperator && (
        <CodeEditor
          operatorNode={selectedOperator}
          onClose={onCloseCodeEditor}
        />
      )}
    </div>
  );
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
    try {
      const current = JSON.parse(proposalText);
      const base = Array.isArray(current) ? current : [];
      setProposalText(JSON.stringify([...base, { operator: opName, params: {} }], null, 2));
      setProposalError(null);
    } catch {
      setProposalText(JSON.stringify([{ operator: opName, params: {} }], null, 2));
      setProposalError(null);
    }
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
        <textarea
          value={proposalText}
          onChange={e => { setProposalText(e.target.value); setProposalError(null); }}
          spellCheck={false}
          rows={5}
          style={{
            width: '100%', boxSizing: 'border-box', resize: 'vertical',
            background: 'var(--bg-base)', border: `1px solid ${proposalError ? 'var(--danger)' : 'var(--border-mid)'}`,
            borderRadius: 'var(--radius-sm)', padding: '6px 8px',
            fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-primary)', outline: 'none',
          }}
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
          SCORE · BRANCH {selectedBranch || 'A.2'}
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
              const delta = step.state_after ? Object.entries(step.state_after).filter(([k]) => k !== '_stub') : [];
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
