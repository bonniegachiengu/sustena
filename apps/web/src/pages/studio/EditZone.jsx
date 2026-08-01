/**
 * apps/web/src/pages/studio/EditZone.jsx
 *
 * Inspect-only, and said so plainly. §4I (Editing engine) — changing a
 * definition safely while it runs (typed edits, migration predicate,
 * fork+sim before commit, edit-authority token) is real, scoped work for
 * the NEXT slice. This slice reads the real definition
 * (GET /devui/sustain/{id}/definition) and renders it — nothing here can
 * mutate the definition.
 */
import React from 'react';
import { sectionLabel, mutedText, Empty } from './shared.jsx';

export default function EditZone({ definition, loading }) {
  if (loading) return <Empty text="loading definition…" />;
  if (!definition?.found) return <Empty text="no definition to inspect — select a sustain" />;

  const stateSchema = definition.state_schema || {};
  const invariants = definition.invariants || [];
  const operators = definition.operators || [];
  const operatives = Object.keys(definition.operatives || {});
  const curatedWidgets = definition.curated_widgets || [];

  return (
    <div>
      <div style={{
        padding: '10px 14px', borderRadius: 'var(--radius-sm)', marginBottom: 20,
        background: 'var(--amber-glow)', border: '1px solid var(--amber-border)',
        fontFamily: 'var(--ui)', fontSize: 12, color: 'var(--text-secondary)',
      }}>
        <b style={{ color: 'var(--amber)' }}>inspect only</b> — this reads the real definition, it cannot change it yet.
        The drag-to-edit structural editor (π : D → G) is scoped for the next Studio slice.
      </div>

      <div style={{ marginBottom: 4, fontFamily: 'var(--mono)', fontSize: 13, fontWeight: 700, color: 'var(--text-primary)' }}>
        {definition.display_name || definition.id}
      </div>
      <div style={{ ...mutedText, marginBottom: 18 }}>
        {definition.description || 'no description declared'} · v{definition.version || '—'}
      </div>

      <div style={{ marginBottom: 20 }}>
        <div style={sectionLabel}>state schema — S</div>
        {Object.entries(stateSchema).length === 0 && <div style={mutedText}>no dimensions declared</div>}
        {Object.entries(stateSchema).map(([name, def]) => (
          <div key={name} style={{ padding: '6px 0', borderBottom: '1px solid var(--border)' }}>
            <div style={{ display: 'flex', gap: 10, alignItems: 'baseline' }}>
              <span style={{ fontFamily: 'var(--ui)', fontSize: 12.5, color: 'var(--text-primary)' }}>{name}</span>
              <span style={{ fontFamily: 'var(--mono)', fontSize: 10, color: 'var(--node-state)' }}>{def?.type || 'object'}</span>
            </div>
            {def?.description && <div style={{ ...mutedText, fontSize: 11, marginTop: 2 }}>{def.description}</div>}
          </div>
        ))}
      </div>

      <div style={{ marginBottom: 20 }}>
        <div style={sectionLabel}>viable region V — invariants</div>
        {invariants.length === 0 && <div style={mutedText}>no invariants declared</div>}
        {invariants.map(inv => (
          <div key={inv.id} style={{ padding: '6px 0', borderBottom: '1px solid var(--border)' }}>
            <div style={{ fontFamily: 'var(--ui)', fontSize: 12.5, color: 'var(--text-primary)' }}>{inv.id}</div>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 10.5, color: 'var(--node-constraint)', marginTop: 2 }}>{inv.expression}</div>
            {inv.description && <div style={{ ...mutedText, fontSize: 11, marginTop: 2 }}>{inv.description}</div>}
          </div>
        ))}
        <div style={{ ...mutedText, marginTop: 6 }}>
          enforcement: {definition.enforcement?.enabled ? <span style={{ color: 'var(--ok)' }}>enabled — the gate refuses violating writes</span> : 'not enabled for this sustain'}
        </div>
      </div>

      <div style={{ marginBottom: 20 }}>
        <div style={sectionLabel}>operators — T ({operators.length})</div>
        {operators.length === 0 && <div style={mutedText}>none declared</div>}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))', gap: 8 }}>
          {operators.map(op => {
            const name = typeof op === 'string' ? op : op.name;
            const desc = typeof op === 'string' ? '' : op.description;
            return (
              <div key={name} style={{
                background: 'var(--bg-raised)', border: '1px solid var(--border)',
                borderRadius: 'var(--radius-sm)', padding: '8px 10px',
              }}>
                <div style={{ fontFamily: 'var(--mono)', fontSize: 10.5, color: 'var(--node-operator)' }}>{name}</div>
                {desc && <div style={{ ...mutedText, fontSize: 10, marginTop: 2 }}>{desc}</div>}
              </div>
            );
          })}
        </div>
      </div>

      <div style={{ marginBottom: 20 }}>
        <div style={sectionLabel}>operatives — the council ({operatives.length})</div>
        {operatives.length === 0 && <div style={mutedText}>none declared</div>}
        {operatives.map(name => (
          <span key={name} style={{
            display: 'inline-block', marginRight: 8, marginBottom: 8,
            fontFamily: 'var(--mono)', fontSize: 10.5, color: 'var(--node-operative)',
            border: '1px solid var(--teal-border)', background: 'var(--teal-glow)',
            borderRadius: 'var(--radius-sm)', padding: '4px 9px',
          }}>
            {name}
          </span>
        ))}
      </div>

      <div>
        <div style={sectionLabel}>curated widgets — w = ⟨inputs, render, emits⟩</div>
        {curatedWidgets.length === 0 && <div style={mutedText}>none declared</div>}
        {curatedWidgets.map(w => (
          <div key={w.id} style={{ padding: '8px 0', borderBottom: '1px solid var(--border)' }}>
            <div style={{ fontFamily: 'var(--ui)', fontSize: 12.5, color: 'var(--text-primary)' }}>{w.id}</div>
            <div style={{ ...mutedText, fontSize: 11, marginTop: 2 }}>{w.description}</div>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 9.5, color: 'var(--text-dim)', marginTop: 4 }}>
              {w.event_class ? `on ${w.event_class}` : 'always eligible'} · render {w.render} · emits {(w.emits || []).join(', ') || 'none'}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
