import React from 'react';
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
          <Card title="SCENARIO TREE" sub="4 BRANCHES" padded={false} scroll>
            <ScenarioTree
              tree={SCENARIO_TREE}
              selected={selectedBranch}
              onSelect={setSelectedBranch}
            />
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
          <Card title="STATE DIFF" sub={`+12 / −3`} padded scroll>
            <StateDiff tick={tick} />
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

/* State Diff right panel */
function StateDiff({ tick }) {
  const diffs = [
    { field: 'finances.cash_position',   before: 184250, after: 174250, op: '−10,000 KSH', tone: 'amber' },
    { field: 'finances.pockets.food',    before: 8420,   after: 8420,   op: 'no change',   tone: 'muted' },
    { field: 'pantry.tomatoes_kg',       before: 4,      after: 8,      op: '+4 kg',       tone: 'ok' },
    { field: 'chama.contributions',      before: 198400, after: 198400, op: 'no change',   tone: 'muted' },
    { field: 'system.pawa_balance',      before: 8420,   after: 8378,   op: '−42 pwa',     tone: 'amber' },
  ];
  const cstrs = [
    { name: 'reserve > 50,000',     status: 'PASS' },
    { name: 'pockets.sum ≤ income', status: 'PASS' },
    { name: 'no_negative_balance',  status: 'PASS' },
    { name: 'cycle_quorum >= 80%',  status: 'PASS' },
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 8 }}>OUTCOME · BRANCH A.2</span>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 6 }}>
          <span style={{ fontFamily: 'var(--mono)', fontSize: 36, fontWeight: 500, color: 'var(--teal)' }}>0.87</span>
          <span className="meta-10">/ 1.00 score</span>
        </div>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>rank · 1 of 100</span>
      </div>

      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 8 }}>STATE Δ</span>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {diffs.map((d, i) => (
            <div key={i} style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{d.field}</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 11, color: d.tone === 'ok' ? 'var(--teal)' : d.tone === 'amber' ? 'var(--amber)' : 'var(--text-muted)' }}>
                {d.op}
              </span>
            </div>
          ))}
        </div>
      </div>

      <div>
        <span className="label-10" style={{ display: 'block', marginBottom: 8 }}>CONSTRAINTS</span>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          {cstrs.map((c, i) => (
            <div key={i} style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '4px 0' }}>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)' }}>{c.name}</span>
              <Badge tone="ok">{c.status}</Badge>
            </div>
          ))}
        </div>
      </div>

      <div style={{ borderTop: '1px solid var(--border)', paddingTop: 12 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
          <span className="label-10">PAWA COST</span>
          <span className="val-12" style={{ color: 'var(--amber)' }}>{42 - (tick % 5)} <span style={{ color: 'var(--text-muted)' }}>pwa</span></span>
        </div>
      </div>
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
