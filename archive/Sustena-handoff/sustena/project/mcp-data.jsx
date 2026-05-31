/* Sustena Mycelium Control Panel — shared data, DAG primitives, node renderers */

const { useState: dUseState, useEffect: dUseEffect, useRef: dUseRef, useMemo: dUseMemo, useCallback: dUseCallback } = React;

/* ───────────────────────────────────────────────────────────
   ACTIVE SUSTAINS — data model
   ─────────────────────────────────────────────────────────── */
const SUSTAINS = [
  { id: 'homestead.bonnie',    label: 'HOMESTEAD',           sub: 'B.GACHIENGU · NAIROBI',     status: 'live',   ops: 24,  events: 8429112 },
  { id: 'biashara.vyyb',       label: 'BIASHARA · VYYB',     sub: 'NYAMA CHOMA · OUTLET-01',   status: 'live',   ops: 41,  events: 8429084 },
  { id: 'chama.upendo',        label: 'CHAMA · UPENDO',      sub: 'RUIRU · 18 MEMBERS',        status: 'live',   ops: 12,  events: 8428901 },
  { id: 'mkulima.murang_a',    label: 'MKULIMA',             sub: 'MURANG\'A · COFFEE',        status: 'idle',   ops: 8,   events: 8428712 },
  { id: 'platform.ops',        label: 'PLATFORM · OPS',      sub: 'SUSTENA SELF-MONITOR',      status: 'live',   ops: 18,  events: 8429120 },
];

/* ───────────────────────────────────────────────────────────
   STATE FIELDS — for Monitor's Live State Stream
   ─────────────────────────────────────────────────────────── */
const STATE_TREE = [
  { path: 'finances.cash_position',        value: 184250, fmt: 'ksh',  cstr: 'ok',    desc: 'within reserve floor' },
  { path: 'finances.burn_rate',            value: 4214,   fmt: 'ksh',  cstr: 'amber', desc: 'approaching ceiling 4500' },
  { path: 'finances.pockets.food.spent',   value: 8420,   fmt: 'ksh',  cstr: 'ok',    desc: 'of 12,000 budget' },
  { path: 'finances.pockets.rent.alloc',   value: 22000,  fmt: 'ksh',  cstr: 'ok',    desc: 'locked · due day 30' },
  { path: 'pantry.cooking_oil_l',          value: 0.4,    fmt: 'L',    cstr: 'amber', desc: 'reorder threshold 0.5' },
  { path: 'pantry.maize_flour_kg',         value: 2.8,    fmt: 'kg',   cstr: 'ok',    desc: '' },
  { path: 'pantry.matumbo_kg',             value: 0.0,    fmt: 'kg',   cstr: 'red',   desc: 'CONSTRAINT FAILED · empty' },
  { path: 'chama.contributions.cycle_14',  value: 198400, fmt: 'ksh',  cstr: 'ok',    desc: '16 of 18 members' },
  { path: 'chama.cred_score',              value: 712,    fmt: 'idx',  cstr: 'ok',    desc: 'tier B+' },
  { path: 'biashara.orders_today',         value: 42,     fmt: 'n',    cstr: 'ok',    desc: 'avg 38 · trend +10%' },
  { path: 'biashara.margin_gross',         value: 31,     fmt: '%',    cstr: 'ok',    desc: 'target 28%' },
  { path: 'system.pawa_balance',           value: 8420,   fmt: 'pwa',  cstr: 'ok',    desc: 'burn 28 / hr' },
];

/* ───────────────────────────────────────────────────────────
   EVENT LOG — Monitor's Event Stream
   ─────────────────────────────────────────────────────────── */
const EVENT_SEEDS = [
  { sustain: 'homestead.bonnie',  operator: 'budget.spend',        delta: 'food: 8200 → 8420',                op: 'EXECUTED',  tone: 'ok'  },
  { sustain: 'biashara.vyyb',     operator: 'sale.commit',         delta: 'orders.today: 41 → 42 · +KSH 320',  op: 'EXECUTED', tone: 'ok'  },
  { sustain: 'chama.upendo',      operator: 'contribution.submit', delta: 'cycle_14: 186k → 198k · +12,400',  op: 'EXECUTED',  tone: 'ok'  },
  { sustain: 'homestead.bonnie',  operator: 'pantry.consume',      delta: 'matumbo_kg: 0.4 → 0.0',            op: 'CSTR FAIL',  tone: 'danger' },
  { sustain: 'mkulima.murang_a',  operator: 'price.fetch',         delta: 'cherry_kg: 76 → 78',               op: 'EXECUTED',  tone: 'ok'  },
  { sustain: 'platform.ops',      operator: 'llm.infer.haiku',     delta: 'p95: 412ms → 418ms',               op: 'EXECUTED',  tone: 'ok'  },
  { sustain: 'biashara.vyyb',     operator: 'inventory.reorder',   delta: 'tomatoes_kg: 4 → 8 · −KSH 416',    op: 'PROPOSED',  tone: 'amber' },
  { sustain: 'chama.upendo',      operator: 'orchie.propose',      delta: 'SUS-0148 reallocation drafted',    op: 'AGENT',     tone: 'teal' },
  { sustain: 'homestead.bonnie',  operator: 'mentor.alert',        delta: 'burn_rate approaching ceiling',    op: 'AGENT',     tone: 'amber' },
  { sustain: 'platform.ops',      operator: 'pawa.charge',         delta: 'pawa_balance: 8448 → 8420 · −28',  op: 'EXECUTED',  tone: 'ok'  },
];

function pickEvent(seed) {
  const e = EVENT_SEEDS[seed % EVENT_SEEDS.length];
  const t = new Date(Date.now() - (seed % 60) * 1000);
  const time = `${pad2(t.getUTCHours())}:${pad2(t.getUTCMinutes())}:${pad2(t.getUTCSeconds())}`;
  return { ...e, time, key: `${seed}-${t.getTime()}` };
}
function pad2(n) { return n.toString().padStart(2, '0'); }

/* ───────────────────────────────────────────────────────────
   OPERATIVES — for Monitor's Operative Activity
   ─────────────────────────────────────────────────────────── */
const OPERATIVES = [
  {
    id: 'mentor',  name: 'Mentor',  role: 'BUDGET WATCHDOG',
    status: 'active', confidence: 92, pawa: 142,
    task: 'Burn rate audit · day 23/30',
    subtasks: [
      { name: 'Parse M-Pesa history',          progress: 100, done: true,  cost: 0.3 },
      { name: 'Categorise transactions',       progress: 78,  cost: 0.6 },
      { name: 'Compute burn rate',             progress: 0 },
      { name: 'Generate summary widget',       progress: 0 },
    ],
  },
  {
    id: 'protege', name: 'Protégé', role: 'SCHEDULER',
    status: 'active', confidence: 86, pawa: 84,
    task: 'Cycle-14 payout coordination',
    subtasks: [
      { name: 'Read calendar', progress: 100, done: true, cost: 0.1 },
      { name: 'Cross-ref council quorum', progress: 100, done: true, cost: 0.2 },
      { name: 'Draft member notifications', progress: 40 },
    ],
  },
  {
    id: 'curator', name: 'Curator', role: 'INVENTORY & ASSETS',
    status: 'idle', confidence: 78, pawa: 28,
    task: 'Idle · last action 4m ago',
    subtasks: [
      { name: 'Pantry threshold scan', progress: 100, done: true, cost: 0.2 },
    ],
  },
  {
    id: 'navigator', name: 'Navigator', role: 'LOGISTICS',
    status: 'alert', confidence: 64, pawa: 112,
    task: 'Anomaly · matumbo zero in pantry',
    subtasks: [
      { name: 'Detect anomaly', progress: 100, done: true, cost: 0.1 },
      { name: 'Find proximate vendor', progress: 62, cost: 0.3 },
      { name: 'Propose restock STK push', progress: 0 },
    ],
  },
];

/* ───────────────────────────────────────────────────────────
   SIMULATOR — scenarios + DAG nodes
   ─────────────────────────────────────────────────────────── */
const SCENARIO_TREE = [
  {
    id: 'root', label: 'ROOT · state @ 14:32',
    children: [
      {
        id: 'A',  label: 'BRANCH A · reallocate −400k', score: 0.84, cost: 12, hot: true,
        children: [
          { id: 'A1', label: 'A.1 · timelock 24h', score: 0.81 },
          { id: 'A2', label: 'A.2 · timelock 48h', score: 0.87, hot: true },
        ],
      },
      { id: 'B',  label: 'BRANCH B · defer 14 days',     score: 0.62, cost: 8 },
      { id: 'C',  label: 'BRANCH C · split tranches',    score: 0.71, cost: 18 },
      { id: 'D',  label: 'BRANCH D · do nothing',        score: 0.42, cost: 0 },
    ],
  },
];

/* DAG nodes for Simulator's execution canvas */
const SIM_NODES = [
  { id: 'state0', type: 'state',      label: 'STATE @ T0',        x: 60,  y: 200 },
  { id: 'gate0',  type: 'time',       label: 'CRON · DAILY',      x: 160, y: 200 },
  { id: 'op1',    type: 'operator',   label: 'budget.audit',      x: 280, y: 120 },
  { id: 'op2',    type: 'operator',   label: 'pantry.scan',       x: 280, y: 280 },
  { id: 'cst1',   type: 'constraint', label: 'reserve > 50k',     x: 420, y: 80  },
  { id: 'cst2',   type: 'constraint', label: 'pockets_sum',       x: 420, y: 160 },
  { id: 'ag1',    type: 'operative',  label: 'MENTOR',            x: 560, y: 120 },
  { id: 'ag2',    type: 'operative',  label: 'NAVIGATOR',         x: 560, y: 280 },
  { id: 'op3',    type: 'operator',   label: 'orchie.propose',    x: 700, y: 200 },
  { id: 'ev1',    type: 'event',      label: 'PROPOSAL_DRAFTED',  x: 840, y: 200 },
];
const SIM_EDGES = [
  ['state0','gate0'], ['gate0','op1'], ['gate0','op2'],
  ['op1','cst1'], ['op1','cst2'], ['op2','cst2'],
  ['cst1','ag1'], ['cst2','ag1'], ['op2','ag2'],
  ['ag1','op3'], ['ag2','op3'], ['op3','ev1'],
];

/* ───────────────────────────────────────────────────────────
   COUNCIL PROPOSALS — for Controller
   ─────────────────────────────────────────────────────────── */
const PROPOSALS = [
  {
    id: 'SUS-0148', sustain: 'chama.upendo',
    title: 'Advance Q3 disbursement to Carbon-R&D pool by 14 days',
    author: 'mentor.agent',
    summary: 'Releases liquidity for two pending field trials (Boreal-N, Coastal-W) ahead of seasonal weather windows.',
    council: { for: 7, against: 2, abstain: 1 },
    cost: 42, autonomy: 'HIGH',
    sim: { outcomeScore: 0.84, constraintPassRate: 1.0, runs: 100, projection: '+KSH 1.20M release · 9 days earlier' },
    cta: 'EXECUTE',
  },
  {
    id: 'SUS-0147', sustain: 'platform.ops',
    title: 'Switch routine M-Pesa parsing → fine-tuned local model',
    author: 'staff-orchie.agent',
    summary: 'Save $120/month in Haiku costs; routine classification accuracy 96.4% in sim.',
    council: { for: 6, against: 3, abstain: 1 },
    cost: 18, autonomy: 'LOW',
    sim: { outcomeScore: 0.78, constraintPassRate: 0.99, runs: 100, projection: '−$120/mo · accuracy −0.6%' },
    cta: 'EXECUTE',
  },
  {
    id: 'SUS-0146', sustain: 'biashara.vyyb',
    title: 'Reorder tomatoes via NCBA Pochi — KES 416',
    author: 'curator.agent',
    summary: 'Stock below 4kg threshold. Vendor: Mama Mboga · Githurai · 12-min ETA.',
    council: { for: 9, against: 0, abstain: 1 },
    cost: 4, autonomy: 'AUTO',
    sim: { outcomeScore: 0.94, constraintPassRate: 1.0, runs: 50, projection: 'Inventory → 8kg · margin unaffected' },
    cta: 'AUTO-EXECUTE',
  },
];

/* ───────────────────────────────────────────────────────────
   MYCELIUM LIBRARY entries
   ─────────────────────────────────────────────────────────── */
const LIBRARY = {
  operatives: [
    { name: 'Mentor',     author: 'sustena.core',    version: '4.2.1',  pawa: '0.14/op',  trust: 98, downloads: 1820, desc: 'Budget Watchdog. Burn-rate alerts. HoE sub-operative.' },
    { name: 'Protégé',    author: 'sustena.core',    version: '4.1.0',  pawa: '0.08/op',  trust: 94, downloads: 1480, desc: 'Scheduler. Calendar reconciliation, deadline drafting.' },
    { name: 'Curator',    author: 'sustena.core',    version: '3.9.4',  pawa: '0.12/op',  trust: 91, downloads: 1240, desc: 'Inventory & assets. Pantry, depreciation, surplus listing.' },
    { name: 'Navigator',  author: 'sustena.core',    version: '4.0.2',  pawa: '0.18/op',  trust: 89, downloads: 980,  desc: 'Logistics. Route, vendor proximity, restock triage.' },
    { name: 'Mkulima',    author: 'colosso.dev',     version: '2.4.0',  pawa: '0.22/op',  trust: 86, downloads: 612,  desc: 'Farm manager. Crop calendar, market price, buyer pipeline.' },
    { name: 'Mama Mboga', author: 'mycelium.eve-w',  version: '0.9.2',  pawa: '0.06/op',  trust: 72, downloads: 184,  desc: 'Community vendor matcher. Surfaces local restock options.' },
  ],
  operators: [
    { name: 'budget.allocate',     author: 'sustena.core', version: '2.1',  pawa: 0.02, trust: 99, downloads: 4820, desc: 'Allocate to pocket; sum_constraint, balance_constraint.' },
    { name: 'mpesa.parse',         author: 'sustena.core', version: '5.3',  pawa: 0.04, trust: 98, downloads: 8920, desc: 'Parse M-Pesa SMS → structured event.' },
    { name: 'chama.contribution',  author: 'colosso.dev',  version: '1.4',  pawa: 0.03, trust: 96, downloads: 1240, desc: 'Validate + record chama contribution against cycle.' },
    { name: 'sale.commit',         author: 'sustena.core', version: '3.0',  pawa: 0.05, trust: 97, downloads: 3120, desc: 'Commit POS sale; updates inventory + ledger atomically.' },
    { name: 'pantry.consume',      author: 'sustena.core', version: '2.0',  pawa: 0.02, trust: 95, downloads: 920,  desc: 'Decrement pantry; triggers reorder threshold check.' },
    { name: 'orchie.propose',      author: 'sustena.core', version: '4.2',  pawa: 0.20, trust: 94, downloads: 612,  desc: 'Draft a Council proposal from sim evidence.' },
  ],
  spores: [
    { name: 'Homestead',  author: 'sustena.core', version: '1.0',  pawa: 'free', trust: 99, downloads: 2418, desc: 'Reference household sustain. Budget, pantry, chama hook.' },
    { name: 'Biashara',   author: 'sustena.core', version: '1.0',  pawa: 'free', trust: 99, downloads: 1820, desc: 'Reference food business. Recipes, P&L, KDS.' },
    { name: 'Chama',      author: 'colosso.dev',  version: '0.9',  pawa: 'free', trust: 96, downloads: 1240, desc: 'Group savings DAO. Contributions, payouts, voting.' },
    { name: 'Mkulima',    author: 'colosso.dev',  version: '0.8',  pawa: 'free', trust: 92, downloads: 612,  desc: 'Smallholder farm. Crop calendar, market, buyer.' },
    { name: 'Driver',     author: 'mycelium.kev', version: '0.4',  pawa: 'free', trust: 78, downloads: 184,  desc: 'Boda boda operator. Fuel, fares, savings.' },
  ],
  widgets: [
    { name: 'budget_allocation_card',  author: 'sustena.core', version: '1.2', pawa: 'free', trust: 99, downloads: 4820, desc: 'Pocket allocation card with constraint progress bar.' },
    { name: 'proposal_card',           author: 'sustena.core', version: '1.4', pawa: 'free', trust: 98, downloads: 3120, desc: 'Council proposal preview · vote tally · evidence.' },
    { name: 'mpesa_confirmation',      author: 'sustena.core', version: '2.0', pawa: 'free', trust: 99, downloads: 8920, desc: 'M-Pesa transaction confirmation with category.' },
    { name: 'burn_sparkline',          author: 'sustena.core', version: '1.0', pawa: 'free', trust: 95, downloads: 1240, desc: 'Burn-rate sparkline with safe-to-spend overlay.' },
    { name: 'pantry_status_card',      author: 'mycelium.amy', version: '0.6', pawa: 'free', trust: 84, downloads: 412,  desc: 'Pantry status with reorder thresholds.' },
    { name: 'alert_banner_amber',      author: 'mycelium.kev', version: '0.3', pawa: 'free', trust: 76, downloads: 280,  desc: 'Amber alert banner with CTA + dismiss.' },
  ],
};

/* ───────────────────────────────────────────────────────────
   DAG NODE RENDERER
   ─────────────────────────────────────────────────────────── */
function DagNode({ node, active, pulse, onClick }) {
  const config = {
    state:      { color: 'var(--node-state)',      shape: 'rect',     width: 110, height: 36 },
    operator:   { color: 'var(--node-operator)',   shape: 'pill',     width: 130, height: 32 },
    constraint: { color: 'var(--node-constraint)', shape: 'diamond',  width: 120, height: 40 },
    event:      { color: 'var(--node-event)',      shape: 'hex',      width: 130, height: 40 },
    operative:  { color: 'var(--node-operative)',  shape: 'octagon',  width: 110, height: 40 },
    time:       { color: 'var(--node-time)',       shape: 'circle',   width: 90,  height: 36 },
  }[node.type];

  const baseStyle = {
    left: node.x, top: node.y,
    transform: 'translate(-50%, -50%)',
    width: config.width, height: config.height,
    color: config.color,
    paddingLeft: 10, paddingRight: 10,
  };

  if (config.shape === 'pill') {
    baseStyle.borderRadius = config.height / 2;
    baseStyle.borderColor = active ? config.color : 'var(--border-mid)';
    baseStyle.background = active ? `color-mix(in oklab, ${config.color} 10%, var(--bg-surface))` : 'var(--bg-surface)';
  } else if (config.shape === 'diamond') {
    baseStyle.transform = 'translate(-50%, -50%) rotate(45deg)';
    baseStyle.width = config.width * 0.75;
    baseStyle.height = config.width * 0.75;
    baseStyle.borderColor = active ? config.color : 'var(--border-mid)';
    baseStyle.background = active ? `color-mix(in oklab, ${config.color} 14%, var(--bg-surface))` : 'var(--bg-surface)';
  } else if (config.shape === 'hex') {
    baseStyle.clipPath = 'polygon(12% 0%, 88% 0%, 100% 50%, 88% 100%, 12% 100%, 0% 50%)';
    baseStyle.borderColor = 'transparent';
    baseStyle.background = active
      ? `linear-gradient(${config.color}, ${config.color}) padding-box, ${config.color} border-box`
      : 'var(--bg-surface)';
    baseStyle.border = `1px solid ${active ? config.color : 'var(--border-mid)'}`;
  } else if (config.shape === 'octagon') {
    baseStyle.clipPath = 'polygon(28% 0%, 72% 0%, 100% 28%, 100% 72%, 72% 100%, 28% 100%, 0% 72%, 0% 28%)';
    baseStyle.background = active ? `color-mix(in oklab, ${config.color} 16%, var(--bg-surface))` : 'var(--bg-surface)';
    baseStyle.border = `1px solid ${active ? config.color : 'var(--border-mid)'}`;
  } else if (config.shape === 'circle') {
    baseStyle.borderRadius = '50%';
    baseStyle.width = config.height;
    baseStyle.height = config.height;
    baseStyle.padding = 0;
    baseStyle.borderColor = active ? config.color : 'var(--border-mid)';
    baseStyle.background = active ? 'var(--bg-raised)' : 'var(--bg-surface)';
  } else {
    baseStyle.borderRadius = 'var(--radius-sm)';
    baseStyle.borderColor = active ? config.color : 'var(--border-mid)';
    baseStyle.background = active ? `color-mix(in oklab, ${config.color} 12%, var(--bg-surface))` : 'var(--bg-surface)';
  }

  return (
    <div
      className={`dag-node ${active ? 'dag-node--active' : ''} ${pulse ? 'dag-node--pulse' : ''}`}
      style={baseStyle}
      onClick={onClick}
    >
      <span style={{
        color: config.shape === 'diamond' ? config.color : (active ? config.color : 'var(--text-primary)'),
        transform: config.shape === 'diamond' ? 'rotate(-45deg)' : 'none',
        whiteSpace: 'nowrap',
        fontSize: 9,
      }}>{node.label}</span>
    </div>
  );
}

/* ───────────────────────────────────────────────────────────
   DAG EDGES — bezier connectors with optional flow dot
   ─────────────────────────────────────────────────────────── */
function DagEdges({ nodes, edges, activePath = [], width, height }) {
  const byId = Object.fromEntries(nodes.map(n => [n.id, n]));
  return (
    <svg width={width} height={height} style={{ position: 'absolute', inset: 0, pointerEvents: 'none' }}>
      <defs>
        <marker id="arr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
          <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--border-light)" />
        </marker>
        <marker id="arr-amber" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
          <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--amber)" />
        </marker>
      </defs>
      {edges.map(([a, b], i) => {
        const A = byId[a], B = byId[b];
        if (!A || !B) return null;
        const dx = B.x - A.x;
        const cpx1 = A.x + dx * 0.5;
        const cpx2 = B.x - dx * 0.5;
        const d = `M ${A.x} ${A.y} C ${cpx1} ${A.y}, ${cpx2} ${B.y}, ${B.x} ${B.y}`;
        const isActive = activePath.includes(a) && activePath.includes(b);
        return (
          <g key={i}>
            <path d={d} fill="none"
              stroke={isActive ? 'var(--amber)' : 'var(--border)'}
              strokeWidth={isActive ? 1.4 : 1}
              markerEnd={`url(#${isActive ? 'arr-amber' : 'arr'})`}
            />
            {isActive && (
              <circle r="3" fill="var(--amber)" style={{ filter: 'drop-shadow(0 0 4px var(--amber))' }}>
                <animateMotion dur="1.6s" repeatCount="indefinite" path={d} />
              </circle>
            )}
          </g>
        );
      })}
    </svg>
  );
}

/* ───────────────────────────────────────────────────────────
   Export
   ─────────────────────────────────────────────────────────── */
Object.assign(window, {
  SUSTAINS, STATE_TREE, EVENT_SEEDS, pickEvent,
  OPERATIVES, SCENARIO_TREE, SIM_NODES, SIM_EDGES,
  PROPOSALS, LIBRARY,
  DagNode, DagEdges,
});
