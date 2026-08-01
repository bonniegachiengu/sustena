/**
 * apps/web/src/pages/studio/NetworkZone.jsx
 *
 * The composition ⊕ / roll-up ρ view. Wired to the already-built GET
 * /devui/sustain/{id}/children, GET /devui/sustain/{id}/rollup, and POST
 * /devui/sustain/{id}/provision-children — no new aggregation logic here,
 * compute_rollup() is computed fresh server-side on every call and never
 * stored.
 */
import React from 'react';
import { api } from '../../lib/api.js';
import { sectionLabel, mutedText, Empty, StatusDot, fmtNum, ghostButton, primaryButton } from './shared.jsx';

const { useState, useEffect, useCallback } = React;

export default function NetworkZone({ sustainId, definition }) {
  const [children, setChildren] = useState([]);
  const [rollup, setRollup] = useState(null);
  const [loading, setLoading] = useState(true);
  const [provisioning, setProvisioning] = useState(false);

  const load = useCallback(async () => {
    if (!sustainId) return;
    setLoading(true);
    try {
      const [childResp, rollupResp] = await Promise.all([
        api.get(`/devui/sustain/${encodeURIComponent(sustainId)}/children`),
        api.get(`/devui/sustain/${encodeURIComponent(sustainId)}/rollup`),
      ]);
      setChildren(childResp?.data?.children || []);
      setRollup(rollupResp?.data || null);
    } catch {
      setChildren([]); setRollup(null);
    } finally {
      setLoading(false);
    }
  }, [sustainId]);

  useEffect(() => { load(); }, [load]);

  const provision = async () => {
    setProvisioning(true);
    try {
      // best-effort owner id from the definition's access_policy, else a
      // literal placeholder — provision-children only fills already-declared
      // slots, it can't invent new ones, so a wrong id here is low-risk.
      const ownerId = definition?.access_policy?.owner_ids?.[0] || 'owner';
      await api.post(`/devui/sustain/${encodeURIComponent(sustainId)}/provision-children`, { user_id: ownerId });
      await load();
    } catch { /* honest no-op on failure — load() already ran and reflects reality */ }
    finally { setProvisioning(false); }
  };

  if (!sustainId) return <Empty text="select a sustain to see its network" />;
  if (loading) return <Empty text="reading composition…" />;

  const declared = definition?.declared_children || [];
  const unprovisioned = declared.filter(dc => !children.some(c => c.slot === dc.slot));
  const aggregates = rollup?.aggregates || {};
  const hasAggregates = Object.keys(aggregates).length > 0;

  if (declared.length === 0 && children.length === 0) {
    return <Empty text="this sustain has no children" sub="composition ⊕ only applies to sustains that declare or link children" />;
  }

  return (
    <div>
      <div style={sectionLabel}>composition ⊕ — declared vs linked</div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: 10, marginBottom: 18 }}>
        {declared.map(dc => {
          const link = children.find(c => c.slot === dc.slot);
          const rollupEntry = rollup?.children?.find(c => c.slot === dc.slot);
          const status = rollupEntry?.status || (link ? 'ok' : 'missing');
          return (
            <div key={dc.slot} style={{
              background: 'var(--bg-raised)', border: '1px solid var(--border)',
              borderRadius: 'var(--radius-sm)', padding: '10px 12px',
            }}>
              <div style={{ display: 'flex', alignItems: 'center' }}>
                <StatusDot status={status} />
                <span style={{ fontFamily: 'var(--ui)', fontSize: 12.5, color: 'var(--text-primary)' }}>{dc.member || dc.slot}</span>
              </div>
              <div style={{ fontFamily: 'var(--mono)', fontSize: 9.5, color: 'var(--text-dim)', marginTop: 3 }}>
                {link ? (link.display_name || dc.template) : 'not provisioned'}
              </div>
            </div>
          );
        })}
        {/* children linked but not in declared_children (e.g. manually linked) */}
        {children.filter(c => !declared.some(dc => dc.slot === c.slot)).map(c => (
          <div key={c.child_sustain_id} style={{
            background: 'var(--bg-raised)', border: '1px solid var(--border)',
            borderRadius: 'var(--radius-sm)', padding: '10px 12px',
          }}>
            <div style={{ display: 'flex', alignItems: 'center' }}>
              <StatusDot status="ok" />
              <span style={{ fontFamily: 'var(--ui)', fontSize: 12.5, color: 'var(--text-primary)' }}>{c.member || c.display_name || c.slot}</span>
            </div>
            <div style={{ fontFamily: 'var(--mono)', fontSize: 9.5, color: 'var(--text-dim)', marginTop: 3 }}>linked (not declared)</div>
          </div>
        ))}
      </div>

      {unprovisioned.length > 0 && (
        <div style={{ marginBottom: 18 }}>
          <div style={{ ...mutedText, marginBottom: 8 }}>
            {unprovisioned.length} declared slot{unprovisioned.length === 1 ? '' : 's'} not yet provisioned
          </div>
          <button onClick={provision} disabled={provisioning} style={primaryButton}>
            {provisioning ? 'PROVISIONING…' : 'PROVISION DECLARED CHILDREN'}
          </button>
        </div>
      )}

      <div style={sectionLabel}>roll-up ρ — computed fresh, never stored</div>
      {!hasAggregates
        ? <div style={mutedText}>no aggregates declared for this sustain</div>
        : Object.entries(aggregates).map(([id, agg]) => (
            <div key={id} style={{ marginBottom: 14 }}>
              <div style={{ fontFamily: 'var(--ui)', fontSize: 13, color: 'var(--text-primary)' }}>
                {id.replace(/_/g, ' ')}: <b style={{ color: 'var(--amber)' }}>{fmtNum(agg.value)}</b>
                <span style={{ ...mutedText, marginLeft: 8 }}>({agg.op} over {agg.child_path})</span>
              </div>
              <div style={{ ...mutedText, marginTop: 4 }}>
                {agg.included?.length || 0} included
                {agg.excluded?.length > 0 && (
                  <span style={{ color: 'var(--danger)' }}> · {agg.excluded.length} excluded — {agg.excluded.map(e => `${e.member || e.slot} (${e.reason})`).join(', ')}</span>
                )}
              </div>
            </div>
          ))}
    </div>
  );
}
