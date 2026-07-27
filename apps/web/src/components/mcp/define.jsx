import React from 'react';
import { api } from '../../lib/api.js';
/* DEFINE panel — Slice 6, create/definition flow.
   A non-technical person builds their own sustain: dimensions (state
   schema), invariants (viable region), and which existing operators to
   attach — no hand-written JSON. Everything created here instantiates
   through the exact same POST /devui/sustains + SustainEngine.instantiate()
   path as homestead/habitat (see sustain_engine.py's _load_spec()). */

const { useState: dUseState, useEffect: dUseEffect, useMemo: dUseMemo, useRef: dUseRef } = React;

const DIMENSION_TYPES = ['number', 'string', 'boolean'];
const COMPARATORS = ['>=', '<=', '>', '<', '==', '!='];

function newDimension() {
  return { key: Math.random().toString(36).slice(2), name: '', type: 'number', description: '', default_value: '', minimum: '' };
}

function newInvariantRow(dimensions) {
  return {
    key: Math.random().toString(36).slice(2),
    id: '', field: dimensions[0]?.name || '', comparator: '>=', value: '', description: '',
    _valid: null, _errors: [],
  };
}

// Build the same flat state_schema shape SustainEngine._build_spec_dict
// produces, purely for live invariant validation against candidate
// dimensions before they've been saved.
function dimensionsToStateSchema(dimensions) {
  const schema = {};
  dimensions.forEach(d => {
    if (!d.name) return;
    schema[d.name] = { type: d.type, description: d.description || '' };
  });
  return schema;
}

function composeExpression(field, comparator, value, fieldType) {
  if (!field) return '';
  let literal = value;
  if (fieldType === 'string') literal = `'${String(value).replace(/'/g, "\\'")}'`;
  else if (fieldType === 'boolean') literal = value ? 'true' : 'false';
  return `${field} ${comparator} ${literal}`;
}

/* ─── Shared bits ─────────────────────────────────────────────────────── */

function EmptyLine({ children }) {
  return <div style={{ padding: 14 }}><span className="meta-10" style={{ color: 'var(--text-dim)' }}>{children}</span></div>;
}

function FieldLabel({ children }) {
  return <label className="label-11" style={{ display: 'block', marginBottom: 4, color: 'var(--text-muted)' }}>{children}</label>;
}

const inputStyle = {
  width: '100%', background: 'var(--bg-base)', border: '1px solid var(--border)',
  borderRadius: 6, padding: '7px 9px', fontFamily: 'var(--mono)', fontSize: 12,
  color: 'var(--text-primary)', outline: 'none',
};

const btnStyle = (tone = 'muted') => ({
  fontFamily: 'var(--mono)', fontSize: 10, letterSpacing: '0.04em', textTransform: 'uppercase',
  padding: '6px 12px', borderRadius: 6, cursor: 'pointer', border: '1px solid var(--border)',
  background: tone === 'amber' ? 'var(--amber)' : tone === 'danger' ? 'transparent' : 'var(--bg-surface)',
  color: tone === 'amber' ? 'var(--bg-base)' : tone === 'danger' ? 'var(--danger)' : 'var(--text-secondary)',
  borderColor: tone === 'danger' ? 'var(--danger)' : 'var(--border)',
});

/* ─── Definitions list ────────────────────────────────────────────────── */

function DefinitionsList({ definitions, loading, onNew, onEdit, onInstantiate, instantiatingId }) {
  return (
    <div style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 'var(--radius-md)', overflow: 'hidden' }}>
      <div style={{ padding: '10px 14px', borderBottom: '1px solid var(--border)', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <span className="label-11">MY DEFINITIONS</span>
        <button style={btnStyle('amber')} onClick={onNew}>+ NEW DEFINITION</button>
      </div>
      {loading ? (
        <EmptyLine>loading…</EmptyLine>
      ) : definitions.length === 0 ? (
        <EmptyLine>no definitions yet · create one above</EmptyLine>
      ) : (
        <div>
          {definitions.map((d, i) => (
            <div key={d.template_id} style={{
              display: 'flex', alignItems: 'center', gap: 12, padding: '12px 14px',
              borderBottom: i < definitions.length - 1 ? '1px solid var(--border)' : 'none',
            }}>
              <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 3 }}>
                <span style={{ fontFamily: 'var(--mono)', fontSize: 12, fontWeight: 600, color: 'var(--text-primary)' }}>
                  {d.display_name}
                </span>
                <span className="meta-10" style={{ color: 'var(--text-muted)' }}>
                  {d.dimension_count} dimension{d.dimension_count === 1 ? '' : 's'} · {d.invariant_count} invariant{d.invariant_count === 1 ? '' : 's'} · {d.operator_count} operator{d.operator_count === 1 ? '' : 's'} · v{d.version}
                </span>
              </div>
              <button style={btnStyle()} onClick={() => onEdit(d.template_id)}>EDIT</button>
              <button style={btnStyle('amber')} disabled={instantiatingId === d.template_id} onClick={() => onInstantiate(d.template_id)}>
                {instantiatingId === d.template_id ? 'CREATING…' : 'CREATE SUSTAIN'}
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/* ─── Dimension row ───────────────────────────────────────────────────── */

function DimensionRow({ dim, onChange, onRemove }) {
  return (
    <div style={{ display: 'grid', gridTemplateColumns: '1.2fr 0.8fr 1fr 0.8fr auto', gap: 8, alignItems: 'start', padding: '8px 0', borderBottom: '1px solid var(--border)' }}>
      <div>
        <FieldLabel>NAME</FieldLabel>
        <input style={inputStyle} placeholder="moisture_level" value={dim.name}
          onChange={e => onChange({ ...dim, name: e.target.value.trim() })} />
      </div>
      <div>
        <FieldLabel>TYPE</FieldLabel>
        <select style={inputStyle} value={dim.type} onChange={e => onChange({ ...dim, type: e.target.value })}>
          {DIMENSION_TYPES.map(t => <option key={t} value={t}>{t}</option>)}
        </select>
      </div>
      <div>
        <FieldLabel>DEFAULT VALUE</FieldLabel>
        <input style={inputStyle} placeholder={dim.type === 'number' ? '0' : dim.type === 'boolean' ? 'false' : ''}
          value={dim.default_value} onChange={e => onChange({ ...dim, default_value: e.target.value })} />
      </div>
      <div>
        <FieldLabel>{dim.type === 'number' ? 'MINIMUM' : ' '}</FieldLabel>
        {dim.type === 'number'
          ? <input style={inputStyle} placeholder="none" value={dim.minimum} onChange={e => onChange({ ...dim, minimum: e.target.value })} />
          : null}
      </div>
      <div style={{ paddingTop: 18 }}>
        <button style={btnStyle('danger')} onClick={onRemove}>REMOVE</button>
      </div>
    </div>
  );
}

/* ─── Invariant row — field/operator/value builder, never raw DSL typing ── */

function InvariantRow({ inv, dimensions, onChange, onRemove, stateSchema }) {
  const field = dimensions.find(d => d.name === inv.field);
  const fieldType = field?.type || 'number';
  const validateTimer = dUseRef(null);

  dUseEffect(() => {
    if (!inv.field || inv.value === '') { onChange({ ...inv, _valid: null, _errors: [] }); return; }
    clearTimeout(validateTimer.current);
    validateTimer.current = setTimeout(async () => {
      const expression = composeExpression(inv.field, inv.comparator, inv.value, fieldType);
      try {
        const d = await api.post('/devui/validate-invariant', { expression, state_schema: stateSchema });
        onChange({ ...inv, _valid: !!d?.data?.valid, _errors: d?.data?.errors || [] });
      } catch {
        onChange({ ...inv, _valid: null, _errors: [] });
      }
    }, 350);
    return () => clearTimeout(validateTimer.current);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inv.field, inv.comparator, inv.value, JSON.stringify(stateSchema)]);

  return (
    <div style={{ padding: '10px 0', borderBottom: '1px solid var(--border)' }}>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr 1.2fr auto', gap: 8, alignItems: 'start' }}>
        <div>
          <FieldLabel>ID</FieldLabel>
          <input style={inputStyle} placeholder="moisture_non_negative" value={inv.id}
            onChange={e => onChange({ ...inv, id: e.target.value.trim() })} />
        </div>
        <div>
          <FieldLabel>FIELD</FieldLabel>
          <select style={inputStyle} value={inv.field} onChange={e => onChange({ ...inv, field: e.target.value })}>
            {dimensions.length === 0 && <option value="">— no dimensions yet —</option>}
            {dimensions.map(d => <option key={d.name} value={d.name}>{d.name || '(unnamed)'}</option>)}
          </select>
        </div>
        <div>
          <FieldLabel>MUST BE</FieldLabel>
          <select style={inputStyle} value={inv.comparator} onChange={e => onChange({ ...inv, comparator: e.target.value })}>
            {COMPARATORS.map(c => <option key={c} value={c}>{c}</option>)}
          </select>
        </div>
        <div>
          <FieldLabel>VALUE</FieldLabel>
          {fieldType === 'boolean' ? (
            <select style={inputStyle} value={inv.value} onChange={e => onChange({ ...inv, value: e.target.value })}>
              <option value="">—</option>
              <option value="true">true</option>
              <option value="false">false</option>
            </select>
          ) : (
            <input style={inputStyle} value={inv.value} onChange={e => onChange({ ...inv, value: e.target.value })} />
          )}
        </div>
        <div style={{ paddingTop: 18 }}>
          <button style={btnStyle('danger')} onClick={onRemove}>REMOVE</button>
        </div>
      </div>
      <div style={{ marginTop: 6 }}>
        <input style={{ ...inputStyle, fontSize: 11 }} placeholder="description (optional)" value={inv.description}
          onChange={e => onChange({ ...inv, description: e.target.value })} />
      </div>
      {inv.field && inv.value !== '' && (
        <div style={{ marginTop: 6 }}>
          {inv._valid === true && <span className="meta-10" style={{ color: 'var(--ok, #4ade80)' }}>✓ valid — {composeExpression(inv.field, inv.comparator, inv.value, fieldType)}</span>}
          {inv._valid === false && <span className="meta-10" style={{ color: 'var(--danger)' }}>✗ {inv._errors.join('; ') || 'invalid'}</span>}
        </div>
      )}
    </div>
  );
}

/* ─── Operator picker — plain add/remove list, not draggable.
   Drag-and-drop operator cards (per the Curated-UI reference) would be a
   nicer interaction but is real extra weight for this walking skeleton —
   a searchable attach/detach list is the deliberate, disclosed substitute. */

function OperatorPicker({ registry, attached, onToggle }) {
  const [query, setQuery] = dUseState('');
  const names = dUseMemo(() => Object.keys(registry).sort(), [registry]);
  const filtered = dUseMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return names;
    return names.filter(n => n.toLowerCase().includes(q) || (registry[n]?.description || '').toLowerCase().includes(q));
  }, [names, query, registry]);

  return (
    <div>
      <input style={{ ...inputStyle, marginBottom: 10 }} placeholder="search operators…" value={query} onChange={e => setQuery(e.target.value)} />
      <div style={{ maxHeight: 260, overflowY: 'auto', border: '1px solid var(--border)', borderRadius: 6 }}>
        {filtered.length === 0 ? (
          <EmptyLine>no operators match</EmptyLine>
        ) : filtered.map((name, i) => {
          const meta = registry[name] || {};
          const isAttached = attached.has(name);
          return (
            <div key={name} style={{
              display: 'flex', alignItems: 'center', gap: 10, padding: '8px 10px',
              borderBottom: i < filtered.length - 1 ? '1px solid var(--border)' : 'none',
            }}>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontFamily: 'var(--mono)', fontSize: 11, color: 'var(--text-primary)' }}>{name}</div>
                <div className="meta-10" style={{ color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {meta.description || '—'} · pawa {meta.pawa_cost ?? 0}
                </div>
              </div>
              <button style={btnStyle(isAttached ? 'danger' : 'amber')} onClick={() => onToggle(name)}>
                {isAttached ? 'DETACH' : 'ATTACH'}
              </button>
            </div>
          );
        })}
      </div>
      <div className="meta-10" style={{ color: 'var(--text-muted)', marginTop: 8 }}>
        {attached.size} operator{attached.size === 1 ? '' : 's'} attached
      </div>
    </div>
  );
}

/* ─── Builder — create or edit a definition ──────────────────────────── */

function DefinitionBuilder({ editingId, initialSpec, opRegistry, userId, onSaved, onCancel }) {
  const [displayName, setDisplayName] = dUseState(initialSpec?.display_name || '');
  const [description, setDescription] = dUseState(initialSpec?.description || '');
  const [dimensions, setDimensions] = dUseState(() => {
    if (!initialSpec) return [newDimension()];
    const schema = initialSpec.state_schema || {};
    const defaults = initialSpec.default_state || {};
    const rows = Object.entries(schema).map(([name, node]) => ({
      key: Math.random().toString(36).slice(2), name, type: node.type || 'number',
      description: node.description || '', default_value: defaults[name] ?? '', minimum: node.minimum ?? '',
    }));
    return rows.length ? rows : [newDimension()];
  });
  const [invariants, setInvariants] = dUseState(() => {
    if (!initialSpec) return [];
    return (initialSpec.invariants || []).map(inv => {
      // Best-effort decompose "field OP value" back into the structured
      // row shape for editing; falls back to a raw, non-decomposed row
      // (still saved as-is) if the expression doesn't match that shape —
      // this only affects re-editing convenience, not correctness.
      const m = /^(\S+)\s*(>=|<=|==|!=|>|<)\s*(.+)$/.exec(inv.expression || '');
      return {
        key: Math.random().toString(36).slice(2), id: inv.id,
        field: m ? m[1] : '', comparator: m ? m[2] : '>=',
        value: m ? m[3].replace(/^['"]|['"]$/g, '') : '',
        description: inv.description || '', _valid: true, _errors: [],
      };
    });
  });
  const [attached, setAttached] = dUseState(() => new Set((initialSpec?.operators || []).map(o => o.name)));
  const [saving, setSaving] = dUseState(false);
  const [saveError, setSaveError] = dUseState(null);
  const [refusal, setRefusal] = dUseState(null);
  const [savedResult, setSavedResult] = dUseState(null);

  const stateSchema = dUseMemo(() => dimensionsToStateSchema(dimensions), [dimensions]);

  const canSave = displayName.trim() && dimensions.some(d => d.name) &&
    invariants.every(inv => !inv.field || inv._valid !== false);

  const buildPayload = () => ({
    display_name: displayName.trim(),
    description: description.trim(),
    dimensions: dimensions.filter(d => d.name).map(d => ({
      name: d.name, type: d.type, description: d.description,
      default_value: d.default_value === '' ? null : (d.type === 'number' ? Number(d.default_value) : d.type === 'boolean' ? d.default_value === 'true' || d.default_value === true : d.default_value),
      minimum: d.minimum === '' ? null : Number(d.minimum),
    })),
    invariants: invariants.filter(inv => inv.id && inv.field && inv.value !== '').map(inv => {
      const field = dimensions.find(d => d.name === inv.field);
      return {
        id: inv.id,
        expression: composeExpression(inv.field, inv.comparator, inv.value, field?.type || 'number'),
        description: inv.description,
      };
    }),
    operator_names: Array.from(attached),
  });

  const handleSave = async () => {
    setSaving(true); setSaveError(null); setRefusal(null); setSavedResult(null);
    try {
      const payload = buildPayload();
      const resp = editingId
        ? await api.patch(`/devui/definitions/${editingId}`, { user_id: userId, ...payload })
        : await api.post('/devui/definitions', { user_id: userId, ...payload });
      const data = resp?.data;
      if (data?.status === 'refused') {
        setRefusal(data);
      } else {
        setSavedResult(data);
        window.flash?.(`"${displayName}" ${editingId ? 'updated' : 'created'}`, 'ok');
        onSaved?.(data);
      }
    } catch (e) {
      setSaveError(String(e?.message || e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 'var(--radius-md)', padding: 14 }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
          <span className="label-11">{editingId ? 'EDIT DEFINITION' : 'NEW DEFINITION'}</span>
          <button style={btnStyle()} onClick={onCancel}>← BACK</button>
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 2fr', gap: 10, marginBottom: 4 }}>
          <div>
            <FieldLabel>NAME</FieldLabel>
            <input style={inputStyle} placeholder="Plant Tracker" value={displayName} onChange={e => setDisplayName(e.target.value)} />
          </div>
          <div>
            <FieldLabel>DESCRIPTION</FieldLabel>
            <input style={inputStyle} placeholder="Tracks a houseplant's watering and health" value={description} onChange={e => setDescription(e.target.value)} />
          </div>
        </div>
      </div>

      <div style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 'var(--radius-md)', padding: 14 }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
          <span className="label-11">DIMENSIONS · your sustain's state</span>
          <button style={btnStyle()} onClick={() => setDimensions([...dimensions, newDimension()])}>+ ADD DIMENSION</button>
        </div>
        {dimensions.map(dim => (
          <DimensionRow key={dim.key} dim={dim}
            onChange={next => setDimensions(dimensions.map(d => d.key === dim.key ? next : d))}
            onRemove={() => setDimensions(dimensions.filter(d => d.key !== dim.key))} />
        ))}
      </div>

      <div style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 'var(--radius-md)', padding: 14 }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
          <span className="label-11">INVARIANTS · viable region</span>
          <button style={btnStyle()} disabled={dimensions.every(d => !d.name)}
            onClick={() => setInvariants([...invariants, newInvariantRow(dimensions.filter(d => d.name))])}>
            + ADD INVARIANT
          </button>
        </div>
        {invariants.length === 0
          ? <EmptyLine>no invariants yet · your sustain has no viable-region rules</EmptyLine>
          : invariants.map(inv => (
            <InvariantRow key={inv.key} inv={inv} dimensions={dimensions.filter(d => d.name)} stateSchema={stateSchema}
              onChange={next => setInvariants(invariants.map(i => i.key === inv.key ? next : i))}
              onRemove={() => setInvariants(invariants.filter(i => i.key !== inv.key))} />
          ))}
      </div>

      <div style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 'var(--radius-md)', padding: 14 }}>
        <div style={{ marginBottom: 8 }}><span className="label-11">OPERATORS · attach existing capabilities</span></div>
        <OperatorPicker registry={opRegistry} attached={attached}
          onToggle={name => setAttached(prev => {
            const next = new Set(prev);
            next.has(name) ? next.delete(name) : next.add(name);
            return next;
          })} />
      </div>

      {refusal && (
        <div style={{ background: 'var(--bg-surface)', border: '1px solid var(--danger)', borderRadius: 'var(--radius-md)', padding: 14 }}>
          <div className="label-11" style={{ color: 'var(--danger)', marginBottom: 6 }}>EDIT REFUSED</div>
          <div className="meta-10" style={{ color: 'var(--text-secondary)', marginBottom: 8 }}>{refusal.reason}</div>
          {(refusal.blocked_by || []).map((v, i) => (
            <div key={i} className="meta-10" style={{ color: 'var(--text-muted)', padding: '4px 0' }}>
              sustain {v.sustain_id.slice(0, 8)}… — invariant '{v.invariant_id}' ({v.expression}): {v.reason}
            </div>
          ))}
        </div>
      )}
      {saveError && <div className="meta-10" style={{ color: 'var(--danger)' }}>save failed · {saveError}</div>}

      {savedResult ? (
        <div style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 'var(--radius-md)', padding: 14, display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <span className="meta-10" style={{ color: 'var(--text-secondary)' }}>✓ saved · v{savedResult.version}</span>
          <button style={btnStyle()} onClick={onCancel}>DONE</button>
        </div>
      ) : (
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 10 }}>
          <button style={btnStyle('amber')} disabled={!canSave || saving} onClick={handleSave}>
            {saving ? 'SAVING…' : editingId ? 'SAVE CHANGES' : 'CREATE DEFINITION'}
          </button>
        </div>
      )}
    </div>
  );
}

/* ─── Root panel ──────────────────────────────────────────────────────── */

function DefinePanel({ authUser, refreshSustains, switchPanel }) {
  // GET /api/v1/users/me (what populates authUser in shell.jsx) returns
  // "user_id", not "id" — unlike the sustains/definitions tables' own row
  // shape, which uses "id"/"owner_user_id". Easy to get wrong once; kept
  // as one named constant so it's wrong in at most one place.
  const userId = authUser?.user_id;
  const [view, setView] = dUseState('list');
  const [definitions, setDefinitions] = dUseState([]);
  const [loading, setLoading] = dUseState(true);
  const [editingId, setEditingId] = dUseState(null);
  const [editingSpec, setEditingSpec] = dUseState(null);
  const [opRegistry, setOpRegistry] = dUseState({});
  const [instantiatingId, setInstantiatingId] = dUseState(null);

  const loadDefinitions = () => {
    if (!userId) return;
    setLoading(true);
    api.get(`/devui/definitions?user_id=${encodeURIComponent(userId)}`)
      .then(d => setDefinitions(d?.data?.definitions || []))
      .catch(() => setDefinitions([]))
      .finally(() => setLoading(false));
  };

  dUseEffect(() => { loadDefinitions(); }, [userId]);
  dUseEffect(() => {
    api.get('/devui/registry/operators').then(d => setOpRegistry(d?.data?.operators || {})).catch(() => {});
  }, []);

  const openNew = () => { setEditingId(null); setEditingSpec(null); setView('builder'); };

  const openEdit = async (templateId) => {
    try {
      const d = await api.get(`/devui/definitions/${templateId}`);
      setEditingId(templateId);
      setEditingSpec(d?.data?.spec || null);
      setView('builder');
    } catch (e) {
      window.flash?.(`could not load definition · ${String(e?.message || e)}`, 'err');
    }
  };

  const handleInstantiate = async (templateId) => {
    setInstantiatingId(templateId);
    try {
      const d = await api.post('/devui/sustains', { template_id: templateId, user_id: userId, parameters: {} });
      const sid = d?.data?.sustain_id;
      await refreshSustains?.(sid);
      window.flash?.('sustain created from definition', 'ok');
      switchPanel?.('monitor');
    } catch (e) {
      window.flash?.(`create failed · ${String(e?.message || e)}`, 'err');
    } finally {
      setInstantiatingId(null);
    }
  };

  const handleSaved = () => {
    loadDefinitions();
  };

  if (!userId) {
    return (
      <div style={{ padding: '24px 28px' }}>
        <EmptyLine>sign in to define your own sustains</EmptyLine>
      </div>
    );
  }

  return (
    <div className="panel-enter" style={{ display: 'flex', flexDirection: 'column', gap: 18, padding: '24px 28px', overflowY: 'auto', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <span className="meta-10" style={{ color: 'var(--text-muted)' }}>SUSTENA XII · DEFINE · CREATE YOUR OWN SUSTAIN</span>
      </div>

      {view === 'list' && (
        <DefinitionsList
          definitions={definitions} loading={loading}
          onNew={openNew} onEdit={openEdit}
          onInstantiate={handleInstantiate} instantiatingId={instantiatingId}
        />
      )}

      {view === 'builder' && (
        <DefinitionBuilder
          editingId={editingId} initialSpec={editingSpec} opRegistry={opRegistry} userId={userId}
          onSaved={handleSaved}
          onCancel={() => { setView('list'); loadDefinitions(); }}
        />
      )}
    </div>
  );
}

window.DefinePanel = DefinePanel;
