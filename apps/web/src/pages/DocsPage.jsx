import React, { useState, useEffect, useRef, useMemo, useCallback } from 'react';

const SECTIONS = [
  { id: 'overview',    label: 'Overview',         part: null },
  { id: 'claim',       label: 'The Core Claim',    part: 'Part I' },
  { id: 'loop',        label: 'The Sustena Loop',  part: 'Part I' },
  { id: 'tenets',      label: 'The Tenets',        part: 'Part I' },
  { id: 'primitives',  label: 'Seven Primitives',  part: 'Part IV' },
  { id: 'fractal',     label: 'Fractal Architecture', part: 'Part IV' },
  { id: 'entities',    label: 'Entity Structure',  part: 'Part III' },
  { id: 'roadmap',     label: 'Roadmap Phases',    part: 'Part V' },
  { id: 'references',  label: 'References',        part: null },
];

function Sect({ id, children }) {
  return <section id={id} style={{ scrollMarginTop: 24 }}>{children}</section>;
}

function DocsPage() {
  const [active, setActive] = useState('overview');
  const contentRef = useRef(null);

  const scrollTo = (id) => {
    const el = document.getElementById(id);
    if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    setActive(id);
  };

  useEffect(() => {
    const el = contentRef.current;
    if (!el) return;
    const onScroll = () => {
      const sectionEls = SECTIONS.map(s => document.getElementById(s.id)).filter(Boolean);
      const threshold = 120;
      for (let i = sectionEls.length - 1; i >= 0; i--) {
        if (sectionEls[i].getBoundingClientRect().top <= threshold) {
          setActive(SECTIONS[i].id);
          break;
        }
      }
    };
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => el.removeEventListener('scroll', onScroll);
  }, []);

  const groups = [
    { label: null,      ids: ['overview'] },
    { label: 'Part I — Vision', ids: ['claim', 'loop', 'tenets'] },
    { label: 'Part IV — Architecture', ids: ['primitives', 'fractal'] },
    { label: 'Part III — Entities', ids: ['entities'] },
    { label: 'Part V — Roadmap', ids: ['roadmap'] },
    { label: null,      ids: ['references'] },
  ];

  return (
    <div style={{ height: '100%', background: 'var(--bg-base)' }}>

      <div style={{ display: 'grid', gridTemplateColumns: '240px 1fr', height: '100%', overflow: 'hidden' }}>
        {/* Left TOC — right-aligned, hugging the divider */}
        <aside style={{ borderRight: '1px solid var(--border)', overflowY: 'auto', padding: '16px 0', display: 'flex', flexDirection: 'column' }}>
          <div style={{ fontFamily: 'var(--mono)', fontSize: 8, fontWeight: 500, textTransform: 'uppercase', letterSpacing: '0.12em', color: 'var(--text-dim)', padding: '4px 12px 14px', textAlign: 'right', display: 'block' }}>Sustena XII</div>
          <div style={{ flex: 1 }}>
            {groups.map((g, gi) => (
              <div key={gi} style={{ marginBottom: 4 }}>
                {g.label && (
                  <span style={{
                    fontFamily: 'var(--mono)', fontSize: 9, fontWeight: 500,
                    textTransform: 'uppercase', letterSpacing: '0.12em',
                    color: 'var(--text-dim)', padding: '12px 12px 4px',
                    textAlign: 'right', display: 'block',
                  }}>{g.label}</span>
                )}
                {g.ids.map(id => {
                  const s = SECTIONS.find(s => s.id === id);
                  const isActive = active === id;
                  return (
                    <button
                      key={id}
                      onClick={() => scrollTo(id)}
                      style={{
                        display: 'block', width: '100%',
                        padding: '5px 12px',
                        fontFamily: 'var(--mono)', fontSize: 10,
                        letterSpacing: '0.06em',
                        color: isActive ? 'var(--text-primary)' : 'var(--text-muted)',
                        background: isActive ? 'var(--bg-raised)' : 'none',
                        border: 'none', borderRadius: 'var(--radius-sm)',
                        cursor: 'pointer', textAlign: 'right',
                        transition: 'all var(--t-fast)',
                      }}
                      onMouseEnter={e => { if (!isActive) e.currentTarget.style.color = 'var(--text-secondary)'; }}
                      onMouseLeave={e => { if (!isActive) e.currentTarget.style.color = 'var(--text-muted)'; }}
                    >
                      {s?.label}
                    </button>
                  );
                })}
              </div>
            ))}
          </div>
          <div style={{ padding: '12px 12px', borderTop: '1px solid var(--border)', textAlign: 'right' }}>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 8, color: 'var(--text-dim)', lineHeight: 1.7, letterSpacing: '0.06em' }}>v1.0 · May 2026<br />CONFIDENTIAL<br />365+ Ventures</div>
          </div>
        </aside>

        {/* Main content */}
        <div ref={contentRef} style={{ overflowY: 'auto', padding: '48px 64px 120px' }}>
          <div style={{ maxWidth: 'var(--content-width)' }}>

            {/* ── Overview ── */}
            <Sect id="overview">
              <div className="label-10" style={{ marginBottom: 16, color: 'var(--teal)' }}>SUSTENA XII · MASTER STRATEGY</div>
              <h1 className="doc-h1">Sustena — Infrastructure for Governed Intelligence</h1>
              <p className="doc-lead">
                Building the infrastructure for intelligent systems that enable complex life to flourish: within itself, and within the solar system it inhabits.
              </p>
              <div className="doc-quote">
                <p>"The beginning of infinity is the moment a system acquires the capacity to correct its own errors. Sustena is that moment — applied to every system that can be described."</p>
                <cite>— After David Deutsch, The Beginning of Infinity</cite>
              </div>
              <p className="doc-p">
                This document is the complete synthesis of Sustena, Colosso, Vyyb, and Homestead into a unified strategy — Sustena as the human-agent reality interface and computation network, with Colosso (financial intelligence), Vyyb (food business intelligence), and Homestead (household intelligence) as its first three sustains.
              </p>
              <p className="doc-p">
                <strong style={{ color: 'var(--text-primary)' }}>Founders:</strong> Bonnie Gachiengu (systems architect, Sustena + Vyyb), Brian Gitonga (finance, Colosso), Kang'iri (strategic partner). Holding entity: 365+ Ventures.
              </p>
              <div className="doc-divider" />
            </Sect>

            {/* ── Core Claim ── */}
            <Sect id="claim">
              <div className="label-10" style={{ marginBottom: 10, color: 'var(--amber)' }}>PART I · 1.1</div>
              <h2 className="doc-h2">The Core Claim</h2>
              <p className="doc-p">
                Any system that can be described can be modelled, simulated, and optimised before execution.
              </p>
              <p className="doc-p">
                This is not a product tagline. It is a physical and computational claim — and Sustena is the infrastructure that makes it actionable for any system, anywhere, at any scale.
              </p>
              <p className="doc-p">
                Where most platforms build tools for specific domains, Sustena builds the language in which all tools are expressed. Where most AI products automate tasks, Sustena builds the environment in which intelligent agents simulate, decide, and act within governed constraints. Where most Web3 projects tokenise assets, Sustena governs entire systems — their operators, their state transitions, their agents, and their futures — through programmable consensus.
              </p>
              <p className="doc-p">
                Sustena is the <strong style={{ color: 'var(--text-primary)' }}>human-agent reality interface</strong>. It makes the invisible architecture of complex systems visible, simulatable, and governable — for a smallholder farmer in Murang'a, a chama secretary in Ruiru, a food entrepreneur in Githurai, and an investor reviewing a portfolio in Westlands.
              </p>
              <div className="doc-divider" />
            </Sect>

            {/* ── The Sustena Loop ── */}
            <Sect id="loop">
              <div className="label-10" style={{ marginBottom: 10, color: 'var(--amber)' }}>PART I · 1.3</div>
              <h2 className="doc-h2">The Sustena Loop</h2>
              <code className="doc-code">DESCRIBE → MODEL → SIMULATE → OPTIMISE → EXECUTE → OBSERVE → (repeat)</code>
              <p className="doc-p">
                Every Sustain (any described system) lives inside this loop. Orchie (the Orchestrator Operative) drives the loop for the user. The Council (a DAO of user-aligned operatives) governs which strategies advance from simulation to execution. Operators execute the state transitions. The Mycelium (the Sustena network) provides the infrastructure.
              </p>
              <p className="doc-p">This loop is simultaneously:</p>
              <table className="doc-table">
                <tbody>
                  <tr><td><strong>Product model</strong></td><td>User describes a goal → Orchie simulates paths → Council votes → execution happens</td></tr>
                  <tr><td><strong>Software pattern</strong></td><td>Spec → fork state → run scenarios → commit or discard</td></tr>
                  <tr><td><strong>Governance model</strong></td><td>Proposals → simulation evidence → DAO vote → implementation</td></tr>
                  <tr><td><strong>Scientific method</strong></td><td>Hypothesis → model → test → update</td></tr>
                </tbody>
              </table>
              <div className="doc-divider" />
            </Sect>

            {/* ── Tenets ── */}
            <Sect id="tenets">
              <div className="label-10" style={{ marginBottom: 10, color: 'var(--amber)' }}>PART I · 1.4</div>
              <h2 className="doc-h2">The Sustena Tenets</h2>
              <p className="doc-p" style={{ marginBottom: 28 }}>Sustena's constitution. They define the relationship between intelligence, autonomy, privacy, and collective governance.</p>
              {[
                { n: 1, h: 'Sustenance, privacy, and free will are rights of every sentient being.', b: 'Every user owns their data, their agents, their governance votes, and their simulated futures. Sustena builds no moat from user lock-in. The system\'s value is the network — and the network is strongest when every participant is free.' },
                { n: 2, h: 'You cannot have privacy without security.', b: 'Sustena\'s privacy architecture is zero-knowledge by design. Operatives process anonymised state, not raw personal data. M-Pesa message content is parsed on-device and only structured metadata reaches the cloud.' },
                { n: 3, h: 'You cannot have security without accountability.', b: 'Every state transition in a Sustain is an immutable event in the event log. Every operative action is signed, timestamped, and traceable. In a world where AI agents act on users\' behalf with real financial consequences, accountability is not a compliance requirement — it is the product.' },
                { n: 4, h: 'You cannot have accountability without transparency.', b: 'Orchie explains every strategy it proposes in plain language. The simulation evidence behind every recommendation is visible. Operative logic — the why behind every action — is auditable by the user.' },
                { n: 5, h: 'You cannot have transparency without surveillance.', b: 'Sustena\'s "surveillance" is consensual, self-directed, and owned by the user. The sensors are M-Pesa messages, IoT devices, bank statements, and calendar entries — all opted into explicitly. The user is the watcher and the watched.' },
                { n: 6, h: 'You cannot have surveillance in a free world without trust.', b: 'Trust in Sustena is programmable. It is the Chama Cred score, the Operative Trust Rating, the DAO voting record, and the smart contract execution history. Trust is not claimed — it is demonstrated, recorded, and composable.' },
                { n: 7, h: 'You cannot have trust without informed consent.', b: 'Every operative deployed in a user\'s Sustain requires explicit authorisation. The Council deliberates; the user decides. An operative can be highly confident, backed by strong simulation evidence, and still be vetoed. This is not a bug — it is the consent mechanism encoded into the governance layer.' },
                { n: 8, h: 'Sustenance, privacy, and free will are only possible with mutual interdependence.', b: 'The Sustena network is strongest when its participants are interdependent — when a chama secretary\'s Orchie can coordinate with members\' Orchies, when a Mkulima farmer\'s buyer pipeline connects to a Biashara merchant\'s procurement operative.' },
              ].map(t => (
                <div key={t.n} className="tenet">
                  <h3 className="doc-h3" style={{ marginTop: 0 }}>Tenet {t.n} — {t.h}</h3>
                  <p className="doc-p" style={{ marginBottom: 0 }}>{t.b}</p>
                </div>
              ))}
              <div className="doc-divider" />
            </Sect>

            {/* ── Seven Primitives ── */}
            <Sect id="primitives">
              <div className="label-10" style={{ marginBottom: 10, color: 'var(--teal)' }}>PART IV · 4.1</div>
              <h2 className="doc-h2">The Seven Primitives</h2>
              <p className="doc-p">All Sustena computation reduces to seven primitives. These are not design decisions — they are the minimum necessary vocabulary for describing any real-world system.</p>
              <table className="doc-table">
                <thead>
                  <tr><th>Primitive</th><th>Definition</th><th>Implementation</th></tr>
                </thead>
                <tbody>
                  {[
                    { p: 'State',      d: 'The complete structured representation of a system at a point in time',     i: 'Typed nested key-value tree; dot-path access; snapshot-able' },
                    { p: 'Operator',   d: 'A deterministic, composable unit of state transformation',                  i: 'Pure function: (State, Inputs, Logic) → (Delta, Events)' },
                    { p: 'Constraint', d: 'A declarative predicate that must hold for a transition to be valid',       i: 'DSL expressions evaluated pre- and post-execution' },
                    { p: 'Event',      d: 'An immutable, structured record of a state transition',                    i: 'Append-only event log; the ground truth of the system' },
                    { p: 'Time',       d: 'The ordering and progression mechanism governing when transitions occur',   i: 'Vector clocks + wall clock; simulation uses accelerated time' },
                    { p: 'Consensus',  d: 'The mechanism by which participants agree on shared state',                i: 'DAO voting; Nash bargaining; smart contract enforcement (mocked → live)' },
                    { p: 'Operative',  d: 'An autonomous agent that observes state and generates strategies',        i: 'LLM + tool use + constraint-bounded execution' },
                  ].map(row => (
                    <tr key={row.p}><td><strong>{row.p}</strong></td><td>{row.d}</td><td style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--teal)' }}>{row.i}</td></tr>
                  ))}
                </tbody>
              </table>
              <div className="doc-divider" />
            </Sect>

            {/* ── Fractal Architecture ── */}
            <Sect id="fractal">
              <div className="label-10" style={{ marginBottom: 10, color: 'var(--teal)' }}>PART IV · 4.3</div>
              <h2 className="doc-h2">The Fractal Architecture</h2>
              <p className="doc-p">Every concept in Sustena is self-similar at all scales. This is not aesthetic — it is architectural necessity for infinite extensibility.</p>
              <code className="doc-code">{`Level 0 — The Sustenaverse (all sustains)
Level 1 — Sustain (a described system: Homestead, Vyyb, Colosso)
Level 2 — Microsustain (a subsystem: the pantry, the chama, the farm plot)
Level 3 — Nanosustain (a subsubsystem: a specific budget pocket, a crop cycle, a recipe batch)`}</code>
              <p className="doc-p">Each level has its own State, Operators, Constraints, Events, Time budget, Consensus mechanism, and Operative roster. A Microsustain can recursively contain its own Microsustains. The engine is identical at every level — only the spec changes.</p>
              <code className="doc-code">{`Physical Reality ←— sensors/controllers —→ Sustenaverse Digital Twin
                                          ├── Homestead Sustain
                                          │   ├── Budget Microsustain
                                          │   ├── Chama Microsustain
                                          │   └── Pantry/IoT Microsustain
                                          ├── Vyyb Sustain
                                          │   ├── Outlet Microsustain
                                          │   └── The Hive Microsustain
                                          └── Colosso Sustain
                                              ├── Personal Finance Microsustain
                                              └── Mkulima Microsustain`}</code>
              <p className="doc-p">Operatives live entirely within forked simulation states — they never directly mutate the real sustain. Only approved, consensus-validated strategies reach execution. This is the key safety guarantee.</p>
              <div className="doc-divider" />
            </Sect>

            {/* ── Entity Structure ── */}
            <Sect id="entities">
              <div className="label-10" style={{ marginBottom: 10, color: 'var(--info)' }}>PART III · 3.1</div>
              <h2 className="doc-h2">Entity Structure</h2>
              <code className="doc-code">{`365+ Ventures (Holding)
├── Sustena        — technology contractor (Bonnie 51%)
│   ├── The Mycelium  — open source network protocol
│   └── Sustena Platform — proprietary intelligence layer
├── Colosso Finance — financial intelligence (Brian 51%)
│   ├── Chama         — DAO sustain product
│   └── Biashara      — SME intelligence
├── Vyyb            — food business intelligence (Bonnie 51%)
│   └── The Hive      — delivery + procurement system
├── Homestead       — household management (Bonnie + Brian)
└── Lore            — public knowledge sustain (lore.dev)`}</code>
              <table className="doc-table">
                <thead><tr><th>Entity</th><th>365+ Ventures</th><th>Hero Founder</th><th>Notes</th></tr></thead>
                <tbody>
                  <tr><td><strong>Sustena</strong></td><td>49%</td><td>Bonnie — 51%</td><td>Platform contractor for all sustains</td></tr>
                  <tr><td><strong>Colosso Finance</strong></td><td>49%</td><td>Brian — 51%</td><td>Includes Chama DAO product</td></tr>
                  <tr><td><strong>Vyyb</strong></td><td>49%</td><td>Bonnie — 51%</td><td>Reference sustain; Biashara proof-of-concept</td></tr>
                </tbody>
              </table>
              <p className="doc-p">When external investors enter, dilution applies proportionally. The hero founder must never fall below 51% in their primary sustain without a special majority resolution of all three founders.</p>
              <div className="doc-divider" />
            </Sect>

            {/* ── Roadmap ── */}
            <Sect id="roadmap">
              <div className="label-10" style={{ marginBottom: 10, color: 'var(--ok)' }}>PART V</div>
              <h2 className="doc-h2">Roadmap Phases</h2>
              {[
                { phase: 'Phase 0', label: 'WhatsApp Validation', status: 'ACTIVE', color: 'var(--ok)', duration: 'Weeks 1–4', items: ['WhatsApp bot: M-Pesa parsing + Orchie intent classification', 'Budget pocket tracking for Homestead and Vyyb', 'Orchie sends constraint-bounded proposals via voice note', 'Milestone: 50 active users with real transaction data'] },
                { phase: 'Phase 1', label: 'Platform + Mycelium Alpha', status: 'NEXT', color: 'var(--amber)', duration: 'Weeks 5–16', items: ['Sustena web app (this prototype) → production build', 'Council DAO: first live governance votes', 'Mycelium Alpha: 5 first-party operative packages published', 'Vyyb full deployment: delivery dispatch + procurement', 'Kenyan entity incorporation via eCitizen'] },
                { phase: 'Phase 2', label: 'Network + Governance', status: 'PLANNED', color: 'var(--info)', duration: 'Month 4–9', items: ['Arena public launch: community can publish operators + operatives', 'Chama DAO sustain: chama-specific governance', 'Mkulima sustain + produce marketplace in Arena', 'DAO governance overlay (Cooperative Society or Association)', 'ODPC registration as Data Controller'] },
                { phase: 'Phase 3', label: 'Scale + Licensing', status: 'FUTURE', color: 'var(--text-muted)', duration: 'Month 9+', items: ['CBK Digital Credit Provider licence (for Colosso credit products)', 'Daraja production M-Pesa integration', 'IoT integrations: soil sensors, smart meters, inventory scales', 'Regional expansion beyond Kenya'] },
              ].map(ph => (
                <div key={ph.phase} style={{ marginBottom: 28, padding: 20, border: '1px solid var(--border)', borderLeft: `2px solid var(--border-light)`, borderRadius: 'var(--radius-md)' }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 6 }}>
                    <div style={{ display: 'flex', gap: 10, alignItems: 'baseline' }}>
                      <span style={{ fontFamily: 'var(--mono)', fontSize: 11, fontWeight: 500, color: 'var(--text-muted)' }}>{ph.phase}</span>
                      <span style={{ fontFamily: 'var(--ui)', fontSize: 14, fontWeight: 600, color: 'var(--text-primary)' }}>{ph.label}</span>
                    </div>
                    <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-dim)', letterSpacing: '0.08em', textTransform: 'uppercase' }}>{ph.duration}</span>
                      <span style={{ fontFamily: 'var(--mono)', fontSize: 9, color: 'var(--text-muted)', border: '1px solid var(--border-mid)', padding: '1px 6px', borderRadius: 8, letterSpacing: '0.08em' }}>{ph.status}</span>
                    </div>
                  </div>
                  <ul style={{ paddingLeft: 0, listStyle: 'none', display: 'flex', flexDirection: 'column', gap: 4 }}>
                    {ph.items.map((item, i) => (
                      <li key={i} style={{ display: 'flex', gap: 8, fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-muted)', lineHeight: 1.5 }}>
                        <span style={{ color: 'var(--text-dim)', flexShrink: 0, marginTop: 1 }}>·</span>
                        {item}
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
              <div className="doc-divider" />
            </Sect>

            {/* ── References ── */}
            <Sect id="references">
              <div className="label-10" style={{ marginBottom: 10 }}>REFERENCES</div>
              <h2 className="doc-h2">References</h2>
              {[
                '[1] David Deutsch, The Beginning of Infinity (2011)',
                '[2] David Deutsch, The Fabric of Reality (1997)',
                '[3] Category theory: MacLane, Categories for the Working Mathematician (1971)',
                '[4] Prediction Machine: Agrawal, Gans & Goldfarb, Prediction Machines (2018)',
                '[5] Shannon, A Mathematical Theory of Communication (1948)',
                '[8] Feynman, Space-Time Approach to Non-Relativistic Quantum Mechanics (1948)',
                '[11] Nash, Equilibrium Points in N-Person Games (1950)',
                '[12] Nash, The Bargaining Problem (1950)',
                '[14] Pete Docter & Ronaldo del Carmen, Inside Out (2015)',
                '[17] Petri, Communication with Automata (1962)',
              ].map((ref, i) => (
                <p key={i} style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-muted)', lineHeight: 1.6, marginBottom: 6 }}>{ref}</p>
              ))}
            </Sect>

          </div>
        </div>
      </div>
    </div>
  );
}


export default DocsPage;
