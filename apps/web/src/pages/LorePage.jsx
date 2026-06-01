import React, { useState, useEffect, useRef, useMemo, useCallback } from 'react';

const ENTRY_TYPES = {
  vision:     { label: 'VISION'     },
  technical:  { label: 'TECHNICAL'  },
  reflection: { label: 'REFLECTION' },
  update:     { label: 'UPDATE'     },
};

/* Blog entries — populated from API / CMS */
const ENTRIES_TOMBSTONE = [
  {
    id: 'b001',
    type: 'vision',
    title: 'On Building Sustena — a note from the architect',
    date: '2026-05-20',
    readTime: '6 min',
    tags: ['founding', 'philosophy', 'africa'],
    preview: 'Every complex system — a household, a farm, a food business, a chama — already has intelligence embedded in it. Sustena does not bring intelligence to these systems. It makes the intelligence already there visible, simulatable, and governable.',
    body: [
      { type: 'lead', text: 'Every complex system — a household, a farm, a food business, a chama — already has intelligence embedded in it. Sustena does not bring intelligence to these systems. It makes the intelligence already there visible, simulatable, and governable.' },
      { type: 'p', text: 'We started with a deceptively simple observation: the informal systems that drive the majority of economic activity in Kenya — the chama secretary who tracks twenty members\' contributions in a notebook, the mama mboga who knows exactly which supplier gives the best sukuma on Tuesdays, the boda boda operator who has internalised the optimal route to the hospital at 6am — these people are not unsophisticated. They are running complex, adaptive, constraint-bounded systems. They just lack the digital infrastructure to model, simulate, and optimise those systems before acting.' },
      { type: 'h2', text: 'The claim we are making' },
      { type: 'p', text: 'Any system that can be described can be modelled, simulated, and optimised before execution. This is not a product tagline. It is a physical and computational claim — and Sustena is the infrastructure that makes it actionable.' },
      { type: 'p', text: 'Where most platforms build tools for specific domains, Sustena builds the language in which all tools are expressed. The seven primitives — State, Operators, Constraints, Events, Time, Consensus, Operatives — are the minimal correct vocabulary for describing any real-world system. A chama\'s rotation schedule, a farm\'s crop calendar, a food business\'s recipe system: none of these require changing the primitives. They require only new Operators and new Operative configurations.' },
      { type: 'h2', text: 'Why now, why Nairobi' },
      { type: 'p', text: 'Nairobi in 2026 is the right environment for this. M-Pesa has already won: digital money flows freely. Smartphones are ubiquitous. The informal economy is enormous and sophisticated. What is missing is not payment rails or devices — it is the intelligence layer that sits above them and helps people coordinate, simulate, and decide.' },
      { type: 'p', text: 'Sustena does not import Western ERP logic into Nairobi. It builds intelligence from the ground up, in the language of how Kenyans actually work. The first sustains we\'re building for ourselves — Vyyb (a street food business), Homestead (a household), and Colosso (a financial intelligence layer) — are not demos. They are production systems running our lives. The intelligence we build is the intelligence we need.' },
      { type: 'h2', text: 'What Orchie actually is' },
      { type: 'p', text: 'Orchie is not a chatbot. Orchie is an Orchestrator Operative — an autonomous agent that observes your sustain\'s state, proposes strategies, simulates their consequences, and presents them for your approval. Orchie does not decide for you. The Council deliberates; you decide. The 51% user vote is not a courtesy — it is the consent mechanism encoded into the governance layer. An operative can be highly confident, backed by strong simulation evidence, and still be vetoed. This is not a bug. This is the product.' },
    ],
    sidenotes: [
      { anchor: 'primitives', text: 'Seven primitives: State, Operators, Constraints, Events, Time, Consensus, Operatives', color: 'var(--amber)' },
    ],
  },
  {
    id: 'b002',
    type: 'technical',
    title: 'The Mycelium: open infrastructure for governed intelligence',
    date: '2026-05-15',
    readTime: '5 min',
    tags: ['architecture', 'mycelium', 'network'],
    preview: 'The Mycelium is Sustena\'s open network layer — the marketplace, the protocol, and the trust system that enables operatives, operators, and sustain templates to flow freely between participants while remaining governed and accountable.',
    body: [
      { type: 'lead', text: 'The Mycelium is Sustena\'s open network layer — the marketplace, the protocol, and the trust system that enables operatives, operators, and sustain templates to flow freely between participants while remaining governed and accountable.' },
      { type: 'h2', text: 'What the Mycelium is not' },
      { type: 'p', text: 'It is not an app store. App stores are one-directional: a developer uploads; a user downloads. The Mycelium is bidirectional and live — every operative you deploy in your sustain participates in the network, improves from cross-sustain signals (anonymised), and can be forked by other participants. It is infrastructure, not a distribution channel.' },
      { type: 'p', text: 'It is not a blockchain. We use blockchain architecture for the governance and trust layers — smart contracts for proposal voting, immutable event logs for accountability — but the Mycelium\'s primary computational substrate is the Sustena Platform\'s constraint engine. The blockchain is the notary, not the computer.' },
      { type: 'h2', text: 'The three layers' },
      { type: 'p', text: 'Layer 1: the Protocol. The seven primitives define a common language. Any operator published to the Mycelium must conform to the (State, Inputs, Logic) → (Delta, Events) contract. Any operative must declare its constraint footprint, access requirements, and performance metric. This is the type system of the network.' },
      { type: 'p', text: 'Layer 2: the Trust System. Every package on the Mycelium has a trust score — a function of its deployment count, its error rate across deployments, its governance history, and the reputation of its author. Trust is not claimed; it is demonstrated, recorded, and composable. The Chama Cred score, the Operative Trust Rating, the DAO voting record are all instances of the same trust primitive.' },
      { type: 'p', text: 'Layer 3: the Arena. The Arena is the public-facing marketplace where anyone — authenticated or not — can discover, browse, and evaluate everything on the Mycelium. Developers browse to find building blocks. Operators browse to find operatives for their sustain. Farmers list produce. Food businesses list menu items. The network is the marketplace.' },
      { type: 'h2', text: 'Why openness and governance are not opposites' },
      { type: 'p', text: 'Openness without governance produces spam and exploitation. Governance without openness produces lock-in and monopoly. The Mycelium threads this needle by making the governance layer itself programmable and open. The trust scoring parameters are not set by Sustena — they are governed by DAO proposal and vote across the network. Sustena builds the protocol; the network governs itself.' },
    ],
    sidenotes: [
      { anchor: 'arena', text: 'Arena: browse.sustena.network — the public marketplace layer', color: 'var(--teal)' },
    ],
  },
  {
    id: 'b003',
    type: 'reflection',
    title: 'Building for founders with no runway — the real constraint',
    date: '2026-05-10',
    readTime: '4 min',
    tags: ['founding', 'constraints', 'strategy'],
    preview: 'Bonnie has no income. Brian carries debt and works long hours. This is not a weakness to be managed quietly — it is the founding constraint that determines every prioritisation decision. We build what gives us immediate leverage first.',
    body: [
      { type: 'lead', text: 'Bonnie has no income. Brian carries debt and works long hours. This is not a weakness to be managed quietly — it is the founding constraint that determines every prioritisation decision. We build what gives us immediate leverage first.' },
      { type: 'h2', text: 'The survival-first rule' },
      { type: 'p', text: 'The Budget Watchdog Operative and Scheduling Operative are not features for a future user base. They are tools we build for ourselves, today, as proof-of-concept and survival infrastructure simultaneously. Sustena is its own first user. Every operative we deploy to our own sustains earns its place by making our lives materially better — not by demonstrating potential for a hypothetical user.' },
      { type: 'p', text: 'This rule has a clarifying effect on the roadmap. It collapses the distance between "building the product" and "using the product" to zero. The Vyyb sustain — tracking deliveries, managing procurement, computing contribution margins in real time — is not a demo. It is the live system behind a real food business. If the sustain fails, Bonnie\'s income fails. That constraint is clarifying.' },
      { type: 'h2', text: 'Why constraints are features' },
      { type: 'p', text: 'Sustena\'s architecture is deliberately constraint-first. Every operator has a pre-condition and a post-condition. Every operative has an explicit constraint footprint. Every proposal must satisfy the constraint engine before it reaches the Council. This is not bureaucracy — it is the memory of every bad decision made without constraints.' },
      { type: 'p', text: 'When resources are scarce, constraints become the most valuable thing you have. They tell you what not to do, faster than any framework. The founders\' resource constraint is not an obstacle to building Sustena — it is the prototype of the constraint engine that Sustena will eventually run for everyone.' },
      { type: 'h2', text: 'What this means for the roadmap' },
      { type: 'p', text: 'Phase 0 is a WhatsApp bot. Not a white-label app, not a polished product, not a fundraising demo. A WhatsApp bot that parses M-Pesa messages, tracks budget pockets, and lets Orchie send constraint-bounded proposals via voice note. It runs on our phones. It serves our needs. It proves the primitive. Everything else is Phase 1 and beyond.' },
    ],
    sidenotes: [],
  },
  {
    id: 'b004',
    type: 'update',
    title: 'Vyyb Phase 1 complete — delivery dispatch live',
    date: '2026-05-25',
    readTime: '2 min',
    tags: ['vyyb', 'phase-1', 'update'],
    preview: 'Vyyb Phase 1 is live. Delivery dispatch is running. Entity documentation is filed. The Curator operative has its first production context: a cooking oil restock threshold alert.',
    body: [
      { type: 'lead', text: 'Vyyb Phase 1 is live. Delivery dispatch is running. Entity documentation is filed. The Curator operative has its first production context: a cooking oil restock threshold alert.' },
      { type: 'p', text: 'Phase 1 of the Vyyb sustain is the minimum viable sustain: a self-hosted system that tracks deliveries, manages procurement contacts, and monitors pantry thresholds. It is not a product yet — it is a system. The distinction matters.' },
      { type: 'h2', text: 'What\'s running' },
      { type: 'p', text: 'The Curator operative is deployed. Its current mandate: monitor the pantry state and generate a restock proposal whenever a staple ingredient falls below the defined threshold. Cooking oil is critical (below 2L threshold as of this writing). A proposal is queued — awaiting Council vote.' },
      { type: 'p', text: 'Delivery dispatch is scheduled for Monday 08:00. The Navigator operative has the route optimisation strategy ready. This is the first time a Sustena operative will take an action that has direct financial consequences in the physical world.' },
      { type: 'h2', text: 'What we learned' },
      { type: 'p', text: 'The pantry-to-procurement chain is more complex than the spec suggested. The gap between "ingredient below threshold" and "purchase order sent" has four intermediate steps, two external dependencies (supplier contact + M-Pesa balance), and one constraint (weekly budget ceiling). The Curator operative modelled all four steps correctly on the first pass. The constraint engine prevented an over-budget proposal before it reached the Council. This is the system working as designed.' },
    ],
    sidenotes: [
      { anchor: 'vyyb', text: 'Vyyb: street food QSR sustain — Bonnie Gachiengu, Nairobi', color: 'var(--ok)' },
    ],
  },
];

/* Active entries — empty until API is wired */
const ENTRIES = [];

const CATEGORIES = [
  { id: 'all',        label: 'ALL',        count: ENTRIES.length },
  { id: 'vision',     label: 'VISION',     count: ENTRIES.filter(e => e.type === 'vision').length },
  { id: 'technical',  label: 'TECHNICAL',  count: ENTRIES.filter(e => e.type === 'technical').length },
  { id: 'reflection', label: 'REFLECTION', count: ENTRIES.filter(e => e.type === 'reflection').length },
  { id: 'update',     label: 'UPDATE',     count: ENTRIES.filter(e => e.type === 'update').length },
];

/* ── Entry body renderer ─────────────────────────────── */
function BodyBlock({ block }) {
  const base = { fontFamily: 'var(--ui)', color: 'var(--text-secondary)', lineHeight: 1.75 };
  if (block.type === 'lead')    return <p style={{ ...base, fontSize: 15, color: 'var(--text-primary)', fontWeight: 400, marginBottom: 20 }}>{block.text}</p>;
  if (block.type === 'p')       return <p style={{ ...base, fontSize: 14, marginBottom: 16 }}>{block.text}</p>;
  if (block.type === 'h2')      return <h2 style={{ fontFamily: 'var(--ui)', fontSize: 14, fontWeight: 600, color: 'var(--text-primary)', marginTop: 28, marginBottom: 12, letterSpacing: '-0.01em' }}>{block.text}</h2>;
  if (block.type === 'sidenote') return null;
  return null;
}

/* ── Entry card — right-aligned, minimal ────────────── */
function EntryCard({ entry, active, onSelect }) {
  const t = ENTRY_TYPES[entry.type] || ENTRY_TYPES.reflection;
  return (
    <button onClick={() => onSelect(entry.id)} style={{
      width: '100%', textAlign: 'right',
      padding: '18px 0 18px 16px',
      borderBottom: '1px solid var(--border)',
      background: active ? 'var(--bg-raised)' : 'transparent',
      transition: 'background var(--t-fast)',
    }}
    onMouseEnter={e => { if (!active) e.currentTarget.style.background = 'var(--bg-raised)'; }}
    onMouseLeave={e => { if (!active) e.currentTarget.style.background = 'transparent'; }}
    >
      <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.1em', textTransform: 'uppercase', marginBottom: 7 }}>
        {t.label} · {entry.date}
      </div>
      <div style={{ fontFamily: 'var(--ui)', fontSize: 12, fontWeight: active ? 500 : 400, color: active ? 'var(--text-primary)' : 'var(--text-secondary)', lineHeight: 1.5, marginBottom: 6 }}>
        {entry.title}
      </div>
      <div style={{ display: 'flex', gap: 5, justifyContent: 'flex-end', flexWrap: 'wrap' }}>
        {entry.tags.map(tag => (
          <span key={tag} style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.08em', textTransform: 'uppercase' }}>{tag}</span>
        ))}
      </div>
    </button>
  );
}

/* ── Entry reader — kaobook: main column + right margin ─ */
function EntryReader({ entry }) {
  const t = ENTRY_TYPES[entry.type] || ENTRY_TYPES.reflection;
  return (
    <div className="panel-enter" style={{ display: 'grid', gridTemplateColumns: '1fr 180px', height: '100%', overflow: 'hidden' }}>
      {/* Main text column */}
      <div style={{ overflowY: 'auto', padding: '56px 52px 80px 52px' }}>
        <div style={{ maxWidth: 580 }}>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.12em', textTransform: 'uppercase', marginBottom: 16, display: 'flex', gap: 10, alignItems: 'center' }}>
            <span>{t.label}</span>
            <span style={{ opacity: 0.4 }}>·</span>
            <span>{entry.date}</span>
            <span style={{ opacity: 0.4 }}>·</span>
            <span>{entry.readTime}</span>
          </div>
          <h1 style={{ fontFamily: 'var(--ui)', fontSize: 22, fontWeight: 500, lineHeight: 1.35, letterSpacing: '-0.015em', color: 'var(--text-primary)', marginBottom: 20 }}>
            {entry.title}
          </h1>
          <div style={{ display: 'flex', gap: 5, marginBottom: 32, paddingBottom: 24, borderBottom: '1px solid var(--border)', flexWrap: 'wrap' }}>
            {entry.tags.map(tag => (
              <span key={tag} style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-muted)', letterSpacing: '0.08em', textTransform: 'uppercase', padding: '2px 6px', border: '1px solid var(--border)', borderRadius: 3 }}>{tag}</span>
            ))}
          </div>
          {entry.body.map((block, i) => <BodyBlock key={i} block={block} />)}
        </div>
      </div>
      {/* Right margin — sidenotes, left-aligned */}
      <div style={{ borderLeft: '1px solid var(--border)', padding: '56px 20px 80px 22px', overflowY: 'auto' }}>
        {entry.sidenotes?.length > 0 && (
          <>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.14em', textTransform: 'uppercase', marginBottom: 20 }}>CONTEXT</div>
            {entry.sidenotes.map((n, i) => (
              <div key={i} style={{ marginBottom: 28, borderLeft: '1px solid var(--border-mid)', paddingLeft: 10 }}>
                <div style={{ fontFamily: 'var(--mono)', fontSize: 7, color: 'var(--text-dim)', letterSpacing: '0.1em', textTransform: 'uppercase', marginBottom: 6 }}>{n.anchor}</div>
                <div style={{ fontFamily: 'var(--ui)', fontSize: 11, color: 'var(--text-muted)', lineHeight: 1.65 }}>{n.text}</div>
              </div>
            ))}
          </>
        )}
      </div>
    </div>
  );
}

/* ── Main page — T boundary layout ──────────────────── */
function LorePage() {
  const [activeId, setActiveId] = useState(null);
  const [category, setCategory] = useState('all');

  const filtered = useMemo(() =>
    category === 'all' ? ENTRIES : ENTRIES.filter(e => e.type === category),
  [category]);

  const active = ENTRIES.find(e => e.id === activeId) || null;

  useEffect(() => {
    if (filtered.length > 0 && !filtered.find(e => e.id === activeId)) setActiveId(filtered[0]?.id);
  }, [category]);

  return (
    <div style={{ height: '100%', background: 'var(--bg-base)' }}>

      {/* Body — T columns */}
      <div style={{ display: 'grid', gridTemplateColumns: '240px 1fr', height: '100%', overflow: 'hidden' }}>
        {/* Left: sidebar — all text right-aligned, hugging the divider */}
        <div style={{ borderRight: '1px solid var(--border)', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
          {/* Category rail */}
          <div style={{ borderBottom: '1px solid var(--border)', padding: '14px 0', display: 'flex', gap: 2, flexWrap: 'wrap', justifyContent: 'flex-end' }}>
            {CATEGORIES.map(cat => (
              <button key={cat.id} onClick={() => setCategory(cat.id)} style={{
                padding: '3px 8px',
                fontFamily: 'var(--mono)', fontSize: 8, letterSpacing: '0.1em', textTransform: 'uppercase',
                color: category === cat.id ? 'var(--text-primary)' : 'var(--text-dim)',
                background: 'transparent', border: 'none',
                transition: 'color var(--t-fast)', cursor: 'pointer',
              }}>{cat.label} <span style={{ opacity: 0.4 }}>{cat.count}</span></button>
            ))}
          </div>
          {/* Entry list */}
          <div style={{ overflowY: 'auto', flex: 1 }}>
            {filtered.length === 0
              ? <div style={{ padding: '24px 12px', textAlign: 'right', fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)' }}>No entries yet</div>
              : filtered.map(entry => (
                  <EntryCard key={entry.id} entry={entry} active={entry.id === activeId} onSelect={setActiveId} />
                ))
            }
          </div>
          {/* Footer */}
          <div style={{ borderTop: '1px solid var(--border)', padding: '10px 0', textAlign: 'right' }}>
            <span style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', letterSpacing: '0.08em' }}>{ENTRIES.length} entries · lore.sustena</span>
          </div>
        </div>

        {/* Reader */}
        <div style={{ overflow: 'hidden' }}>
          {active ? <EntryReader entry={active} /> : (
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-muted)' }}>
              <span className="label-10">No entries yet — Waiting for API</span>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}


export default LorePage;
