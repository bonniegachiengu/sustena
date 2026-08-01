/**
 * apps/web/src/pages/studio/SustainGraph.jsx
 *
 * The Studio's persistent visual representation of the selected Sustain —
 * the seed of the future drag-to-edit structural editor (π : D → G, §4I of
 * SUSTENA_UPGRADE_SPEC.md). Rendered directly from the real definition
 * (GET /devui/sustain/{id}/definition) plus live composition links (GET
 * /devui/sustain/{id}/children) — every count and label on this diagram is
 * real data, nothing decorative.
 *
 * Deliberately honest about scope: this slice draws a hub-and-cluster map
 * (Sustain -> State / Viable region / Operators / Operatives / Composition)
 * with real per-cluster item lists in the side detail panel. It does NOT
 * attempt to infer operator-to-dimension data-flow edges (which ui_schema
 * field bindings could in principle support) — that's real future work,
 * not claimed here.
 */
import React, { useMemo, useState } from 'react';
import { sectionLabel, mutedText, StatusDot } from './shared.jsx';

const CLUSTERS = [
  { key: 'state',       label: 'STATE',          color: 'var(--node-state)',      angle: -90 },
  { key: 'viable',      label: 'VIABLE REGION',  color: 'var(--node-constraint)', angle: -18 },
  { key: 'operators',   label: 'OPERATORS',      color: 'var(--node-operator)',   angle: 54 },
  { key: 'operatives',  label: 'OPERATIVES',     color: 'var(--node-operative)',  angle: 126 },
  { key: 'composition', label: 'COMPOSITION',    color: 'var(--node-event)',      angle: 198 },
];

function polar(cx, cy, r, angleDeg) {
  const a = (angleDeg * Math.PI) / 180;
  return { x: cx + r * Math.cos(a), y: cy + r * Math.sin(a) };
}

function groupOperatorsByNamespace(operators) {
  const groups = {};
  for (const op of operators) {
    const name = typeof op === 'string' ? op : op.name;
    if (!name) continue;
    const ns = name.includes('.') ? name.split('.').slice(0, -1).join('.') : name;
    (groups[ns] = groups[ns] || []).push(name);
  }
  return groups;
}

export default function SustainGraph({ definition, children, loading }) {
  const [selected, setSelected] = useState('state');
  const W = 520, H = 300, CX = 190, CY = 150, R = 108;

  const stateDims = useMemo(() => Object.keys(definition?.state_schema || {}), [definition]);
  const invariants = definition?.invariants || [];
  const operators = definition?.operators || [];
  const operatorGroups = useMemo(() => groupOperatorsByNamespace(operators), [operators]);
  const operatives = Object.keys(definition?.operatives || {});
  const declaredChildren = definition?.declared_children || [];
  const liveChildren = children || [];

  const counts = {
    state: stateDims.length,
    viable: invariants.length,
    operators: operators.length,
    operatives: operatives.length,
    composition: declaredChildren.length || liveChildren.length,
  };

  const hasAny = definition?.found && (
    counts.state || counts.viable || counts.operators || counts.operatives || counts.composition
  );

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 260px', gap: 18, alignItems: 'stretch' }}>
      <div style={{ position: 'relative' }}>
        {loading && (
          <div style={{ ...mutedText, position: 'absolute', top: 6, left: 6, zIndex: 2 }}>loading definition…</div>
        )}
        {!loading && !hasAny && (
          <div style={{ ...mutedText, padding: '60px 0', textAlign: 'center' }}>
            no definition to show — select a sustain
          </div>
        )}
        {!loading && hasAny && (
          <svg viewBox={`0 0 ${W} ${H}`} width="100%" height={H} style={{ overflow: 'visible' }}>
            {/* edges: hub -> each cluster */}
            {CLUSTERS.map(c => {
              const p = polar(CX, CY, R, c.angle);
              const dim = counts[c.key] === 0;
              return (
                <line
                  key={`edge-${c.key}`}
                  x1={CX} y1={CY} x2={p.x} y2={p.y}
                  stroke={c.color} strokeOpacity={dim ? 0.15 : 0.5}
                  strokeWidth={selected === c.key ? 2 : 1}
                />
              );
            })}

            {/* hub */}
            <circle cx={CX} cy={CY} r={30} fill="var(--bg-overlay)" stroke="var(--border-light)" strokeWidth={1.5} />
            <text x={CX} y={CY - 3} textAnchor="middle" fontFamily="var(--mono)" fontSize={8.5} fontWeight={700} fill="var(--text-primary)">
              {(definition?.display_name || definition?.id || 'SUSTAIN').toUpperCase().slice(0, 12)}
            </text>
            <text x={CX} y={CY + 9} textAnchor="middle" fontFamily="var(--mono)" fontSize={7} fill="var(--text-dim)">
              v{definition?.version || '—'}
            </text>

            {/* cluster nodes */}
            {CLUSTERS.map(c => {
              const p = polar(CX, CY, R, c.angle);
              const n = counts[c.key];
              const dim = n === 0;
              const isSel = selected === c.key;
              const radius = 20 + Math.min(n, 8) * 1.6;
              return (
                <g
                  key={c.key}
                  onClick={() => setSelected(c.key)}
                  style={{ cursor: 'pointer' }}
                >
                  <circle
                    cx={p.x} cy={p.y} r={radius}
                    fill={dim ? 'var(--bg-raised)' : c.color}
                    fillOpacity={dim ? 1 : (isSel ? 0.28 : 0.16)}
                    stroke={c.color} strokeOpacity={dim ? 0.3 : 1}
                    strokeWidth={isSel ? 2.5 : 1.4}
                  />
                  <text x={p.x} y={p.y - 3} textAnchor="middle" fontFamily="var(--mono)" fontSize={7.5} fontWeight={700}
                        fill={dim ? 'var(--text-dim)' : 'var(--text-primary)'}>
                    {c.label}
                  </text>
                  <text x={p.x} y={p.y + 9} textAnchor="middle" fontFamily="var(--mono)" fontSize={9.5} fontWeight={700}
                        fill={dim ? 'var(--text-dim)' : c.color}>
                    {n}
                  </text>
                </g>
              );
            })}
          </svg>
        )}
      </div>

      {/* Detail panel — real items for the selected cluster */}
      <div style={{
        borderLeft: '1px solid var(--border)', paddingLeft: 16,
        maxHeight: H, overflowY: 'auto',
      }}>
        {!hasAny && <div style={mutedText}>—</div>}
        {hasAny && selected === 'state' && (
          <>
            <div style={sectionLabel}>state dimensions</div>
            {stateDims.length === 0 && <div style={mutedText}>no dimensions declared</div>}
            {stateDims.map(d => (
              <div key={d} style={rowStyle}>
                <StatusDot status="ok" />
                <span style={rowLabel}>{d}</span>
                <span style={rowSub}>{definition.state_schema[d]?.type || 'object'}</span>
              </div>
            ))}
          </>
        )}
        {hasAny && selected === 'viable' && (
          <>
            <div style={sectionLabel}>viable region — invariants</div>
            {invariants.length === 0 && <div style={mutedText}>no invariants declared</div>}
            {invariants.map(inv => (
              <div key={inv.id} style={{ marginBottom: 10 }}>
                <div style={{ ...rowLabel, marginBottom: 2 }}>{inv.id}</div>
                <div style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--text-secondary)' }}>{inv.expression}</div>
              </div>
            ))}
            {definition?.enforcement?.enabled && (
              <div style={{ ...mutedText, marginTop: 8, color: 'var(--ok)' }}>✓ enforcement enabled — the gate refuses violating writes</div>
            )}
          </>
        )}
        {hasAny && selected === 'operators' && (
          <>
            <div style={sectionLabel}>operators — the transition set T</div>
            {Object.keys(operatorGroups).length === 0 && <div style={mutedText}>none declared</div>}
            {Object.entries(operatorGroups).map(([ns, names]) => (
              <div key={ns} style={{ marginBottom: 8 }}>
                <div style={rowLabel}>{ns}.*  <span style={rowSub}>×{names.length}</span></div>
              </div>
            ))}
          </>
        )}
        {hasAny && selected === 'operatives' && (
          <>
            <div style={sectionLabel}>operatives — the council</div>
            {operatives.length === 0 && <div style={mutedText}>none declared</div>}
            {operatives.map(name => (
              <div key={name} style={rowStyle}>
                <StatusDot status="ok" />
                <span style={rowLabel}>{name}</span>
              </div>
            ))}
          </>
        )}
        {hasAny && selected === 'composition' && (
          <>
            <div style={sectionLabel}>composition ⊕ — declared vs linked</div>
            {declaredChildren.length === 0 && liveChildren.length === 0 && (
              <div style={mutedText}>this sustain has no children</div>
            )}
            {declaredChildren.map(dc => {
              const live = liveChildren.find(c => c.slot === dc.slot);
              return (
                <div key={dc.slot} style={rowStyle}>
                  <StatusDot status={live ? 'live' : 'missing'} />
                  <span style={rowLabel}>{dc.member || dc.slot}</span>
                  <span style={rowSub}>{live ? 'linked' : 'not provisioned'}</span>
                </div>
              );
            })}
          </>
        )}
      </div>
    </div>
  );
}

const rowStyle = { display: 'flex', alignItems: 'center', gap: 4, padding: '4px 0' };
const rowLabel = { fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-primary)' };
const rowSub = { fontFamily: 'var(--mono)', fontSize: 9.5, color: 'var(--text-dim)', marginLeft: 'auto' };
